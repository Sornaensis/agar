//! Stoichiometric reactions and inventories, independent of authored species names.
use super::*;

const IDLE: &str = "thrust=0;turn=0;photosynthesize=0;absorb=0;feed=0;divide=false;";
fn close(a: f64, b: f64) { assert!((a-b).abs() < 1e-8 + a.abs().max(b.abs()) * 1e-11, "{a:.14} != {b:.14}"); }
fn programs(source: &str) -> Vec<Genome> { vec![Genome::new(source.into(), Program::compile(source).unwrap(), 0, 0)] }
fn world(fixture: BodyFixture, oxygen: f64, carbon: f64) -> World {
    let mut world = World::empty(71);
    world.environment.set_initial_gases(oxygen, carbon);
    world.environment.configure(0.5, 1.0, 0.0);
    world.reset_empty(71);
world.mutation_rate = 0.0;
    world.try_seed_fixture(1, 1, 300.0, 300.0, fixture).unwrap();
    world.cells[0].x = 300.0; world.cells[0].y = 300.0;
    world.cells[0].vx = 0.0; world.cells[0].vy = 0.0;
    world
}
fn balances(world: &World) {
    close(world.carbon_total() + world.environment.carbon_output_total(), world.carbon_input_total());
    close(world.environment.oxygen_total() + world.oxygen_cell_total() + world.environment.oxygen_output_total(),
        world.environment.oxygen_input_total() + world.carbon_fixed_total - world.carbon_respired_total - world.decay_carbon_respired_total);
    close(world.nutrient_total() + world.environment.nutrient_removed_total(), world.nutrient_input_total());
    assert!(world.energy_total() <= world.energy_input_total() + world.photosynthesis_total + 1e-7);
}
fn import_to_gut(world: &mut World, kind: OrganicKind) {
    world.fixture_exudate(300.0, 300.0, 1);
    let food = world.food.pop().unwrap();
    world.cells[0].ingest(kind, food.energy, food.nutrients);
}

#[test]
fn fixation_requires_inorganic_carbon_and_light_and_does_not_make_usable_energy() {
    let mut w = world(BodyFixture::Producer, 1.0, 1.0);
    let caps = vec![w.cells[0].body.capacities()];
    let outputs = vec![Outputs { photosynthesize: 1.0, ..Outputs::default() }];
    w.photosynthesize(&outputs, &caps, &mut [1e9]);
    assert_eq!(w.carbon_fixed_total, 0.0, "light alone cannot create carbon");
    let patch = w.environment.patch_index(300.0, 300.0);
    w.cells[0].internal_inorganic_carbon = w.environment.take_inorganic_carbon(patch, 0.5);
    let energy = w.cells[0].energy;
    let carbon = w.cells[0].internal_inorganic_carbon;
    let organics = w.organic_cell_total() + w.organic_food_total();
    w.photosynthesize(&outputs, &caps, &mut [0.0]);
    let fixed = w.carbon_fixed_total;
    assert!(fixed > 0.0);
    close(w.cells[0].energy, energy);
    close(carbon - w.cells[0].internal_inorganic_carbon, fixed);
    close(w.cells[0].internal_oxygen, fixed);
    close(w.organic_cell_total() + w.organic_food_total() - organics, fixed);
    close(w.organic_food_total(), 0.0);
    w.environment.configure(0.5, 0.0, 0.0);
    w.photosynthesize(&outputs, &caps, &mut [1e9]);
    close(w.carbon_fixed_total, fixed);
    balances(&w);
}

#[test]
fn aerobic_respiration_is_limited_by_both_substrates_and_physical_capacity() {
    let mut w = world(BodyFixture::Producer, 1.0, 1.0);
    let mut caps = vec![w.cells[0].body.capacities()];
    w.respire_in(&caps);
    assert_eq!(w.carbon_respired_total, 0.0);
    let patch = w.environment.patch_index(300.0, 300.0);
    w.cells[0].internal_oxygen = w.environment.take_oxygen(patch, 0.0001);
    let before = (w.cells[0].energy, w.cells[0].organic_carbon);
    w.respire_in(&caps);
    let reacted = w.carbon_respired_total;
    close(reacted, 0.0001);
    close(w.cells[0].internal_oxygen, 0.0);
    close(w.cells[0].internal_inorganic_carbon, reacted);
    close(w.cells[0].energy-before.0, RESPIRATION_ENERGY_PER_CARBON*reacted);
    close(before.1-w.cells[0].organic_carbon, reacted);
    w.cells[0].internal_oxygen = w.environment.take_oxygen(patch, 0.5);
    caps[0].respiration = 0.0;
    w.respire_in(&caps);
    close(w.carbon_respired_total, reacted);
    let reserve = w.cells[0].organic_carbon;
    w.cells[0].organic_carbon = 0.0;
    w.emit_organic_batch(vec![OrganicEmission::new(OrganicKind::Exudate, 350.0, 350.0, reserve*CHEMICAL_ENERGY_PER_CARBON, 0.0)]);
    caps[0].respiration = 1000.0;
    w.respire_in(&caps);
    close(w.carbon_respired_total, reacted);
    balances(&w);
}

#[test]
fn a_phototroph_respires_in_darkness_and_oxygen_starvation_is_real() {
    let mut aerobic = world(BodyFixture::Producer, 1.0, 1.0);
    let mut anoxic = world(BodyFixture::Producer, 0.0, 1.0);
    aerobic.environment.configure(0.5, 0.0, 0.0);
    anoxic.environment.configure(0.5, 0.0, 0.0);
    let mut a = programs(IDLE); let mut b = a.clone();
    for _ in 0..60 { aerobic.step(&mut a); anoxic.step(&mut b); }
    assert!(aerobic.carbon_respired_total > 0.0);
    assert_eq!(anoxic.carbon_respired_total, 0.0);
    assert_eq!(aerobic.carbon_fixed_total, 0.0);
    assert!(aerobic.cells[0].energy > anoxic.cells[0].energy);
    balances(&aerobic); balances(&anoxic);
}

#[test]
fn aerobic_machinery_uses_actual_recipe_activity_without_a_hidden_core() {
    for activity in [0, 1] {
        let source = format!("assembly Breath: respire(oxygen) {{ Catalyst: 0.025; Wall: 0.015; activity: {activity}; }}");
        let (mut world, mut genomes) = super::mechanism_tests::equipped(&source);
        assert_eq!(world.cells[0].body.capacities().metabolism, 0.0);
        let before=world.cells[0].energy;
        world.step(&mut genomes);
        if activity == 0 { assert_eq!(world.carbon_respired_total, 0.0); }
        else {
            assert!(world.carbon_respired_total > 0.0);
            assert!(world.cells[0].energy > before);
            close(world.cells[0].respiration_rate*DT, world.carbon_respired_total);
        }
        balances(&world);
    }
}

#[test]
fn the_minimal_core_maintains_an_atp_working_pool_without_filling_all_storage() {
    let mut world=World::empty(41); 
    world.seed_cells(1,1,300.0,300.0);
    world.exchange_gases_in(&mut PhaseWorkspace::default());
    let caps=vec![world.cells[0].body.capacities()];
    let carbon=world.cells[0].organic_carbon;
    world.respire_in(&caps);
    close(world.cells[0].organic_carbon,carbon);
    assert_eq!(world.carbon_respired_total,0.0);
    world.cells[0].energy=caps[0].energy*0.25-0.001;
    world.respire_in(&caps);
    close(world.cells[0].energy,caps[0].energy*0.25);
    assert!(world.carbon_respired_total>0.0);
    balances(&world);
}

#[test]
fn digestion_transfers_organic_carbon_to_reserves_and_residue_without_energy_credit() {
    let mut w = world(BodyFixture::Scavenger, 0.0, 0.0);
    import_to_gut(&mut w, OrganicKind::Tissue);
    let initial = (w.cells[0].energy, w.cells[0].organic_carbon, w.cells[0].gut_energy);
    let caps = vec![w.cells[0].body.capacities()];
    w.digest(&[Outputs { feed: 1.0, digest: [1.0;3], ..Outputs::default() }], &caps, &mut [0.0]);
    let raw = initial.2 - w.cells[0].gut_energy;
    let stored = (w.cells[0].organic_carbon-initial.1)*CHEMICAL_ENERGY_PER_CARBON;
    assert!(raw > 0.0 && stored > 0.0);
    close(initial.0-w.cells[0].energy, raw*0.02);
    close(raw, stored + w.organic_food_total()*CHEMICAL_ENERGY_PER_CARBON);
    assert!(w.food.iter().all(|food| food.kind == OrganicKind::Waste));
    balances(&w);
}

#[test]
fn background_decay_stops_without_oxygen_and_accounts_for_each_reaction() {
    for oxygen in [0.0, 0.001] {
        let mut w = World::empty(7);
        w.environment.set_initial_gases(oxygen, 0.0); w.reset_empty(7);
w.fixture_exudate(300.0, 300.0, 5);
        let food = w.organic_food_total();
        for _ in 0..200 { w.decay_food_in(&mut PhaseWorkspace::default()); }
        close(food-w.organic_food_total(), w.decay_carbon_respired_total);
        if oxygen == 0.0 { assert_eq!(w.decay_carbon_respired_total, 0.0); }
        else { assert!(w.decay_carbon_respired_total > 0.0); }
        balances(&w);
    }
}

#[test]
fn crowded_membranes_share_a_finite_patch_without_cell_order_bias() {
    let mut w = World::empty(17); 
    w.try_seed_cells(1, 600, 300.0, 300.0).unwrap();
    for cell in &mut w.cells { cell.x=300.0; cell.y=300.0; }
    w.exchange_gases_in(&mut PhaseWorkspace::default());
    let expected = 1.0 / 600.0;
    for cell in &w.cells { close(cell.internal_oxygen, expected); close(cell.internal_inorganic_carbon, expected); }
    let patch = w.environment.patch_index(300.0, 300.0);
    close(w.environment.oxygen_patch(patch), 0.0);
    close(w.environment.inorganic_carbon_patch(patch), 0.0);
    w.cells.reverse(); w.exchange_gases_in(&mut PhaseWorkspace::default());
    balances(&w);
}

#[test]
fn partial_particle_capture_keeps_even_a_sub_micro_energy_carbon_tail() {
    let mut w = world(BodyFixture::Scavenger, 0.0, 0.0);
    w.fixture_exudate(300.0, 300.0, 1);
    w.food[0].x=300.0; w.food[0].y=300.0;
    let before = w.cells[0].gut_energy+w.food[0].energy;
    let mut caps = vec![w.cells[0].body.capacities()];
    caps[0].capture = (w.food[0].energy - 5e-7)/(1200.0*DT);
    w.eat(&[Outputs { feed:1.0, ..Outputs::default() }], &caps);
    assert_eq!(w.food.len(), 1, "a positive carbon remainder must persist");
    assert!(w.food[0].energy > 0.0 && w.food[0].energy < 1e-6);
    assert!((before-w.cells[0].gut_energy-w.food[0].energy).abs()<1e-12);
    balances(&w);
}

#[test]
fn carbon_and_oxygen_close_through_growth_death_resize_and_checkpoint_replay() {
    let mut engine = crate::Engine::new();
    let source = include_str!("../../../examples/phototroph.cell");
    assert_eq!(engine.compile(source), 1);
    engine.world.environment.configure(0.5, 0.8, 0.0);
engine.world.mutation_rate=0.0;
    engine.world.seed_cells(1, 2, 300.0, 300.0);
    for _ in 0..300 { engine.world.step(&mut engine.genomes); balances(&engine.world); }
    assert!(engine.world.carbon_fixed_total > 0.0);
    assert!(engine.world.carbon_respired_total > 0.0);
    engine.world.cells[0].integrity=0.0; engine.world.step(&mut engine.genomes);
    assert_eq!(engine.world.deaths, 1); balances(&engine.world);
    assert!(engine.world.set_world_size(1600.0, 1000.0)); balances(&engine.world);
    let saved=crate::checkpoint::save(&engine).unwrap();
    let mut copy=crate::checkpoint::restore(&saved).unwrap();
    assert_eq!(crate::checkpoint::save(&copy).unwrap(), saved);
    for _ in 0..120 {
        engine.world.step(&mut engine.genomes); copy.world.step(&mut copy.genomes);
        balances(&engine.world); balances(&copy.world);
    }
    assert_eq!(crate::checkpoint::save(&engine).unwrap(), crate::checkpoint::save(&copy).unwrap());
}

#[test]
fn legacy_migration_classifies_old_carbon_once_and_preserves_it_on_resave() {
    let bytes=include_bytes!("../../fixtures/execution-plan-v4.bin");
    let old=crate::checkpoint::restore(bytes).unwrap();
    assert!(old.world.legacy_carbon_imports>0.0);
    assert!(old.world.cells.iter().all(|cell| cell.organic_carbon == 0.0 && cell.internal_oxygen == 0.0 && cell.internal_inorganic_carbon == 0.0));
    balances(&old.world);
    let saved=crate::checkpoint::save(&old).unwrap();
    let new=crate::checkpoint::restore(&saved).unwrap();
    assert_eq!(new.world.legacy_carbon_imports, old.world.legacy_carbon_imports);
    assert_eq!(crate::checkpoint::save(&new).unwrap(), saved);
}

#[test]
fn translation_in_a_living_founder_requires_both_free_mineral_and_organic_carbon() {
    let mut genomes = programs("cellular; product Blank=scaffold(); gene make: Blank=1;");
    for (with_carbon, with_mineral) in [(false, true), (true, false), (true, true)] {
        let mut w = World::empty(41);
        w.environment.set_initial_gases(0.0, 0.0);
        w.environment.set_initial_nutrients(0.0);
        w.environment.configure(0.5, 0.0, 0.0);
        w.reset_empty(41);
        w.place_cell(1, 200.0, 200.0, &genomes).unwrap();
        let c = &mut w.cells[0];
        if !with_carbon {
            // Transfer the founder's real carbon to unavailable dissolved inorganic C.
            w.environment.release_inorganic_carbon(c.x, c.y, std::mem::take(&mut c.organic_carbon));
        }
        if !with_mineral {
            w.environment.recycle_nutrients(c.x, c.y, std::mem::take(&mut c.nutrient_stored));
        }
        let initial = (c.organic_carbon, c.nutrient_stored);
        for _ in 0..6 { w.step(&mut genomes); }
        let c = &w.cells[0];
        assert!(c.energy > 0.0);
        let pool = &c.body.cellular.as_ref().unwrap().pools[0];
        assert_eq!(pool.expression_requested, 1.0);
        assert_eq!(pool.mass > 0.0, with_carbon && with_mineral);
        close(initial.0 - c.organic_carbon, pool.mass);
        close(initial.1 - c.nutrient_stored, pool.mass);
        balances(&w);
    }
}
