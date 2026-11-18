use super::*;
use crate::checkpoint::{Reader, Writer};
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

fn saved(env: &Environment) -> Vec<u8> {
    let mut w = Writer::new();
    env.write_checkpoint(&mut w);
    w.0
}
fn restore(env: &Environment) -> Environment {
    Environment::read_checkpoint(&mut Reader::new(&saved(env)), 8).unwrap()
}

#[test]
fn edits_preserve_identity_and_existing_material_while_individual_flow_sources_are_directed() {
    let mut env = Environment::default();
    let total = env.nutrient_total();
    let id = env.add_source(6, 600.0, 380.0, 0.5, 40.0);
    assert!(id > 0);
    assert!(env.update_source(id, 6, 600.0, 380.0, 0.5, 40.0, 0.0, 1000.0));
    let forward = env.flow_at(700.0, 380.0, 0.0);
    assert!((forward.0 - 30.0).abs() < 1e-10 && forward.1.abs() < 1e-10);
    assert!(env.flow_at(600.0, 500.0, 0.0).0 < 0.01);
    assert!(env.update_source(id, 6, 600.0, 380.0, -0.5, 40.0, FRAC_PI_2, 600.0));
    let reverse = env.flow_at(600.0, 400.0, 0.0);
    assert!(reverse.0.abs() < 1e-10 && reverse.1 < -29.9);
    assert!(env.update_source(id, 5, 600.0, 380.0, 1.0, 40.0, 0.0, 200.0));
    assert!(env.flow_at(600.0, 380.0, 0.0).0 > 59.9);
    assert!(env.flow_at(1000.0, 380.0, 0.0).0 < 0.01);
    assert!(env.update_source(id, 7, 600.0, 380.0, 1.0, 100.0, 0.0, 600.0));
    assert!(env.flow_at(650.0, 380.0, 0.0).0 > 0.0);
    assert!(env.update_source(id, 7, 600.0, 380.0, -1.0, 100.0, 0.0, 600.0));
    assert!(env.flow_at(650.0, 380.0, 0.0).0 < 0.0);
    assert_eq!(env.sources.len(), 1);
    assert_eq!(env.sources[0].id, id);
    assert_eq!(env.nutrient_total(), total);
    let before = saved(&env);
    assert!(!env.update_source(id, 7, f64::NAN, 0.0, 1.0, 100.0, 0.0, 600.0));
    assert_eq!(saved(&env), before);
}

#[test]
fn absorber_exports_only_available_mass_and_has_a_persistent_separate_ledger() {
    let mut env = Environment::default();
    env.set_initial_nutrients(0.0);
    env.reset_dynamic();
    env.add_nutrients(610.0, 390.0, 0.1);
    let initial = env.nutrient_total();
    let sink = env.add_source(4, 610.0, 390.0, 1.0, 20.0);
    for tick in 0..100 {
        env.step(tick as f64 * DT);
    }
    assert!(env.nutrient_removed_total() > 0.0 && env.nutrient_removed_total() <= initial);
    assert!(env.nutrients.iter().all(|&v| v >= 0.0));
    assert!((env.nutrient_total() + env.nutrient_removed_total() - initial).abs() < 1e-12);
    assert_eq!(saved(&env), saved(&restore(&env)));
    assert!(env.remove_source(sink));
    let removed = env.nutrient_removed_total();
    env.step(101.0 * DT);
    assert_eq!(removed, env.nutrient_removed_total());
    env.reset_dynamic();
    assert_eq!(env.nutrient_removed_total(), 0.0);
}

#[test]
fn resize_preserves_mass_and_hot_field_checkpoints_for_non_integer_scale_ratios() {
    let mut env = Environment::default();
    env.configure(1.0, 0.2, 0.0);
    env.emit_organic(710.0, 330.0, 1e6);
    let initial = env.nutrient_total();
    let inputs = env.nutrient_input_total();
    let id = env.add_source(3, 600.0, 380.0, 1.0, 100.0);
    for (width, height) in [
        (2380.0, 1500.0),
        (1180.0, 740.0),
        (4800.0, 3040.0),
        (1200.0, 760.0),
    ] {
        assert!(env.set_size(width, height));
        assert!((env.nutrient_total() - initial).abs() < 1e-8);
        assert_eq!(env.nutrient_input_total(), inputs);
        assert_eq!(env.sources[0].id, id);
        assert_eq!(env.patch_count(), (width * height / 400.0) as usize);
        assert_eq!(saved(&env), saved(&restore(&env)));
        assert!(env.temperature.iter().all(|&v| v <= 1.0 && v >= 0.0));
    }
    let before = saved(&env);
    assert!(!env.set_size(1201.0, 760.0));
    assert_eq!(saved(&env), before);
}

#[test]
fn fixed_walls_block_mineral_flux_heat_and_organic_transport_without_destroying_material() {
    let mut env = Environment::default();
    env.set_initial_nutrients(0.0);
    env.reset_dynamic();
    assert!(env.add_barrier(400.0, 380.0, 760.0, 20.0, FRAC_PI_2, false) > 0);
    assert!(env.add_barrier(600.0, 380.0, 760.0, 20.0, FRAC_PI_2, false) > 0);
    env.add_nutrients(510.0, 390.0, 100.0);
    env.emit_organic(510.0, 390.0, 100.0);
    let jet = env.add_source(6, 500.0, 380.0, 1.0, 500.0);
    env.update_source(jet, 6, 500.0, 380.0, 1.0, 500.0, 0.0, 1200.0);
    for tick in 0..90 {
        env.step(tick as f64 * DT);
    }
    assert!((env.nutrient_total() - 100.0).abs() < 1e-9);
    for i in 0..env.patch_count() {
        let (x, _) = env.patch_position(i);
        if !(400.0..600.0).contains(&x) {
            assert_eq!(env.nutrients[i], 0.0, "mineral leaked at {x}");
            assert_eq!(env.organic[i], 0.0, "cue leaked at {x}");
        }
    }
    // Heat travels by the same blocked-edge diffusion and interpolated advection.
    env.clear_sources();
    let inside = env.patch_index(510.0, 390.0);
    env.temperature[inside] = 1.0;
    for tick in 90..120 {
        env.step(tick as f64 * DT);
    }
    for i in 0..env.patch_count() {
        let (x, _) = env.patch_position(i);
        if !(400.0..600.0).contains(&x) {
            assert_eq!(env.temperature[i], 0.5);
        }
    }
}

#[test]
fn swept_collisions_do_not_tunnel_and_handle_torus_seams_and_rotated_walls() {
    let mut env = Environment::default();
    let wall = env.add_barrier(600.0, 380.0, 760.0, 20.0, FRAC_PI_2, false);
    assert!(wall > 0);
    let point = env.move_circle((400.0, 380.0), (800.0, 400.0), 7.0);
    assert!(point.0 < 583.01 && point.0 > 582.9);
    assert!((point.1 - 400.0).abs() < 0.01);
    let projected = env.move_circle((600.0, 380.0), (600.0, 380.0), 7.0);
    assert!(!env.solid_at(projected.0, projected.1));
    assert!(env.update_barrier(wall, 0.0, 380.0, 760.0, 20.0, FRAC_PI_2, false));
    let point = env.move_circle((1160.0, 380.0), (40.0, 380.0), 7.0);
    assert!(point.0 < 1183.01 && point.0 > 1182.9);
    assert!(env.remove_barrier(wall));
    let point = env.move_circle((1160.0, 380.0), (40.0, 380.0), 7.0);
    assert_eq!(point, (40.0, 380.0));
    assert!(env.add_barrier(600.0, 380.0, 400.0, 20.0, FRAC_PI_4, false) > 0);
    assert!(env.segment_blocked((400.0, 380.0), (800.0, 380.0)));
    let point = env.move_circle((400.0, 380.0), (800.0, 380.0), 7.0);
    assert!(!env.solid_at(point.0, point.1));
}

#[test]
fn floating_barriers_drift_fixed_barriers_do_not_and_continuation_is_exact() {
    let mut env = Environment::default();
    let source = env.add_source(6, 600.0, 200.0, 0.5, 150.0);
    env.update_source(source, 6, 600.0, 200.0, 0.5, 150.0, 0.0, 1200.0);
    let fixed = env.add_barrier(100.0, 500.0, 120.0, 20.0, 0.0, false);
    let floating = env.add_barrier(300.0, 200.0, 120.0, 20.0, 0.0, true);
    for tick in 0..10 {
        env.step(tick as f64 * DT);
    }
    assert_eq!(
        env.barriers.iter().find(|b| b.id == fixed).unwrap().x,
        100.0
    );
    assert!(env.barriers.iter().find(|b| b.id == floating).unwrap().x > 300.0);
    let mut resumed = restore(&env);
    for tick in 10..25 {
        env.step(tick as f64 * DT);
        resumed.step(tick as f64 * DT);
    }
    assert_eq!(saved(&env), saved(&resumed));
    // Opposite world images overlap despite their shortest center delta not overlapping.
    env.clear_barriers();
    env.clear_sources();
    assert!(env.set_size(400.0, 400.0));
    let fixed = env.add_barrier(200.0, 200.0, 500.0, 20.0, FRAC_PI_4, false);
    let moving = env.add_barrier(10.0, 390.0, 500.0, 20.0, FRAC_PI_4, true);
    assert!(fixed > 0 && moving > 0);
    env.step(0.0);
    let b = env.barriers.iter().find(|b| b.id == moving).unwrap();
    assert!(b.x != 10.0 || b.y != 390.0, "periodic overlap was ignored");
    let stable = (b.x, b.y);
    env.step(DT);
    let b = env.barriers.iter().find(|b| b.id == moving).unwrap();
    assert!(
        (b.x - stable.0).hypot(b.y - stable.1) < 1e-6,
        "periodic separation must settle"
    );
}

#[test]
fn invalid_barrier_geometry_and_incompatible_resize_are_atomic() {
    let mut env = Environment::default();
    let id = env.add_barrier(600.0, 380.0, 1000.0, 20.0, 0.0, false);
    assert!(id > 0);
    let before = saved(&env);
    assert!(!env.set_size(400.0, 400.0));
    assert_eq!(saved(&env), before);
    assert!(!env.update_barrier(id, 600.0, 380.0, 2400.0, 400.0, FRAC_PI_4, false));
    assert_eq!(saved(&env), before);
    assert_eq!(
        env.add_barrier(600.0, 380.0, 2400.0, 400.0, FRAC_PI_4, false),
        0
    );
    assert_eq!(saved(&env), before);
    // Exact cardinal orientations survive the ABI's f32 angle precision.
    assert!(env.add_barrier(300.0, 380.0, 760.0, 40.0, (FRAC_PI_2 as f32) as f64, false) > 0);
}

#[test]
fn cached_grid_flow_matches_direct_flow_after_edits_motion_resize_and_restore() {
    let mut env = Environment::default();
    let band = env.add_source(6, 600.0, 380.0, 0.7, 90.0);
    let vortex = env.add_source(2, 350.0, 410.0, -0.8, 140.0);
    env.add_barrier(580.0, 370.0, 240.0, 40.0, 0.3, true);
    env.add_barrier(10.0, 730.0, 300.0, 25.0, 0.9, false);
    for revision in 0..6 {
        match revision {
            1 => {
                assert!(env.update_source(band, 5, 650.0, 420.0, -0.4, 180.0, 1.3, 900.0));
            }
            2 => {
                env.configure(0.5, 0.0, 12.0);
            }
            3 => {
                env.configure(0.5, 0.0, 0.0);
                assert!(env.set_size(2380.0, 1500.0));
            }
            4 => {
                assert!(env.remove_source(vortex));
                env = restore(&env);
            }
            5 => {
                env.clear_sources();
                env.clear_barriers();
            }
            _ => {}
        }
        let time = revision as f64 * DT;
        env.step(time);
        for i in 0..env.patch_count() {
            let (x, y) = env.patch_position(i);
            assert_eq!(
                env.grid_flow(i, 0, time),
                env.flow_at(x, y, time),
                "center {i}, revision {revision}"
            );
            if !env.blocked_right[i] {
                assert_eq!(
                    env.grid_flow(i, 1, time),
                    env.flow_at(x + 10.0, y, time),
                    "right {i}, revision {revision}"
                );
            }
            if !env.blocked_down[i] {
                assert_eq!(
                    env.grid_flow(i, 2, time),
                    env.flow_at(x, y + 10.0, time),
                    "down {i}, revision {revision}"
                );
            }
        }
    }
}
