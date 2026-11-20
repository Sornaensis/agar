//! A complete heterotrophic meal must cross the actual world phases. A weapon
//! alone supplies neither a gut nor enzymes, and contact damage is not food.
use super::*;

fn hunter_source(pierce: bool, capture: bool, digest: bool) -> String {
    let source = r#"cellular;
product Mount = anchor();
product Wall = membrane();
product Catalyst = enzyme();
product Rotor = motor();
product Tail = filament();
let need_gut = built.Gut + 0.000001 < 0.06;
let need_capture = built.Collector + 0.000001 < 0.03;
let need_digest = built.Hydrolase + 0.000001 < 0.04;
let need_exudate = built.ExudateEnzyme + 0.000001 < 0.02;
let need_weapon = built.Weapon + 0.000001 < 0.04;
let need_motor = built.Drive + 0.000001 < 0.025;
let need_respiration = built.Breath + 0.000001 < 0.04;
gene walls: Wall = if (need_gut || need_respiration) && pool.Wall < 0.001 then 1 else 0;
gene mounts: Mount = if (need_capture || need_weapon || need_motor) && pool.Mount < 0.001 then 1 else 0;
gene enzymes: Catalyst = if (need_digest || need_exudate || need_weapon || need_respiration) && pool.Catalyst < 0.001 then 1 else 0;
gene rotors: Rotor = if (need_capture || need_motor) && pool.Rotor < 0.001 then 1 else 0;
gene filaments: Tail = if (need_weapon || need_motor) && pool.Tail < 0.001 then 1 else 0;
assembly Gut: store(food) { Wall: 0.06; start_when: need_gut; }
assembly Breath: respire(oxygen) { Catalyst: 0.025; Wall: 0.015; start_when: need_respiration; activity: if carbon.organic > 0.2 || energy < 20 then 1 else 0; }
assembly Collector: import(particle) {
  Mount: 0.01; Rotor: 0.02; start_when: need_capture && CAPTURE;
}
assembly Hydrolase: digest(tissue) {
  Catalyst: 0.04; start_when: need_digest && DIGEST;
}
assembly ExudateEnzyme: digest(exudate) {
  Catalyst: 0.02; start_when: need_exudate && DIGEST;
}
assembly Weapon: pierce() {
  Mount: 0.01; Tail: 0.02; Catalyst: 0.01;
  start_when: need_weapon && PIERCE; at: 0; direction: 0;
}
assembly Drive: propel() {
  Mount: 0.005; Rotor: 0.01; Tail: 0.01;
  start_when: need_motor; activity: if gut.fraction < 0.6 then 0.12 else 0;
  at: 0; direction: 0;
}
"#;
    source.replace("CAPTURE", if capture { "true" } else { "false" })
        .replace("DIGEST", if digest { "true" } else { "false" })
        .replace("PIERCE", if pierce { "true" } else { "false" })
}

fn grown_hunter(pierce: bool, capture: bool, digest: bool) -> (World, Vec<Genome>) {
    let source = hunter_source(pierce, capture, digest);
    let program = Program::compile(&source).unwrap();
    let mut genomes = vec![Genome::new(source, program, 0, 0)];
    let mut world = World::empty(42);
    world.environment.configure(0.5, 0.0, 0.0);
    world.reset_empty(42);

    world.set_mutation_rate(0.0);
    world.seed_cells(1, 1, 600.0, 380.0);
    let initial_energy = world.cells[0].energy;
    let initial_minerals = world.cells[0].nutrient_stored;
    let expected = 3 + usize::from(pierce) + usize::from(capture) + 2 * usize::from(digest);
    for _ in 0..600 {
        world.step(&mut genomes);
        assert_eq!(world.cells.len(), 1, "The hunter must fund its own development");
        if world.cells[0].body.cellular.as_ref().is_some_and(|state|
            state.assemblies.iter().filter(|a| a.complete()).count() == expected) { break; }
    }
    let hunter = &mut world.cells[0];
    assert_eq!(hunter.body.cellular.as_ref().unwrap().assemblies.iter()
        .filter(|a| a.complete()).count(), expected);
    assert!(hunter.energy - world.carbon_respired_total * RESPIRATION_ENERGY_PER_CARBON < initial_energy
        && hunter.nutrient_stored < initial_minerals);
    assert_eq!(hunter.gut_energy, 0.0);
    assert_eq!(world.photosynthesis_total, 0.0);
    // Fix only initial encounter geometry; machinery was built and paid for
    // through the same expression/assembly phases as an authored organism.
    hunter.x = 600.0;
    hunter.y = 380.0;
    hunter.angle = 0.0;
    hunter.vx = 0.0;
    hunter.vy = 0.0;
    let idle = "thrust=0; turn=0; divide=false; feed=0;";
    genomes.push(Genome::new(idle.into(), Program::compile(idle).unwrap(), 0, 0));
    world.seed_cells(2, 1, 616.0, 380.0);
    world.cells[1].x = 616.0;
    world.cells[1].y = 380.0;
    world.cells[1].vx = 0.0;
    world.cells[1].vy = 0.0;
    (world, genomes)
}

#[test]
fn paid_piercing_kill_becomes_a_captured_and_digested_tissue_meal() {
    let (mut world, mut genomes) = grown_hunter(true, true, true);
    let initial_energy = world.cells[0].energy;
    let mut saw_nonlethal_damage = false;
    let mut saw_tissue = false;
    let mut saw_capture = false;
    let mut saw_digestion = false;
    let mut saw_tissue_digestion = false;
    let mut previous_tissue = 0.0;
    let mut best_energy_after_meal = 0.0_f64;
    for _ in 0..1200 {
        world.step(&mut genomes);
        let hunter = world.cells.iter().find(|c| c.id == 1).expect("Hunter survives its meal");
        if let Some(prey) = world.cells.iter().find(|c| c.id == 2) {
            if prey.integrity < 1.0 {
                saw_nonlethal_damage = true;
                assert_eq!(hunter.gut[OrganicKind::Tissue as usize].energy, 0.0,
                    "A primitive weapon cannot extract living tissue; damaged stores may leak exudate");
            }
        }
        saw_tissue |= world.food.iter().any(|f| f.kind == OrganicKind::Tissue);
        let tissue = hunter.gut[OrganicKind::Tissue as usize].energy;
        saw_capture |= tissue > 0.0;
        saw_digestion |= hunter.digestion_rate > 0.0;
        saw_tissue_digestion |= tissue < previous_tissue && hunter.digestion_rate > 0.0;
        previous_tissue = tissue;
        if saw_digestion { best_energy_after_meal = best_energy_after_meal.max(hunter.energy); }
        assert!((world.nutrient_total() - world.nutrient_input_total()).abs() < 1e-7);
        assert!(world.energy_total() <= world.energy_input_total() + 1e-7);
    }
    assert!(saw_nonlethal_damage && saw_tissue && saw_capture && saw_digestion && saw_tissue_digestion);
    assert_eq!(world.predations, 1);
    let mut census = Vec::new();
    world.ecology_snapshot(&genomes, &mut census);
    assert_eq!(census[2+8], 1.0, "the hunter received the kill");
    assert_eq!(&census[22+1..22+9], &[0.,1.,0.,1.,1.,0.,0.,0.], "the extinct prey retained its cause of death");
    assert!(world.attacks > 0);
    assert!(best_energy_after_meal > initial_energy,
        "The meal must repay hunting/upkeep energy: {best_energy_after_meal} <= {initial_energy}");
    assert_eq!(world.photosynthesis_total, 0.0);
}

#[test]
fn predation_requires_each_separate_mechanism() {
    for (pierce, capture, digest) in [(false, true, true), (true, false, true), (true, true, false)] {
        let (mut world, mut genomes) = grown_hunter(pierce, capture, digest);
        let mut captured = false;
        let mut digested = false;
        for _ in 0..900 {
            world.step(&mut genomes);
            if let Some(hunter) = world.cells.iter().find(|c| c.id == 1) {
                captured |= hunter.gut_energy > 0.0;
                digested |= hunter.digestion_rate > 0.0;
            }
            assert!((world.nutrient_total() - world.nutrient_input_total()).abs() < 1e-7);
            assert!(world.energy_total() <= world.energy_input_total() + 1e-7);
        }
        assert_eq!(world.predations, u64::from(pierce));
        assert_eq!(world.attacks > 0, pierce);
        assert_eq!(captured, pierce && capture);
        assert!(!digested, "A missing stage cannot borrow another mechanism's capability");
    }
}

#[test]
fn both_partitioned_daughters_capture_and_digest_accounted_tissue() {
    use crate::body::EMBODIED_FRACTION;
    use crate::cellular::{AssemblyInstance, Ingredient};
    let source = "assembly FoodTank: store(food) { Wall: 0.04; }\n\
        assembly Hydrolase: digest(tissue) { Catalyst: 0.02; }\n\
        assembly Collector: import(particle) { Mount: 0.01; Rotor: 0.01; }\n\
        assembly Respiration: respire(oxygen) { Catalyst: 0.02; Wall: 0.01; }";
    let (mut world, mut genomes) = super::mechanism_tests::equipped(source);
    // The helper paid for one copy of each recipe from founder reserves. Pay
    // separately for a second physical instance, with a fresh ID and ingredient
    // ledger. This isolates inheritance from the full-cycle WASM tests.
    let parent = &mut world.cells[0];
    let state = parent.body.cellular.as_mut().unwrap();
    let recipes: Vec<_> = state.assemblies.iter().map(|a| a.recipe.clone()).collect();
    for recipe in recipes {
        let ingredients = recipe.parts.iter().map(|part| {
            let paid = part.required_mass * (part.product.kind.synthesis_cost() + 30.0);
            parent.energy -= paid;
            parent.nutrient_stored -= part.required_mass;
            parent.organic_carbon -= part.required_mass;
            Ingredient { mass: part.required_mass, embodied_energy: paid * EMBODIED_FRACTION }
        }).collect();
        state.assemblies.push(AssemblyInstance { inherited_at: None, id: state.next_assembly_id,
            recipe, ingredients, integrity: 1.0, activity: 1.0 });
        state.next_assembly_id += 1;
    }
    assert!(parent.energy > DIVISION_COST * parent.body.area && parent.nutrient_stored > 0.0);
    parent.body.validate().unwrap();
    let parent = parent.clone();
    let mut original_ids: Vec<_> = parent.body.cellular.as_ref().unwrap().assemblies.iter().map(|a| a.id).collect();
    original_ids.sort_unstable();
    let (left, right) = parent.body.partition().expect("Two paid copies fit in the two daughters");
    assert!((left.structural_mass() + right.structural_mass() - parent.body.structural_mass()).abs() < 1e-9);
    assert!((left.embodied_energy() + right.embodied_energy() - parent.body.embodied_energy()).abs() < 1e-9);
    world.cells.clear();
    let mut inherited_ids = Vec::new();
    for (index, body) in [left, right].into_iter().enumerate() {
        let cap = body.capacities();
        assert!(cap.gut_energy > 0.0 && cap.capture > 0.0 && cap.substrate_digestion[1] > 0.0);
        assert_eq!(cap.metabolism, 0.0, "No legacy core supplies hidden digestive capacity");
        inherited_ids.extend(body.cellular.as_ref().unwrap().assemblies.iter().map(|a| a.id));
        let mut child = parent.clone();
        child.id = index as u32 + 2;
        child.parent_id = parent.id;
        child.generation += 1;
        child.birth_area = body.area;
        child.body = body;
        child.energy = (parent.energy - DIVISION_COST * parent.body.area) * 0.5;
        child.nutrient_stored *= 0.5;
        child.organic_carbon *= 0.5;
        child.internal_oxygen *= 0.5;
        child.internal_inorganic_carbon *= 0.5;
        child.scale_gut(0.5);
        child.x = 500.0 + index as f64 * 200.0;
        child.y = 380.0;
        child.vx = 0.0;
        child.vy = 0.0;
        child.cooldown = DIVISION_COOLDOWN;
        world.cells.push(child);
    }
    world.next_id = 4;
    inherited_ids.sort_unstable();
    assert_eq!(inherited_ids, original_ids, "Partition moves each paid physical record exactly once");
    let before = [world.cells[0].energy, world.cells[1].energy];
    for index in 0..2 {
        let (x, y) = world.cells[index].inlet_fixture_position();
        let start = world.food.len();
        assert_eq!(world.fixture_exudate(x, y, 2), 2);
        // Accounted external tissue, placed at each daughter's capture surface.
        // Only substrate and position change; imported mass and energy are kept.
        for food in &mut world.food[start..] { food.kind = OrganicKind::Tissue; food.composition = Composition::pool(1); food.x = x; food.y = y; }
    }
    let mut digested = [false; 2];
    for _ in 0..180 {
        world.step(&mut genomes);
        assert_eq!(world.cells.len(), 2);
        for (index, child) in world.cells.iter().enumerate() {
            digested[index] |= child.digestion_rate > 0.0;
            assert_eq!(child.gut[OrganicKind::Exudate as usize].energy, 0.0);
        }
        assert!((world.nutrient_total() - world.nutrient_input_total()).abs() < 1e-7);
        assert!(world.energy_total() <= world.energy_input_total() + 1e-7);
    }
    assert_eq!(digested, [true, true]);
    for (index, child) in world.cells.iter().enumerate() {
        assert!(child.energy > before[index], "Each daughter gains usable energy from its own captured tissue");
    }
    assert_eq!(world.photosynthesis_total, 0.0);
}

#[test]
fn daughter_that_dies_during_division_returns_all_its_material() {
    use crate::body::EMBODIED_FRACTION;
    let (mut world, mut genomes) = super::mechanism_tests::equipped(
        "assembly SpareStructure: support() { Frame: 0.04; }",
    );
    let parent = &mut world.cells[0];
    parent.birth_area = parent.body.area * 0.5;
    parent.cooldown = 0.0;
    let state = parent.body.cellular.as_mut().unwrap();
    // The paid fixture has two supports but one energy compartment. Damage
    // reduces the parent's storage requirement without deleting its material.
    state.assemblies.iter_mut().find(|a| a.recipe.name == "Battery").unwrap().integrity = 0.5;
    let replication_mass = 0.01 * parent.body.area;
    let replication_cost = 80.0 * replication_mass;
    parent.nutrient_stored -= replication_mass;
    parent.energy -= replication_cost;
    state.replica_mass = replication_mass;
    state.replica_energy = replication_cost * EMBODIED_FRACTION;
    state.constriction = 1.0;
    assert!(parent.division_ready());
    let (left, right) = parent.body.partition().unwrap();
    assert!(left.energy_capacity() > 0.0);
    assert_eq!(right.energy_capacity(), 0.0,
        "A physically supported daughter can inherit no functioning energy compartment");
    let material = world.nutrient_total();
    assert!((material - world.nutrient_input_total()).abs() < 1e-8);
    world.step(&mut genomes);
    assert_eq!(world.births, 1);
    assert_eq!(world.deaths, 1);
    assert_eq!(world.cells.len(), 1);
    assert!((world.nutrient_total() - material).abs() < 1e-8,
        "An immediately dead daughter must become tissue before it is removed: delta {}",
        world.nutrient_total() - material);
    assert!(world.food.iter().any(|food| food.kind == OrganicKind::Tissue));
    assert!(world.energy_total() <= world.energy_input_total() + 1e-8);
}

pub(super) fn paid_fission_fixture(storage_integrity: f64) -> (World, Vec<Genome>) {
    use crate::body::EMBODIED_FRACTION;
    let (mut world, genomes) = super::mechanism_tests::equipped(
        "assembly SpareStructure: support() { Frame: 0.04; }\n\
         assembly SpareBattery: store(energy) { Wall: 0.04; }",
    );
    let cell = &mut world.cells[0];
    cell.birth_area = cell.body.area * 0.5;
    cell.cooldown = 0.0;
    let state = cell.body.cellular.as_mut().unwrap();
    for assembly in &mut state.assemblies {
        if assembly.recipe.kind == AssemblyKind::Mechanism(MechanismKind::EnergyStorage) {
            // Alter capacity, preserving each separately paid material record.
            assembly.integrity = storage_integrity;
        }
    }
    // Prime a paid, completed cell cycle. The unchanged full-cycle WASM tests
    // cover source-controlled replication/constriction; this fixture isolates
    // the final fission affordability check from the path to maturity.
    let replica_mass = 0.01 * cell.body.area;
    let replica_cost = 80.0 * replica_mass;
    cell.nutrient_stored -= replica_mass;
    cell.energy -= replica_cost + 8.0 * cell.body.area;
    state.replica_mass = replica_mass;
    state.replica_energy = replica_cost * EMBODIED_FRACTION;
    state.constriction = 1.0;
    let charge = DIVISION_COST * cell.body.area + 24.0;
    assert!(cell.energy > charge && cell.nutrient_stored > 0.0);
    cell.energy = charge;
    assert!(cell.body.can_partition());
    (world, genomes)
}

#[test]
fn cellular_fission_pays_actual_cost_independently_of_empty_storage_capacity() {
    let mut capacities = Vec::new();
    for storage_integrity in [0.25, 1.0] {
        let (mut world, mut genomes) = paid_fission_fixture(storage_integrity);
        let cell = &world.cells[0];
        capacities.push(cell.energy_capacity());
        assert!(cell.division_ready(), "Empty storage must not increase the fission price");
        let before = cell.energy;
        let cost = DIVISION_COST * cell.body.area;
        let maintenance = cell.basal_metabolism() * DT;
        let material = world.nutrient_total();
        world.step(&mut genomes);
        assert_eq!(world.births, 1);
        assert_eq!(world.cells.len(), 2);
        assert!((before - world.cells.iter().map(|c| c.energy).sum::<f64>()
            - maintenance - cost).abs() < 1e-9);
        assert!((world.nutrient_total() - material).abs() < 1e-8);
    }
    assert!(capacities[1] > 3.9 * capacities[0]);
    for fraction in [0.5, 1.0] {
        let (mut world, mut genomes) = paid_fission_fixture(1.0);
        world.cells[0].energy = DIVISION_COST * world.cells[0].body.area * fraction;
        assert!(!world.cells[0].division_ready());
        world.step(&mut genomes);
        assert_eq!(world.births, 0, "Fission cannot spend unavailable usable energy");
    }
    let mut legacy = World::empty(1);
    legacy.try_seed_fixture(1, 1, 600.0, 380.0, BodyFixture::Bare).unwrap();
    let cell = &mut legacy.cells[0];
    cell.birth_area = cell.body.area * 0.5;
    cell.cooldown = 0.0;
    assert!(cell.body.cellular.is_none() && cell.body.can_partition());
    cell.energy = DIVISION_COST * cell.body.area + 0.2 * cell.energy_capacity();
    assert!(!cell.division_ready(), "Legacy programs retain their existing reserve rule");
    cell.energy = DIVISION_COST * cell.body.area + 0.36 * cell.energy_capacity();
    assert!(cell.division_ready());
}

#[test]
fn cellular_envelope_repair_spends_available_energy_without_an_empty_storage_reserve() {
    for extra_storage in [false, true] {
        let source = format!("assembly Mender: repair() {{ Catalyst: 0.025; }}\n{}",
            if extra_storage { "assembly EmptyBattery: store(energy) { Wall: 0.04; }" } else { "" });
        let (mut world, mut genomes) = super::mechanism_tests::equipped(&source);
        let cell = &mut world.cells[0];
        cell.integrity = 0.5;
        assert!(cell.energy > 16.0);
        cell.energy = 16.0;
        let before = cell.energy;
        let maintenance = cell.basal_metabolism() * DT;
        let repaired = cell.body.capacities().repair * DT;
        let cost = repaired * 20.0 * cell.body.area;
        let material = world.nutrient_total();
        world.step(&mut genomes);
        assert_eq!(world.cells.len(), 1);
        let cell = &world.cells[0];
        assert!((cell.integrity - 0.5 - repaired).abs() < 1e-10,
            "An affordable cellular repair must not depend on unused storage");
        assert!((before - cell.energy - maintenance - cost).abs() < 1e-9);
        assert!((world.nutrient_total() - material).abs() < 1e-8);
    }
    let mut world = World::empty(1);
    world.try_seed_fixture(1, 1, 600.0, 380.0, BodyFixture::Bare).unwrap();
    let source = "thrust=0;turn=0;photosynthesize=0;absorb=0;feed=0;grow=0;divide=false;repair=1;";
    let mut genomes = vec![Genome::new(source.into(), Program::compile(source).unwrap(), 0, 0)];
    world.cells[0].integrity = 0.5;
    world.cells[0].energy = 0.1 * world.cells[0].energy_capacity();
    world.step(&mut genomes);
    assert_eq!(world.cells[0].integrity, 0.5, "Legacy repair retains its existing reserve rule");
}

#[test]
fn fresh_digestive_waste_clears_capture_and_remains_edible_after_actual_approach() {
    let source = r#"
product Tail = filament();
assembly FoodTank: store(food) { Wall: 0.04; }
assembly Hydrolase: digest(tissue) { Catalyst: 0.04; }
assembly Collector: import(particle) { Mount: 0.01; Rotor: 0.01; }
assembly Rear: import(particle) { Mount: 0.01; Rotor: 0.01; at:3.14159;
    activity:if age > 1 && gut.structural.energy < 0.001 then 1 else 0; }
assembly Reverse: propel() {
  Mount: 0.005; Rotor: 0.01; Tail: 0.01;
  activity: if age > 1 && gut.tissue.energy < 0.001 then 0.2 else 0;
  at: 0; direction: 3.141592653589793;
}
"#;
    for (x, y, angle) in [(600.0, 380.0, 0.0), (1.0, 1.0, PI * 0.25),
        (WIDTH - 1.0, HEIGHT - 1.0, PI * 1.25)] {
        let (mut world, mut genomes) = super::mechanism_tests::equipped(source);
        let cell = &mut world.cells[0];
        cell.x = x; cell.y = y; cell.angle = angle;
        let (fx,fy) = cell.inlet_fixture_position();
        assert_eq!(world.fixture_exudate(x, y, 1), 1);
        let food = world.food.last_mut().unwrap();
        food.kind = OrganicKind::Tissue; food.composition = Composition::pool(1); food.x = x; food.y = y;
        food.x = fx; food.y = fy;
        let mut waste_id = None;
        let mut secreted = false;
        let mut captured_after_approach = false;
        for _ in 0..450 {
            world.step(&mut genomes);
            assert_eq!(world.cells.len(), 1);
            let cell = &world.cells[0];
            let moved = delta(cell.x - x, WIDTH).hypot(delta(cell.y - y, HEIGHT));
            let waste: Vec<_> = world.food.iter().filter(|f| f.kind == OrganicKind::Waste).collect();
            if moved < 1e-9 && !waste.is_empty() {
                secreted = true;
                assert_eq!(waste.len(), 1, "Repeated secretions should coalesce locally");
                let particle = waste[0];
                if let Some(id) = waste_id { assert_eq!(particle.id, id); }
                else { waste_id = Some(particle.id); }
                let distance = delta(particle.x - cell.x, WIDTH).hypot(delta(particle.y - cell.y, HEIGHT));
                assert!(distance > cell.radius() + 2.5,
                    "Fresh waste must be strictly beyond the inclusive capture boundary: {distance}");
                assert_eq!(cell.gut[OrganicKind::Waste as usize].energy, 0.0,
                    "A stationary cell must not immediately recapture its own secretion");
            }
            if cell.gut[OrganicKind::Waste as usize].energy > 0.0 {
                assert!(moved > 0.0);
                captured_after_approach = true;
            }
            assert!((world.nutrient_total() - world.nutrient_input_total()).abs() < 1e-7);
            assert!(world.energy_total() <= world.energy_input_total() + 1e-7);
            if captured_after_approach { break; }
        }
        assert!(secreted && captured_after_approach,
            "The same emitter can capture old waste after its paid motor physically approaches it");
    }
}

/// Furnish tissue at existing inlets without editing the cells' bodies or reserves.
fn feed_stock_culture(world: &mut World) {
    let portions: Vec<_> = world.cells.iter().filter(|cell| {
        let nearby: f64 = world.food.iter().filter(|food| food.kind == OrganicKind::Tissue
            && delta(food.x - cell.x, WIDTH).hypot(delta(food.y - cell.y, HEIGHT)) <= cell.radius() + 2.5)
            .map(|food| food.energy).sum();
        cell.gut[OrganicKind::Tissue as usize].energy < 30.0 && nearby < 12.0
    }).map(Cell::inlet_fixture_position).collect();
    for (x, y) in portions {
        assert_eq!(world.fixture_exudate(x, y, 1), 1);
        let food = world.food.last_mut().unwrap();
        food.kind = OrganicKind::Tissue;
        food.composition = Composition::pool(1);
        food.x = x;
        food.y = y;
    }
}

#[test]
fn shipped_predator_repairs_grows_and_divides_on_accounted_tissue() {
    let source = include_str!("../../../examples/predator.cell");
    let program = Program::compile(source).unwrap();
    let essential: Vec<_> = ["Gut", "Capture", "ExudateEnzyme", "TissueEnzyme",
        "LeftMotor", "RightMotor", "Vision", "Touch", "Piercer"]
        .into_iter().map(|name| {
            let definition = program.cellular().unwrap().assemblies.iter()
                .find(|a| a.name == name).unwrap();
            (name, definition.parts.iter().map(|(_, mass)| mass).sum::<f64>())
        }).collect();
    // Keep the original nine-machine trigger for the injury pulse, but require
    // both paid temperature receptors to remain functional after actual fission.
    let heat_recipes: Vec<_> = ["LeftHeat", "RightHeat"].into_iter().map(|name| {
        let (index, definition) = program.cellular().unwrap().assemblies.iter()
            .enumerate().find(|(_, a)| a.name == name).unwrap();
        (index, name, definition.parts.iter().map(|(_, mass)| mass).sum::<f64>())
    }).collect();
    let mut inherited = Vec::new();
    let mut mature = None;
    let mut genomes = vec![Genome::new(source.into(), program, 0, 0)];
    let mut world = World::empty(42);
    world.environment.configure(0.5, 0.0, 0.0);
    world.reset_empty(42);

    world.set_mutation_rate(0.0);
    world.seed_cells(1, 1, 600.0, 380.0);
    let mut heat_until = None;
    let mut damaged = false;
    let mut digested = false;
    for tick in 0..18000 {
        assert_eq!(world.cells.len(), 1, "The paid tissue culture must keep its founder alive");
        let cell = &world.cells[0];
        let ready = cell.body.cellular.as_ref().is_some_and(|state|
            essential.iter().all(|(name, mass)| state.assemblies.iter()
                .filter(|a| a.recipe.name == *name).map(|a| a.functional_mass()).sum::<f64>()
                + 1e-7 >= 0.9 * mass));
        if heat_until.is_none() && ready {
            assert!(cell.body.area < 1.1, "The packing regression begins before growth");
            world.environment.configure(1.0, 0.0, 0.0);
            heat_until = Some(tick + 30);
        } else if heat_until == Some(tick) {
            world.environment.configure(0.5, 0.0, 0.0);
        }
        // This is an explicitly fed culture, not the mixed-world scenario.
        // Food enters through the normal accounted import and capture paths;
        // no body, gene controls, or cell reserves are supplied after seeding.
        if tick % 15 == 0 { feed_stock_culture(&mut world); }
        // Capture the individual assemblies before a physically possible split.
        // One pole carries the frontal tools and the other carries the tail tools.
        if world.cells[0].body.area >= 1.99 * world.cells[0].birth_area {
            mature = Some(world.cells[0].clone());
            inherited = world.cells[0].body.cellular.as_ref().unwrap().assemblies.iter()
                .map(|assembly| (assembly.id, assembly.recipe.name.clone(), assembly.mass())).collect();
        }
        world.step(&mut genomes);
        damaged |= world.cells.iter().any(|c| c.integrity < 0.99);
        digested |= world.cells.iter().any(|c| c.digestion_rate > 0.0);
        assert!((world.nutrient_total() - world.nutrient_input_total()).abs() < 1e-7);
        assert!(world.energy_total() <= world.energy_input_total() + 1e-7);
        if world.births > 0 {
            eprintln!("Paid tissue culture divided at {} s", world.time());
            break;
        }
    }
    assert!(damaged && digested);
    assert_eq!(world.photosynthesis_total, 0.0);
    assert_eq!(world.births, 1, "The shipped Predator must grow and divide after paid thermal repair: {:?}",
        world.cells.iter().map(|c| (c.energy, c.gut.map(|pool| pool.energy), c.nutrient_stored, c.integrity,
            c.body.area, c.body.space(), c.body.cellular.as_ref().map(|s|
                (s.replication_progress(c.body.area), s.constriction,
                 s.assemblies.iter().map(|a| (a.recipe.name.as_str(), a.mass(), a.integrity)).collect::<Vec<_>>())))
        ).collect::<Vec<_>>());
    assert_eq!(world.cells.len(), 2);
    assert!(world.cells.iter().any(|cell| cell.body.cellular.as_ref().unwrap().assemblies
        .iter().any(|a| a.recipe.name == "Repair" && a.complete())));
    for cell in &world.cells {
        assert!(cell.integrity > 0.99);
        assert!(cell.body.space().used <= cell.body.space().capacity + 1e-8);
        let inputs = world.sense_program(cell.id, &genomes[0].program).unwrap();
        for (index, name, _) in &heat_recipes {
            let installed = cell.body.active_recipe_mass(genomes[0].program.cellular().unwrap(), *index) > 0.0;
            assert_eq!(inputs.receptors[*index].available, installed, "Inherited receptor {name}");
        }
    }
    super::contact_tests::mature_target_sweep(&mature.unwrap());
    assert!(!inherited.is_empty());
    for (id, name, before) in inherited {
        let mut owners = world.cells.iter().flat_map(|cell| &cell.body.cellular.as_ref().unwrap().assemblies)
            .filter(|assembly| assembly.id == id && assembly.recipe.name == name);
        let retained = owners.next().unwrap_or_else(|| panic!("Partition lost {name} #{id}"));
        assert!(retained.mass() + 1e-7 >= before, "Partition lost material from {name}");
        assert!(owners.next().is_none(), "Partition duplicated {name} #{id}");
    }
    for cell in &world.cells {
        let assemblies = &cell.body.cellular.as_ref().unwrap().assemblies;
        for alternatives in [["Capture", "TailInlet"], ["Piercer", "TailPiercer"], ["Touch", "TailTouch"]] {
            assert!(assemblies.iter().any(|assembly| alternatives.contains(&assembly.recipe.name.as_str())
                && assembly.working_mass() > 0.0), "Hungry daughter lacks {:?}", alternatives);
        }
    }
    let before = world.attacks;
    super::contact_tests::check_newborn_recognition(&mut world, &mut genomes, true);
    for _ in 1..30 { world.step(&mut genomes); }
    assert_eq!(world.attacks, before);
    assert_eq!(world.cells.len(), 2);

    // Separate the first pair after the immediate encounter, then continue the
    // paid tissue culture. This challenges a body whose original display
    // patches were already moved by a previous division.
    world.cells[0].x = 300.0;
    world.cells[1].x = 900.0;
    for tick in 0..27000 {
        if tick % 15 == 0 { feed_stock_culture(&mut world); }
        world.step(&mut genomes);
        if world.births > 1 { break; }
    }
    assert!(world.births > 1, "A fed descendant must undergo a subsequent cleavage in the assay");
    super::contact_tests::check_newborn_recognition(&mut world, &mut genomes, false);

}

#[test]
fn shipped_predator_pays_for_local_heat_sensing_and_escapes_while_fed() {
    let source = include_str!("../../../examples/predator.cell");
    let no_sensors = source.replace("start_when: need_LeftHeat;", "start_when: false;")
        .replace("start_when: need_RightHeat;", "start_when: false;");
    assert_ne!(source, no_sensors, "The control disables the actual heat recipes");

    // Establish a real source-driven gradient once and reuse that habitat for
    // independent encounters. Neither bodies nor reserves are copied between
    // cases; every founder builds its own machinery from its original budget.
    let mut habitat = World::empty(42);
    habitat.environment.configure(0.5, 0.85, 0.0);
    habitat.environment.set_initial_nutrients(3.0);
    habitat.environment.add_source(1, 700.0, 380.0, 1.0, 180.0);
    habitat.reset_empty(42);

    for _ in 0..3600 { habitat.step(&mut Vec::new()); }

    for (label, source, angle, senses_heat) in [
        ("ahead", source, 0.0, true),
        ("left", source, -PI * 0.5, true),
        ("right", source, PI * 0.5, true),
        ("behind", source, PI, true),
        ("no receptors", no_sensors.as_str(), 0.0, false),
    ] {
        let program = Program::compile(source).unwrap();
        let heat_indices: Vec<_> = program.cellular().unwrap().assemblies.iter()
            .enumerate().filter(|(_, a)| matches!(a.name.as_str(), "LeftHeat" | "RightHeat"))
            .map(|(index, _)| index).collect();
        assert_eq!(heat_indices.len(), 2);
        let mut genomes = vec![Genome::new(source.into(), program, 0, 0)];
        let mut world = World::empty(42);
        world.environment.configure(0.5, 0.85, 0.0);
        world.environment.set_initial_nutrients(3.0);
        world.reset_empty(42);
        world.environment = habitat.environment.clone();

        world.set_mutation_rate(0.0);
        assert!((world.nutrient_total() - world.nutrient_input_total()).abs() < 1e-7);
        world.seed_cells(1, 1, 100.0, 380.0);
        {
            let cell = &mut world.cells[0];
            cell.x = 100.0; cell.y = 380.0; cell.angle = 0.0;
            cell.vx = 0.0; cell.vy = 0.0;
        }
        let inputs = world.sense_program(1, &genomes[0].program).unwrap();
        assert!(heat_indices.iter().all(|&index| !inputs.receptors[index].available));
        let initial_energy = world.cells[0].energy;
        let essential = ["Gut", "Capture", "ExudateEnzyme", "TissueEnzyme",
            "LeftMotor", "RightMotor", "Vision", "Touch", "Piercer"];
        let mut ready = false;
        for _ in 0..2700 {
            world.step(&mut genomes);
            assert_eq!(world.cells.len(), 1, "{label}: startup must fund itself");
            let state = world.cells[0].body.cellular.as_ref().unwrap();
            let complete = |name: &str| state.assemblies.iter()
                .any(|a| a.recipe.name == name && a.complete() && a.functional_mass() > 0.0);
            ready = essential.iter().all(|name| complete(name))
                && (!senses_heat || (complete("LeftHeat") && complete("RightHeat")));
            if ready { break; }
        }
        assert!(ready, "{label}: the authored program must build its machinery");
        let cell = &world.cells[0];
        let heat: Vec<_> = cell.body.cellular.as_ref().unwrap().assemblies.iter()
            .filter(|a| matches!(a.recipe.name.as_str(), "LeftHeat" | "RightHeat"))
            .collect();
        let heat_mass: f64 = heat.iter().map(|a| a.mass()).sum();
        let heat_embodied: f64 = heat.iter().map(|a| a.embodied_energy()).sum();
        if senses_heat {
            assert!((heat_mass - 0.02).abs() < 1e-7);
            // Each .002 anchor + .008 receptor costs .66 to synthesize and
            // .30 to assemble. Each receiver retains its paid fabrication share.
            assert!((heat_embodied - 1.92 * crate::body::EMBODIED_FRACTION).abs() < 1e-7);
            assert!(cell.energy - world.carbon_respired_total * RESPIRATION_ENERGY_PER_CARBON < initial_energy - 1.92,
                "Respiration can replenish usable energy, but cannot erase paid sensor fabrication");
        } else {
            assert_eq!(heat_mass, 0.0);
            assert_eq!(heat_embodied, 0.0);
        }
        assert_eq!(cell.body.capacities().protection, 0.0);

        // A genuine captured tissue meal puts the normal motor policy into
        // its rest condition. Heat escape must override that condition.
        let (x, y) = cell.inlet_fixture_position();
        let first = world.food.len();
        assert_eq!(world.fixture_exudate(x, y, 2), 2);
        for food in &mut world.food[first..] {
            food.kind = OrganicKind::Tissue; food.composition = Composition::pool(1); food.x = x; food.y = y;
        }
        for _ in 0..120 {
            let (fx,fy) = world.cells[0].inlet_fixture_position();
            for food in &mut world.food {
                if food.kind == OrganicKind::Tissue { food.x = fx; food.y = fy; }
            }
            world.step(&mut genomes);
            assert_eq!(world.cells.len(), 1);
            if world.cells[0].gut[OrganicKind::Tissue as usize].energy > 30.0 { break; }
        }
        assert!(world.cells[0].gut_energy > 30.0);
        let x = (350..700).map(|x| x as f64).min_by(|a, b|
            (world.environment.temperature_at(*a, 380.0) - 0.83).abs()
                .total_cmp(&(world.environment.temperature_at(*b, 380.0) - 0.83).abs()))
            .unwrap();
        {
            let cell = &mut world.cells[0];
            cell.x = x; cell.y = 380.0; cell.angle = angle;
            cell.vx = 0.0; cell.vy = 0.0;
        }
        // A real visible meal lies across the thermal boundary. This gives the
        // heat-blind control a reason to swim inward once its gut empties.
        // All four equipped orientations encounter the same attraction.
        let hot_edge = (x as usize..700)
            .find(|&x| world.environment.temperature_at(x as f64, 380.0) >= 0.98).unwrap();
        let lure_x = hot_edge as f64 + CELL_RADIUS + 2.5;
        assert_eq!(world.fixture_exudate(lure_x, 380.0, 1), 1);
        let lure = world.food.last_mut().unwrap();
        (lure.x, lure.y) = (lure_x, 380.0);
        let start_temperature = world.environment.temperature_at(x, 380.0);
        let inputs = world.sense_program(1, &genomes[0].program).unwrap();
        assert_eq!(heat_indices.iter().all(|&index|
            inputs.receptors[index].available && inputs.receptors[index].valid), senses_heat);
        let mut maximum_temperature = start_temperature;
        let mut minimum_integrity = 1.0f64;
        for tick in 0..600 {
            world.step(&mut genomes);
            assert_eq!(world.cells.len(), 1, "{label}: the border encounter must remain viable");
            let cell = &world.cells[0];
            maximum_temperature = maximum_temperature.max(
                world.environment.temperature_at(cell.x, cell.y));
            if tick == 0 {
                let motor_activity: f64 = cell.body.cellular.as_ref().unwrap().assemblies.iter()
                    .filter(|a| matches!(a.recipe.name.as_str(), "LeftMotor" | "RightMotor"))
                    .map(|a| a.activity).sum();
                assert_eq!(motor_activity > 0.0, senses_heat,
                    "Only paid local receptors can trigger escape while fed");
            }
            minimum_integrity = minimum_integrity.min(cell.integrity);
            assert!((world.nutrient_total() - world.nutrient_input_total()).abs() < 1e-7);
            assert!(world.energy_total() <= world.energy_input_total() + 1e-7);
        }
        let cell = &world.cells[0];
        if senses_heat {
            // The forward-mounted motors turn a head-on swimmer through a wider
            // arc than a flank encounter. Track the whole excursion, including
            // any brief injury, as well as the final escape into cooler water.
            assert!(minimum_integrity > 0.98, "{label}: minimum integrity {minimum_integrity}");
            assert!(maximum_temperature < 0.90, "{label}: peak temperature {maximum_temperature}");
            assert!(world.environment.temperature_at(cell.x, cell.y) < start_temperature - 0.04);
        } else {
            assert!(maximum_temperature > 0.9 && cell.integrity < 0.8,
                "Without thermal receptors the hunter follows food into dangerous heat: peak {maximum_temperature}, integrity {}", cell.integrity);
        }
        assert_eq!(world.photosynthesis_total, 0.0);
    }
}

#[test]
fn shipped_predator_keeps_searching_when_waste_occupies_its_gut_in_darkness() {
    let source = include_str!("../../../examples/predator.cell");
    let program = Program::compile(source).unwrap();
    let index = |name: &str| program.cellular().unwrap().assemblies.iter()
        .position(|a| a.name == name).unwrap();
    let (vision, left_nose, right_nose, left_motor, right_motor) =
        (index("Vision"), index("LeftNose"), index("RightNose"), index("LeftMotor"), index("RightMotor"));
    let mut genomes = vec![Genome::new(source.into(), program, 0, 0)];
    let mut world = World::empty(42);
    world.environment.configure(0.5, 0.0, 0.0);

    world.set_mutation_rate(0.0);
    world.try_seed_cells_in_registry(1, 1, 600.0, 380.0, &genomes).unwrap();
    for _ in 0..1200 {
        world.step(&mut genomes);
        let observed = world.sense_program(1, &genomes[0].program).unwrap();
        if observed.receptors[left_nose].available && observed.receptors[right_nose].available { break; }
    }
    // Construct the specific digestive state using paid particles and the same
    // ingestion operation as capture. The separate hunting trials test pursuit
    // and physical capture. Replacing a particle's substrate preserves its stock.
    let (x, y) = (world.cells[0].x, world.cells[0].y);
    assert_eq!(world.fixture_exudate(x, y, 2), 2);
    for _ in 0..2 {
        let particle = world.food.pop().unwrap();
        world.cells[0].ingest(OrganicKind::Waste, particle.energy, particle.nutrients);
    }
    let cell = &world.cells[0];
    assert!(cell.gut[OrganicKind::Waste as usize].energy > 0.45 * cell.gut_capacity());
    assert_eq!(cell.gut[OrganicKind::Exudate as usize].energy + cell.gut[OrganicKind::Tissue as usize].energy, 0.0);
    // A single real plume from paid, decaying food lies on either flank when
    // the observer turns around. Both comparisons read the identical field.
    (world.cells[0].x, world.cells[0].y) = (600.0, 380.0);
    assert_eq!(world.fixture_exudate(600.0, 390.0, 8), 8);
    for particle in &mut world.food { (particle.x, particle.y) = (600.0, 390.0); }
    let mut workspace = PhaseWorkspace::default();
    for _ in 0..8 { world.decay_food_in(&mut workspace); }
    for (angle, turn) in [(0.0, 1.0), (PI, -1.0)] {
        world.cells[0].angle = angle;
        let input = world.sense_program(1, &genomes[0].program).unwrap();
        assert!(!input.receptors[vision].valid);
        assert!(input.receptors[left_nose].valid && input.receptors[right_nose].valid);
        assert!((input.receptors[left_nose].organic - input.receptors[right_nose].organic) * turn > 0.001);
        let mut memory = world.cells[0].memory.clone();
        let controls = genomes[0].program.evaluate_cell(&input, &mut memory, &mut EvalScratch::default()).cellular.unwrap();
        let left = controls.activity[left_motor];
        let right = controls.activity[right_motor];
        assert!(left + right > 1.0, "A dark, waste-fed hunter keeps seeking useful food");
        assert!((right - left) * turn > 0.1, "Its differential motors turn toward the plume");
    }
}

#[test]
fn whole_engulf_needs_room_for_retained_energy_and_preserves_prey_substrates() {
    let mut w = World::empty(9);
    w.try_seed_fixture(1, 1, 300.0, 300.0, BodyFixture::Scavenger).unwrap();
    w.try_seed_fixture(2, 1, 320.0, 300.0, BodyFixture::Bare).unwrap();
    let a = &mut w.cells[0];
    a.body.area = 2.0;
    (a.x, a.y, a.angle) = (300.0, 300.0, 0.0);
    let edge = a.x + a.radius() + w.cells[1].radius();
    let b = &mut w.cells[1];
    (b.x, b.y, b.integrity) = (edge, 300.0, 0.2);
    b.reserve_enrichment = 5.0;
    b.structural_enrichment = 3.0;
    b.energy = 7.0;
    b.organic_carbon = 0.5;
    b.ingest_enriched(OrganicKind::Waste, 12.0, 0.2, 4.0);
    let meal = b.food_potential();
    let tissue_extra = b.tissue_extra_energy();
    let total_mineral = b.mass();
    let cost = 8.0 * b.body.area;
    let start_atp = w.cells[0].energy;
    let mut caps: Vec<_> = w.cells.iter().map(|c| c.body.capacities()).collect();
    let outputs = [Outputs { engulf: true, feed: 1.0, ..Outputs::default() }, Outputs::default()];
    caps[0].gut_minerals = total_mineral + 1.0;
    caps[0].gut_energy = meal.total() - 0.1;
    assert_eq!(w.resolve_predation(&outputs, &caps), [false, false]);
    assert_eq!(w.cells[0].energy, start_atp);
    caps[0].gut_energy = meal.total() + 0.1;
    assert_eq!(w.resolve_predation(&outputs, &caps), [false, true]);
    let a = &w.cells[0];
    let close = |v: f64, expected: f64| assert!((v - expected).abs() < 1e-8, "{v} != {expected}");
    close(a.energy, start_atp - cost);
    close(a.gut_energy, meal.total());
    close(a.gut_nutrients, total_mineral);
    close(a.gut[0].extra_energy + a.gut[1].extra_energy, tissue_extra);
    close((a.gut[0].energy-a.gut[0].extra_energy)/20.0, 0.5);
    close(a.gut[OrganicKind::Waste as usize].energy, 12.0);
    close(a.gut[OrganicKind::Waste as usize].extra_energy, 4.0);
    close(w.cells[1].gut_energy, 0.0);
    close(w.cells[1].energy, 0.0);
}

#[test]
fn live_bites_only_take_a_proportional_share_of_enriched_reserves() {
    let mut w = World::empty(19);
    for recipe in [BodyFixture::Hunter, BodyFixture::Bare] {
        w.try_seed_fixture(1, 1, 300.0, 300.0, recipe).unwrap();
    }
    for (i, c) in w.cells.iter_mut().enumerate() {
        (c.x, c.y, c.angle) = (300.0 + 14.0 * i as f64, 300.0, 0.0);
    }
    let b = &mut w.cells[1];
    b.reserve_enrichment = 20.0;
    b.organic_carbon = 1.0;
    b.nutrient_stored = 0.5;
    b.structural_enrichment = 7.0;
    let atp = b.energy;
    let body = b.food_potential().body;
    let caps: Vec<_> = w.cells.iter().map(|c| c.body.capacities()).collect();
    let outputs = [Outputs { bite: 1.0, feed: 1.0, ..Outputs::default() }, Outputs::default()];
    w.resolve_predation(&outputs, &caps);
    let b = &w.cells[1];
    let meal = w.cells[0].gut[0];
    assert!(b.alive() && b.integrity < 1.0 && meal.energy > 0.0);
    assert_eq!(b.energy, atp);
    assert_eq!(b.food_potential().body, body);
    assert!((meal.extra_energy - meal.energy * 0.5).abs() < 1e-10);
    assert!((b.reserve_enrichment + meal.extra_energy - 20.0).abs() < 1e-10);
    assert!((b.organic_carbon + (meal.energy - meal.extra_energy) / 20.0 - 1.0).abs() < 1e-10);
    assert!((b.nutrient_stored + meal.nutrients - 0.5).abs() < 1e-10);
}

#[test]
fn partial_particle_capture_keeps_the_same_carbon_and_energy_density_in_both_shares() {
    let mut w = World::empty(72);
    w.try_seed_fixture(1, 1, 300.0, 300.0, BodyFixture::Scavenger).unwrap();
    let (x,y) = (w.cells[0].x,w.cells[0].y);
    assert!(w.particle(OrganicKind::Tissue,x,y,20.0,0.4));
    w.food[0].composition = Composition::pool(1);
    w.food[0].extra_energy = 12.0;
    let caps = [w.cells[0].body.capacities()];
    w.eat(&[Outputs {feed:1.0,..Outputs::default()}], &caps);
    let meal = w.cells[0].gut[OrganicKind::Tissue as usize];
    let rest = &w.food[0];
    assert!(meal.energy > 0.0 && rest.energy > 0.0);
    assert!((meal.energy + rest.energy - 20.0).abs() < 1e-10);
    assert!((meal.extra_energy / meal.energy - 0.6).abs() < 1e-10);
    assert!((rest.extra_energy / rest.energy - 0.6).abs() < 1e-10);
    assert!((meal.nutrients + rest.nutrients - 0.4).abs() < 1e-10);
}
