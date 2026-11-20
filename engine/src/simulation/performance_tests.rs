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
