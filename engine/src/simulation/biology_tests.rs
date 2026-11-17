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
    world.food_rate = 0.0;
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
        cell.body.work(
            1.0,
            &BodyControls::default(),
            0.0,
            &mut cell.energy,
            &mut cell.nutrient_stored,
            1.0,
            target,
        );
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
    assert!(b.cells[0].body.mass(Kind::Flagellum) > 0.0);
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
    close(world.energy_input_total(), 6.0 * budget.introduced_energy);
    world.reset(82, hunter);
    assert!(world.cells.iter().all(|cell| cell.body == expected_body));
}

#[test]
fn idle_and_action_only_genomes_cannot_gain_specialization_without_construction() {
    let mut programs = registry(IDLE);
    let actions = add_program(&mut programs, "thrust=1;turn=1;divide=true;grow=1;photosynthesize=1;absorb=1;feed=1;bite=1;engulf=true;repair=1;");
    let mut world = World::empty(64);
    world.food_rate = 0.0;
    world.mutation_rate = 0.0;
    world.environment.configure(0.5, 1.0, 0.0);
    world.try_seed_cells(1, 1, 300.0, 300.0).unwrap();
    world.try_seed_cells(actions, 1, 300.0, 300.0).unwrap();
    world.add_food(300.0, 300.0, 64);
    let (body, budget) = Body::founder();
    advance(&mut world, &mut programs, 300);
    assert_eq!(world.cells.len(), 2);
    assert_eq!(world.births, 0);
    assert_eq!(world.photosynthesis_total, 0.0);
    assert_eq!(world.attacks, 0);
    for cell in &world.cells {
        assert_eq!(cell.body, body);
        assert!(cell.energy < budget.energy);
        close(cell.nutrient_stored, budget.minerals);
        close(cell.gut_energy, 0.0);
    }
}

#[test]
fn universal_newborn_builds_feeding_organs_and_a_paid_conservative_daughter() {
    let source = "let feeding = organelle.digestive.capacity>=1 && organelle.phagocyte.capacity>=1; \
        let processing = organelle.metabolic.capacity>=1; \
        thrust=0;turn=0;photosynthesize=0;absorb=0;feed=1; \
        target.digestive.count=if processing then 2 else 1; target.digestive.size=.04; build.digestive=1; \
        target.phagocyte.count=if processing then 2 else 1; target.phagocyte.size=.025; build.phagocyte=1; \
        target.metabolic.count=if feeding then 2 else 0; target.metabolic.size=.03; build.metabolic=1; \
        target.core.count=2; target.core.size=.04; build.core=if processing && energy>50 then 1 else 0; \
        grow=if body.maturity<1 && energy>90 then .8 else 0; \
        divide=body.division_ready && energy>120;";
    let mut fed = World::empty(73);
    let mut unfed = World::empty(73);
    for world in [&mut fed, &mut unfed] {
        world.food_rate = 0.0;
        world.mutation_rate = 0.0;
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
    fed.add_food(x, y, 256);
    let mut programs = registry(source);
    let mut control_programs = programs.clone();
    let mut constructed = false;
    let mut digested = false;
    let mut received_energy = false;
    for tick in 0..5400 {
        if tick > 0 && tick % 60 == 0 {
            let cell = &fed.cells[0];
            fed.add_food(cell.x, cell.y, 5);
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
    assert!(world.cells[0].body.functional_mass(Kind::Flagellum) > 0.0);
    assert!(world.cells[0].construction_rate > 0.0);
    assert!(world.energy_total() < before);
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
    world.food_rate = 0.0;
    world.mutation_rate = 0.0;
    world.environment.configure(0.5, 0.8, 3.0);
    for recipe in BodyFixture::ALL {
        world.try_seed_fixture(1, 2, -1.0, -1.0, recipe).unwrap();
    }
    world.add_food(-1.0, -1.0, 40);
    mature(&mut world.cells[0]);
    world.cells[1].age = LIFESPAN - DT;
    for tick in 0..420 {
        if tick == 60 {
            world.environment.add_nutrients(100.0, 100.0, 3.0);
        }
        if tick == 120 {
            world.add_food(300.0, 300.0, 3);
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
    world.add_food(300.0, 300.0, 8);
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
fn broken_storage_spills_energy_and_minerals_without_erasing_contents() {
    let mut world = world(BodyFixture::Scavenger);
    let cell = &mut world.cells[0];
    cell.energy = cell.energy_capacity();
    cell.nutrient_stored = cell.nutrient_capacity();
    cell.gut_energy = cell.gut_capacity();
    cell.gut_nutrients = cell.body.capacities().gut_minerals;
    let energy = world.energy_total();
    let minerals = world.nutrient_total();
    let structure = world.cells[0].body.structural_mass();
    for organ in &mut world.cells[0].body.organs {
        if matches!(organ.kind, Kind::Reserve | Kind::Digestive) {
            organ.integrity = 0.0;
        }
    }
    world.spill_excess_stores();
    close(world.energy_total(), energy);
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
    cell.gut_energy = 40.0;
    cell.gut_nutrients = 0.6;
    let energy = world.energy_total();
    let minerals = world.nutrient_total();
    let mut heat = 0.0;
    let mut request = BodyControls::default();
    request.recycle[Kind::Reserve as usize] = 1.0;
    request.recycle[Kind::Digestive as usize] = 1.0;
    for _ in 0..500 {
        let cell = &mut world.cells[0];
        let report = cell.body.work(
            0.0,
            &request,
            0.0,
            &mut cell.energy,
            &mut cell.nutrient_stored,
            DT,
            MAX_AREA,
        );
        heat += report.dissipated_energy;
        world.spill_excess_stores();
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
        maintenance + DIVISION_COST * original.area,
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
    world.food_rate = 0.0;
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
    }
    world.energy_imports += 1200.0;
    world.nutrient_imports += 20.0;
    let material = world.nutrient_total();
    let energy = world.energy_total();
    for cell in &mut world.cells {
        for _ in 0..1000 {
            cell.body.work(
                1.0,
                &request,
                0.0,
                &mut cell.energy,
                &mut cell.nutrient_stored,
                1.0,
                8.0,
            );
            if cell.body.area >= 8.0 - 1e-10 {
                break;
            }
        }
        close(cell.body.area, 8.0);
        cell.body.validate().unwrap();
        cell.y = 300.0;
    }
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
    world.food_rate = 0.0;
    world.environment.configure(0.5, 1.0, 0.0);
    world
        .try_seed_fixture(1, 120, 110.0, 110.0, BodyFixture::Producer)
        .unwrap();
    for cell in &mut world.cells {
        cell.x = 110.0;
        cell.y = 110.0;
        cell.energy = 50.0;
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
        "a complete paid producer lifecycle must finish before senescence"
    );
    assert_eq!(world.cells.len(), 2);
    assert!(world.time() < LIFESPAN);
    assert!(world.photosynthesis_total > 0.0);
    assert!(world.cells.iter().all(|c| c.gut_energy == 0.0));
    close(world.nutrient_total(), material);
}

#[test]
fn capturing_and_digesting_are_separate_and_conversion_has_a_real_energy_loss() {
    let mut world = world(BodyFixture::Scavenger);
    world.cells[0].energy = 50.0;
    world.particle(300.0, 300.0, 20.0, 0.4);
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
    let mut processing = [capacities[0].metabolism * DT];
    world.digest(&outputs, &capacities, &mut processing);
    let processed = before - world.cells[0].gut_energy;
    assert!(processed > 0.0 && processed <= capacities[0].digestion * DT);
    close(
        world.cells[0].energy - 50.0,
        processed * ASSIMILATION_EFFICIENCY,
    );
    close(
        energy - world.energy_total(),
        processed * (1.0 - ASSIMILATION_EFFICIENCY),
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
    world.cells[0].gut_nutrients = world.cells[0].body.capacities().gut_minerals;
    world.particle(300.0, 300.0, 12.0, 0.4);
    let capacities = caps(&world);
    let outputs = [Outputs {
        feed: 1.0,
        ..Outputs::default()
    }];
    world.eat(&outputs, &capacities);
    close(world.cells[0].gut_energy, 0.0);
    close(world.food[0].energy, 12.0);
    world.cells[0].gut_nutrients = 0.0;
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
    close(predator_energy - world.cells[0].energy, expected_cost);
    close(world.nutrient_total(), material);
    assert!(world.energy_total() < energy);
}

#[test]
fn senescence_returns_all_body_and_gut_material_even_when_particles_are_full() {
    let mut world = world(BodyFixture::Scavenger);
    world.add_food(-1.0, -1.0, MAX_FOOD);
    world.cells[0].gut_energy = 20.0;
    world.cells[0].gut_nutrients = 0.5;
    world.cells[0].age = LIFESPAN - DT;
    let material = world.nutrient_total();
    let energy = world.energy_total();
    world.step(&mut registry(IDLE));
    assert!(world.cells.is_empty());
    assert_eq!(world.deaths, 1);
    assert_eq!(world.food.len(), MAX_FOOD);
    close(world.nutrient_total(), material);
    assert!(world.energy_total() <= energy);
}

#[test]
fn resource_imports_count_actual_introductions_and_reset_replaces_old_history() {
    let mut world = World::empty(11);
    close(world.nutrient_total(), world.nutrient_input_total());
    for recipe in BodyFixture::ALL {
        world.try_seed_fixture(1, 2, -1.0, -1.0, recipe).unwrap();
    }
    world.add_food(-1.0, -1.0, MAX_FOOD);
    let full = world.nutrient_input_total();
    assert!(world.try_seed_cells(1, usize::MAX, 100.0, 100.0).is_err());
    assert_eq!(world.add_food(100.0, 100.0, 5), 0);
    close(world.nutrient_input_total(), full);
    close(world.nutrient_total(), full);
    world.environment.set_initial_nutrients(0.1);
    close(world.nutrient_total(), full);
    world.reset(11, 1);
    let (_, budget) = Body::founder();
    close(
        world.nutrient_input_total(),
        0.1 * PATCHES as f64 + 360.0 * FOOD_NUTRIENTS + 22.0 * budget.introduced_minerals,
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
    close(energy - world.energy_total(), upkeep + DIVISION_COST * area);
}

#[test]
fn deterministic_reset_replays_mutation_ids_bodies_and_resource_state() {
    let mut programs = registry(DIVIDE);
    let mut world = World::empty(81);
    world.food_rate = 0.0;
    world.mutation_rate = 1.0;
    let mut expected = Vec::new();
    let mut expected_registry = 0;
    for attempt in 0..2 {
        world.reset_empty(81, 0);
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
    let mut fed_programs = registry(source);
    let mut unfed_programs = registry(source);
    let founder_energy = fed.cells[0].energy;
    let founder_id = fed.cells[0].id;
    let founder_inputs = fed.energy_input_total();
    assert!(founder_energy / fed.cells[0].energy_capacity() < 0.9);
    assert_eq!(fed.cells[0].body.functional_mass(Kind::Photosynthetic), 0.0);
    assert_eq!(fed.add_food(300.0, 300.0, 256), 256);
    let mut saw_ingestion = false;
    let mut saw_assimilation = false;
    let mut exceeded_founder_reserve = false;
    for tick in 0..3600 {
        if tick > 0 && tick % 60 == 0 {
            fed.add_food(300.0, 300.0, 5);
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
    assert!(fed.time() < LIFESPAN);
}
