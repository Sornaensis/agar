use super::*;
use super::mechanism_tests::equipped;
use crate::genome::Value;

const BADGE: &str = "assembly Badge: display() { Mount:0.004; Wall:0.006; }";

const TOUCH: &str = r#"
product Hair = filament();
assembly Touch: sense(contact) { Mount:0.005; Detector:0.01; Hair:0.005; at:0; }
rf002 = if sensed.Touch.message_valid then sensed.Touch.message else -100;
"#;

fn genome(source: &str) -> Genome {
    Genome::new(source.into(), Program::compile(source).unwrap(), 0, 0)
}

fn place(cell: &mut Cell, x: f64, y: f64, angle: f64) {
    cell.x = x;
    cell.y = y;
    cell.vx = 0.0;
    cell.vy = 0.0;
    cell.angle = angle;
}

fn seed(world: &mut World, genomes: &[Genome], id: u32, x: f64, y: f64) -> usize {
    let index = world.cells.len();
    world.try_seed_cells_in_registry(id, 1, x, y, genomes).unwrap();
    place(&mut world.cells[index], x, y, 0.0);
    index
}

/// Add a complete paid test sender with its actual recipe, body and genome.
fn sender(world: &mut World, genomes: &mut Vec<Genome>, declarations: &str, x: f64, y: f64, angle: f64) -> usize {
    let (mut batch, mut stock) = equipped(&format!("{BADGE}\n{declarations}"));
    let g = stock.remove(0);
    let mut cell = batch.cells.remove(0);
    g.program.initialize_message(&mut cell.memory);
    cell.id = world.next_id;
    world.next_id += 1;
    cell.genome_id = genomes.len() as u32 + 1;
    place(&mut cell, x, y, angle);
    genomes.push(g);
    world.cells.push(cell);
    world.cells.len()-1
}

fn read_touch(world: &World, genomes: &[Genome], id: u32) -> ReceptorInput {
    let program = &genomes[0].program;
    let recipe = program.cellular().unwrap().assemblies.iter().position(|a| a.name == "Touch").unwrap();
    world.sense_program_in_registry(id, program, genomes).unwrap().receptors[recipe]
}

#[test]
fn message_requires_a_working_receptor_and_its_actual_publishing_contact() {
    let (mut world, mut genomes) = equipped(&format!("{TOUCH} rb010=sensed.Touch.signal_valid;rf010=sensed.Touch.signal;"));
    place(&mut world.cells[0], WIDTH - 7.0, 100.0, 0.0);
    genomes.push(genome("cellular; publish rf001 = 0; signal rf005 = -2.5;"));
    let private = seed(&mut world, &genomes, 2, 7.0, 100.0);
    let hidden = read_touch(&world, &genomes, 1);
    assert!(hidden.valid && !hidden.message_valid && !hidden.signal_valid, "a bare newborn has no display");
    place(&mut world.cells[private], 200.0, 100.0, 0.0);
    let sender = sender(&mut world, &mut genomes, "publish rf001=0; signal rf005=-2.5;", 7.0, 100.0, 0.0);
    assert!(!read_touch(&world, &genomes, 1).message_valid, "the badge faces away");
    world.cells[sender].angle = PI;
    let read = read_touch(&world, &genomes, 1);
    assert!(read.valid && read.message_valid);
    assert_eq!(read.message, 0.0, "an occupied public zero crosses the seam");
    assert!(read.signal_valid);
    assert_eq!(read.signal,-2.5);
    place(&mut world.cells[private], 6.5, 100.0, 0.0);
    let read = read_touch(&world, &genomes, 1);
    assert!(read.valid && !read.message_valid && !read.signal_valid, "the nearer private sender wins contact");
    assert_eq!(read.message, 0.0);
    place(&mut world.cells[private], 200.0, 100.0, 0.0);
    world.fixture_exudate(0.0, 100.0, 1);
    world.food[0].x = 0.0;
    world.food[0].y = 100.0;
    let read = read_touch(&world, &genomes, 1);
    assert!(read.valid && !read.message_valid && !read.signal_valid, "a closer particle cannot relay the other cell's message");
    world.food.clear();
    let wall = world.environment.add_barrier(3.5, 100.0, 30.0, 10.0, PI * 0.5, false);
    assert!(wall > 0);
    assert!(!read_touch(&world, &genomes, 1).message_valid, "contact messages cannot cross walls");
    assert!(!read_touch(&world, &genomes, 1).signal_valid);
    world.environment.remove_barrier(wall);
    place(&mut world.cells[sender], 30.0, 100.0, 0.0);
    assert!(!read_touch(&world, &genomes, 1).message_valid);
    assert!(!read_touch(&world, &genomes, 1).signal_valid);
    place(&mut world.cells[sender], 7.0, 100.0, PI);
    let touch = world.cells[0].body.cellular.as_mut().unwrap().assemblies.iter_mut()
        .find(|assembly| assembly.recipe.name == "Touch").unwrap();
    touch.integrity = 0.0;
    assert_eq!(read_touch(&world, &genomes, 1), ReceptorInput::default());
}

#[test]
fn contact_reads_last_committed_values_across_cell_evaluation_order() {
    let (mut world, mut genomes) = equipped(&format!("{TOUCH}{BADGE}\npublish rf001 = 2; rf001 = rf001 + 1;"));
    // Both paid receivers have the same program and face one another. The
    // earlier cell's write must not be visible to the later cell this tick.
    genomes[0].program.initialize_message(&mut world.cells[0].memory);
    let mut second = world.cells[0].clone();
    second.id = world.next_id;
    world.next_id += 1;
    world.cells.push(second);
    for prior in [2.0, 3.0, 4.0] {
        place(&mut world.cells[0], 600.0, 380.0, 0.0);
        place(&mut world.cells[1], 614.0, 380.0, PI);
        world.step(&mut genomes);
        for cell in &world.cells {
            assert_eq!(cell.memory.value("rf002"), Some(&Value::Scalar(prior)));
            assert_eq!(cell.memory.value("rf001"), Some(&Value::Scalar(prior + 1.0)));
        }
        world.cells.reverse();
    }
}

#[test]
fn payload_reads_frozen_selected_peer_while_identity_stays_constant() {
    let (mut world, mut genomes) = equipped(&format!("{TOUCH}{BADGE}
publish rf001=17; signal rf009=0;
rf009=rf009+2; rf004=if sensed.Touch.signal_valid then sensed.Touch.signal else -99;"));
    genomes[0].program.initialize_message(&mut world.cells[0].memory);
    let mut second = world.cells[0].clone();
    second.id = world.next_id;
    world.next_id += 1;
    world.cells.push(second);
    for prior in [0.0,2.0,4.0] {
        place(&mut world.cells[0], 600.0,380.0,0.0);
        place(&mut world.cells[1], 614.0,380.0,PI);
        world.step(&mut genomes);
        for cell in &world.cells {
            assert_eq!(cell.memory.value("rf002"),Some(&Value::Scalar(17.0)));
            assert_eq!(cell.memory.value("rf004"),Some(&Value::Scalar(prior)));
            assert_eq!(cell.memory.value("rf009"),Some(&Value::Scalar(prior+2.0)));
        }
        world.cells.reverse();
    }
    // A peer can send this payload without opting into identity matching.
    genomes.push(genome("cellular; signal rf009=0;"));
    let recipient = world.cells[0].id;
    world.cells[1].genome_id = 2;
    world.cells[1].memory = Registers::default();
    genomes[1].program.initialize_message(&mut world.cells[1].memory);
    place(&mut world.cells[0],600.0,380.0,0.0);
    place(&mut world.cells[1],614.0,380.0,PI);
    let input=read_touch(&world,&genomes,recipient);
    assert!(input.valid && input.signal_valid && !input.message_valid);
    assert_eq!(input.signal,0.0);
}

#[test]
fn three_cell_relay_advances_one_physical_contact_per_tick() {
    // B and C each have a paid receptor on their left membrane. They forward
    // its scalar while retaining their own independent identity of 17.
    let (mut world, mut genomes) = equipped(&format!("{TOUCH}{BADGE}
publish rf001=17; signal rf009=0;
assembly Back: display() {{ Mount:0.004; Wall:0.006; at:3.141592653589793; }}
rf009=if sensed.Touch.signal_valid then sensed.Touch.signal else -99;"));
    let relay_id = world.cells[0].id;
    genomes[0].program.initialize_message(&mut world.cells[0].memory);
    let mut last = world.cells[0].clone();
    last.id = world.next_id;
    let last_id = last.id;
    world.next_id += 1;
    world.cells.push(last);
    let source = sender(&mut world, &mut genomes, "publish rf001=91; signal rf011=0; rf011=7;", 586.0,380.0,0.0);
    let source_id = world.cells[source].id;
    // The pulse starts as an ordinary A register write at tick 1. B samples
    // that change at tick 2, and C can first sample B's forwarded value at 3.
    for (relay, downstream) in [(0.0,0.0),(7.0,0.0),(7.0,7.0)] {
        for cell in &mut world.cells {
            let x = if cell.id == source_id { 586.0 } else if cell.id == relay_id { 600.0 } else { 614.0 };
            place(cell,x,380.0,if cell.id == source_id {0.0} else {PI});
        }
        world.step(&mut genomes);
        let by_id = |id| world.cells.iter().find(|cell| cell.id == id).unwrap();
        assert_eq!(genomes[1].program.contact_signal(&by_id(source_id).memory),Some(7.0));
        assert_eq!(genomes[0].program.contact_signal(&by_id(relay_id).memory),Some(relay));
        assert_eq!(genomes[0].program.contact_signal(&by_id(last_id).memory),Some(downstream));
        assert_eq!(by_id(relay_id).memory.value("rf002"),Some(&Value::Scalar(91.0)));
        assert_eq!(by_id(last_id).memory.value("rf002"),Some(&Value::Scalar(17.0)));
        world.cells.reverse();
    }
}

#[test]
fn founders_and_new_daughters_initialize_only_the_authored_public_slot() {
    let (mut dividing, mut genomes) = super::predation_tests::paid_fission_fixture(1.0);
    genomes[0] = genome(&format!("{}\npublish rf007 = 17; rf007 = 91; rb009 = true;", genomes[0].source));
    let mut founders = World::empty(11);
    founders.try_reset_in_registry(11, 1, &genomes).unwrap();
    assert_eq!(founders.cells.len(), 22);
    for founder in &founders.cells {
        assert_eq!(founder.memory.value("rf007"), Some(&Value::Scalar(17.0)));
        assert_eq!(founder.memory.populated_count(), 1);
    }
    let first = &mut founders.cells[0];
    genomes[0].program.evaluate_cell(&Inputs::default(), &mut first.memory, &mut EvalScratch::default());
    founders.try_seed_cells_in_registry(1, 1, 20.0, 20.0, &genomes).unwrap();
    assert_eq!(founders.cells[0].memory.value("rf007"), Some(&Value::Scalar(91.0)));
    assert_eq!(founders.cells.last().unwrap().memory.value("rf007"), Some(&Value::Scalar(17.0)));
    assert!(founders.try_reset_in_registry(12, 0, &genomes).is_err());
    assert_eq!(founders.cells.len(), 23);

    genomes[0].program.initialize_message(&mut dividing.cells[0].memory);
    dividing.step(&mut genomes);
    assert_eq!(dividing.births, 1);
    let parent = &dividing.cells[0];
    let daughter = &dividing.cells[1];
    assert_eq!(parent.memory.value("rf007"), Some(&Value::Scalar(91.0)));
    assert_eq!(parent.memory.value("rb009"), Some(&Value::Boolean(true)));
    assert_eq!(daughter.memory.value("rf007"), Some(&Value::Scalar(17.0)));
    assert_eq!(daughter.memory.populated_count(), 1);
}

#[test]
fn primitive_and_legacy_piercers_exclude_only_matching_reachable_victims() {
    let weapons = r#"
product Barb = filament();
assembly Sting: pierce() { engage_when: !(populated(rf001) && peer.message_valid && peer.message == rf001); Mount:0.01; Barb:0.04; Catalyst:0.01; at:0; direction:0; }
assembly Old: piercer { engage_when: !(populated(rf001) && peer.message_valid && peer.message == rf001); Mount:0.01; Barb:0.04; Catalyst:0.01; }
publish rf001 = 17;
"#;
    for (marked, reader) in [(true, true), (false, true), (true, false)] {
        let (mut world, mut genomes) = equipped(&format!("{TOUCH}{weapons}"));
        if !reader {
            world.cells[0].body.cellular.as_mut().unwrap().assemblies.iter_mut()
                .find(|part| part.recipe.name == "Touch").unwrap().integrity = 0.0;
        }
        genomes[0].program.initialize_message(&mut world.cells[0].memory);
        genomes.push(genome("cellular; rf001 = 17;"));
        place(&mut world.cells[0], WIDTH-7.0, 100.0, 0.0);
        let closer = sender(&mut world, &mut genomes, if marked { "publish rf001=17;" } else { "publish rf001=23;" }, 7.0, 100.0, PI);
        let prey = seed(&mut world, &genomes, 2, 7.0, 104.0);
        let caps: Vec<_> = world.cells.iter().map(|cell| cell.body.capacities()).collect();
        let mut workspace = PhaseWorkspace::default();
        world.freeze_contact_signals(&genomes, &mut workspace.signals);
        workspace.geometry.rebuild(&world.cells, false, true);
        let mut outputs = vec![Outputs::default(); 3];
        outputs[0].bite = 1.0;
        world.resolve_predation_in(&outputs,&caps,&genomes,&mut workspace);
        let victim = if marked && reader { prey } else { closer };
        let untouched = if marked && reader { closer } else { prey };
        assert_eq!(workspace.targets[0], Some(victim), "whole-cell weapon");
        assert_eq!(workspace.primitive_hits[0].1, victim, "mounted weapon");
        assert_eq!(world.attacks, 2);
        assert_eq!(world.cells[untouched].integrity, 1.0);
        assert!(world.cells[victim].integrity < 1.0);
    }
}

#[test]
fn engulf_uses_actual_victim_prior_message_even_after_an_authorized_change() {
    let (mut world, mut genomes) = equipped(&format!("{TOUCH}{}",r#"
assembly Gut: store(food) { Wall:0.2; }
assembly Mouth: engulf() { engage_when: !(populated(rf001) && peer.message_valid && peer.message == rf001); Mount:0.03; Rotor:0.03; Wall:0.04; }
publish rf001 = 0; rf001=3;
"#));
    genomes[0].program.initialize_message(&mut world.cells[0].memory);
    world.cells[0].body.area = 2.0;
    place(&mut world.cells[0], 600.0, 380.0, 0.0);
    let edge = 600.0 + world.cells[0].radius() + CELL_RADIUS;
    let marked = sender(&mut world, &mut genomes, "publish rf001=0; rf001=3;", edge, 380.0, PI);
    let prey = sender(&mut world, &mut genomes, "publish rf001=3;", edge, 382.0, PI);
    for i in [marked, prey] {
        // Both candidate meals fit the paid gut, including remaining ATP.
        let cell = &mut world.cells[i];
        cell.integrity = 0.2;
        cell.energy = 5.0;
        cell.organic_carbon = 0.25;
    }
    let caps: Vec<_> = world.cells.iter().map(|cell| cell.body.capacities()).collect();
    let mut workspace = PhaseWorkspace::default();
    world.freeze_contact_signals(&genomes, &mut workspace.signals);
    genomes[0].program.evaluate_cell(&Inputs::default(), &mut world.cells[0].memory, &mut EvalScratch::default());
    assert_eq!(genomes[0].program.published_message(&world.cells[0].memory), Some(3.0));
    genomes[1].program.evaluate_cell(&Inputs::default(), &mut world.cells[marked].memory, &mut EvalScratch::default());
    assert_eq!(genomes[1].program.published_message(&world.cells[marked].memory), Some(3.0));
    workspace.geometry.rebuild(&world.cells, false, true);
    let mut outputs = vec![Outputs::default(); 3];
    outputs[0].engulf = true;
    world.resolve_predation_in(&outputs,&caps,&genomes,&mut workspace);
    assert_eq!(world.predations, 1);
    assert!(world.cells[marked].alive());
    assert!(workspace.consumed[prey]);
}

/// Let the stock founders build their own surface hardware before controlling only
/// the encounter geometry. Restore the same paid bodies for each angle.
fn grown_contact_founders(counted: bool) -> (World, Vec<Genome>) {
    let mut genomes = vec![genome(include_str!("../../../examples/predator.cell")),
        genome(include_str!("../../../examples/forager.cell")),
        genome(include_str!("../../../examples/phototroph.cell"))];
    let mut world = World::empty(201);
    world.environment.configure(0.45, 0.95, 0.0);
    world.environment.set_initial_nutrients(2.0);
    world.reset_empty(201);
    world.set_count_chemistry(counted).unwrap();
    world.set_mutation_rate(0.0);
    for g in 1..=3 {
        seed(&mut world, &genomes, g, 200.0 * f64::from(g), 380.0);
    }
    let ready = |cell: &Cell, name: &str| cell.body.cellular.as_ref().is_some_and(|body|
        body.assemblies.iter().any(|a| a.recipe.name == name && a.complete() && a.working_mass() > 0.0));
    for _ in 0..2700 {
        world.step(&mut genomes);
        assert_eq!(world.cells.len(), 3, "Founders mature apart before their first encounter");
        if world.cells.iter().all(|c| ready(c, "Badge"))
            && ready(&world.cells[0], "Touch") && ready(&world.cells[0], "Piercer") {
            return (world, genomes);
        }
    }
    panic!("Stock founders must grow a readable display and functional attack apparatus");
}

fn face_target(world: &mut World, hunter: &Cell, target: &Cell, angle: f64) {
    let x = 600.0 + hunter.radius() + target.radius();
    world.cells = vec![hunter.clone(), target.clone()];
    if world.cells[0].id == world.cells[1].id {
        world.cells[1].id = world.next_id;
        world.next_id += 1;
    }
    place(&mut world.cells[0], 600.0, 380.0, 0.0);
    place(&mut world.cells[1], x, 380.0, angle);
}

/// Sample an inherited physical display with a separate cell's ordinary contact
/// receptor. Keep the target's body, angle, membrane mounts and memory intact.
fn external_badge_read(target: &Cell, emitter: &Genome) -> ReceptorInput {
    let (mut probe, mut genomes) = equipped(TOUCH);
    let display = target.body.cellular.as_ref().unwrap().assemblies.iter().find(|a|
        a.recipe.kind == AssemblyKind::Mechanism(MechanismKind::Display)
            && a.working_mass() * a.activity > 0.0).expect("Each half inherits a working display");
    let bearing = target.angle + display.at();
    let distance = target.radius() + probe.cells[0].radius();
    place(&mut probe.cells[0], target.x + distance * bearing.cos(),
        target.y + distance * bearing.sin(), bearing + PI);
    let mut emitter_cell = target.clone();
    emitter_cell.id = probe.next_id;
    emitter_cell.genome_id = 2;
    probe.cells.push(emitter_cell);
    genomes.push(emitter.clone());
    let read = read_touch(&probe, &genomes, 1);
    assert!(read.valid, "The independent probe must physically touch the inherited display");
    read
}

/// Run the same 15-degree heading sweep against a naturally grown adult.
pub(super) fn mature_target_sweep(target: &Cell) {
    let (mut world, genomes) = grown_contact_founders(target.counted.is_some());
    let hunter = world.cells[0].clone();
    let mut visible = 0;
    let mut injured = 0;
    for angle in (0..360).step_by(15) {
        face_target(&mut world, &hunter, target, (angle as f64).to_radians());
        let seen = read_touch(&world, &genomes, hunter.id);
        assert!(seen.valid);
        let presented = seen.message_valid || seen.signal_valid;
        if presented { visible += 1; }
        // Resolve a single front-weapon contact on the measured geometry,
        // without subsequently moving or dividing the mature test target.
        for a in &mut world.cells[0].body.cellular.as_mut().unwrap().assemblies {
            if a.recipe.kind == AssemblyKind::Mechanism(MechanismKind::Pierce) { a.activity = 1.0; }
        }
        let caps: Vec<_> = world.cells.iter().map(|c| c.body.capacities()).collect();
        let mut phases = PhaseWorkspace::default();
        world.freeze_contact_signals(&genomes, &mut phases.signals);
        phases.geometry.rebuild(&world.cells, false, true);
        let integrity = world.cells[1].integrity;
        world.resolve_predation_in(&[Outputs { bite: 1.0, ..Outputs::default() }, Outputs::default()],
            &caps, &genomes, &mut phases);
        if world.cells[1].integrity < integrity {
            assert!(!presented, "A presented badge protects the fully equipped adult");
            injured += 1;
        }
    }
    assert_eq!(visible, 24, "The mature display ring covers all tested target headings");
    assert_eq!(injured, 0);
}

/// Put the parent's inherited weapon directly against its own temporarily
/// silent daughter. A hungry attacker one tick past the quiet period is the
/// matched positive control. Keep the victim's own weapons inert in this assay.
fn post_split_attack_probe(parent: &Cell, newborn: &Cell, stock: &Genome) -> bool {
    let mut world = World::empty(81);
    world.reset_empty(81);
    world.set_mutation_rate(0.0);
    let mut attacker = parent.clone();
    let weapon = attacker.body.cellular.as_ref().unwrap().assemblies.iter()
        .find(|a| a.recipe.kind == AssemblyKind::Mechanism(MechanismKind::Pierce) && a.working_mass() > 0.0)
        .expect("Parent retained a working piercer");
    let bearing = attacker.angle + weapon.at();
    let mut target = newborn.clone();
    target.memory = Default::default();
    for a in &mut target.body.cellular.as_mut().unwrap().assemblies {
        if a.recipe.kind == AssemblyKind::Mechanism(MechanismKind::Pierce) { a.integrity = 0.0; }
    }
    let distance = attacker.radius() + target.radius();
    place(&mut target, attacker.x + distance * bearing.cos(), attacker.y + distance * bearing.sin(), 0.0);
    attacker.vx = 0.0;
    attacker.vy = 0.0;
    attacker.energy = 20.0;
    attacker.organic_carbon = 0.1;
    attacker.scale_gut(0.0);
    world.cells = vec![attacker, target];
    world.next_id = world.cells.iter().map(|c| c.id).max().unwrap() + 1;
    let mut genomes = vec![stock.clone()];
    let before = world.cells[1].integrity;
    world.step(&mut genomes);
    let attacked = world.attacks > 0;
    assert_eq!(world.cells[1].integrity < before, attacked);
    attacked
}

/// Sample the actual inherited mounts at the cleavage angle. Isolate the two
/// cells at tangency so a nearby tissue particle cannot cover either receptor.
fn seam_displays(parent: &Cell, daughter: &Cell, stock: &Genome) -> [bool; 2] {
    let mut w = World::empty(81);
    w.cells = vec![parent.clone(), daughter.clone()];
    let dx = delta(daughter.x - parent.x, WIDTH);
    let dy = delta(daughter.y - parent.y, HEIGHT);
    let bearing = dy.atan2(dx);
    let distance = parent.radius() + daughter.radius();
    place(&mut w.cells[1], parent.x + distance * bearing.cos(), parent.y + distance * bearing.sin(), daughter.angle);
    let registry = std::slice::from_ref(stock);
    std::array::from_fn(|i| {
        let inputs = w.sense_program_in_registry(w.cells[i].id, &stock.program, registry).unwrap();
        stock.program.cellular().unwrap().assemblies.iter().zip(&inputs.receptors)
            .any(|(a, r)| a.kind == AssemblyKind::Mechanism(MechanismKind::SenseContact)
                && r.valid && r.message_valid && r.message == 17.0 && r.signal_valid && r.signal == -2.5)
    })
}

/// Both products must inherit functioning displays, independently readable as
/// soon as their own registers hold the tags. Check this on two successive cuts.
pub(super) fn check_newborn_recognition(world: &mut World, genomes: &mut Vec<Genome>, check_weapons: bool) {
    let newborn = world.cells.iter().find(|c| c.id == world.next_id - 1).unwrap().clone();
    let parent_row = world.cells.iter().position(|c| c.id == newborn.parent_id).unwrap();
    let child_row = world.cells.iter().position(|c| c.id == newborn.id).unwrap();
    assert!(!world.division_interface(newborn.parent_id, newborn.id), "Stock relatives are unbonded");
    let attacks_before = world.attacks;
    for after_commit in [false, true] {
        if after_commit { world.step(genomes); }
        let parent = &world.cells[parent_row];
        let child = &world.cells[child_row];
        for (cell, readable) in [(parent, true), (child, after_commit)] {
            let seen = external_badge_read(cell, &genomes[0]);
            assert_eq!((seen.message_valid, seen.signal_valid), (readable, readable));
            if readable { assert_eq!((seen.message, seen.signal), (17.0, -2.5)); }
        }
        assert_eq!(seam_displays(parent, child, &genomes[0]), [after_commit, true],
            "Each inherited reader sees its partner as soon as those tags have been written");
        if check_weapons {
            assert_eq!(post_split_attack_probe(parent, &newborn, &genomes[0]), after_commit,
                "At the same weapon and contact, the parent waits precisely through the blank tick");
        }
    }
    assert_eq!(world.attacks, attacks_before);
}

#[test]
fn shipped_predator_spares_readable_kin_and_strikes_foreign_or_unmarked_prey() {
    let (mut world, mut genomes) = grown_contact_founders(true);
    let founders = world.cells.clone();
    let mut identified = 0;
    let mut attacked_headings = 0;
    for bearing in (0..360).step_by(15) {
        face_target(&mut world, &founders[0], &founders[0], (bearing as f64).to_radians());
        let seen = read_touch(&world, &genomes, founders[0].id);
        assert!(seen.valid, "All angles must produce physical contact");
        if seen.message_valid {
            identified += 1;
            assert_eq!(seen.message, 17.0);
        }
        let before = world.attacks;
        world.step(&mut genomes);
        if world.attacks > before { attacked_headings += 1; }
        if seen.message_valid || seen.signal_valid {
            assert_eq!(world.attacks, before, "A readable tag must protect kin at {bearing} degrees");
        }
    }
    assert_eq!(identified, 24, "The paid display ring covers each tested target heading");
    assert_eq!(attacked_headings, 0);

    for prey in &founders[1..] {
        for angle in [PI, 0.0] {
            face_target(&mut world, &founders[0], prey, angle);
            let seen = read_touch(&world, &genomes, founders[0].id);
            assert_eq!(seen.message_valid, angle == PI);
            if seen.message_valid {
                assert_ne!(seen.message, 17.0);
            }
            let attacks = world.attacks;
            let integrity = world.cells[1].integrity;
            world.step(&mut genomes);
            assert_eq!(world.attacks - attacks, 1, "Foreign or unreadable prey must remain huntable");
            assert!(world.cells[1].integrity < integrity);
        }
    }

    // Single-register mutations (or deliberate mimics) with either matching tag
    // still elicit kin recognition from this one physical display.
    for (marker, signal) in [(29.0, -2.5), (17.0, 3.0)] {
        face_target(&mut world, &founders[0], &founders[0], PI);
        let writes = genome(&format!("cellular; rf001 = {marker}; rf002 = {signal};"));
        writes.program.evaluate_cell(&Inputs::default(), &mut world.cells[1].memory, &mut EvalScratch::default());
        let seen = read_touch(&world, &genomes, founders[0].id);
        assert!(seen.message_valid && seen.signal_valid);
        assert_eq!((seen.message, seen.signal), (marker, signal));
        let before = world.attacks;
        world.step(&mut genomes);
        assert_eq!(world.attacks, before, "Either shared tag protects against a strike");
        assert!(world.cells[0].memory.value("rf004").is_some(), "The encounter updates kin memory");
    }
    // An attacker without its own comparison values waits. A mature hunter can
    // still attack when the target has a broken display or empty registers.
    for missing in ["own marker", "own payload", "target display", "target register"] {
        face_target(&mut world, &founders[0], &founders[1], PI);
        match missing {
            "own marker" | "own payload" => {
                let register = if missing == "own marker" { "rf001" } else { "rf002" };
                genome(&format!("cellular; clear {register};")).program.evaluate_cell(&Inputs::default(),
                    &mut world.cells[0].memory, &mut EvalScratch::default());
            }
            "target register" => world.cells[1].memory = Default::default(),
            _ => for a in &mut world.cells[1].body.cellular.as_mut().unwrap().assemblies {
                if a.recipe.kind == AssemblyKind::Mechanism(MechanismKind::Display) { a.integrity = 0.0; }
            },
        }
        let attacks = world.attacks;
        world.step(&mut genomes);
        let damaged = missing.starts_with("target");
        assert_eq!(world.attacks - attacks, u64::from(damaged), "Missing {missing}");
    }
}

#[test]
fn shipped_predator_rations_identified_prey_after_recognizing_kin_until_fuel_is_needed() {
    let (mut world, mut genomes) = grown_contact_founders(true);
    let founders = world.cells.clone();
    face_target(&mut world, &founders[0], &founders[0], PI);
    world.step(&mut genomes);
    assert!(world.cells[0].memory.value("rf004").is_some(), "First contact committed the kin memory");
    let recognized = world.cells[0].clone();
    for hungry in [false, true] {
        face_target(&mut world, &recognized, &founders[1], PI);
        let cap = world.cells[0].body.capacities().energy;
        world.set_cell_stores(recognized.id, Some(if hungry { 5.0 } else { cap }), None).unwrap();
        world.cells[0].counted.as_mut().unwrap().carbon = if hungry { 1000 } else { 20000 };
        world.cells[0].project_counts();
        let before = world.attacks;
        world.step(&mut genomes);
        assert_eq!(world.attacks - before, u64::from(hungry));
    }
}

#[test]
fn shipped_predator_rear_piercer_and_engulfer_spare_matching_tags_and_accept_silent_prey() {
    // Isolate both rear effectors with paid, recipe-matched hardware at the
    // stock angles. Use a large paid gut so every wounded experimental target
    // is small enough to swallow. Only identity, heading and injury vary.
    let stock = include_str!("../../../examples/predator.cell");
    let foreign = include_str!("../../../examples/phototroph.cell");
    for swallow in [false, true] {
        for (target_source, angle, clonal_interface, accepts) in [
            (stock, 0.0, false, false),
            (stock, PI / 2.0, false, true),
            (foreign, 0.0, false, true),
            (foreign, PI / 2.0, false, true),
            (foreign, 0.0, true, false),
        ] {
            let (mut world, _) = equipped(r#"
product Tail = filament();
assembly LargeGut:store(food) { Wall:0.2; }
assembly TailTouch:sense(contact) { Mount:0.002; Detector:0.006; Tail:0.002; at:3.141592653589793; }
assembly Engulfment:engulf() { Mount:0.005; Rotor:0.01; Wall:0.01; at:3.141592653589793; }
assembly TailPiercer:pierce() { Mount:0.0055; Tail:0.0145; Catalyst:0.005; at:3.141592653589793; direction:3.141593; }
"#);
            let genomes = [genome(stock), genome(target_source)];
            world.cells[0].body.area = 2.0;
            place(&mut world.cells[0], 600.0, 380.0, 0.0);
            let mut target = equipped(BADGE).0.cells.remove(0);
            target.id = world.next_id;
            world.next_id += 1;
            target.genome_id = 2;
            target.integrity = if swallow { 0.2 } else { 1.0 };
            target.energy = 5.0;
            target.organic_carbon = 0.25;
            let x = 600.0 - world.cells[0].radius() - target.radius();
            place(&mut target, x, 380.0, angle);
            world.cells.push(target);
            for (g, cell) in genomes.iter().zip(&mut world.cells) {
                g.program.evaluate_cell(&Inputs::default(), &mut cell.memory, &mut EvalScratch::default());
            }
            if clonal_interface {
                world.contacts.insert((world.cells[0].id, world.cells[1].id), adhesion::CLONAL);
            }
            let capacities: Vec<_> = world.cells.iter().map(|c| c.body.capacities()).collect();
            let mut phases = PhaseWorkspace::default();
            world.freeze_contact_signals(&genomes, &mut phases.signals);
            phases.geometry.rebuild(&world.cells, false, true);
            let kin = target_source == stock;
            let info = format!("engulf {swallow}, kin {kin}, heading {angle}, division interface {clonal_interface}");
            let controls = [Outputs { engulf: swallow, bite: if swallow { 0.0 } else { 1.0 }, ..Outputs::default() }, Outputs::default()];
            world.resolve_predation_in(&controls, &capacities, &genomes, &mut phases);
            assert_eq!(world.attacks > 0, !swallow && accepts, "{info}");
            assert_eq!(phases.consumed[1], swallow && accepts, "{info}");
            assert_eq!(world.predations, u64::from(swallow && accepts), "{info}");
        }
    }
}

const PORT: &str = r#"
product GlueMaterial = adhesin(Frame);
assembly Glue: adhere() { engage_when: true; GlueMaterial:0.006; Mount:0.01; Wall:0.01; }
assembly RearGlue: adhere() { at:3.14159265; engage_when: true; GlueMaterial:0.006; Mount:0.01; Wall:0.01; }
assembly Bus: junction(signal) { Detector:0.01; Wall:0.01; }
assembly Rear: junction(signal) { at:3.14159265; Detector:0.01; Wall:0.01; }
signal rf003=0;
rb005=sensed.Bus.valid; rf004=sensed.Bus.value; rb007=sensed.Bus.signal_valid;
rb008=sensed.Rear.valid; rf006=sensed.Rear.value;
"#;

fn read_bus(world: &World, genomes: &[Genome], id: u32, name: &str) -> ReceptorInput {
    let cell = world.cells.iter().find(|cell| cell.id == id).unwrap();
    let program = &genomes[(cell.genome_id - 1) as usize].program;
    let index = program.cellular().unwrap().assemblies.iter().position(|recipe| recipe.name == name).unwrap();
    world.sense_program_in_registry(id, program, genomes).unwrap().receptors[index]
}

#[test]
fn signal_ports_report_distinct_local_peers_and_distinguish_silence_from_zero() {
    let (mut world, mut genomes) = equipped(PORT);
    let original = genomes[0].source.clone();
    place(&mut world.cells[0],200.0,200.0,0.0);
    for (x,declaration) in [(214.2,"signal rf003=0;"),(185.8,"signal rf003=-6;")] {
        let mut peer = world.cells[0].clone();
        genomes.push(genome(&original.replace("signal rf003=0;", declaration)));
        peer.id=world.next_id; world.next_id+=1;
        peer.genome_id=genomes.len() as u32;
        genomes.last().unwrap().program.initialize_message(&mut peer.memory);
        place(&mut peer,x,200.0,0.0);
        world.cells.push(peer);
    }
    adhesion::advance_contact_phases(&mut world,&genomes,adhesion::MATURE);
    let sample = |w:&World,name:&str| { let s = read_bus(w,&genomes,1,name); (s.valid,s.signal_valid,s.value) };
    assert_eq!(sample(&world,"Bus"), (true,true,0.0));
    assert_eq!(sample(&world,"Rear"), (true,true,-6.0));
    world.cells[1].memory=Registers::default();
    assert_eq!(sample(&world,"Bus"), (true,false,0.0));
    world.cells[1].body.cellular.as_mut().unwrap().assemblies.iter_mut().find(|a|a.recipe.name=="Rear").unwrap().activity=0.0;
    assert_eq!(sample(&world,"Bus"), (false,false,0.0),"active far-side port cannot serve the contact");
    assert_eq!(sample(&world,"Rear"), (true,true,-6.0));
}

#[test]
fn signal_junction_relays_one_frozen_hop_per_tick() {
    let (mut world,mut genomes)=equipped(&format!("{PORT} rf003=sensed.Rear.value;"));
    let original=genomes[0].source.clone();
    genomes.push(genome(&original.replace("rf003=sensed.Rear.value;","rf003=8;")));
    for i in 0..3 {
        if i>0 { let mut next=world.cells[0].clone();next.id=world.next_id;world.next_id+=1;world.cells.push(next); }
        let cell=&mut world.cells[i];cell.genome_id=if i==0 {2}else{1};
        place(cell,200.0 + i as f64 * 14.2,200.0,0.0);
        genomes[(cell.genome_id-1) as usize].program.initialize_message(&mut cell.memory);
    }
    adhesion::advance_contact_phases(&mut world,&genomes,adhesion::MATURE);
    for expected in [[8.0,0.0,0.0],[8.0,8.0,0.0],[8.0,8.0,8.0]] {
        world.step(&mut genomes);
        for (i,value) in expected.iter().enumerate() {
            let cell=world.cells.iter().find(|cell|cell.id==i as u32 + 1).unwrap();
            assert_eq!(cell.memory.value("rf003"),Some(&Value::Scalar(*value)));
        }
        world.cells.reverse();
    }
}

#[test]
fn touch_reports_only_the_confirmed_bond_to_the_selected_surface() {
    let (mut world, genomes) = equipped(&format!("{TOUCH}{BADGE}
        product GlueMaterial = adhesin(Frame); assembly Glue: adhere() {{ engage_when: peer.message_valid && populated(rf020) && peer.message == rf020; GlueMaterial:0.006; Mount:0.01; Wall:0.01; }}
        publish rf020=41; rb101=sensed.Touch.bonded;"));
    place(&mut world.cells[0],100.0,100.0,0.0);
    genomes[0].program.initialize_message(&mut world.cells[0].memory);
    let mut partner=world.cells[0].clone();
    partner.id=world.next_id; world.next_id+=1;
    partner.x=114.5; partner.angle=PI; world.cells.push(partner);
    let first=read_touch(&world,&genomes,1);
    assert!(first.valid && first.message_valid && !first.bonded);
    adhesion::advance_contact_phases(&mut world,&genomes,adhesion::MATURE);
    assert!(read_touch(&world,&genomes,1).bonded);
    // A closer newly arrived cell has the very same public identity.
    let mut stranger=world.cells[1].clone();
    stranger.id=world.next_id; world.next_id+=1;
    stranger.x=114.0; stranger.adhered_neighbors=0; world.cells.push(stranger);
    let read=read_touch(&world,&genomes,1);
    assert!(read.valid && read.message_valid && !read.bonded);
    assert_eq!(read.message,41.0);
}

#[test]
fn installed_piercers_apply_separate_predicates_to_each_reachable_surface() {
    let (mut world, mut genomes) = equipped(&format!("{TOUCH}{}",
        r#"
        product Barb = filament();
        assembly Low: pierce() { Mount:0.01; Barb:0.04; Catalyst:0.01;
            engage_when: !peer.division_interface && peer.signal_valid && peer.signal < 2; }
        assembly High: pierce() { Mount:0.01; Barb:0.04; Catalyst:0.01;
            engage_when: peer.signal_valid && peer.signal >= 2; }
    "#));
    place(&mut world.cells[0], 200.0, 200.0, 0.0);
    let low = sender(&mut world, &mut genomes, "signal rf010=0;", 214.0, 200.0, PI);
    let high = sender(&mut world, &mut genomes, "signal rf010=3;", 214.0, 203.0, (200.0_f64-203.0).atan2(-7.0));
    let caps: Vec<_> = world.cells.iter().map(|cell| cell.body.capacities()).collect();
    let mut ws = PhaseWorkspace::default();
    world.freeze_contact_signals(&genomes, &mut ws.signals);
    ws.geometry.rebuild(&world.cells, false, true);
    let mut outputs = vec![Outputs::default(); 3];
    outputs[0].bite = 1.0;
    world.resolve_predation_in(&outputs, &caps, &genomes, &mut ws);
    assert_eq!(ws.primitive_hits.iter().map(|hit| hit.1).collect::<Vec<_>>(), [low, high]);
    assert_eq!(world.attacks, 2);
    let a = world.cells[0].id;
    let b = world.cells[low].id;
    world.contacts.insert((a.min(b), a.max(b)), adhesion::CLONAL);
    world.resolve_predation_in(&outputs, &caps, &genomes, &mut ws);
    assert_eq!(ws.primitive_hits.iter().map(|hit| hit.1).collect::<Vec<_>>(), [high],
        "a retained division interface is visible only on that physical pair");

}

#[test]
fn independently_gated_engulfers_contribute_only_their_accepted_capacity() {
    let (mut world, mut genomes) = equipped(&format!("{TOUCH}{}",
        r#"
        assembly Gut: store(food) { Wall:0.20; }
        assembly Small: engulf() { Mount:0.008; Rotor:0.008; Wall:0.009; engage_when: peer.signal_valid; }
        assembly Large: engulf() { Mount:0.03; Rotor:0.03; Wall:0.04; engage_when: peer.signal > 2; }
    "#));
    world.cells[0].body.area = 2.0;
    let x = world.cells[0].x + world.cells[0].radius() + CELL_RADIUS;
    let y = world.cells[0].y;
    let target = sender(&mut world, &mut genomes, "signal rf003=1;", x, y, PI);
    world.cells[target].body.area = 1.1;
    world.cells[target].integrity = 0.2;
    world.cells[target].organic_carbon = 0.25;
    world.cells[target].energy = 5.0;
    let caps: Vec<_> = world.cells.iter().map(|c| c.body.capacities()).collect();
    let mut outputs = vec![Outputs::default(); 2];
    outputs[0].engulf = true;
    let mut ws = PhaseWorkspace::default();
    world.freeze_contact_signals(&genomes, &mut ws.signals);
    ws.geometry.rebuild(&world.cells, false, true);
    world.resolve_predation_in(&outputs, &caps, &genomes, &mut ws);
    assert_eq!(ws.engulf_targets[0], None);
    assert_eq!(world.predations, 0, "only the 0.025-mass surface accepts this partner");
    ws.signals[target].signal = Some(3.0);
    world.resolve_predation_in(&outputs, &caps, &genomes, &mut ws);
    assert_eq!(world.predations, 1, "the larger mouth now admits the same target");
}

