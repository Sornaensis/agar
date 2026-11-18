//! Authored feeding policy is exercised through paid development and real meals.
//! These explicitly fed cultures are separate from the unfed mixed ecosystem.
use super::*;

fn forager(light: f64) -> (World, Vec<Genome>) {
    let source = include_str!("../../../examples/forager.cell");
    let mut genomes = vec![Genome::new(source.into(), Program::compile(source).unwrap(), 0, 0)];
    let mut world = World::empty(42);
    world.environment.configure(0.5, light, 0.0);
    world.environment.set_initial_nutrients(3.0);
    world.environment.configure_gas_exchange(true, 0.5, 1.0, 1.0);
    world.reset_empty(42, 0);
    world.set_food_rate(0.0);
    world.set_mutation_rate(0.0);
    world.seed_cells(1, 1, 600.0, 380.0);
    let initial_energy = world.cells[0].energy;
    for _ in 0..900 {
        checked_step(&mut world, &mut genomes);
        assert_eq!(world.cells.len(), 1);
    }
    assert!(world.cells[0].energy - world.carbon_respired_total * RESPIRATION_ENERGY_PER_CARBON < initial_energy);
    assert_eq!(world.cells[0].body.area, 1.0);
    assert_eq!(world.cells[0].gut_energy, 0.0);
    assert_eq!(world.eaten, 0);
    assert_eq!(world.births, 0);
    assert_eq!(working(&world.cells[0], "TissueEnzyme"), 0.0);
    assert_eq!(working(&world.cells[0], "WasteEnzyme"), 0.0);
    (world, genomes)
}

fn checked_step(world: &mut World, genomes: &mut Vec<Genome>) {
    world.step(genomes);
    assert!((world.nutrient_total() - world.nutrient_input_total()).abs() < 1e-7);
    assert!(world.energy_total() <= world.energy_input_total() + 1e-7);
    assert_eq!(world.photosynthesis_total, 0.0);
    assert!((world.carbon_total() + world.environment.carbon_output_total() - world.carbon_input_total()).abs() < 1e-7);
    assert!((world.environment.oxygen_total() + world.oxygen_cell_total() + world.environment.oxygen_output_total()
        - world.environment.oxygen_input_total() - world.carbon_fixed_total
        + world.carbon_respired_total + world.decay_carbon_respired_total).abs() < 1e-7);
}

fn working(cell: &Cell, name: &str) -> f64 {
    cell.body.cellular.as_ref().unwrap().assemblies.iter()
        .filter(|a| a.recipe.name == name).map(|a| a.functional_mass()).sum()
}

fn activity(cell: &Cell, name: &str) -> f64 {
    cell.body.cellular.as_ref().unwrap().assemblies.iter()
        .filter(|a| a.recipe.name == name).map(|a| a.activity).sum()
}

fn supply(world: &mut World, kind: OrganicKind, x: f64, y: f64) {
    // The ordinary import accounts for every unit of energy and mineral. Only
    // the controlled substrate and encounter position differ from add_food's
    // usual exudate scatter; organism reserves and machinery are never patched.
    assert_eq!(world.add_food(x, y, 1), 1);
    let food = world.food.last_mut().unwrap();
    food.kind = kind;
    food.x = wrap(x, WIDTH);
    food.y = wrap(y, HEIGHT);
}

#[test]
fn shipped_forager_induces_tissue_digestion_and_funds_whole_daughters() {
    let (mut world, mut genomes) = forager(0.0);
    let essential: Vec<_> = ["Respiration", "Gut", "ExudateEnzyme", "Capture", "TissueEnzyme",
        "LeftMotor", "RightMotor", "Vision", "Touch"]
        .into_iter().map(|name| {
            let recipe = genomes[0].program.cellular().unwrap().assemblies.iter()
                .find(|a| a.name == name).unwrap();
            (name, recipe.parts.iter().map(|(_, mass)| mass).sum::<f64>())
        }).collect();
    let mut useful = 0.0;
    for tick in 0..570 * 30 {
        assert_eq!(world.cells.len(), 1, "The founder must fund its growth from captured tissue");
        let cell = &world.cells[0];
        if tick % 15 == 0 && cell.gut[OrganicKind::Tissue as usize].energy < 30.0 {
            let nearby: f64 = world.food.iter().filter(|food| food.kind == OrganicKind::Tissue
                && delta(food.x - cell.x, WIDTH).hypot(delta(food.y - cell.y, HEIGHT))
                    <= cell.radius() + 2.5).map(|food| food.energy).sum();
            if nearby < 12.0 {
                let (x, y) = (cell.x, cell.y);
                supply(&mut world, OrganicKind::Tissue, x, y);
            }
        }
        checked_step(&mut world, &mut genomes);
        useful += world.cells.iter().map(|cell| cell.digestion_rate * DT).sum::<f64>();
        if world.births > 0 { break; }
    }
    assert_eq!(world.births, 1,
        "A supplied tissue meal must fund the authored cell cycle: {:?}", world.cells.iter().map(|c| (c.energy,c.organic_carbon,c.respiration_rate,c.gut_energy,c.body.area,c.body.space(),c.body.cellular.as_ref().map(|s| s.assemblies.iter().map(|a| (a.recipe.name.as_str(),a.mass())).collect::<Vec<_>>()))).collect::<Vec<_>>());
    assert_eq!(world.cells.len(), 2);
    assert!(useful > 100.0);
    for cell in &world.cells {
        assert!(cell.body.space().used <= cell.body.space().capacity + 1e-8);
        for (name, mass) in &essential {
            assert!(working(cell, name) + 1e-7 >= 0.9 * mass,
                "Daughter {} must inherit a working {name}", cell.id);
        }
    }
    eprintln!("Paid Forager tissue culture divided at {:.2}s", world.time());
}

#[test]
fn shipped_forager_seeks_a_meal_with_waste_in_its_gut_without_overfilling_it() {
    let (mut world, mut genomes) = forager(0.8);
    let cell = &world.cells[0];
    let (x, y, angle) = (cell.x, cell.y, cell.angle);
    let started = world.time();
    supply(&mut world, OrganicKind::Waste, x, y);
    supply(&mut world, OrganicKind::Waste, x, y);
    supply(&mut world, OrganicKind::Exudate, x + 30.0 * angle.cos(), y + 30.0 * angle.sin());
    let mut searches_with_waste = false;
    let mut capture_pauses_when_full = false;
    let mut captured_at = None;
    let mut useful = 0.0;
    for _ in 0..30 * 30 {
        checked_step(&mut world, &mut genomes);
        assert_eq!(world.cells.len(), 1);
        let cell = &world.cells[0];
        let motors: f64 = cell.body.cellular.as_ref().unwrap().assemblies.iter()
            .filter(|a| matches!(a.recipe.name.as_str(), "LeftMotor" | "RightMotor"))
            .map(|a| a.activity * a.functional_mass()).sum();
        searches_with_waste |= cell.gut[OrganicKind::Waste as usize].energy > 0.45 * cell.gut_capacity()
            && motors > 0.015;
        capture_pauses_when_full |= cell.gut_energy > 0.7 * cell.gut_capacity()
            && activity(cell, "Capture") == 0.0;
        assert!(cell.gut_energy <= cell.gut_capacity() + 1e-8);
        if captured_at.is_none() && cell.gut[OrganicKind::Exudate as usize].energy > 0.0 {
            captured_at = Some(world.time() - started);
            assert!(cell.gut[OrganicKind::Waste as usize].energy > 0.45 * cell.gut_capacity());
        }
        useful += cell.digestion_rate * DT;
    }
    assert!(searches_with_waste, "Low-yield residue cannot signal behavioral satiation");
    assert!(capture_pauses_when_full, "Actual total gut occupancy still limits particle import");
    assert!(captured_at.is_some_and(|seconds| seconds < 10.0),
        "A visible nearby meal must be approached while the old waste is still being processed");
    assert!(useful > 10.0, "assimilated {useful}, cells {:?}",world.cells.iter().map(|c| (c.energy,c.organic_carbon,c.respiration_rate,c.gut_energy,c.body.area)).collect::<Vec<_>>());
    eprintln!("Forager with more than 45% waste captured exudate after {:.2}s", captured_at.unwrap());
}

fn fed_target(source: &str) -> (World, Vec<Genome>) {
    let mut genomes = vec![Genome::new(source.into(), Program::compile(source).unwrap(), 0, 0)];
    let mut world = World::empty(42);
    world.environment.configure(0.5, 0.8, 0.0);
    world.environment.set_initial_nutrients(0.3);
    world.environment.configure_gas_exchange(true, 0.5, 1.0, 1.0);
    world.reset_empty(42, 0);
    world.set_food_rate(0.0);
    world.set_mutation_rate(0.0);
    world.seed_cells(1, 1, 600.0, 380.0);
    for _ in 0..600 {
        checked_step(&mut world, &mut genomes);
        assert_eq!(world.cells.len(), 1);
        if ["Gut", "ExudateEnzyme", "Capture", "LeftMotor", "RightMotor", "Vision", "Touch"]
            .into_iter().all(|name| working(&world.cells[0], name) > 0.0) { break; }
    }
    assert!(world.cells[0].energy > 30.0);
    let (x, y) = (world.cells[0].x, world.cells[0].y);
    supply(&mut world, OrganicKind::Tissue, x, y);
    supply(&mut world, OrganicKind::Tissue, x, y);
    for _ in 0..30 { checked_step(&mut world, &mut genomes); }
    assert!(world.cells[0].gut[OrganicKind::Tissue as usize].energy > 25.0);
    (world, genomes)
}

#[derive(Debug)]
struct WoundEncounter {
    alive: bool,
    first_injury: Option<[f64; 3]>,
    last_energy: f64,
    last_gut: f64,
    final_integrity: f64,
    minimum_integrity: f64,
    motor_energy: f64,
    attacks: u64,
    fed_injured_motion: bool,
    injury_without_contact: bool,
    cleared: Option<[f64; 2]>,
    trace: Vec<[f64; 10]>,
}

fn wounded_encounter(source: &str, attack: bool) -> WoundEncounter {
    // Calibrate the ordinary seed scatter by replaying initialization. The
    // actual world receives a bare new attacker; no body, reserve, damage,
    // position or receptor reading is patched after seeding.
    let (mut probe, _) = fed_target(source);
    probe.seed_cells(1, 1, 0.0, 0.0);
    let jitter = (delta(probe.cells[1].x, WIDTH), delta(probe.cells[1].y, HEIGHT));
    let attacker_angle = probe.cells[1].angle;
    let (mut world, mut genomes) = fed_target(source);
    let target = &world.cells[0];
    let (x, y, angle) = (target.x, target.y, target.angle);
    let at = delta(angle + PI - attacker_angle, TAU);
    let weapon = format!(r#"cellular;
product Mount = anchor(); product Tail = filament(); product Catalyst = enzyme();
let need = built.Weapon + 0.000001 < 0.05;
gene m: Mount = if need && pool.Mount < 0.001 then 0.333333 else 0;
gene t: Tail = if need && pool.Tail < 0.001 then 1 else 0;
gene e: Catalyst = if need && pool.Catalyst < 0.001 then 0.333333 else 0;
assembly Weapon: pierce() {{
  Mount: 0.01; Tail: 0.03; Catalyst: 0.01;
  start_when: need; activity: {}; at: {}; direction: {};
}}"#, usize::from(attack), at, at);
    genomes.push(Genome::new(weapon.clone(), Program::compile(&weapon).unwrap(), 0, 0));
    world.seed_cells(2, 1, wrap(x + 14.1 * angle.cos() - jitter.0, WIDTH),
        wrap(y + 14.1 * angle.sin() - jitter.1, HEIGHT));
    let started = world.time();
    let touch = genomes[0].program.cellular().unwrap().assemblies.iter()
        .position(|recipe| recipe.name == "Touch").unwrap();
    let mut result = WoundEncounter {
        alive: false, first_injury: None, last_energy: 0.0, last_gut: 0.0,
        final_integrity: 1.0, minimum_integrity: 1.0, motor_energy: 0.0,
        attacks: 0, fed_injured_motion: false, injury_without_contact: false,
        cleared: None, trace: Vec::new(),
    };
    for tick in 0..60 * 30 {
        let Some(cell) = world.cells.iter().find(|cell| cell.id == 1) else { break; };
        // Stop supplying the finite meal once either policy first experiences
        // injury. Both attacked worlds have identical paid history to that tick.
        if result.first_injury.is_none() && tick % 15 == 0 && cell.gut[1].energy < 30.0 {
            let nearby: f64 = world.food.iter().filter(|food| food.kind == OrganicKind::Tissue
                && delta(food.x - cell.x, WIDTH).hypot(delta(food.y - cell.y, HEIGHT))
                    <= cell.radius() + 2.5).map(|food| food.energy).sum();
            if nearby < 12.0 {
                let (x, y) = (cell.x, cell.y);
                supply(&mut world, OrganicKind::Tissue, x, y);
            }
        }
        let cell = world.cells.iter().find(|cell| cell.id == 1).unwrap();
        let input = world.sense_program(1, &genomes[0].program).unwrap();
        let mut memory = cell.memory.clone();
        let controls = genomes[0].program.evaluate_cell(&input, &mut memory, &mut EvalScratch::default());
        let actions = cell.body.clone().cellular_actions(genomes[0].program.cellular().unwrap(),
            controls.cellular.as_ref().unwrap());
        result.motor_energy += actions.motor_cost * DT;
        result.last_energy = cell.energy;
        result.last_gut = cell.gut_energy;
        result.final_integrity = cell.integrity;
        if cell.integrity < 0.995 && cell.gut[0].energy + cell.gut[1].energy > 2.0 {
            result.fed_injured_motion |= actions.motor_cost > 0.0;
            result.injury_without_contact |= !input.receptors[touch].valid;
        }
        result.trace.push([cell.x, cell.y, cell.angle, cell.energy, cell.integrity,
            cell.body.area, cell.gut[0].energy, cell.gut[1].energy, cell.gut[2].energy, actions.motor_cost]);
        let attacker = world.cells.iter().find(|cell| cell.id == 2).unwrap();
        if result.first_injury.is_some() && result.cleared.is_none()
            && delta(attacker.x - cell.x, WIDTH).hypot(delta(attacker.y - cell.y, HEIGHT)) > 18.0 {
            result.cleared = Some([world.time() - started, result.motor_energy]);
        }
        checked_step(&mut world, &mut genomes);
        if let Some(cell) = world.cells.iter().find(|cell| cell.id == 1) {
            result.minimum_integrity = result.minimum_integrity.min(cell.integrity);
            if cell.integrity < 0.995 && result.first_injury.is_none() {
                result.first_injury = Some([world.time() - started, cell.energy, cell.gut_energy]);
            }
        }
    }
    result.alive = world.cells.iter().any(|cell| cell.id == 1);
    result.attacks = world.attacks;
    result
}

#[test]
fn shipped_forager_pays_to_escape_injury_while_fed_and_preserves_healthy_rest() {
    let source = include_str!("../../../examples/forager.cell");
    let no_reflex = source.replace("let injured = integrity < 0.995;", "let injured = false;");
    assert_ne!(source, no_reflex, "The negative control disables the authored injury response");
    let resting = wounded_encounter(source, false);
    let resting_control = wounded_encounter(&no_reflex, false);
    assert_eq!(resting.trace, resting_control.trace, "Healthy fed behavior remains identical every tick");
    assert!(resting.alive && resting.first_injury.is_none());
    assert_eq!(resting.motor_energy, 0.0);

    let passive = wounded_encounter(&no_reflex, true);
    let escaping = wounded_encounter(source, true);
    assert_eq!(passive.first_injury, escaping.first_injury);
    assert!(passive.first_injury.is_some_and(|first| first[1] > 30.0 && first[2] > 25.0));
    assert!(!passive.alive && passive.last_energy > 20.0 && passive.last_gut > 2.0,
        "The disabled reflex leaves a solvent, fed organism exposed to the actual weapon");
    assert!(escaping.alive && escaping.minimum_integrity > 0.8 && escaping.final_integrity > 0.99,
        "escaped alive={} integrity={}..{} energy={} gut={} motors={} first={:?} cleared={:?}",
        escaping.alive,escaping.minimum_integrity,escaping.final_integrity,escaping.last_energy,escaping.last_gut,
        escaping.motor_energy,escaping.first_injury,escaping.cleared);
    assert!(escaping.fed_injured_motion && escaping.injury_without_contact,
        "Injury remains observable when a narrow front receptor cannot see the attacker");
    assert!(escaping.cleared.is_some_and(|clear| clear[0] < 10.0 && clear[1] > 0.0));
    assert!(escaping.attacks < passive.attacks / 2);
    assert!(escaping.motor_energy > 0.0 && escaping.last_energy < resting.last_energy);
    eprintln!("Paid wound escape: {} vs {} attack ticks; {:.3} ATP motor cost over 60s; {:.3} ATP remains",
        escaping.attacks, passive.attacks, escaping.motor_energy, escaping.last_energy);
}
