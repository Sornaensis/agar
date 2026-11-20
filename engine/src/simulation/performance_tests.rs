//! Exact phase comparisons against the pre-optimization capture algorithm.
use super::*;
use crate::checkpoint::{Reader, Writer};

impl World {
    fn eat_reference_in(
        &mut self,
        outputs: &[Outputs],
        caps: &[BodyCapacities],
        workspace: &mut PhaseWorkspace,
    ) {
        let PhaseWorkspace {
            geometry,
            cell_grid: grid,
            ..
        } = workspace;
        grid.configure(self.environment.width,self.environment.height);
        let max_radius = grid.index_cells(geometry);
        let mut budgets = Vec::<f64>::new();
        budgets.extend(
            caps.iter()
                .zip(outputs)
                .map(|(c, o)| {
                    // Cellular capture and digestive activity are separate
                    // machines. The legacy feed command controls both phases.
                    let feed = if o.cellular.is_some() { 1.0 } else { o.feed };
                    c.capture * 1200.0 * feed * DT
                }),
        );
        self.food.retain_mut(|food| {
            let mut winner = None;
            let mut best = f64::INFINITY;
            for i in grid.nearby_radius(food.x, food.y, max_radius + 2.5) {
                let c = &self.cells[i];
                if !geometry.alive[i]
                    || budgets[i] <= 0.0
                    || c.gut_fraction_for(food.energy, food.nutrients, &caps[i]) <= 0.0
                {
                    continue;
                }
                let distance = delta(geometry.positions[i][0] - food.x, self.environment.width)
                    .hypot(delta(geometry.positions[i][1] - food.y, self.environment.height));
                if distance <= geometry.radii[i] + 2.5
                    && !self.environment.segment_blocked((c.x,c.y),(food.x,food.y))
                    && (distance < best
                        || (distance == best
                            && winner.is_none_or(|old: usize| geometry.ids[i] < geometry.ids[old])))
                {
                    best = distance;
                    winner = Some(i);
                }
            }
            if let Some(i) = winner {
                let c = &mut self.cells[i];
                let fraction = c
                    .gut_fraction_for(food.energy, food.nutrients, &caps[i])
                    .min(budgets[i] / food.energy)
                    .clamp(0.0, 1.0);
                let energy = food.energy * fraction;
                let minerals = food.nutrients * fraction;
                c.ingest_mixture(food.composition, energy, minerals, 0.0);
                food.energy = (food.energy - energy).max(0.0);
                food.nutrients = (food.nutrients - minerals).max(0.0);
                budgets[i] = (budgets[i] - energy).max(0.0);
                if food.energy == 0.0 {
                    self.environment
                        .recycle_nutrients(food.x, food.y, food.nutrients);
                    self.eaten += 1;
                    return false;
                }
            }
            true
        });
    }

}

fn state(world: &World) -> Vec<u8> {
    let mut writer = Writer::new();
    world.write_checkpoint(&mut writer);
    writer.0
}

#[test]
fn sparse_patch_groups_preserve_dense_order_and_reset_after_resize() {
    let mut workspace = PhaseWorkspace::default();
    for count in [36_480, 400, 2_280, 36_480] {
        workspace.clear_groups(count);
        assert_eq!(occupied_patches(&workspace.group_patches).count(), 0);
        assert!(workspace.groups.iter().all(Vec::is_empty));
        let mut reference = vec![Vec::new(); count];
        for (i, patch) in [count - 1, 0, 65, 63, 64, 65, count / 2, 0].into_iter().enumerate() {
            let request = (i, (i as f64 - 2.0) * 0.003);
            reference[patch].push(request);
            workspace.group_request(patch, request);
        }
        let sparse: Vec<_> = occupied_patches(&workspace.group_patches)
            .map(|patch| (patch, workspace.groups[patch].clone())).collect();
        let dense: Vec<_> = reference.into_iter().enumerate().filter(|(_, group)| !group.is_empty()).collect();
        assert_eq!(sparse, dense);
    }
}

#[test]
fn eligible_capture_index_preserves_empty_exhausted_wrapped_and_mineral_cases() {
    for scenario in 0..8 {
        let mut optimized = World::empty(317);

        if scenario != 0 {
            optimized.try_seed_fixture(1, 8, WIDTH - 3.0, HEIGHT - 4.0, BodyFixture::Scavenger).unwrap();
            for (i, cell) in optimized.cells.iter_mut().enumerate() {
                cell.x = wrap(WIDTH - 3.0 + i as f64 * 3.0, WIDTH);
                cell.y = HEIGHT - 4.0;
            }
        }
        optimized.fixture_exudate(-1.0, -1.0, 150);
        for (i, food) in optimized.food.iter_mut().enumerate() {
            food.x = wrap(WIDTH - 2.0 + (i % 10) as f64, WIDTH);
            food.y = wrap(HEIGHT - 4.0 + (i % 3) as f64, HEIGHT);
            food.kind = OrganicKind::ALL[i % 3];
        }
        if scenario == 4 { optimized.environment.add_barrier(5.0, HEIGHT-4.0, 30.0, 3.0, PI*0.5, false); }
        let before = state(&optimized);
        let mut reader = Reader::new(&before);
        let mut reference = World::read_checkpoint(&mut reader, 1, crate::checkpoint::VERSION).unwrap();
        assert!(reader.finished());
        let mut caps: Vec<_> = optimized.cells.iter().map(|cell| cell.body.capacities()).collect();
        let outputs: Vec<_> = caps.iter().enumerate().map(|(i, _)| Outputs { feed: if i % 3 == 0 { 0.0 } else { 1.0 }, ..Outputs::default() }).collect();
        for cap in &mut caps {
            cap.capture = if scenario == 1 { 0.0 } else { 0.001 };
            if scenario == 2 { cap.gut_energy = 0.0; }
            if scenario == 3 { cap.gut_minerals = 0.0; }
        }
        if scenario == 5 {
            for world in [&mut optimized, &mut reference] {
                world.food[0].energy = 0.0;
                world.food[1].nutrients = 0.0;
            }
        }
        if scenario == 6 {
            for world in [&mut optimized, &mut reference] { world.cells[3].energy = 0.0; }
        }
        if scenario == 7 {
            for cap in &mut caps { cap.gut_energy = 0.25; }
        }
        for world in [&mut optimized, &mut reference] {
            world.workspace.phases.geometry.rebuild(&world.cells, false, false);
        }
        let mut optimized_work = std::mem::take(&mut optimized.workspace.phases);
        let mut reference_work = std::mem::take(&mut reference.workspace.phases);
        optimized.eat_in(&outputs, &caps, &[], &mut optimized_work);
        reference.eat_reference_in(&outputs, &caps, &mut reference_work);
        assert_eq!(state(&optimized), state(&reference), "capture scenario {scenario}");
    }
}

/// Isolated release-mode timings. Construction, checkpoint hashing and all UI
/// work are outside the timed loops. Repeat on identical builds and hardware.
#[test]
#[ignore = "manual physical-phase benchmark"]
fn benchmark_field_transport_and_cell_collisions() {
    use std::time::Instant;
    let fingerprint = |bytes: Vec<u8>| bytes.iter().fold(0xcbf29ce484222325u64, |h, b|
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3));
    for &collisions in &[false, true] {
        let mut world = World::empty(42);
        let env = &mut world.environment;
        assert!(env.set_size(1600.0, 1000.0));
        env.configure(0.5, 0.6, 8.0);
        env.add_source(2, 1050.0, 760.0, 0.5, 180.0);
        env.add_source(1, 420.0, 480.0, 0.65, 200.0);
        env.add_source(3, 410.0, 230.0, 0.5, 120.0);
        env.add_source(4, 1350.0, 780.0, 0.4, 140.0);
        assert!(env.add_barrier(12.0, 500.0, 180.0, 28.0, 0.4, true) > 0);
        assert!(env.add_barrier(800.0, 500.0, 230.0, 24.0, 1.1, false) > 0);
        assert!(env.add_barrier(1110.0, 150.0, 240.0, 30.0, 0.7, true) > 0);
        if collisions {
            world.try_seed_fixture(1, 1536, -1.0, -1.0, BodyFixture::Scavenger).unwrap();
        } else {
            env.set_count_chemistry(true);
        }
        let mut phases = PhaseWorkspace::default();
        let mut solve = |tick| {
            if collisions {
                world.resolve_collisions_in(&mut phases);
            } else {
                world.environment.step(f64::from(tick) * DT);
            }
        };
        for tick in 0..10 { solve(tick); }
        let start = Instant::now();
        for tick in 10..130 { solve(tick); }
        let ms = start.elapsed().as_secs_f64() * 1000.0 / 120.0;
        let name = if collisions { "collision" } else { "transport" };
        println!("{name} {ms:.5} ms/step checkpoint {:016x}", fingerprint(state(&world)));
    }
}

#[test]
fn bounded_buckets_cover_exact_radius_across_seams_and_bucket_edges() {
    let mut grid = Grid::new();
    grid.configure(700.0, 460.0);
    let points: Vec<_> = (0..=80).flat_map(|y| (0..=120).map(move |x|
        (x as f64 * 700.0 / 120.0, y as f64 * 460.0 / 80.0))).collect();
    for (i, &(x, y)) in points.iter().enumerate() { grid.insert(x, y, i); }
    for &(x, y) in &[(0.0, 0.0), (699.99, 459.99), (-1e-12, 2.0),
        (700.0 / 12.0, 460.0 / 8.0), (707.25, -2.0)] {
        for radius in [0.0, 0.9, 58.3333333333, 101.0, 230.0, 350.0, 750.0] {
            let mut found = vec![false; points.len()];
            for index in grid.nearby_bounds(x, y, radius) {
                assert!(!found[index], "duplicate wrapped bucket");
                found[index] = true;
            }
            for (i, &(px, py)) in points.iter().enumerate() {
                if delta(px - x, grid.width).hypot(delta(py - y, grid.height)) <= radius {
                    assert!(found[i], "missed point ({px},{py}) from ({x},{y}) at {radius}");
                }
            }
        }
    }
}
