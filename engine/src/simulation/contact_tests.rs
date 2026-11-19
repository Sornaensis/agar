use super::*;
use super::mechanism_tests::equipped;
use crate::genome::Value;

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
    genomes.push(genome("cellular; rf001 = 19;"));
    let sender = seed(&mut world, &genomes, 2, 7.0, 100.0);
    let private = seed(&mut world, &genomes, 3, 200.0, 100.0);
    genomes[2].program.evaluate_cell(&Inputs::default(), &mut world.cells[private].memory, &mut EvalScratch::default());
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
    place(&mut world.cells[sender], 7.0, 100.0, 0.0);
    let touch = world.cells[0].body.cellular.as_mut().unwrap().assemblies.iter_mut()
        .find(|assembly| assembly.recipe.name == "Touch").unwrap();
    touch.integrity = 0.0;
    assert_eq!(read_touch(&world, &genomes, 1), ReceptorInput::default());
}

#[test]
fn contact_reads_last_committed_values_across_cell_evaluation_order() {
    let (mut world, mut genomes) = equipped(&format!("{TOUCH}\npublish rf001 = 2; rf001 = rf001 + 1;"));
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
    let (mut world, mut genomes) = equipped(&format!("{TOUCH}
publish rf001=17; signal rf009=0; avoid_message rf001;
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
    let (mut world, mut genomes) = equipped(&format!("{TOUCH}
publish rf001=17; signal rf009=0;
rf009=if sensed.Touch.signal_valid then sensed.Touch.signal else -99;"));
    let relay_id = world.cells[0].id;
    genomes[0].program.initialize_message(&mut world.cells[0].memory);
    let mut last = world.cells[0].clone();
    last.id = world.next_id;
    let last_id = last.id;
    world.next_id += 1;
    world.cells.push(last);
    genomes.push(genome("cellular; publish rf001=91; signal rf011=0; rf011=7;"));
    let source = seed(&mut world, &genomes, 2, 586.0,380.0);
    let source_id = world.cells[source].id;
    // The pulse starts as an ordinary A register write at tick 1. B samples
    // that change at tick 2, and C can first sample B's forwarded value at 3.
    for (relay, downstream) in [(0.0,0.0),(7.0,0.0),(7.0,7.0)] {
        for cell in &mut world.cells {
            let x = if cell.id == source_id { 586.0 } else if cell.id == relay_id { 600.0 } else { 614.0 };
            place(cell,x,380.0,PI);
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
assembly Sting: pierce() { Mount:0.01; Barb:0.04; Catalyst:0.01; at:0; direction:0; }
assembly Old: piercer { Mount:0.01; Barb:0.04; Catalyst:0.01; }
publish rf001 = 17; avoid_message rf001;
"#;
    for marked in [true, false] {
        let (mut world, mut genomes) = equipped(weapons);
        genomes[0].program.initialize_message(&mut world.cells[0].memory);
        genomes.push(genome(if marked { "cellular; publish rf001 = 17;" } else { "cellular; publish rf001 = 23;" }));
        genomes.push(genome("cellular; rf001 = 17;"));
        place(&mut world.cells[0], WIDTH-7.0, 100.0, 0.0);
        let closer = seed(&mut world, &genomes, 2, 7.0, 100.0);
        let prey = seed(&mut world, &genomes, 3, 7.0, 104.0);
        let caps: Vec<_> = world.cells.iter().map(|cell| cell.body.capacities()).collect();
        let mut workspace = PhaseWorkspace::default();
        world.freeze_contact_signals(&genomes, &mut workspace.signals);
        workspace.geometry.rebuild(&world.cells, false, true);
        let mut outputs = vec![Outputs::default(); 3];
        outputs[0].bite = 1.0;
        world.resolve_predation_in(&outputs, &caps, &mut workspace);
        let victim = if marked { prey } else { closer };
        let untouched = if marked { closer } else { prey };
        assert_eq!(workspace.targets[0], Some(victim), "whole-cell weapon");
        assert_eq!(workspace.primitive_hits[0].1, victim, "mounted weapon");
        assert_eq!(world.attacks, 2);
        assert_eq!(world.cells[untouched].integrity, 1.0);
        assert!(world.cells[victim].integrity < 1.0);
    }
}

#[test]
fn engulf_uses_actual_victim_prior_message_even_after_an_authorized_change() {
    let (mut world, mut genomes) = equipped(r#"
assembly Gut: store(food) { Wall:0.2; }
assembly Mouth: engulf() { Mount:0.03; Rotor:0.03; Wall:0.04; }
publish rf001 = 0; avoid_message rf001;
"#);
    genomes[0].program.initialize_message(&mut world.cells[0].memory);
    genomes.push(genome("cellular; publish rf001 = 0; rf001 = 3;"));
    genomes.push(genome("cellular; publish rf001 = 3;"));
    world.cells[0].body.area = 2.0;
    place(&mut world.cells[0], 600.0, 380.0, 0.0);
    let edge = 600.0 + world.cells[0].radius() + CELL_RADIUS;
    let marked = seed(&mut world, &genomes, 2, edge, 380.0);
    let prey = seed(&mut world, &genomes, 3, edge, 382.0);
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
    genomes[1].program.evaluate_cell(&Inputs::default(), &mut world.cells[marked].memory, &mut EvalScratch::default());
    assert_eq!(genomes[1].program.published_message(&world.cells[marked].memory), Some(3.0));
    workspace.geometry.rebuild(&world.cells, false, true);
    let mut outputs = vec![Outputs::default(); 3];
    outputs[0].engulf = true;
    world.resolve_predation_in(&outputs, &caps, &mut workspace);
    assert_eq!(world.predations, 1);
    assert!(world.cells[marked].alive());
    assert!(workspace.consumed[prey]);
}

#[test]
fn shipped_predator_spares_a_newborn_peer_and_can_strike_adjacent_unmarked_prey() {
    // Install only the two paid surface devices needed for this encounter.
    // Geometry, activity expressions, publication and matching use the shipped
    // programs. Three unit-radius bodies start clear of all overlaps.
    let (mut world, _) = equipped(r#"
product Tail = filament();
assembly Touch: sense(contact) { Mount:0.002; Detector:0.006; Tail:0.002; }
assembly Piercer: pierce() { Mount:0.01; Tail:0.03; Catalyst:0.01; }
"#);
    let mut genomes = vec![genome(include_str!("../../../examples/predator.cell")),
        genome(include_str!("../../../examples/forager.cell"))];
    let predator = &genomes[0].program;
    predator.initialize_message(&mut world.cells[0].memory);
    let marker = predator.published_message(&world.cells[0].memory);
    assert!(marker.is_some());
    assert_eq!(marker, predator.avoid_message(&world.cells[0].memory));
    place(&mut world.cells[0], 600.0, 380.0, 0.0);
    let position = |angle: f64| (600.0 + 14.05 * angle.to_radians().cos(), 380.0 + 14.05 * angle.to_radians().sin());
    let (x, y) = position(20.0);
    let peer = seed(&mut world, &genomes, 1, x, y);
    let (x, y) = position(-40.0);
    let prey = seed(&mut world, &genomes, 2, x, y);
    world.step(&mut genomes);
    assert_eq!(world.cells[peer].integrity, 1.0, "the marker protects a first-contact peer");
    assert!(world.cells[prey].integrity < 1.0, "a closer peer must not suppress a reachable prey target");
    assert_eq!(world.attacks, 1);
}

const PORT: &str = r#"
assembly Glue: adhere() { Mount:0.01; Wall:0.01; }
assembly Bus: junction(signal) { Detector:0.01; Wall:0.01; }
publish rf001=83; adhere_message rf001;
signal rf003=0;
rf006=sensed.Bus.count; rb005=sensed.Bus.valid; rf004=sensed.Bus.value;
"#;

fn read_bus(world: &World, genomes: &[Genome], id: u32) -> ReceptorInput {
    let cell = world.cells.iter().find(|cell| cell.id == id).unwrap();
    let program = &genomes[(cell.genome_id - 1) as usize].program;
    let index = program.cellular().unwrap().assemblies.iter().position(|recipe| recipe.name == "Bus").unwrap();
    world.sense_program_in_registry(id, program, genomes).unwrap().receptors[index]
}

#[test]
fn signal_junction_averages_only_attached_paid_publishers_and_accepts_zero() {
    let (mut world, mut genomes) = equipped(PORT);
    let original = genomes[0].source.clone();
    place(&mut world.cells[0], 200.0, 200.0, 0.0);
    genomes[0].program.initialize_message(&mut world.cells[0].memory);
    for (offset, declaration) in [
        ([14.2, 0.0], "signal rf003=0;"),
        ([-14.2, 0.0], "signal rf003=-6;"),
        ([0.0, 14.2], "signal rf003=12;"),
        ([0.0, -14.2], "signal rf003;"),
    ] {
        let mut peer = world.cells[0].clone();
        genomes.push(genome(&original.replace("signal rf003=0;", declaration)));
        peer.id = world.next_id;
        world.next_id += 1;
        peer.genome_id = genomes.len() as u32;
        peer.memory = Registers::default();
        genomes.last().unwrap().program.initialize_message(&mut peer.memory);
        place(&mut peer, 200.0 + offset[0], 200.0 + offset[1], PI);
        world.cells.push(peer);
    }
    let sample = |w:&World| { let read=read_bus(w,&genomes,1); (read.available,read.valid,read.count,read.value) };
    assert_eq!(sample(&world),(true,true,3,2.0)); // Occupied zero contributes; the fourth port is silent.
    let block = world.environment.add_barrier(207.1,200.0,20.0,10.0,PI*0.5,false);
    assert!(block>0);
    assert_eq!(sample(&world),(true,true,2,3.0));
    assert!(world.environment.remove_barrier(block));
    let remote = &mut world.cells[3];
    remote.body.cellular.as_mut().unwrap().assemblies.iter_mut().find(|a|a.recipe.name=="Bus").unwrap().activity=0.0;
    assert_eq!(sample(&world),(true,true,2,-3.0));
    world.cells[2].x=140.0;
    assert_eq!(sample(&world),(true,true,1,0.0));
    world.cells[1].body.cellular.as_mut().unwrap().assemblies.iter_mut().find(|a|a.recipe.name=="Glue").unwrap().integrity=0.0;
    assert_eq!(sample(&world),(true,false,0,0.0));
    world.cells[0].body.cellular.as_mut().unwrap().assemblies.iter_mut().find(|a|a.recipe.name=="Bus").unwrap().activity=0.0;
    assert_eq!(sample(&world),(false,false,0,0.0));
}

#[test]
fn signal_junction_relays_one_frozen_hop_per_tick() {
    let (mut world,mut genomes)=equipped(&format!("{PORT} rf003=sensed.Bus.value;"));
    let original=genomes[0].source.clone();
    genomes.push(genome(&original.replace("rf003=sensed.Bus.value;","rf003=8;")));
    for i in 0..3 {
        if i>0 { let mut next=world.cells[0].clone();next.id=world.next_id;world.next_id+=1;world.cells.push(next); }
        let cell=&mut world.cells[i];cell.genome_id=if i==0 {2}else{1};
        place(cell,200.0 + i as f64 * 14.2,200.0,0.0);
        genomes[(cell.genome_id-1) as usize].program.initialize_message(&mut cell.memory);
    }
    for expected in [[8.0,0.0,0.0],[8.0,4.0,0.0],[8.0,4.0,4.0]] {
        world.step(&mut genomes);
        for (i,value) in expected.iter().enumerate() {
            let cell=world.cells.iter().find(|cell|cell.id==i as u32 + 1).unwrap();
            assert_eq!(cell.memory.value("rf003"),Some(&Value::Scalar(*value)));
        }
        world.cells.reverse();
    }
}
