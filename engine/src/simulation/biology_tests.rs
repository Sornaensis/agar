//! System invariants across physical bodies, world actions and genome execution.
use super::*;
use crate::body::{Organelle, EMBODIED_FRACTION};
use crate::genome::Value;

const IDLE: &str = "thrust=0; turn=0; photosynthesize=0; absorb=0; feed=0; divide=false;";
const DIVIDE: &str = "thrust=0; turn=0; photosynthesize=0; absorb=0; feed=0; divide=true;";

fn registry(source: &str) -> Vec<Genome> {
    vec![Genome::new(
        source.to_owned(),
        Program::compile(source).unwrap(),
        0,
        0,
    )]
}
fn add_program(registry: &mut Vec<Genome>, source: &str) -> u32 {
    registry.push(Genome::new(
        source.to_owned(),
        Program::compile(source).unwrap(),
        0,
        0,
    ));
    registry.len() as u32
}
fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-7,
        "{actual:.12} != {expected:.12}"
    );
}
fn world(recipe: BodyFixture) -> World {
    let mut world = World::empty(73);

    world.mutation_rate = 0.0;
    world.try_seed_fixture(1, 1, 300.0, 300.0, recipe).unwrap();
    let cell = &mut world.cells[0];
    cell.x = 300.0;
    cell.y = 300.0;
    cell.angle = 0.0;
    cell.vx = 0.0;
    cell.vy = 0.0;
    world
}
fn advance(world: &mut World, registry: &mut Vec<Genome>, ticks: usize) {
    for _ in 0..ticks {
        world.try_step(registry).unwrap();
    }
}
/// Test maturation goes through paid frame construction, without editing area,
/// body ledgers, material inventory or founder machinery.
fn mature(cell: &mut Cell) {
    let target = cell.birth_area * 2.0;
    for _ in 0..1000 {
        if cell.body.area + 1e-10 >= target {
            break;
        }
        let mass = cell.body.structural_mass();
        cell.body.work(
            1.0,
            &BodyControls::default(),
            0.0,
            &mut cell.energy,
            &mut cell.nutrient_stored,
            1.0,
            target,
        );
        cell.organic_carbon -= cell.body.structural_mass() - mass;
    }
    close(cell.body.area, target);
    cell.cooldown = 0.0;
    assert!(cell.division_ready());
}
/// A partial fixture organ is fabricated from the same paid ledgers. This lets a
/// one-tick activation test start immediately below its functional threshold.
fn partial_organ(cell: &mut Cell, kind: Kind, mass: f64) {
    let energy = mass * kind.construction_cost();
    assert!(cell.energy > energy && cell.nutrient_stored > mass);
    cell.energy -= energy;
    cell.nutrient_stored -= mass;
    cell.organic_carbon -= mass;
    cell.body.organs.push(Organelle {
        id: cell.body.next_organelle_id,
        kind,
        mass,
        integrity: 1.0,
        embodied_energy: energy * EMBODIED_FRACTION,
    });
    cell.body.next_organelle_id += 1;
    cell.body.validate().unwrap();
}
fn caps(world: &World) -> Vec<BodyCapacities> {
    world.cells.iter().map(|c| c.body.capacities()).collect()
}

#[test]
fn preassembled_test_fixture_is_independent_of_the_program_and_every_introduction_is_charged() {
    let demanding = "thrust=0; turn=0; divide=false; target.flagellum.count=1000; target.flagellum.size=10; build.flagellum=1; target.piercer.count=10; build.piercer=1;";
    let mut programs = registry(IDLE);
    add_program(&mut programs, demanding);
    let mut a = world(BodyFixture::Bare);
    let mut b = world(BodyFixture::Bare);
    b.cells[0].genome_id = 2;
    assert_eq!(a.cells[0].body, b.cells[0].body);
    close(a.nutrient_total(), a.nutrient_input_total());
    close(b.nutrient_total(), b.nutrient_input_total());
    assert_eq!(b.cells[0].body.functional_mass(Kind::Flagellum), 0.0);
    let initial_material = b.nutrient_total();
    let initial_energy = b.energy_total();
    a.step(&mut programs);
    b.step(&mut programs);
    assert_eq!(
        b.cells[0].body.functional_mass(Kind::Flagellum),
        0.0,
        "incomplete growth supplies no thrust"
    );
    assert!(b.cells[0].body.precursor_mass[Kind::Flagellum as usize] > 0.0);
    assert_eq!(
        b.cells[0].body.mass(Kind::Flagellum),
        0.0,
        "translation precedes assembly"
    );
    assert!(b.energy_total() < initial_energy);
    close(initial_material, b.nutrient_total());
    assert_eq!(a.cells[0].body.count(Kind::Flagellum), 0);
}

#[test]
fn production_seeding_gives_every_program_the_same_minimal_founder() {
    let mut programs = registry(IDLE);
    let producer = add_program(&mut programs, "thrust=0;turn=0;divide=false;photosynthesize=1;target.photosynthetic.count=2;build.photosynthetic=1;");
    let hunter = add_program(
        &mut programs,
        "thrust=1;turn=1;divide=true;bite=1;engulf=true;target.piercer.count=2;build.piercer=1;",
    );
    let mut world = World::empty(82);
    let (expected_body, budget) = Body::founder();
    for id in [1, producer, hunter] {
        world.try_seed_cells(id, 2, -1.0, -1.0).unwrap();
    }
    assert_eq!(world.cells.len(), 6);
    for cell in &world.cells {
        assert_eq!(cell.body, expected_body);
        close(cell.energy, budget.energy);
        close(cell.nutrient_stored, budget.minerals);
        close(cell.gut_energy, 0.0);
        assert!(!cell.division_ready());
    }
    close(world.nutrient_total(), world.nutrient_input_total());
    close(world.energy_input_total(), 6.0 * (budget.introduced_energy + budget.energy
        + expected_body.structural_mass() * CHEMICAL_ENERGY_PER_CARBON));
    world.reset(82, hunter);
    assert!(world.cells.iter().all(|cell| cell.body == expected_body));
}

#[test]
fn idle_and_action_only_genomes_cannot_gain_specialization_without_construction() {
    let mut programs = registry(IDLE);
    let actions = add_program(&mut programs, "thrust=1;turn=1;divide=true;grow=1;photosynthesize=1;absorb=1;feed=1;bite=1;engulf=true;repair=1;");
    let mut world = World::empty(64);

    world.mutation_rate = 0.0;
    world.environment.configure(0.5, 1.0, 0.0);
    world.try_seed_cells(1, 1, 300.0, 300.0).unwrap();
    world.try_seed_cells(actions, 1, 300.0, 300.0).unwrap();
    world.fixture_exudate(300.0, 300.0, 64);
    let (body, budget) = Body::founder();
    advance(&mut world, &mut programs, 300);
    assert_eq!(world.cells.len(), 2);
    assert_eq!(world.births, 0);
    assert_eq!(world.photosynthesis_total, 0.0);
    assert_eq!(world.attacks, 0);
    for cell in &world.cells {
        assert_eq!(cell.body, body);
        assert!(cell.energy + cell.organic_carbon * CHEMICAL_ENERGY_PER_CARBON < 2.0 * budget.energy);
        close(cell.nutrient_stored, budget.minerals);
        close(cell.gut_energy, 0.0);
    }
}

#[test]
fn universal_newborn_builds_feeding_organs_and_a_paid_conservative_daughter() {
    let source = "let digestion = organelle.digestive.capacity>=1; \
        let feeding = digestion && organelle.phagocyte.capacity>=1; \
        let processing = organelle.metabolic.capacity>=1; \
        let portions = if processing then 2 else 1; \
        thrust=0;turn=0;photosynthesize=0;absorb=0;feed=1; \
        target.digestive.count=portions; target.digestive.size=.04; \
        express.digestive=if organelle.digestive.mass<.04*portions && protein.digestive<.008 then 1 else 0; \
        target.phagocyte.count=portions; target.phagocyte.size=.025; \
        express.phagocyte=if digestion && organelle.phagocyte.mass<.025*portions && protein.phagocyte<.008 then 1 else 0; \
        target.metabolic.count=if feeding then 2 else 0; target.metabolic.size=.03; \
        express.metabolic=if feeding && organelle.metabolic.mass<.06 && protein.metabolic<.008 then 1 else 0; \
        target.core.count=2; target.core.size=.04; \
        express.core=if processing && energy>50 && organelle.core.mass<.08 && protein.core<.008 then 1 else 0; \
        grow=if body.maturity<1 && energy>90 then .8 else 0; \
        divide=body.division_ready && energy>120;";
    let mut fed = World::empty(73);
    let mut unfed = World::empty(73);
    for world in [&mut fed, &mut unfed] {

        world.mutation_rate = 0.0;
        // This heavily supplied culture isolates food-limited development.
        // Aeration is explicit; sealed/anoxic limitations are tested separately.
        world.environment.configure_gas_exchange(true, 0.5, 1.0, 1.0);
        world.try_seed_cells(1, 1, 300.0, 300.0).unwrap();
    }
    let x = fed.cells[0].x;
    let y = fed.cells[0].y;
    let parent_id = fed.cells[0].id;
    let initial_energy = fed.cells[0].energy;
    let original_core = fed.cells[0].body.organs[0].clone();
    assert_eq!(fed.cells[0].body.organs.len(), 1);
    assert_eq!(fed.cells[0].body.capacities().capture, 0.0);
    assert_eq!(fed.cells[0].gut_capacity(), 0.0);
    fed.fixture_exudate(x, y, 256);
    let mut programs = registry(source);
    let mut control_programs = programs.clone();
    let mut constructed = false;
    let mut digested = false;
    let mut received_energy = false;
    for tick in 0..5400 {
        if tick > 0 && tick % 60 == 0 {
            let cell = &fed.cells[0];
            fed.fixture_exudate(cell.x, cell.y, 5);
        }
        fed.step(&mut programs);
        unfed.step(&mut control_programs);
        constructed |= fed.cells.iter().any(|cell| cell.construction_rate > 0.0);
        digested |= fed.cells.iter().any(|cell| cell.digestion_rate > 0.0);
        received_energy |= fed.cells.iter().any(|cell| cell.energy > initial_energy);
        close(fed.nutrient_total(), fed.nutrient_input_total());
        close(unfed.nutrient_total(), unfed.nutrient_input_total());
        assert!(fed.energy_total() <= fed.energy_input_total() + 1e-8);
        assert_eq!(fed.photosynthesis_total, 0.0);
        assert_eq!(unfed.births, 0);
        if fed.births > 0 {
            break;
        }
        assert!(
            !fed.cells.is_empty(),
            "bootstrap failed before feeding machinery could sustain the cell"
        );
    }
    assert!(constructed && digested && received_energy);
    assert_eq!(
        fed.births, 1,
        "a minimal cell must physically construct a complete consumer lifecycle"
    );
    assert_eq!(fed.cells.len(), 2);
    assert!(fed
        .cells
        .iter()
        .any(|cell| cell.parent_id == parent_id && cell.generation == 1));
    let inherited: Vec<_> = fed
        .cells
        .iter()
        .flat_map(|cell| &cell.body.organs)
        .collect();
    let mut identifiers: Vec<_> = inherited.iter().map(|organ| organ.id).collect();
    identifiers.sort_unstable();
    identifiers.dedup();
    assert_eq!(
        identifiers.len(),
        inherited.len(),
        "division cloned physical organelle IDs"
    );
    assert_eq!(
        inherited
            .iter()
            .filter(|organ| **organ == &original_core)
            .count(),
        1
    );
    assert!(fed.cells.iter().all(|cell| cell.body.viable()
        && cell.body.count(Kind::Core) == 1
        && cell.gut_capacity() > 0.0));
}

#[test]
fn completing_a_flagellum_pays_now_and_changes_movement_only_next_tick() {
    let mut world = world(BodyFixture::Bare);
    partial_organ(
        &mut world.cells[0],
        Kind::Flagellum,
        Kind::Flagellum.min_mass() - 0.0005,
    );
    let source = "target.flagellum.count=1; target.flagellum.size=.025; build.flagellum=1; thrust=1; turn=0; divide=false; absorb=0; feed=0;";
    let mut programs = registry(source);
    let before = world.energy_total();
    let material = world.nutrient_total();
    world.step(&mut programs);
    close(world.cells[0].vx, 0.0);
    close(world.cells[0].vy, 0.0);
    assert_eq!(world.cells[0].body.functional_mass(Kind::Flagellum), 0.0);
    assert!(world.cells[0].synthesis_rate > 0.0);
    assert!(world.energy_total() < before);
    let mut completed = false;
    for _ in 0..30 {
        world.step(&mut programs);
        close(world.cells[0].vx, 0.0);
        if world.cells[0].body.functional_mass(Kind::Flagellum) > 0.0 {
            assert!(world.cells[0].construction_rate > 0.0);
            completed = true;
            break;
        }
    }
    assert!(completed);
    world.step(&mut programs);
    assert!(world.cells[0].vx > 0.0);
    close(material, world.nutrient_total());
}

#[test]
fn mixture_keeps_a_closed_mineral_ledger_through_growth_recycling_damage_and_death() {
    let source = "photosynthesize=.5; feed=1; absorb=1; thrust=.2; turn=sin(age); bite=.5; repair=1; \
        target.core.count=2; target.core.size=.04*body.area; build.core=1; \
        target.flagellum.count=2; target.flagellum.size=.03; build.flagellum=if age<3 then .3 else 0; \
        recycle.flagellum=if age>3 then 1 else 0; grow=.4; divide=body.division_ready;";
    let mut programs = registry(source);
    let mut world = World::empty(14);

    world.mutation_rate = 0.0;
    world.environment.configure(0.5, 0.8, 3.0);
    for recipe in BodyFixture::ALL {
        world.try_seed_fixture(1, 2, -1.0, -1.0, recipe).unwrap();
    }
    world.fixture_exudate(-1.0, -1.0, 40);
    mature(&mut world.cells[0]);
    world.cells[1].integrity = 0.0;
    for tick in 0..420 {
        if tick == 60 {
            world.environment.add_nutrients(100.0, 100.0, 3.0);
        }
        if tick == 120 {
            world.fixture_exudate(300.0, 300.0, 3);
        }
        if tick == 180 {
            world
                .try_seed_fixture(1, 1, 500.0, 500.0, BodyFixture::Scavenger)
                .unwrap();
        }
        world.step(&mut programs);
        close(world.nutrient_total(), world.nutrient_input_total());
        for cell in &world.cells {
            cell.body.validate().unwrap();
            assert!(cell.energy <= cell.energy_capacity() + 1e-8);
            assert!(cell.nutrient_stored <= cell.nutrient_capacity() + 1e-8);
            assert!(cell.gut_energy <= cell.gut_capacity() + 1e-8);
            assert!(cell.gut_nutrients <= cell.body.capacities().gut_minerals + 1e-8);
        }
    }
    assert!(world.births > 0 && world.deaths > 0);
}

#[test]
fn a_closed_dark_world_cannot_gain_energy_from_build_recycle_feed_or_repair() {
    let source = "thrust=0; turn=0; divide=false; target.flagellum.count=if age<2 then 2 else 0; target.flagellum.size=.035; \
        build.flagellum=1; recycle.flagellum=1; feed=1; absorb=1; grow=.2; repair=1; photosynthesize=0;";
    let mut programs = registry(source);
    let mut world = world(BodyFixture::Scavenger);
    world.cells[0].body.damage(0.1);
    world.fixture_exudate(300.0, 300.0, 8);
    let introduced = world.energy_input_total();
    let mut previous = world.energy_total();
    for _ in 0..360 {
        world.step(&mut programs);
        let next = world.energy_total();
        assert!(
            next <= previous + 1e-8,
            "dark closed energy rose from {previous} to {next}"
        );
        close(world.energy_input_total(), introduced);
        previous = next;
    }
    close(world.nutrient_total(), world.nutrient_input_total());
}

#[test]
fn broken_storage_spills_carbon_and_minerals_and_dissipates_unstored_usable_energy() {
    let mut world = world(BodyFixture::Scavenger);
    let cell = &mut world.cells[0];
    cell.energy = cell.energy_capacity();
    cell.nutrient_stored = cell.nutrient_capacity();
    cell.ingest(
        OrganicKind::Tissue,
        cell.gut_capacity(),
        cell.body.capacities().gut_minerals,
    );
    let energy = world.energy_total();
    let carbon = world.carbon_total();
    let usable = world.cells[0].energy;
    let minerals = world.nutrient_total();
    let structure = world.cells[0].body.structural_mass();
    for organ in &mut world.cells[0].body.organs {
        if matches!(organ.kind, Kind::Reserve | Kind::Digestive) {
            organ.integrity = 0.0;
        }
    }
    world.spill_excess_stores();
    close(world.energy_total() + usable - world.cells[0].energy, energy);
    close(world.carbon_total(), carbon);
    close(world.nutrient_total(), minerals);
    close(world.cells[0].body.structural_mass(), structure);
    close(world.cells[0].gut_energy, 0.0);
    close(world.cells[0].gut_nutrients, 0.0);
    assert!(!world.food.is_empty());
    assert!(world.cells[0].energy <= world.cells[0].energy_capacity());
}

#[test]
fn dismantling_occupied_compartments_returns_all_material_and_accounts_energy_loss() {
    let mut world = world(BodyFixture::Scavenger);
    let cell = &mut world.cells[0];
    cell.energy = cell.energy_capacity();
    cell.ingest(OrganicKind::Tissue, 40.0, 0.6);
    let energy = world.energy_total();
    let minerals = world.nutrient_total();
    let mut heat = 0.0;
    let mut request = BodyControls::default();
    request.recycle[Kind::Reserve as usize] = 1.0;
    request.recycle[Kind::Digestive as usize] = 1.0;
    for _ in 0..500 {
        let cell = &mut world.cells[0];
        let mass = cell.body.structural_mass();
        let report = cell.body.work(
            0.0,
            &request,
            0.0,
            &mut cell.energy,
            &mut cell.nutrient_stored,
            DT,
            MAX_AREA,
        );
        cell.organic_carbon += mass - cell.body.structural_mass();
        heat += report.dissipated_energy;
        let usable = cell.energy;
        world.spill_excess_stores();
        heat += usable - world.cells[0].energy;
    }
    assert_eq!(world.cells[0].body.count(Kind::Reserve), 0);
    assert_eq!(world.cells[0].body.count(Kind::Digestive), 0);
    close(world.nutrient_total(), minerals);
    close(world.energy_total() + heat, energy);
}

#[test]
fn division_preserves_whole_organ_identities_and_pays_the_fission_cost() {
    let mut world = world(BodyFixture::Producer);
    mature(&mut world.cells[0]);
    let mut programs = registry(DIVIDE);
    let original = world.cells[0].body.clone();
    let minerals = world.nutrient_total();
    let energy = world.energy_total();
    let maintenance = world.cells[0].basal_metabolism() * DT;
    world.step(&mut programs);
    assert_eq!(world.births, 1);
    assert_eq!(world.cells.len(), 2);
    close(world.nutrient_total(), minerals);
    close(
        energy - world.energy_total(),
        maintenance + DIVISION_COST * original.area
            + world.carbon_respired_total * (CHEMICAL_ENERGY_PER_CARBON - RESPIRATION_ENERGY_PER_CARBON),
    );
    let mut inherited: Vec<_> = world
        .cells
        .iter()
        .flat_map(|c| c.body.organs.iter().cloned())
        .collect();
    inherited.sort_by_key(|o| o.id);
    assert_eq!(inherited, original.organs);
    for cell in &world.cells {
        assert!(cell.body.viable());
        cell.body.validate().unwrap();
    }
}

#[test]
fn parent_memory_age_and_ancestry_are_retained_while_daughter_memory_starts_empty() {
    let mut world = world(BodyFixture::Bare);
    mature(&mut world.cells[0]);
    world.cells[0].age = 23.0;
    world.cells[0].parent_id = 47;
    world.cells[0].generation = 4;
    let mut programs = registry("thrust=0; turn=0; ri001=ri001+1; rb001=true; rs001=\"kept\"; divide=true; absorb=0; feed=0;");
    let parent_id = world.cells[0].id;
    world.step(&mut programs);
    assert_eq!(world.cells.len(), 2);
    let parent = world.cells.iter().find(|c| c.id == parent_id).unwrap();
    let daughter = world.cells.iter().find(|c| c.id != parent_id).unwrap();
    assert_eq!(parent.memory.value("ri001"), Some(&Value::Integer(1)));
    assert_eq!(parent.memory.populated_count(), 3);
    assert_eq!(daughter.memory.populated_count(), 0);
    close(parent.age, 23.0 + DT);
    close(daughter.age, 0.0);
    assert_eq!(parent.parent_id, 47);
    assert_eq!(parent.generation, 4);
    assert_eq!(daughter.parent_id, parent_id);
    assert_eq!(daughter.generation, 5);
}

#[test]
fn register_energy_cost_uses_previous_tick_occupancy_even_when_clearing() {
    let mut occupied = world(BodyFixture::Bare);
    let mut empty = world(BodyFixture::Bare);
    let mut a = registry(
        "thrust=0; turn=0; divide=false; ri001=0; clear ri001 when age>0; absorb=0; feed=0;",
    );
    let mut b = registry(IDLE);
    occupied.step(&mut a);
    empty.step(&mut b);
    close(occupied.cells[0].energy, empty.cells[0].energy);
    assert_eq!(occupied.cells[0].memory.populated_count(), 1);
    occupied.step(&mut a);
    empty.step(&mut b);
    close(
        empty.cells[0].energy - occupied.cells[0].energy,
        MEMORY_REGISTER_COST * DT,
    );
    assert_eq!(occupied.cells[0].memory.populated_count(), 0);
    occupied.step(&mut a);
    empty.step(&mut b);
    close(
        empty.cells[0].energy - occupied.cells[0].energy,
        MEMORY_REGISTER_COST * DT,
    );
}

#[test]
fn one_large_core_cannot_be_cloned_to_satisfy_division() {
    let mut world = world(BodyFixture::Bare);
    mature(&mut world.cells[0]);
    let body = &mut world.cells[0].body;
    let second = body
        .organs
        .iter()
        .position(|o| o.kind == Kind::Core && o.id != 1)
        .unwrap();
    let removed = body.organs.remove(second);
    let first = body
        .organs
        .iter_mut()
        .find(|o| o.kind == Kind::Core)
        .unwrap();
    first.mass += removed.mass;
    first.embodied_energy += removed.embodied_energy;
    assert!(body.viable());
    assert!(!world.cells[0].division_ready());
    world.step(&mut registry(DIVIDE));
    assert_eq!(world.births, 0);
    assert_eq!(world.cells.len(), 1);
}

#[test]
fn population_can_be_seeded_and_divide_above_the_old_384_ceiling() {
    let mut world = World::empty(29);

    world.mutation_rate = 0.0;
    assert_eq!(
        world
            .try_seed_fixture(1, 400, -1.0, -1.0, BodyFixture::Bare)
            .unwrap(),
        400
    );
    mature(&mut world.cells[0]);
    let minerals = world.nutrient_total();
    let parent = world.cells[0].id;
    world.step(&mut registry(DIVIDE));
    assert_eq!(world.cells.len(), 401);
    assert_eq!(world.births, 1);
    assert_eq!(world.cells.last().unwrap().parent_id, parent);
    let mut ids: Vec<_> = world.cells.iter().map(|c| c.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 401);
    close(world.nutrient_total(), minerals);
}

#[test]
fn paid_frame_can_exceed_area_four_and_large_bodies_collide_across_the_torus_seam() {
    let mut world = world(BodyFixture::Bare);
    world
        .try_seed_fixture(1, 1, 500.0, 300.0, BodyFixture::Bare)
        .unwrap();
    let mut request = BodyControls::default();
    request.target_counts[Kind::Core as usize] = 2;
    request.target_sizes[Kind::Core as usize] = 0.2;
    request.build[Kind::Core as usize] = 1.0;
    // Explicitly introduced test reserves fund the large physical specimens.
    for cell in &mut world.cells {
        cell.energy += 600.0;
        cell.nutrient_stored += 10.0;
        cell.organic_carbon += 10.0;
    }
    world.energy_imports += 1200.0 + 20.0 * CHEMICAL_ENERGY_PER_CARBON;
    world.carbon_imports += 20.0;
    let carbon = world.carbon_total();
    world.nutrient_imports += 20.0;
    let material = world.nutrient_total();
    let energy = world.energy_total();
    for cell in &mut world.cells {
        for _ in 0..1000 {
            let mass = cell.body.structural_mass();
            cell.body.work(
                1.0,
                &request,
                0.0,
                &mut cell.energy,
                &mut cell.nutrient_stored,
                1.0,
                8.0,
            );
            cell.move_construction_carbon(mass);
            if cell.body.area >= 8.0 - 1e-10 {
                break;
            }
        }
        close(cell.body.area, 8.0);
        cell.body.validate().unwrap();
        cell.y = 300.0;
    }
    close(world.carbon_total(), carbon);
    assert!(world.energy_total() < energy);
    close(world.nutrient_total(), material);
    world.cells[0].x = 5.0;
    world.cells[1].x = WIDTH - 20.0;
    let expected = world.cells[0].radius() + world.cells[1].radius();
    world.resolve_collisions();
    close(
        delta(world.cells[1].x - world.cells[0].x, WIDTH)
            .hypot(delta(world.cells[1].y - world.cells[0].y, HEIGHT)),
        expected,
    );
    assert!(world
        .cells
        .iter()
        .all(|c| (0.0..WIDTH).contains(&c.x) && (0.0..HEIGHT).contains(&c.y)));
}

#[test]
fn local_photon_supply_is_finite_and_shared_by_overlapping_producers() {
    let mut world = World::empty(2);

    world.environment.configure(0.5, 1.0, 0.0);
    world
        .try_seed_fixture(1, 120, 110.0, 110.0, BodyFixture::Producer)
        .unwrap();
    for cell in &mut world.cells {
        cell.x = 110.0;
        cell.y = 110.0;
        cell.energy = 50.0;
        cell.internal_inorganic_carbon = 1.0;
    }
    let capacities = caps(&world);
    let mut processing: Vec<_> = capacities.iter().map(|c| c.metabolism * DT).collect();
    let outputs = vec![
        Outputs {
            photosynthesize: 1.0,
            ..Outputs::default()
        };
        world.cells.len()
    ];
    let potential = processing.iter().sum::<f64>();
    let patch_supply: f64 = photon_footprint(110.0, 110.0, 1.0)
        .iter()
        .map(|(p, _)| PHOTO_HABITAT_POWER / PATCHES as f64 * world.environment.patch_light(*p) * DT)
        .sum();
    world.photosynthesize(&outputs, &capacities, &mut processing);
    assert!(world.photosynthesis_total > 0.0);
    assert!(world.photosynthesis_total <= patch_supply + 1e-9);
    assert!(world.photosynthesis_total < potential * 0.1);
    let first = world.cells[0].photosynthesis_rate;
    for cell in &world.cells {
        close(cell.photosynthesis_rate, first);
    }
}

#[test]
fn a_paid_producer_can_grow_and_reproduce_from_light_and_dissolved_minerals() {
    let source = "thrust=0; turn=0; photosynthesize=1; feed=0; absorb=1; target.core.count=2; target.core.size=.04; build.core=1; \
        target.photosynthetic.count=2; target.photosynthetic.size=.1; build.photosynthetic=1; \
        target.metabolic.count=2; target.metabolic.size=.03; build.metabolic=1; \
        target.transporter.count=2; target.transporter.size=.015; build.transporter=1; \
        grow=if body.maturity<1 && energy.fraction>.6 then .8 else 0; divide=body.division_ready;";
    let mut world = world(BodyFixture::Producer);
    world.environment.configure(0.5, 0.8, 0.0);
    world.environment.set_initial_nutrients(3.0);
    world.environment.reset_dynamic();
    let mut programs = registry(source);
    let material = world.nutrient_total();
    for _ in 0..8700 {
        world.step(&mut programs);
        if world.births > 0 {
            break;
        }
    }
    assert_eq!(
        world.births, 1,
        "a complete paid producer lifecycle must finish within the test interval"
    );
    assert_eq!(world.cells.len(), 2);
    assert!(world.time() < 290.0);
    assert!(world.photosynthesis_total > 0.0);
    assert!(world.cells.iter().all(|c| c.gut_energy == 0.0));
    close(world.nutrient_total(), material);
}

#[test]
fn capturing_and_digesting_are_separate_and_conversion_has_a_real_energy_loss() {
    let mut world = world(BodyFixture::Scavenger);
    world.cells[0].energy = 50.0;
    world.particle(OrganicKind::Exudate, 300.0, 300.0, 20.0, 0.4);
    let capacities = caps(&world);
    let outputs = [Outputs {
        feed: 1.0,
        ..Outputs::default()
    }];
    let material = world.nutrient_total();
    let energy = world.energy_total();
    world.eat(&outputs, &capacities);
    close(world.cells[0].energy, 50.0);
    assert!(world.cells[0].gut_energy > 0.0);
    close(world.energy_total(), energy);
    close(world.nutrient_total(), material);
    let before = world.cells[0].gut_energy;
    let carbon = world.cells[0].organic_carbon;
    let mut processing = [capacities[0].metabolism * DT];
    world.digest(&outputs, &capacities, &mut processing);
    let processed = before - world.cells[0].gut_energy;
    assert!(processed > 0.0 && processed <= capacities[0].digestion * DIGESTION_SPEED[0] * DT);
    close(
        world.cells[0].organic_carbon - carbon,
        processed * ASSIMILATION_EFFICIENCY[0] / CHEMICAL_ENERGY_PER_CARBON,
    );
    close(
        energy - world.energy_total(),
        processed * 0.02,
    );
    close(world.nutrient_total(), material);
}

#[test]
fn mineral_full_gut_blocks_capture_and_no_optical_sensor_is_needed_for_contact_feeding() {
    let mut world = world(BodyFixture::Scavenger);
    for organ in &mut world.cells[0].body.organs {
        if organ.kind == Kind::Optical {
            organ.integrity = 0.0;
        }
    }
    let limit = world.cells[0].body.capacities().gut_minerals;
    world.cells[0].ingest(OrganicKind::Tissue, 1.0, limit);
    world.particle(OrganicKind::Exudate, 300.0, 300.0, 12.0, 0.4);
    let capacities = caps(&world);
    let outputs = [Outputs {
        feed: 1.0,
        ..Outputs::default()
    }];
    world.eat(&outputs, &capacities);
    close(world.cells[0].gut_energy, 1.0);
    close(world.food[0].energy, 12.0);
    world.cells[0].scale_gut(0.0);
    world.eat(&outputs, &capacities);
    assert!(world.cells[0].gut_energy > 0.0);
    assert_eq!(capacities[0].optical_sensing, 0.0);
}

#[test]
fn reciprocal_bites_transfer_existing_reserves_without_creating_energy_or_material() {
    let mut world = world(BodyFixture::Hunter);
    world
        .try_seed_fixture(1, 1, 314.0, 300.0, BodyFixture::Hunter)
        .unwrap();
    for (index, cell) in world.cells.iter_mut().enumerate() {
        cell.x = 300.0 + index as f64 * 14.0;
        cell.y = 300.0;
        cell.angle = index as f64 * PI;
        cell.vx = 0.0;
        cell.vy = 0.0;
        cell.energy = 20.0;
        cell.nutrient_stored = 0.2;
    }
    let capacities = caps(&world);
    let outputs = vec![
        Outputs {
            bite: 1.0,
            feed: 1.0,
            ..Outputs::default()
        };
        2
    ];
    let energy = world.energy_total();
    let material = world.nutrient_total();
    world.resolve_predation(&outputs, &capacities);
    assert_eq!(world.attacks, 2);
    assert!(world
        .cells
        .iter()
        .all(|c| c.gut_energy > 0.0 && c.integrity < 1.0));
    close(world.energy_total(), energy);
    close(world.nutrient_total(), material);
    for cell in &world.cells {
        assert!(cell.body.damaged(Kind::Piercer) > 0.0);
    }
}

#[test]
fn engulfing_transfers_one_whole_body_once_and_pays_before_the_meal() {
    let mut world = world(BodyFixture::Scavenger);
    mature(&mut world.cells[0]);
    world
        .try_seed_fixture(2, 1, 320.0, 300.0, BodyFixture::Bare)
        .unwrap();
    let contact = world.cells[0].radius() + world.cells[1].radius();
    world.cells[0].x = 300.0;
    world.cells[0].y = 300.0;
    world.cells[0].angle = 0.0;
    world.cells[1].x = 300.0 + contact;
    world.cells[1].y = 300.0;
    world.cells[1].vx = 0.0;
    world.cells[1].vy = 0.0;
    world.cells[1].integrity = 0.2;
    world.cells[1].energy = 5.0;
    world.cells[1].organic_carbon = 0.25;
    world.cells[1].nutrient_stored = 0.0;
    let energy = world.energy_total();
    let material = world.nutrient_total();
    let predator_energy = world.cells[0].energy;
    let expected_cost = 8.0 * world.cells[1].body.area + world.cells[0].basal_metabolism() * DT;
    let mut programs = registry("thrust=0; turn=0; divide=false; engulf=true; feed=1; absorb=0;");
    add_program(&mut programs, IDLE);
    world.step(&mut programs);
    assert_eq!(world.cells.len(), 1);
    assert_eq!(world.predations, 1);
    assert_eq!(world.deaths, 1);
    assert!(world.cells[0].gut_energy > 10.0 && world.cells[0].engulf_cooldown > 0.0);
    close(predator_energy - world.cells[0].energy
        + world.cells[0].respiration_rate * DT * RESPIRATION_ENERGY_PER_CARBON, expected_cost);
    close(world.nutrient_total(), material);
    assert!(world.energy_total() < energy);
}

#[test]
fn death_returns_body_and_all_gut_substrates_beyond_the_former_particle_cap() {
    let mut world = world(BodyFixture::Scavenger);
    assert_eq!(world.fixture_exudate(-1.0, -1.0, 1300), 1300);
    world.cells[0].ingest(OrganicKind::Exudate, 5.0, 0.1);
    world.cells[0].ingest(OrganicKind::Tissue, 10.0, 0.3);
    world.cells[0].ingest(OrganicKind::Waste, 5.0, 0.1);
    world.cells[0].integrity = 0.0;
    let material = world.nutrient_total();
    let energy = world.energy_total();
    world.step(&mut registry(IDLE));
    assert!(world.cells.is_empty());
    assert_eq!(world.deaths, 1);
    assert!(world.food.len() > 1300);
    for k in 0..3 {
        assert!(world.food.iter().any(|f| f.kind == OrganicKind::Tissue && f.composition.fractions()[k] > 0.0));
    }
    close(world.nutrient_total(), material);
    assert!(world.energy_total() <= energy);
}

#[test]
fn a_healthy_producer_has_no_five_minute_lifetime_limit() {
    let mut world = world(BodyFixture::Producer);
    world.environment.configure(0.5, 1.0, 0.0);
    let mut programs =
        registry("thrust=0;turn=0;divide=false;grow=0;photosynthesize=1;feed=0;absorb=0;");
    let id = world.cells[0].id;
    let material = world.nutrient_total();
    advance(&mut world, &mut programs, 10830);
    assert_eq!(world.cells.len(), 1);
    assert_eq!(world.cells[0].id, id);
    assert!(world.cells[0].age > 360.0);
    assert!(world.cells[0].energy > 0.0 && world.cells[0].integrity == 1.0);
    assert_eq!(world.deaths, 0);
    close(world.nutrient_total(), material);
    assert_eq!(world.sense_cell(id).unwrap().lifespan, 0.0);
}

#[test]
fn resource_imports_count_actual_introductions_and_reset_replaces_old_history() {
    let mut world = World::empty(11);
    close(world.nutrient_total(), world.nutrient_input_total());
    for recipe in BodyFixture::ALL {
        world.try_seed_fixture(1, 2, -1.0, -1.0, recipe).unwrap();
    }
    assert_eq!(world.fixture_exudate(-1.0, -1.0, 2501), 2501);
    let before_extra = world.nutrient_input_total();
    assert!(world.try_seed_cells(1, usize::MAX, 100.0, 100.0).is_err());
    assert_eq!(world.fixture_exudate(100.0, 100.0, 5), 5);
    assert_eq!(world.food.len(), 2506);
    let total = before_extra + 5.0 * FIXTURE_MINERALS;
    close(world.nutrient_input_total(), total);
    close(world.nutrient_total(), total);
    world.environment.set_initial_nutrients(0.1);
    close(world.nutrient_total(), total);
    world.reset(11, 1);
    let (_, budget) = Body::founder();
    close(
        world.nutrient_input_total(),
        0.1 * PATCHES as f64 + 22.0 * budget.introduced_minerals,
    );
    close(world.nutrient_total(), world.nutrient_input_total());
}

#[test]
fn daughter_mutation_changes_only_the_program_not_birth_machinery_or_physics() {
    let mut original = registry(DIVIDE);
    let mut evolving = original.clone();
    let mut off = world(BodyFixture::Producer);
    let mut on = world(BodyFixture::Producer);
    mature(&mut off.cells[0]);
    mature(&mut on.cells[0]);
    on.mutation_rate = 1.0;
    off.step(&mut original);
    on.step(&mut evolving);
    assert_eq!(off.births, 1);
    assert_eq!(on.births, 1);
    assert_eq!(on.mutations, 1);
    assert_eq!(on.cells[0].genome_id, 1);
    assert_ne!(on.cells[1].genome_id, 1);
    let descendant = &evolving[(on.cells[1].genome_id - 1) as usize];
    assert_eq!(descendant.parent_id, 1);
    Program::compile(&descendant.source).unwrap();
    let mut a = Vec::new();
    let mut b = Vec::new();
    off.snapshot(&mut a);
    on.snapshot(&mut b);
    for index in 0..off.cells.len() {
        b[12 + index * CELL_STRIDE + 1] = a[12 + index * CELL_STRIDE + 1];
    }
    assert_eq!(a, b);
    assert_eq!(off.rng.0, on.rng.0);
    for (a, b) in off.cells.iter().zip(&on.cells) {
        assert_eq!(a.body, b.body);
    }
}

#[test]
fn mutation_replay_reuses_existing_lineage_even_at_registry_capacity() {
    let mut programs = registry(DIVIDE);
    let first = mutate_registered(&mut programs, 1, 57, MutationKind::Constant);
    assert!(first.skipped.is_none());
    let template = programs[0].clone();
    programs.resize(MAX_GENOMES, template);
    let replay = mutate_registered(&mut programs, 1, 57, MutationKind::Constant);
    assert!(replay.skipped.is_none());
    assert_eq!(replay.genome_id, first.genome_id);
    assert_eq!(programs.len(), MAX_GENOMES);
}

#[test]
fn registry_byte_budget_blocks_new_mutations_but_preserves_replay() {
    let mut programs = registry(DIVIDE);
    let first = mutate_registered(&mut programs, 1, 57, MutationKind::Constant);
    assert!(first.skipped.is_none());
    programs[0].bytes = MAX_GENOME_BYTES - programs[1].bytes;
    assert_eq!(registry_bytes(&programs), MAX_GENOME_BYTES);
    let replay = mutate_registered(&mut programs, 1, 57, MutationKind::Constant);
    assert!(replay.skipped.is_none());
    assert_eq!(replay.genome_id, first.genome_id);
    let denied = mutate_registered(&mut programs, 1, 58, MutationKind::Constant);
    assert_eq!(denied.genome_id, 1);
    assert!(denied.skipped.unwrap().contains("64 MiB"));
    assert_eq!(programs.len(), 2);
}

#[test]
fn full_genome_registry_keeps_reproduction_and_counts_skipped_mutations() {
    let template = registry(DIVIDE).remove(0);
    let mut programs = vec![template; MAX_GENOMES];
    let mut world = world(BodyFixture::Bare);
    mature(&mut world.cells[0]);
    world.mutation_rate = 1.0;
    let energy = world.energy_total();
    let area = world.cells[0].body.area;
    let upkeep = world.cells[0].basal_metabolism() * DT;
    world.step(&mut programs);
    assert_eq!(world.births, 1);
    assert_eq!(world.cells.len(), 2);
    assert_eq!(world.cells[1].genome_id, 1);
    assert_eq!(world.mutations, 0);
    assert_eq!(world.mutations_skipped, 1);
    assert_eq!(programs.len(), MAX_GENOMES);
    close(energy - world.energy_total(), upkeep + DIVISION_COST * area
        + world.carbon_respired_total * (CHEMICAL_ENERGY_PER_CARBON - RESPIRATION_ENERGY_PER_CARBON));
}

#[test]
fn deterministic_reset_replays_mutation_ids_bodies_and_resource_state() {
    let mut programs = registry(DIVIDE);
    let mut world = World::empty(81);

    world.mutation_rate = 1.0;
    let mut expected = Vec::new();
    let mut expected_registry = 0;
    for attempt in 0..2 {
        world.reset_empty(81);
        world
            .try_seed_fixture(1, 3, -1.0, -1.0, BodyFixture::Producer)
            .unwrap();
        for cell in &mut world.cells {
            mature(cell);
        }
        advance(&mut world, &mut programs, 30);
        assert_eq!(world.births, 3);
        assert!(world.mutations > 0);
        let mut snapshot = Vec::new();
        world.snapshot(&mut snapshot);
        if attempt == 0 {
            expected = snapshot;
            expected_registry = programs.len();
        } else {
            assert_eq!(snapshot, expected);
            assert_eq!(programs.len(), expected_registry);
        }
    }
}

#[test]
fn dark_scavenger_completes_a_meal_dependent_lifecycle_with_accounted_food() {
    // The paid starter already contains two discrete copies of its essential
    // feeding machinery. It must grow the frame, digest external meals, and
    // accumulate more usable energy than the founder budget before division.
    let source = "thrust=0; turn=0; photosynthesize=0; absorb=0; feed=1; \
        target.core.count=2; target.core.size=.04; build.core=1; \
        grow=if body.maturity<1 && energy.fraction>.65 then .8 else 0; \
        divide=body.division_ready && energy.fraction>.9;";
    let mut fed = world(BodyFixture::Scavenger);
    let mut unfed = world(BodyFixture::Scavenger);
    fed.environment.configure(0.5, 0.0, 0.0);
    unfed.environment.configure(0.5, 0.0, 0.0);
    fed.environment.configure_gas_exchange(true, 0.5, 1.0, 1.0);
    unfed.environment.configure_gas_exchange(true, 0.5, 1.0, 1.0);
    let mut fed_programs = registry(source);
    let mut unfed_programs = registry(source);
    let founder_energy = fed.cells[0].energy;
    let founder_id = fed.cells[0].id;
    let founder_inputs = fed.energy_input_total();
    assert!(founder_energy / fed.cells[0].energy_capacity() < 0.9);
    assert_eq!(fed.cells[0].body.functional_mass(Kind::Photosynthetic), 0.0);
    assert_eq!(fed.fixture_exudate(300.0, 300.0, 256), 256);
    let mut saw_ingestion = false;
    let mut saw_assimilation = false;
    let mut exceeded_founder_reserve = false;
    for tick in 0..3600 {
        if tick > 0 && tick % 60 == 0 {
            fed.fixture_exudate(300.0, 300.0, 5);
        }
        fed.step(&mut fed_programs);
        unfed.step(&mut unfed_programs);
        saw_ingestion |= fed.cells.iter().any(|cell| cell.gut_energy > 0.0);
        saw_assimilation |= fed.cells.iter().any(|cell| cell.digestion_rate > 0.0);
        exceeded_founder_reserve |= fed.cells.iter().any(|cell| cell.energy > founder_energy);
        close(fed.nutrient_total(), fed.nutrient_input_total());
        close(unfed.nutrient_total(), unfed.nutrient_input_total());
        assert!(fed.energy_total() <= fed.energy_input_total() + 1e-8);
        assert!(unfed.energy_total() <= unfed.energy_input_total() + 1e-8);
        assert_eq!(fed.photosynthesis_total, 0.0);
        assert_eq!(unfed.photosynthesis_total, 0.0);
        assert_eq!(
            unfed.births, 0,
            "the matched founder budget cannot meet the same division rule"
        );
        if fed.births > 0 {
            break;
        }
    }
    assert!(saw_ingestion && saw_assimilation && exceeded_founder_reserve);
    assert_eq!(
        fed.births, 1,
        "physical feeding must support growth and reproduction in darkness"
    );
    assert_eq!(fed.cells.len(), 2);
    assert!(fed
        .cells
        .iter()
        .any(|cell| cell.generation == 1 && cell.parent_id == founder_id));
    assert!(fed
        .cells
        .iter()
        .all(|cell| cell.body.viable() && cell.gut_capacity() > 0.0));
    assert!(fed.energy_input_total() > founder_inputs);
    close(unfed.energy_input_total(), founder_inputs);
    assert!(fed.time() < 300.0);
}

#[test]
fn digestive_substrates_have_distinct_costs_yields_and_conservative_residue() {
    let mut rates = Vec::new();
    for kind in OrganicKind::ALL {
        let mut world = world(BodyFixture::Scavenger);
        world.cells[0].energy = 50.0;
        world.cells[0].nutrient_stored = 0.0;
        world.cells[0].ingest(kind, 10.0, 0.3);
        let capacities = caps(&world);
        let outputs = [Outputs {
            feed: 1.0,
            digest: [1.0; 3],
            ..Outputs::default()
        }];
        let material = world.nutrient_total();
        let energy = world.energy_total();
        let carbon = world.cells[0].organic_carbon;
        let mut processing = [capacities[0].metabolism * DT];
        world.digest(&outputs, &capacities, &mut processing);
        let raw = 10.0 - world.cells[0].gut_energy;
        assert!(raw > 0.0);
        close(
            world.cells[0].organic_carbon - carbon,
            raw * ASSIMILATION_EFFICIENCY[kind as usize] / CHEMICAL_ENERGY_PER_CARBON,
        );
        close(world.nutrient_total(), material);
        close(
            energy - world.energy_total(),
            raw * 0.02,
        );
        assert!(!world.food.is_empty());
        close(world.food.iter().map(|f| f.energy).sum(), raw * (1.0 - ASSIMILATION_EFFICIENCY[kind as usize]));
        assert!(world.food.iter().all(|f| f.kind == OrganicKind::Waste));
        rates.push(raw);
    }
    assert!(rates[0] > rates[1] && rates[1] > rates[2]);
}

#[test]
fn digestive_preferences_share_capacity_and_can_leave_a_substrate_untouched() {
    let mut world = world(BodyFixture::Scavenger);
    world.cells[0].energy = 20.0;
    world.cells[0].nutrient_stored = 0.0;
    for kind in OrganicKind::ALL {
        world.cells[0].ingest(kind, 10.0, 0.1);
    }
    let capacities = caps(&world);
    let outputs = [Outputs {
        feed: 1.0,
        digest: [1.0, 0.5, 0.0],
        ..Outputs::default()
    }];
    let mut processing = [capacities[0].metabolism * DT];
    let budget = capacities[0].digestion * DT;
    world.digest(&outputs, &capacities, &mut processing);
    let spent_exudate = (10.0 - world.cells[0].gut[0].energy) / DIGESTION_SPEED[0];
    let spent_tissue = (10.0 - world.cells[0].gut[1].energy) / DIGESTION_SPEED[1];
    close(spent_exudate + spent_tissue, budget);
    close(spent_exudate, 2.0 * spent_tissue);
    close(world.cells[0].gut[2].energy, 10.0);
    close(
        world.cells[0].gut_energy,
        world.cells[0].gut.iter().map(|p| p.energy).sum(),
    );
}

#[test]
fn tissue_then_waste_reprocessing_cannot_regenerate_edible_energy_or_material() {
    let mut world = world(BodyFixture::Scavenger);
    world.cells[0].energy = 50.0;
    world.cells[0].nutrient_stored = 0.0;
    world.cells[0].ingest(OrganicKind::Tissue, 20.0, 0.4);
    let capacities = caps(&world);
    let outputs = [Outputs {
        feed: 1.0,
        digest: [1.0; 3],
        ..Outputs::default()
    }];
    let material = world.nutrient_total();
    let carbon = world.cells[0].organic_carbon;
    let mut last_energy = world.energy_total();
    for _ in 0..3000 {
        let mut processing = [capacities[0].metabolism * DT];
        world.digest(&outputs, &capacities, &mut processing);
        // Accounted transfer of all residue back to the gut isolates the most
        // favorable possible recycling loop, without adding new substrate.
        for food in std::mem::take(&mut world.food) {
            assert_eq!(food.kind, OrganicKind::Waste);
            world.cells[0].ingest(food.kind, food.energy, food.nutrients);
        }
        let energy = world.energy_total();
        assert!(energy <= last_energy + 1e-9);
        last_energy = energy;
        close(world.nutrient_total(), material);
        if world.cells[0].gut_energy <= 1e-10 {
            break;
        }
    }
    assert!(world.cells[0].gut_energy <= 1e-10);
    close(world.cells[0].organic_carbon - carbon, 20.0 / CHEMICAL_ENERGY_PER_CARBON);
    assert!(world.cells[0].energy < 50.0, "hydrolysis is paid work, not usable energy production");
    close(world.cells[0].nutrient_stored, 0.4);
}

#[test]
fn particle_capture_partition_and_overflow_preserve_substrate_identity() {
    let mut world = world(BodyFixture::Scavenger);
    mature(&mut world.cells[0]);
    for kind in OrganicKind::ALL {
        world.cells[0].ingest(
            kind,
            3.0 * (kind as u8 + 1) as f64,
            0.03 * (kind as u8 + 1) as f64,
        );
    }
    let before = world.cells[0].gut;
    world.step(&mut registry(DIVIDE));
    assert_eq!(world.cells.len(), 2);
    for kind in OrganicKind::ALL {
        let k = kind as usize;
        close(
            world.cells.iter().map(|c| c.gut[k].energy).sum(),
            before[k].energy,
        );
        close(
            world.cells.iter().map(|c| c.gut[k].nutrients).sum(),
            before[k].nutrients,
        );
    }
    for cell in &mut world.cells {
        for organ in &mut cell.body.organs {
            if organ.kind == Kind::Digestive {
                organ.integrity = 0.0;
            }
        }
    }
    world.spill_excess_stores();
    for kind in OrganicKind::ALL {
        let k = kind as usize;
        close(
            world
                .food
                .iter()
                .filter(|f| f.kind == OrganicKind::Waste)
                .map(|f| f.energy * f.composition.fractions()[k])
                .sum(),
            before[k].energy,
        );
        close(
            world
                .food
                .iter()
                .filter(|f| f.kind == OrganicKind::Waste)
                .map(|f| f.nutrients * f.composition.fractions()[k])
                .sum(),
            before[k].nutrients,
        );
    }
    assert!(world
        .cells
        .iter()
        .all(|c| c.gut_energy == 0.0 && c.gut_nutrients == 0.0));
}

#[test]
fn local_merging_never_blends_tissue_waste_and_exudate() {
    let mut world = World::empty(9);
    let emissions: Vec<_> = OrganicKind::ALL
        .into_iter()
        .flat_map(|kind| (0..4).map(move |_| OrganicEmission::new(kind, 200.0, 200.0, 8.0, 0.1)))
        .collect();
    world.emit_organic_batch(emissions);
    assert_eq!(world.food.len(), 6);
    for kind in OrganicKind::ALL {
        close(
            world
                .food
                .iter()
                .filter(|f| f.kind == kind)
                .map(|f| f.energy)
                .sum(),
            32.0,
        );
        close(
            world
                .food
                .iter()
                .filter(|f| f.kind == kind)
                .map(|f| f.nutrients)
                .sum(),
            0.4,
        );
    }
}

#[test]
fn moving_growing_emitters_accumulate_packets_instead_of_spawning_microscopic_debris() {
    let mut world = World::empty(11);
    let mut emitted_energy = [0.0; 3];
    let mut emitted_minerals = [0.0; 3];
    // Cross spatial buckets and the torus seam while continuously growing.
    // Before the displacement fix this made 18,000 microscopic particles.
    for tick in 0..6000 {
        let x = wrap(1100.0 + tick as f64 * 0.3, WIDTH);
        let y = wrap(700.0 + tick as f64 * 0.1, HEIGHT);
        let radius = 7.0 + tick as f64 * 0.001;
        let mut emissions = Vec::new();
        for kind in OrganicKind::ALL {
            let k = kind as usize;
            let energy = 0.005 + k as f64 * 0.001;
            let minerals = 0.0001 + k as f64 * 0.00001;
            let mut emission = OrganicEmission::new(kind, x + radius + 2.5, y, energy, minerals);
            emission.outside = Some((x, y, radius));
            emissions.push(emission);
            emitted_energy[k] += energy;
            emitted_minerals[k] += minerals;
        }
        world.emit_organic_batch(emissions);
        for kind in OrganicKind::ALL {
            // The newest not-yet-full packet is the one still receiving this
            // cell's material. Full packets remain where they were released.
            let active = world
                .food
                .iter()
                .rev()
                .find(|food| food.kind == kind)
                .unwrap();
            assert!(
                delta(active.x - x, WIDTH).hypot(delta(active.y - y, HEIGHT)) + 1e-8
                    >= radius + 2.5
            );
        }
    }
    assert_eq!(
        world.food.len(),
        6,
        "Only two physically filled packets per substrate are needed"
    );
    for kind in OrganicKind::ALL {
        let k = kind as usize;
        let packets: Vec<_> = world.food.iter().filter(|f| f.kind == kind).collect();
        assert_eq!(packets.len(), 2);
        assert!(packets
            .iter()
            .all(|food| food.energy <= ORGANIC_PACKET_ENERGY + 1e-10));
        close(
            packets.iter().map(|food| food.energy).sum(),
            emitted_energy[k],
        );
        close(
            packets.iter().map(|food| food.nutrients).sum(),
            emitted_minerals[k],
        );
    }
}

#[test]
fn a_large_excretion_batch_stays_outside_the_emitter_across_the_torus_seam() {
    let mut world = World::empty(29);
    let mut emission = OrganicEmission::new(OrganicKind::Waste, 1210.0, 759.0, 200.0, 4.0);
    emission.outside = Some((1199.0, 759.0, 8.5));
    world.emit_organic_batch(vec![emission]);
    assert_eq!(world.food.len(), 9);
    close(world.food.iter().map(|f| f.energy).sum(), 200.0);
    close(world.food.iter().map(|f| f.nutrients).sum(), 4.0);
    for food in &world.food {
        assert!(delta(food.x - 1199.0, WIDTH).hypot(delta(food.y - 759.0, HEIGHT)) + 1e-8 >= 11.0);
        assert_eq!(food.kind, OrganicKind::Waste);
    }
}
