//! Reference-formula checks for derived environment caches.
use super::*;
use crate::checkpoint::{Reader, Writer};

fn state(env: &Environment) -> Vec<u8> {
    let mut writer = Writer::new();
    env.write_checkpoint(&mut writer);
    writer.0
}
fn resume(env: &Environment) -> Environment {
    Environment::read_checkpoint(&mut Reader::new(&state(env)), 17).unwrap()
}
fn bits(pair: (f64, f64)) -> (u64, u64) {
    (pair.0.to_bits(), pair.1.to_bits())
}

// The pre-cache flow formula, deliberately evaluated directly from canonical
// sources rather than using any prepared coefficients or grid tables.
fn reference_flow(env: &Environment, x: f64, y: f64, time: f64) -> (f64, f64) {
    let (mut vx, mut vy) = if env.current_strength == 0.0 {
        (0.0, 0.0)
    } else {
        let phase = time * 0.04;
        let a = TAU * x / env.width + phase;
        let b = TAU * y / env.height - phase;
        (
            env.current_strength * b.sin() * a.cos(),
            -env.current_strength * env.height / env.width * b.cos() * a.sin(),
        )
    };
    for source in &env.sources {
        if !matches!(source.kind, 2 | 5 | 6 | 7) || source.strength == 0.0 {
            continue;
        }
        let dx = delta(x - source.x, env.width);
        let dy = delta(y - source.y, env.height);
        let along = dx * source.angle.cos() + dy * source.angle.sin();
        let across = -dx * source.angle.sin() + dy * source.angle.cos();
        match source.kind {
            2 | 7 => {
                let weight = source.strength * 60.0 / source.radius
                    * (-2.0 * (dx * dx + dy * dy) / source.radius.powi(2)).exp();
                if source.kind == 2 {
                    vx -= dy * weight;
                    vy += dx * weight;
                } else {
                    vx += dx * weight;
                    vy += dy * weight;
                }
            }
            5 => {
                let weight = source.strength
                    * 60.0
                    * (-2.0 * (across / source.radius).powi(2)
                        - 2.0 * (along / (source.length * 0.5)).powi(2))
                    .exp();
                vx += source.angle.cos() * weight;
                vy += source.angle.sin() * weight;
            }
            6 => {
                let end = (along.abs() - source.length * 0.5).max(0.0);
                let weight = source.strength
                    * 60.0
                    * (-2.0 * (across * across + end * end) / source.radius.powi(2)).exp();
                vx += source.angle.cos() * weight;
                vy += source.angle.sin() * weight;
            }
            _ => unreachable!(),
        }
    }
    let speed = vx.hypot(vy);
    if speed > 60.0 {
        (vx * 60.0 / speed, vy * 60.0 / speed)
    } else {
        (vx, vy)
    }
}

fn reference_blocked(env: &Environment, from: (f64, f64), to: (f64, f64)) -> bool {
    let movement = (
        delta(to.0 - from.0, env.width),
        delta(to.1 - from.1, env.height),
    );
    env.barriers.iter().any(|barrier| {
        let c = barrier.angle.cos();
        let s = barrier.angle.sin();
        let dx = delta(from.0 - barrier.x, env.width);
        let dy = delta(from.1 - barrier.y, env.height);
        let half = [barrier.length * 0.5, barrier.width * 0.5];
        let velocity = [
            movement.0 * c + movement.1 * s,
            -movement.0 * s + movement.1 * c,
        ];
        for oy in [-env.height, 0.0, env.height] {
            for ox in [-env.width, 0.0, env.width] {
                let point = [
                    (dx + ox) * c + (dy + oy) * s,
                    -(dx + ox) * s + (dy + oy) * c,
                ];
                let mut entry: f64 = 0.0;
                let mut leave: f64 = 1.0;
                let mut intersects = true;
                for axis in 0..2 {
                    if velocity[axis].abs() < 1e-12 {
                        if point[axis].abs() > half[axis] {
                            intersects = false;
                            break;
                        }
                    } else {
                        let a = (-half[axis] - point[axis]) / velocity[axis];
                        let b = (half[axis] - point[axis]) / velocity[axis];
                        entry = entry.max(a.min(b));
                        leave = leave.min(a.max(b));
                        if entry > leave {
                            intersects = false;
                            break;
                        }
                    }
                }
                if intersects && leave >= 0.0 && entry <= 1.0 {
                    return true;
                }
            }
        }
        false
    })
}

fn reference_transport(env: &Environment, values: &[f64], origin: usize, x: f64, y: f64) -> f64 {
    if env.barriers.is_empty() {
        return env.sample(values, x, y);
    }
    let from = env.patch_position(origin);
    if delta(x - from.0, env.width).abs() + delta(y - from.1, env.height).abs() < 1e-12 {
        return values[origin];
    }
    if reference_blocked(env, from, (x, y)) {
        return values[origin];
    }
    let gx = x.rem_euclid(env.width) / PATCH_SIZE - 0.5;
    let gy = y.rem_euclid(env.height) / PATCH_SIZE - 0.5;
    let ix = gx.floor() as isize;
    let iy = gy.floor() as isize;
    let fx = gx - gx.floor();
    let fy = gy - gy.floor();
    let mut value = 0.0;
    let mut total = 0.0;
    for (dx, dy, weight) in [
        (0, 0, (1.0 - fx) * (1.0 - fy)),
        (1, 0, fx * (1.0 - fy)),
        (0, 1, (1.0 - fx) * fy),
        (1, 1, fx * fy),
    ] {
        if weight <= 0.0 {
            continue;
        }
        let index = (iy + dy).rem_euclid(env.rows as isize) as usize * env.columns
            + (ix + dx).rem_euclid(env.columns as isize) as usize;
        if index == origin || !reference_blocked(env, from, env.patch_position(index)) {
            value += values[index] * weight;
            total += weight;
        }
    }
    if total > 0.0 {
        value / total
    } else {
        values[origin]
    }
}

#[test]
fn ordered_flow_tables_and_cap_fast_path_match_uncached_formula_bit_for_bit() {
    let mut env = Environment::default();
    assert!(env.set_size(400.0, 420.0));
    for (kind, strength, angle) in [
        (6, 1.0, 0.0),
        (5, 0.8, 1.23),
        (2, -0.9, 0.7),
        (7, 0.5, 2.9),
        (6, -0.7, 3.4),
    ] {
        assert!(env.add_source_configured(kind, 13.7, 401.1, strength, 137.0, angle, 600.0) > 0);
    }
    for revision in 0..5 {
        match revision {
            1 => env.configure(0.5, 0.2, 8.0),
            2 => {
                assert!(env.set_size(1180.0, 740.0));
                env.configure(0.5, 0.2, 30.0);
            }
            3 => {
                env = resume(&env);
                env.configure(0.5, 0.2, 0.0);
            }
            4 => {
                env.clear_sources();
                env.configure(0.5, 0.2, 8.0);
            }
            _ => {}
        }
        for time in [0.0, DT, 17.12345] {
            env.prepare_grid_flow(time);
            let phase = env.snapshot_phase(time);
            for index in 0..env.patch_count() {
                let (x, y) = env.patch_position(index);
                for (face, (x, y)) in [(x, y), (x + 10.0, y), (x, y + 10.0)]
                    .into_iter()
                    .enumerate()
                {
                    let expected = bits(reference_flow(&env, x, y, time));
                    assert_eq!(
                        bits(env.grid_flow(index, face, time)),
                        expected,
                        "grid {index}/{face}, revision {revision}"
                    );
                    assert_eq!(
                        bits(env.grid_flow_with_phase(index, face, time, &phase)),
                        expected
                    );
                }
            }
            for point in [
                (0.0, 0.0),
                (-0.0, 0.0),
                (13.7, 401.1),
                (-0.5, -1.25),
                (env.width, env.height),
                (env.width * 1.9, env.height * 2.3),
                (197.123, 241.993),
            ] {
                assert_eq!(
                    bits(env.raw_flow_at(point.0, point.1, time)),
                    bits(reference_flow(&env, point.0, point.1, time))
                );
            }
        }
    }
}

#[test]
fn local_wall_queries_and_scalar_stencils_match_periodic_reference_after_edits() {
    let mut env = Environment::default();
    assert!(env.set_size(400.0, 400.0));
    env.add_source_configured(6, 130.0, 200.0, 0.8, 65.0, 0.12, 500.0);
    let wall = env.add_barrier(
        200.0,
        200.0,
        500.0,
        20.0,
        std::f64::consts::FRAC_PI_4,
        false,
    );
    env.add_barrier(10.0, 390.0, 150.0, 25.0, 0.93, true);
    let mut seed = 149u64;
    for revision in 0..5 {
        match revision {
            1 => {
                env.step(0.0);
                env.step(DT);
            }
            2 => {
                env.update_barrier(wall, 395.0, 12.0, 300.0, 30.0, 0.4, false);
                env = resume(&env);
            }
            3 => {
                assert!(env.set_size(1180.0, 740.0));
            }
            4 => env.clear_barriers(),
            _ => {}
        }
        let values: Vec<_> = (0..env.patch_count())
            .map(|i| (i as f64 * 1.731).sin().abs())
            .collect();
        for origin in 0..env.patch_count() {
            let from = env.patch_position(origin);
            for offset in [
                (0.0, 0.0),
                (1.93, -0.17),
                (-1.99, 0.83),
                (20.0, -20.0),
                (-20.0, 20.0),
                (31.0, -43.0),
            ] {
                let to = (from.0 + offset.0, from.1 + offset.1);
                assert_eq!(
                    env.transport_segment_blocked(origin, to),
                    reference_blocked(&env, from, to)
                );
                assert_eq!(
                    env.transport_stencil(origin, to.0, to.1)
                        .sample(&values)
                        .to_bits(),
                    reference_transport(&env, &values, origin, to.0, to.1).to_bits(),
                    "patch {origin}, revision {revision}"
                );
            }
        }
        for _ in 0..500 {
            let mut next = || {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                (seed >> 11) as f64 / ((1u64 << 53) as f64)
            };
            let from = (
                (next() * 3.0 - 1.0) * env.width,
                (next() * 3.0 - 1.0) * env.height,
            );
            let to = (
                (next() * 3.0 - 1.0) * env.width,
                (next() * 3.0 - 1.0) * env.height,
            );
            assert_eq!(
                env.segment_blocked(from, to),
                reference_blocked(&env, from, to)
            );
        }
    }
}

#[test]
fn source_batches_match_sequential_normalization_and_reject_atomically() {
    let mut sequential = Environment::default();
    sequential.add_source(1, 10.0, 20.0, 0.2, 100.0);
    let mut batched = sequential.clone();
    let configs = [
        SourceConfig {
            kind: 6,
            x: -2.0,
            y: 800.0,
            strength: -2.0,
            radius: 4.0,
            angle: -0.3,
            length: 5000.0,
        },
        SourceConfig {
            kind: 12,
            x: 1300.0,
            y: -80.0,
            strength: 1.3,
            radius: 5000.0,
            angle: 11.0,
            length: 1.0,
        },
    ];
    sequential.clear_sources();
    for source in configs {
        let id = sequential.add_source(
            source.kind,
            source.x,
            source.y,
            source.strength,
            source.radius,
        );
        assert!(sequential.update_source(
            id,
            source.kind,
            source.x,
            source.y,
            source.strength,
            source.radius,
            source.angle,
            source.length
        ));
    }
    assert!(batched.replace_sources(&configs));
    assert_eq!(state(&batched), state(&sequential));
    let before = state(&batched);
    let mut invalid = configs;
    invalid[1].angle = f64::NAN;
    assert!(!batched.replace_sources(&invalid));
    assert_eq!(state(&batched), before);
    assert!(!batched.replace_sources(&vec![configs[0]; MAX_SOURCES + 1]));
    assert_eq!(state(&batched), before);
    batched.next_source_id = u32::MAX;
    let before = state(&batched);
    assert!(!batched.replace_sources(&configs));
    assert_eq!(state(&batched), before);
}

#[test]
fn patch_helpers_and_metadata_preserve_canonical_state_and_full_snapshot_records() {
    let mut coordinates = Environment::default();
    coordinates.configure(0.7, 0.6, 8.0);
    coordinates.add_source_configured(6, 100.0, 200.0, 0.5, 100.0, 0.2, 600.0);
    coordinates.add_barrier(10.0, 20.0, 120.0, 30.0, 0.2, true);
    let mut patches = coordinates.clone();
    for (x, y, amount) in [(10.0, 20.0, 0.1), (-1.0, 800.0, 1.2), (600.0, 500.0, 1e7)] {
        let index = patches.patch_index(x, y);
        coordinates.recycle_nutrients(x, y, amount);
        patches.recycle_nutrients_patch(index, amount);
        coordinates.release_oxygen(x, y, amount);
        patches.release_oxygen_patch(index, amount);
        coordinates.release_inorganic_carbon(x, y, amount);
        patches.release_inorganic_carbon_patch(index, amount);
        coordinates.emit_organic(x, y, amount);
        patches.emit_organic_patch(index, amount);
    }
    assert_eq!(state(&coordinates), state(&patches));
    let before = state(&patches);
    for amount in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        patches.release_oxygen_patch(0, amount);
        patches.release_inorganic_carbon_patch(0, amount);
        patches.recycle_nutrients_patch(0, amount);
        patches.emit_organic_patch(0, amount);
    }
    patches.release_oxygen_patch(usize::MAX, 1.0);
    patches.release_inorganic_carbon_patch(usize::MAX, 1.0);
    patches.recycle_nutrients_patch(usize::MAX, 1.0);
    patches.emit_organic_patch(usize::MAX, 1.0);
    let (mut full, mut metadata) = (Vec::new(), Vec::new());
    patches.snapshot(17.0, &mut full);
    patches.metadata_snapshot(&mut metadata);
    assert_eq!(
        metadata.len(),
        21 + patches.sources.len() * 8 + patches.barriers.len() * 10
    );
    assert_eq!(&metadata[..21], &full[..21]);
    assert_eq!(&metadata[21..], &full[21 + patches.patch_count() * 7..]);
    assert_eq!(state(&patches), before);
}

#[test]
fn reusable_face_velocities_follow_source_edits_ambient_changes_and_resize() {
    let mut env = Environment::default();
    env.configure(0.5, 0.7, 0.0);
    let port = env.add_source(2, 80.0, 80.0, 0.65, 120.0);
    for stage in 0..6 {
        match stage {
            1 => assert!(env.update_source(port, 6, 30.0, 210.0, 0.7, 80.0, 0.6, 300.0)),
            2 => env.configure(0.5, 0.7, 8.0),
            3 => env.configure(0.5, 0.7, 0.0),
            4 => assert!(env.set_size(880.0, 520.0)),
            5 => assert!(env.add_barrier(4.0, 8.0, 80.0, 16.0, 0.4, true) > 0),
            _ => {}
        }
        for t in [0.0, 8.0] {
            assert!(env.prepare_transport_flow(t));
            for i in 0..env.patch_count() {
                let expected = (env.grid_flow(i, 1, t).0, env.grid_flow(i, 2, t).1);
                assert_eq!(bits(env.scratch.face_flow[i]), bits(expected));
            }
        }
    }
}
