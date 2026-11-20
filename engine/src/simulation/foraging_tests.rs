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
    world.reset_empty(42);

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
    // This isolated encounter controls the kind and location of accounted
    // substrate. The organism must capture and process the whole specimen.
    let (x,y) = world.cells.iter().find(|c| c.x == x && c.y == y)
        .map_or((x,y), Cell::inlet_fixture_position);
    assert_eq!(world.fixture_exudate(x, y, 1), 1);
    let food = world.food.last_mut().unwrap();
    food.kind = kind;
    food.composition = Composition::pool(kind as usize);
    food.x = wrap(x, WIDTH);
    food.y = wrap(y, HEIGHT);
}

#[test]
fn shipped_rememberer_catches_bound_carbon_on_its_opposite_flank() {
    let source = include_str!("../../../examples/rememberer.cell");
    let mut genomes = vec![Genome::new(source.into(), Program::compile(source).unwrap(), 0, 0)];
    let mut world = World::empty(42);
    world.environment.configure(0.5, 0.0, 0.0);
    world.environment.set_initial_nutrients(3.0);
    world.reset_empty(42);
    world.seed_cells(1, 1, 600.0, 380.0);
    for _ in 0..900 { checked_step(&mut world, &mut genomes); }
    let cell = &world.cells[0];
    assert_eq!(working(cell, "WasteEnzyme"), 0.0);
    let state = cell.body.cellular.as_ref().unwrap();
    let mount = |name: &str| {
        let inlet = state.assemblies.iter().find(|a| a.recipe.name == name && a.working_mass() > 0.0).unwrap();
        let angle = cell.angle + cell.body.mount_at(inlet);
        (cell.x + cell.radius() * angle.cos(), cell.y + cell.radius() * angle.sin())
    };
    let primary = mount("Capture");
    let opposite = mount("ReturnInlet");
    assert!((primary.0 - opposite.0).hypot(primary.1 - opposite.1) > 1.8 * cell.radius());
    supply(&mut world, OrganicKind::Waste, opposite.0, opposite.1);
    checked_step(&mut world, &mut genomes);
    assert!(world.cells[0].gut[2].energy > 0.0, "The opposite inlet must admit the packet");
    let mut processed = 0.0;
    for _ in 0..900 {
        checked_step(&mut world, &mut genomes);
        processed += world.cells[0].digestion_rate * DT;
    }
    assert!(working(&world.cells[0], "WasteEnzyme") >= 0.035 - 1e-6);
    assert!(processed > 1.0, "Only bound substrate was supplied; processed {processed}");
}

#[test]
fn shipped_forager_induces_tissue_digestion_and_funds_whole_daughters() {
    let (mut world, mut genomes) = forager(0.0);
    let essential: Vec<_> = ["Respiration", "Gut", "ExudateEnzyme", "Capture", "TissueEnzyme", "Minerals",
        "LeftMotor", "RightMotor", "Vision", "Touch"]
        .into_iter().map(|name| {
            let recipe = genomes[0].program.cellular().unwrap().assemblies.iter()
                .find(|a| a.name == name).unwrap();
            (name, recipe.parts.iter().map(|(_, mass)| mass).sum::<f64>())
        }).collect();
    let mut useful = 0.0;
    let mut reserves_at_fission = None;
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
        let mother = &world.cells[0];
        let available = (mother.gut[0].energy + mother.gut[1].energy, mother.organic_carbon);
        checked_step(&mut world, &mut genomes);
        useful += world.cells.iter().map(|cell| cell.digestion_rate * DT).sum::<f64>();
        if world.births > 0 {
            reserves_at_fission = Some(available);
            break;
        }
    }
    assert_eq!(world.births, 1,
        "A supplied tissue meal must fund the authored cell cycle: {:?}", world.cells.iter().map(|c| (c.energy,c.organic_carbon,c.respiration_rate,c.gut_energy,c.body.area,c.body.space(),c.body.cellular.as_ref().map(|s| s.assemblies.iter().map(|a| (a.recipe.name.as_str(),a.mass())).collect::<Vec<_>>()))).collect::<Vec<_>>());
    assert_eq!(world.cells.len(), 2);
    assert!(useful > 100.0);
    let area_before_split = world.cells.iter().map(|c| c.body.area).sum::<f64>();
    let (meal_before, carbon_before) = reserves_at_fission.unwrap();
    assert!(meal_before + carbon_before * CHEMICAL_ENERGY_PER_CARBON >= 7.0 * area_before_split);
    for cell in &world.cells {
        assert!(cell.body.space().used <= cell.body.space().capacity + 1e-8);
        let meal = cell.gut[OrganicKind::Exudate as usize].energy
            + cell.gut[OrganicKind::Tissue as usize].energy;
        // The authored grazer commits at 49 ATP and 7 units of usable meal
        // energy per area. Maintenance and the fission charge are paid before
        // the two daughters receive halves of the available reserves.
        let whole_atp_after_fission = ((49.0 - DIVISION_COST * area_before_split) * 0.5).floor();
        assert!(cell.energy >= whole_atp_after_fission, "Underfunded daughter with {} ATP", cell.energy);
        // Each half keeps installed food storage and its share of this meal
        // up to the physical capacity it inherited.
        assert!(working(cell, "Gut") > 0.0 && cell.gut_capacity() > 0.0);
        let retained_meal = (meal_before * 0.5).min(cell.gut_capacity());
        let retained_carbon = (carbon_before * 0.5).min(cell.organic_carbon_capacity());
        let retained_energy = retained_meal + retained_carbon * CHEMICAL_ENERGY_PER_CARBON;
        assert!(retained_carbon > 0.0 && cell.organic_carbon > 0.0);
        assert!(meal + cell.organic_carbon * CHEMICAL_ENERGY_PER_CARBON >= retained_energy.floor(),
            "Each half retains its share up to the storage capacity it inherited");
    }
    for (name,mass) in &essential {
        assert!(world.cells.iter().map(|c| working(c,name)).sum::<f64>() + 1e-7 >= 1.8 * mass,
            "The physical copies of {name} must survive partition across the two daughters");
    }
    eprintln!("Paid Forager tissue culture divided at {:.2}s", world.time());
}

#[test]
fn shipped_forager_seeks_a_meal_with_waste_in_its_gut_without_overfilling_it() {
    let (mut world, mut genomes) = forager(0.8);
    let cell = &world.cells[0];
    let (x, y) = (cell.x, cell.y);
    supply(&mut world, OrganicKind::Waste, x, y);
    supply(&mut world, OrganicKind::Waste, x, y);
    for _ in 0..120 {
        if world.cells[0].gut[2].energy > 0.5 * world.cells[0].gut_capacity() { break; }
        let (fx,fy) = world.cells[0].inlet_fixture_position();
        for food in &mut world.food { food.x=fx; food.y=fy; }
        checked_step(&mut world, &mut genomes);
    }
    assert!(world.cells[0].gut[2].energy > 0.45 * world.cells[0].gut_capacity());
    let cell = &world.cells[0];
    let (x, y, angle) = (cell.x,cell.y,cell.angle);
    let started = world.time();
    supply(&mut world, OrganicKind::Exudate, x + 30.0 * angle.cos(), y + 30.0 * angle.sin());
    let mut searches_with_waste = false;
    let mut capture_pauses_when_full = false;
    let mut captured_at = None;
    let mut useful = 0.0;
    for _ in 0..45 * 30 {
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
    world.reset_empty(42);

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
    // Begin the injury comparison with digestible tissue in both treatments.
    for _ in 0..8 * 30 {
        checked_step(&mut world, &mut genomes);
        if working(&world.cells[0], "TissueEnzyme") > 0.0
            && world.cells[0].gut[OrganicKind::Tissue as usize].energy > 25.0 { break; }
    }
    assert!(working(&world.cells[0], "TissueEnzyme") > 0.0);
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
    fed_motor_energy: f64,
    fed_ticks: usize,
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
        fed_motor_energy: 0.0, fed_ticks: 0, attacks: 0, fed_injured_motion: false, injury_without_contact: false,
        cleared: None, trace: Vec::new(),
    };
    for tick in 0..60 * 30 {
        let Some(cell) = world.cells.iter().find(|cell| cell.id == 1) else { break; };
        // Keep a finite tissue meal within reach while the attacker develops.
        // Stop at the first wound; all later digestion comes from that same meal.
        if result.first_injury.is_none() && tick % 15 == 0 {
            let (mouth_x, mouth_y) = cell.inlet_fixture_position();
            let nearby: f64 = world.food.iter().filter(|food| food.kind == OrganicKind::Tissue
                && delta(food.x - mouth_x, WIDTH).hypot(delta(food.y - mouth_y, HEIGHT))
                    <= 3.2).map(|food| food.energy).sum();
            if nearby < 12.0 && cell.gut[OrganicKind::Tissue as usize].energy < 28.0 {
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
        let digestible = [("ExudateEnzyme", 0), ("TissueEnzyme", 1)].into_iter()
            .filter(|(enzyme, _)| working(cell, enzyme) > 0.0)
            .map(|(_, pool)| cell.gut[pool].energy).sum::<f64>();
        let fed = digestible > 2.0;
        if fed {
            result.fed_ticks += 1;
            result.fed_motor_energy += actions.motor_cost * DT;
        }
        result.motor_energy += actions.motor_cost * DT;
        result.last_energy = cell.energy;
        result.last_gut = cell.gut_energy;
        result.final_integrity = cell.integrity;
        if cell.integrity < 0.995 && fed {
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
    assert!(resting.fed_ticks >= 30 * 30, "The healthy control is fed for at least half the encounter");
    assert_eq!(resting.fed_motor_energy, 0.0);

    let passive = wounded_encounter(&no_reflex, true);
    let escaping = wounded_encounter(source, true);
    assert_eq!(passive.first_injury, escaping.first_injury);
    assert!(passive.first_injury.is_some_and(|first| first[1] > 30.0 && first[2] > 25.0));
    assert!(!passive.alive && passive.last_energy > 20.0 && passive.last_gut > 2.0,
        "Disabled reflex: alive={}, ATP={}, gut={}, attack ticks={}",
        passive.alive, passive.last_energy, passive.last_gut, passive.attacks);
    assert!(passive.fed_ticks >= 4 * 30);
    assert_eq!(passive.fed_motor_energy, 0.0, "The wounded negative control stays at rest while fed");
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

#[test]
fn wounded_grazer_releases_its_feeding_partner_before_swimming() {
    let program = Program::compile(include_str!("../../../examples/forager.cell")).unwrap();
    let recipes = &program.cellular().unwrap().assemblies;
    let index = |name| recipes.iter().position(|recipe| recipe.name == name).unwrap();
    let mut input = Inputs { integrity: 1.0, ..Inputs::default() };
    input.receptors.resize(recipes.len(), ReceptorInput::default());
    input.receptors[index("Touch")] = ReceptorInput {
        available: true, valid: true, message_valid: true, message: 41.0, bonded: true, ..Default::default()
    };
    let mut memory = Registers::default();
    // The first tick commits the program's recognition marker register.
    program.evaluate_cell(&input, &mut memory, &mut EvalScratch::default());
    for (integrity, should_hold) in [(1.0, true), (0.97, false), (1.0, true)] {
        input.integrity = integrity;
        let controls = program.evaluate_cell(&input, &mut memory, &mut EvalScratch::default()).cellular.unwrap();
        assert_eq!(controls.activity[index("Dock")] > 0.0, should_hold);
        let swimming = controls.activity[index("LeftMotor")] + controls.activity[index("RightMotor")] > 0.0;
        assert_eq!(swimming, !should_hold);
    }
}

#[test]
fn depleted_grazer_waits_for_a_feeding_link_before_leaving_to_search() {
    let program = Program::compile(include_str!("../../../examples/forager.cell")).unwrap();
    let recipes = &program.cellular().unwrap().assemblies;
    let index = |name| recipes.iter().position(|recipe| recipe.name == name).unwrap();
    let mut input = Inputs { integrity: 1.0, body_area: 1.0, energy_fraction: 0.9, ..Inputs::default() };
    input.receptors.resize(recipes.len(), ReceptorInput::default());
    input.receptors[index("Touch")] = ReceptorInput {
        available: true, valid: true, message_valid: true, message: 41.0, bonded: true, ..Default::default()
    };
    let mut memory = Registers::default();
    for tick in 0..=16 * 30 {
        input.age = tick as f64 * DT;
        if tick >= 30 { input.energy_fraction = 0.3; }
        let controls = program.evaluate_cell(&input, &mut memory, &mut EvalScratch::default()).cellular.unwrap();
        let held = controls.activity[index("Dock")] > 0.0;
        let moving = controls.activity[index("LeftMotor")] + controls.activity[index("RightMotor")] > 0.0;
        if tick == 6 * 30 || tick == 16 * 30 {
            assert!(held && !moving, "Wait for delivery and later allow another feeding attempt");
        } else if tick == 8 * 30 {
            assert!(!held && moving, "Release the unproductive dock and swim using the real motor controls");
        }
    }
}

#[test]
fn a_food_plume_primes_the_grazer_dock_but_only_its_partner_opens_transfer_ports() {
    let forager = include_str!("../../../examples/forager.cell");
    let producer = include_str!("../../../examples/phototroph.cell");
    let unprimed = forager.replace("(rb093 || rb097)", "rb093");
    let wrong_badge = producer.replace("rf090 = 41;", "rf090 = 43;");
    assert_ne!(unprimed, forager);
    assert_ne!(wrong_badge, producer);

    let encounter = |grazer: &str, host: &str| {
        let mut genomes = [grazer, host].map(|source|
            Genome::new(source.to_string(), Program::compile(source).unwrap(), 0, 0)).to_vec();
        let mut world = World::empty(42);
        world.environment.configure(0.5, 0.8, 0.0);
        world.environment.set_initial_nutrients(2.0);
        world.reset_empty(42);
        world.set_mutation_rate(0.0);
        world.try_seed_cells_in_registry(1, 1, 600.0, 380.0, &genomes).unwrap();
        world.try_seed_cells_in_registry(2, 1, 250.0, 150.0, &genomes).unwrap();
        let hold = |cell: &mut Cell, x, y, angle| {
            cell.x = x;
            cell.y = y;
            cell.vx = 0.0;
            cell.vy = 0.0;
            cell.angle = angle;
        };
        // Keep the tissue outside inlet reach while its organic plume develops.
        for tick in 0..32 * 30 {
            hold(&mut world.cells[0], 600.0, 380.0, 0.0);
            hold(&mut world.cells[1], 250.0, 150.0, PI);
            if tick == 15 * 30 {
                for _ in 0..4 { supply(&mut world, OrganicKind::Tissue, 615.0, 380.0); }
            }
            world.step(&mut genomes);
        }
        let grazer = &world.cells[0];
        assert_eq!(grazer.gut_energy, 0.0);
        assert!(world.environment.organic_at(grazer.x, grazer.y) > 0.0);
        assert_eq!(working(grazer, "CarbonPort"), 0.0);
        assert_eq!(working(grazer, "MineralPort"), 0.0);
        let primed_mass = working(grazer, "Dock");
        let mut first_bond = None;
        for tick in 1..=15 * 30 {
            let x = 600.0 + world.cells[0].radius() + world.cells[1].radius() + 0.05;
            hold(&mut world.cells[0], 600.0, 380.0, 0.0);
            hold(&mut world.cells[1], x, 380.0, PI);
            world.step(&mut genomes);
            if world.has_bond(1, 2) { first_bond.get_or_insert(tick); }
        }
        (primed_mass, first_bond, working(&world.cells[0], "CarbonPort"))
    };
    let (ready, immediate, open_port) = encounter(forager, producer);
    let (absent, late, _) = encounter(&unprimed, producer);
    let (ready_mismatch, never, closed_port) = encounter(forager, &wrong_badge);
    assert!(ready >= 0.019 && ready_mismatch >= 0.019 && absent == 0.0);
    assert!(immediate.is_some_and(|tick| tick <= 30));
    assert!(late.is_some_and(|tick| tick > 30));
    assert!(open_port > 0.045);
    assert_eq!((never, closed_port), (None, 0.0));
}

#[test]
fn a_grazer_rests_for_a_meal_only_with_its_matching_digestive_enzyme() {
    let program = Program::compile(include_str!("../../../examples/forager.cell")).unwrap();
    let recipes = &program.cellular().unwrap().assemblies;
    let index = |name: &str| recipes.iter().position(|recipe| recipe.name == name).unwrap();
    // An observed Forager carried 3.14 structural units at 1.85 ATP with no
    // working tissue digester. Compare the same reserves and physical gut fill.
    let mut input = Inputs {
        age: 300.0, integrity: 1.0, body_area: 1.0, body_birth_area: 1.0,
        energy: 1.85, energy_capacity: 80.0, energy_fraction: 1.85 / 80.0,
        gut_energy: 3.14, gut_capacity: 108.0, gut_fraction: 3.14 / 108.0,
        ..Inputs::default()
    };
    input.assemblies.resize(recipes.len(), 0.0);
    input.assemblies[index("Gut")] = 0.135;
    input.assemblies[index("Capture")] = 0.03;
    input.assemblies[index("LeftMotor")] = 0.025;
    input.assemblies[index("RightMotor")] = 0.025;
    for (soluble, tissue, soluble_enzyme, tissue_enzyme, should_swim) in [
        (0.0, 3.14, 0.04, 0.0, true),
        (0.0, 3.14, 0.04, 0.04, false),
        (3.14, 0.0, 0.0, 0.04, true),
        (3.14, 0.0, 0.04, 0.04, false),
    ] {
        input.gut_exudate_energy = soluble;
        input.gut_tissue_energy = tissue;
        input.assemblies[index("ExudateEnzyme")] = soluble_enzyme;
        input.assemblies[index("TissueEnzyme")] = tissue_enzyme;
        let controls = program.evaluate_cell(&input, &mut Registers::default(), &mut EvalScratch::default()).cellular.unwrap();
        assert_eq!(controls.activity[index("LeftMotor")] + controls.activity[index("RightMotor")] > 0.0, should_swim);
        assert!(controls.activity[index("Capture")] > 0.0);
    }
    // An undigestible load still physically occupies and closes a full gut.
    input.gut_energy = input.gut_capacity;
    input.gut_fraction = 1.0;
    input.gut_exudate_energy = 0.0;
    input.gut_tissue_energy = input.gut_capacity;
    input.assemblies[index("TissueEnzyme")] = 0.0;
    let controls = program.evaluate_cell(&input, &mut Registers::default(), &mut EvalScratch::default()).cellular.unwrap();
    assert_eq!(controls.activity[index("Capture")], 0.0);
}
