//! Mechanisms must compose through real stores and world phases, without
//! inheriting the unrelated capabilities of the former organelle families.
use super::*;
use crate::body::EMBODIED_FRACTION;
use crate::cellular::{
    AssemblyInstance, AssemblyKind, CellularState, Ingredient, MechanismKind, Recipe, RecipePart,
};
use crate::checkpoint::{Reader, Writer};

const BASE: &str = r#"cellular;
product Frame = scaffold();
product Wall = membrane();
product Catalyst = enzyme();
product Mount = anchor();
product Rotor = motor();
product Pigment = pigment();
product Detector = receptor();
assembly Structure: support() { Frame: 0.04; }
assembly Battery: store(energy) { Wall: 0.04; }
assembly MineralTank: store(mineral) { Wall: 0.03; }
"#;
const FOOD_STORE: &str = "assembly FoodTank: store(food) { Wall: 0.04; activity: 0; }\n";
const TISSUE_DIGEST: &str = "assembly Hydrolase: digest(tissue) { Catalyst: 0.04; activity: 1; }\n";
const CAPTURE: &str = "assembly Collector: import(particle) { Mount: 0.01; Rotor: 0.01; }\n";
const SOLAR: &str = "assembly Solar: harvest(light) { Mount: 0.015; Pigment: 0.015; Catalyst: 0.015; activity: 1; }\n";

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-8,
        "{actual:.12} != {expected:.12}"
    );
}

/// Install explicitly paid test machinery. All material and fabrication energy
/// are deducted from the universal founder's reserves. Reclaim its old core
/// ledger into free stores, leaving no bundled bootstrap capabilities. A test
/// may explicitly request an old family to exercise mixed-program compatibility.
pub(super) fn equipped(extra: &str) -> (World, Vec<Genome>) {
    let source = format!("{BASE}{extra}");
    let program = Program::compile(&source).unwrap();
    let definition = program.cellular().unwrap();
    let mut world = World::empty(293);
    world.environment.configure(0.5, 0.85, 0.0);
    world.environment.set_initial_nutrients(2.0);
    world.reset_empty(293);

    world.mutation_rate = 0.0;
    world.seed_cells(1, 1, 600.0, 380.0);
    let cell = &mut world.cells[0];
    cell.angle = 0.0;
    cell.vx = 0.0;
    cell.vy = 0.0;
    let mut state = CellularState::default();
    for def in &definition.assemblies {
        let parts: Vec<_> = def
            .parts
            .iter()
            .map(|&(index, required_mass)| RecipePart {
                product: definition.products[index].clone(),
                required_mass,
            })
            .collect();
        let ingredients = parts
            .iter()
            .map(|part| {
                let installation = match def.kind {
                    AssemblyKind::Organelle(kind) => kind.construction_cost() * 0.4,
                    _ => 30.0,
                };
                let paid = part.required_mass * (part.product.kind.synthesis_cost() + installation);
                cell.energy -= paid;
                cell.nutrient_stored -= part.required_mass;
                cell.organic_carbon -= part.required_mass;
                Ingredient {
                    mass: part.required_mass,
                    embodied_energy: paid * EMBODIED_FRACTION,
                }
            })
            .collect();
        state.assemblies.push(AssemblyInstance {
            id: state.next_assembly_id,
            recipe: std::rc::Rc::new(Recipe {
                name: def.name.clone(),
                kind: def.kind,
                parts,
                at: def.at,
                direction: def.direction,
            }),
            ingredients,
            integrity: 1.0,
            activity: 1.0,
        });
        state.next_assembly_id += 1;
    }
    for core in cell.body.organs.drain(..) {
        cell.nutrient_stored += core.mass;
        cell.organic_carbon += core.mass;
        cell.energy += core.embodied_energy;
    }
    cell.body.cellular = Some(state);
    assert!(cell.energy > 0.0 && cell.nutrient_stored > 0.0);
    cell.body.validate().unwrap();
    assert!(cell.body.viable());
    close(cell.body.capacities().metabolism, 0.0);
    close(world.nutrient_total(), world.nutrient_input_total());
    assert!(world.energy_total() <= world.energy_input_total());
    (world, vec![Genome::new(source, program, 0, 0)])
}

/// Move existing paid reserves into a known mixed meal, conserving both ledgers.
fn mixed_meal(world: &mut World) {
    let cell = &mut world.cells[0];
    for kind in OrganicKind::ALL {
        cell.gut[kind as usize] = OrganicMatter {
            extra_energy: 0.0,
            energy: 2.0,
            nutrients: 0.02,
        };
        cell.organic_carbon -= 2.0 / CHEMICAL_ENERGY_PER_CARBON;
        cell.nutrient_stored -= 0.02;
    }
    cell.refresh_gut_totals();
}

fn isolated(body: &Body, kind: MechanismKind) -> Body {
    let mut body = body.clone();
    body.cellular
        .as_mut()
        .unwrap()
        .assemblies
        .retain(|a| a.recipe.kind == AssemblyKind::Mechanism(kind));
    body
}

// The extraction fraction is a neutral legacy action multiplier, not a
// capability: without piercing mass it cannot perform any interaction.
fn no_capabilities() -> BodyCapacities {
    BodyCapacities { bite_extraction_fraction: 1.0, ..BodyCapacities::default() }
}

#[test]
fn support_translation_and_stores_have_separate_physical_responsibilities() {
    let (world, _) = equipped(&format!(
        "{FOOD_STORE}assembly Ribosome: translate() {{ Frame: 0.02; Catalyst: 0.02; }}"
    ));
    let body = &world.cells[0].body;
    let support = isolated(body, MechanismKind::Support);
    assert!(support.viable());
    assert_eq!(support.capacities(), no_capabilities());

    let translation = isolated(body, MechanismKind::Translation);
    assert!(
        !translation.viable(),
        "A translator does not grant structural support"
    );
    let cap = translation.capacities();
    assert!(cap.assembly > 0.0);
    assert_eq!(
        cap,
        BodyCapacities {
            assembly: cap.assembly,
            ..no_capabilities()
        }
    );

    let energy = isolated(body, MechanismKind::EnergyStorage).capacities();
    assert!(energy.energy > 0.0);
    assert_eq!(
        energy,
        BodyCapacities {
            energy: energy.energy,
            energy_storage: energy.energy,
            ..no_capabilities()
        }
    );
    let mineral = isolated(body, MechanismKind::MineralStorage).capacities();
    assert!(mineral.minerals > 0.0);
    assert_eq!(
        mineral,
        BodyCapacities {
            minerals: mineral.minerals,
            mineral_storage: mineral.minerals,
            ..no_capabilities()
        }
    );

    let mut food = isolated(body, MechanismKind::FoodStorage);
    let cap = food.capacities();
    assert!(cap.gut_energy > 0.0 && cap.gut_minerals > 0.0);
    assert_eq!(
        cap,
        BodyCapacities {
            gut_energy: cap.gut_energy,
            gut_minerals: cap.gut_minerals,
            gut_storage: cap.gut_energy,
            ..no_capabilities()
        }
    );
    food.cellular.as_mut().unwrap().assemblies[0].activity = 0.0;
    assert_eq!(
        food.capacities(),
        cap,
        "Regulation cannot erase an existing compartment"
    );
    food.cellular.as_mut().unwrap().assemblies[0].integrity = 0.1;
    assert_eq!(
        food.capacities(),
        no_capabilities(),
        "Severely damaged storage cannot retain contents"
    );
}

#[test]
fn complete_support_mechanisms_keep_both_partitioned_daughters_alive() {
    let extra = "assembly StructureB: support() { Frame: 0.04; }\n\
        assembly BatteryB: store(energy) { Wall: 0.04; }\n\
        assembly MineralTankB: store(mineral) { Wall: 0.03; }";
    let (mut world, mut genomes) = equipped(extra);
    let parent = world.cells[0].clone();
    let total = world.nutrient_total();
    let (a, b) = parent
        .body
        .partition()
        .expect("two complete structural supports");
    close(
        a.structural_mass() + b.structural_mass(),
        parent.body.structural_mass(),
    );
    close(
        a.embodied_energy() + b.embodied_energy(),
        parent.body.embodied_energy(),
    );
    let mut inherited_ids = Vec::new();
    world.cells.clear();
    // Body::partition is a conservative preview. Supply the same separate
    // reserve split and division charge that World uses for an actual birth.
    for (index, body) in [a, b].into_iter().enumerate() {
        assert!(
            body.organs.is_empty(),
            "No old core is available to support a daughter"
        );
        assert!(body.viable());
        body.validate().unwrap();
        inherited_ids.extend(
            body.cellular
                .as_ref()
                .unwrap()
                .assemblies
                .iter()
                .map(|a| a.id),
        );
        let mut child = parent.clone();
        child.id = index as u32 + 2;
        child.parent_id = parent.id;
        child.generation += 1;
        child.birth_area = body.area;
        child.body = body;
        child.energy = (parent.energy - DIVISION_COST) * 0.5;
        child.nutrient_stored *= 0.5;
        child.x += if index == 0 { -15.0 } else { 15.0 };
        child.cooldown = DIVISION_COOLDOWN;
        assert!(child.alive());
        world.cells.push(child);
    }
    world.next_id = 4;
    inherited_ids.sort_unstable();
    assert_eq!(
        inherited_ids,
        parent
            .body
            .cellular
            .as_ref()
            .unwrap()
            .assemblies
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>()
    );
    world.step(&mut genomes);
    assert_eq!(world.cells.len(), 2);
    assert!(world.cells.iter().all(Cell::alive));
    close(world.nutrient_total(), total);

    let mut unfinished = parent.body;
    let support = unfinished
        .cellular
        .as_mut()
        .unwrap()
        .assemblies
        .iter_mut()
        .find(|a| a.recipe.name == "StructureB")
        .unwrap();
    support.ingredients[0].mass *= 0.5;
    support.ingredients[0].embodied_energy *= 0.5;
    assert!(unfinished.viable());
    assert!(
        !unfinished.can_partition(),
        "An unfinished second support cannot keep a daughter alive"
    );
}

#[test]
fn tissue_digestion_requires_its_enzyme_and_never_processes_other_substrates() {
    for activity in [false, true] {
        let enzyme = if activity {
            TISSUE_DIGEST.into()
        } else {
            TISSUE_DIGEST.replace("activity: 1", "activity: 0")
        };
        let (mut world, mut genomes) = equipped(&format!("{FOOD_STORE}{enzyme}"));
        mixed_meal(&mut world);
        let old = world.cells[0].gut;
        let mineral_total = world.nutrient_total();
        let energy_total = world.energy_total();
        let stored_energy = world.cells[0].energy;
        let stored_carbon = world.cells[0].organic_carbon;
        assert_eq!(world.cells[0].body.capacities().metabolism, 0.0);
        world.step(&mut genomes);
        assert_eq!(
            world.cells[0].gut[OrganicKind::Exudate as usize],
            old[OrganicKind::Exudate as usize]
        );
        assert_eq!(
            world.cells[0].gut[OrganicKind::Waste as usize],
            old[OrganicKind::Waste as usize]
        );
        if activity {
            assert!(
                world.cells[0].gut[OrganicKind::Tissue as usize].energy
                    < old[OrganicKind::Tissue as usize].energy
            );
            assert!(world.cells[0].organic_carbon > stored_carbon);
            assert!(world.cells[0].energy < stored_energy);
            assert!(world.cells[0].digestion_rate > 0.0);
        } else {
            assert_eq!(world.cells[0].gut, old);
            assert_eq!(world.cells[0].digestion_rate, 0.0);
        }
        close(world.nutrient_total(), mineral_total);
        assert!(
            world.energy_total() < energy_total,
            "Digestion and upkeep dissipate energy"
        );
    }
    let (mut world, mut genomes) = equipped(FOOD_STORE);
    mixed_meal(&mut world);
    let old = world.cells[0].gut;
    world.step(&mut genomes);
    assert_eq!(
        world.cells[0].gut, old,
        "A food compartment cannot digest by itself"
    );
}

#[test]
fn particle_import_requires_a_compartment_and_grants_no_digestion_or_engulfing() {
    for stored in [false, true] {
        let extra = format!("{CAPTURE}{}", if stored { FOOD_STORE } else { "" });
        let (mut world, mut genomes) = equipped(&extra);
        let cap = world.cells[0].body.capacities();
        assert!(cap.capture > 0.0);
        assert_eq!(cap.phagocytosis, 0.0);
        assert_eq!(cap.independent_engulf, 0.0);
        assert_eq!(cap.digestion, 0.0);
        assert_eq!(cap.substrate_digestion, [0.0; 3]);
        let (x, y) = (world.cells[0].x, world.cells[0].y);
        world.fixture_exudate(x, y, 4);
        for particle in &mut world.food {
            particle.x = x;
            particle.y = y;
        }
        let total = world.nutrient_total();
        world.step(&mut genomes);
        assert_eq!(world.cells[0].gut_energy > 0.0, stored);
        assert_eq!(world.cells[0].digestion_rate, 0.0);
        assert_eq!(world.attacks, 0);
        close(world.nutrient_total(), total);
    }
}

#[test]
fn primitive_piercing_does_not_inherit_legacy_live_extraction() {
    let extra = "product Needle = filament();\n\
        assembly Puncture: pierce() { Mount: 0.01; Needle: 0.01; Catalyst: 0.01; }\n\
        assembly OldGut: digestive { Wall: 0.02; Catalyst: 0.02; }";
    let (mut world, mut genomes) = equipped(extra);
    let idle = "thrust=0; turn=0; divide=false; feed=0;";
    genomes.push(Genome::new(
        idle.into(),
        Program::compile(idle).unwrap(),
        0,
        0,
    ));
    world.seed_cells(2, 1, 612.0, 380.0);
    world.cells[0].x = 600.0;
    world.cells[0].y = 380.0;
    world.cells[1].x = 612.0;
    world.cells[1].y = 380.0;
    world.cells[1].vx = 0.0;
    world.cells[1].vy = 0.0;
    let material = world.nutrient_total();
    assert!(world.cells[0].body.capacities().digestion > 0.0);
    assert!(world.cells[0].body.capacities().gut_energy > 0.0);
    world.step(&mut genomes);
    assert!(world.attacks > 0);
    assert_eq!(world.cells.len(), 2);
    assert!(world.cells[1].integrity < 1.0 && world.cells[1].alive());
    assert_eq!(
        world.cells[0].gut_energy, 0.0,
        "Primitive puncturing cannot borrow an old family's extraction behavior"
    );
    close(world.nutrient_total(), material);
}

#[test]
fn harvesting_is_independent_of_metabolism_and_light_sensing_never_harvests() {
    for source in [
        SOLAR,
        "assembly Sensor: sense(light) { Mount: 0.01; Detector: 0.01; Pigment: 0.01; }",
    ] {
        let (mut world, mut genomes) = equipped(source);
        let initial_energy = world.cells[0].energy;
        let initial_carbon = world.cells[0].organic_carbon;
        let mineral_total = world.nutrient_total();
        for _ in 0..30 {
            world.step(&mut genomes);
        }
        assert_eq!(world.cells[0].body.capacities().metabolism, 0.0);
        if source == SOLAR {
            assert!(world.photosynthesis_total > 0.0);
            assert!(world.cells[0].organic_carbon > initial_carbon);
            assert!(world.cells[0].energy < initial_energy, "fixation stores chemical potential; respiration makes usable energy");
            assert_eq!(world.cells[0].body.capacities().light_sensing, 0.0);
        } else {
            assert!(world.cells[0].body.capacities().light_sensing > 0.0);
            assert_eq!(world.photosynthesis_total, 0.0);
            assert!(world.cells[0].energy < initial_energy);
        }
        close(world.nutrient_total(), mineral_total);
        assert!(
            world.energy_total() <= world.energy_input_total() + world.photosynthesis_total + 1e-8
        );
    }
    let (mut world, mut genomes) = equipped(SOLAR);
    let cell = &mut world.cells[0];
    let assembly = cell
        .body
        .cellular
        .as_mut()
        .unwrap()
        .assemblies
        .iter_mut()
        .find(|a| a.recipe.name == "Solar")
        .unwrap();
    let pigment = &mut assembly.ingredients[1];
    cell.nutrient_stored += pigment.mass;
    cell.energy += pigment.embodied_energy;
    pigment.mass = 0.0;
    pigment.embodied_energy = 0.0;
    world.step(&mut genomes);
    assert_eq!(
        world.photosynthesis_total, 0.0,
        "A missing typed ingredient prevents harvesting"
    );
}

#[test]
fn mechanism_checkpoints_resume_exactly_and_reject_unknown_body_tags_atomically() {
    let (mut world, mut genomes) = equipped(&format!("{FOOD_STORE}{TISSUE_DIGEST}{SOLAR}"));
    mixed_meal(&mut world);
    for _ in 0..20 {
        world.step(&mut genomes);
    }
    let mut original = crate::Engine::new();
    original.world = world;
    original.genomes = genomes;
    let bytes = crate::checkpoint::save(&original).unwrap();
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), crate::checkpoint::VERSION);
    let mut restored = crate::checkpoint::restore(&bytes).unwrap();
    assert_eq!(crate::checkpoint::save(&restored).unwrap(), bytes);
    for _ in 0..120 {
        original.world.step(&mut original.genomes);
        restored.world.step(&mut restored.genomes);
    }
    let before = crate::checkpoint::save(&restored).unwrap();
    assert_eq!(crate::checkpoint::save(&original).unwrap(), before);

    let mut historical = restored.world.cells[0].body.clone();
    historical.apply_pre_v13_energy_yield();
    let mut body_writer = Writer::new();
    historical.write_checkpoint(&mut body_writer);
    assert!(Body::read_checkpoint_version(&mut Reader::new(&body_writer.0), 5).is_err());
    assert_eq!(Body::read_checkpoint_version(&mut Reader::new(&body_writer.0), 7).unwrap(),historical);

    let mut corrupted = before.clone();
    let name = b"Hydrolase";
    let mut recipe_header = name.to_vec();
    recipe_header.push(19 + MechanismKind::DigestTissue as u8);
    let position = corrupted
        .windows(recipe_header.len())
        .rposition(|window| window == recipe_header)
        .unwrap()
        + name.len();
    assert_eq!(corrupted[position], 19 + MechanismKind::DigestTissue as u8);
    corrupted[position] = 255;
    let hash = corrupted[20..].iter().fold(0x811c9dc5_u32, |hash, byte| {
        (hash ^ *byte as u32).wrapping_mul(0x01000193)
    });
    corrupted[16..20].copy_from_slice(&hash.to_le_bytes());
    assert!(!restored.restore_state(&corrupted));
    assert!(
        restored.error.contains("unknown assembly"),
        "{}",
        restored.error
    );
    assert_eq!(crate::checkpoint::save(&restored).unwrap(), before);
}

#[test]
fn primitive_digestive_presentation_does_not_grant_legacy_capacity() {
    let (world, _) = equipped(&format!("{FOOD_STORE}assembly Enzyme: digest(tissue) {{ Catalyst: 0.02; }}"));
    let body = &world.cells[0].body;
    let legacy = body.family_summaries();
    let physical = body.capacities_from_summaries(&legacy);
    assert_eq!(physical.digestion, 0.0);
    assert!(physical.substrate_digestion[1] > 0.0);
    let mut display = legacy;
    body.add_visual_mechanisms(&mut display);
    assert!(display[Kind::Digestive as usize].capacity > 0.0,
        "The player must identify a built substrate enzyme as digestive hardware");
    assert_eq!(body.capacities(), physical,
        "Presentation must not add the legacy family's storage or processing effects");
}
