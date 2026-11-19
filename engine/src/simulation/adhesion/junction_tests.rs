use super::*;
use super::super::mechanism_tests::equipped;
use crate::checkpoint::{self, Reader, Writer};

const CHANNELS: &str = r#"
product Carrier = transporter();
assembly Glue: adhere() { Mount:0.01; Wall:0.01; activity:1; }
assembly PoreE: junction(energy) { Wall:0.01; Carrier:0.01; activity:1; }
assembly PoreM: junction(mineral) { Wall:0.01; Carrier:0.01; activity:1; }
assembly PoreC: junction(carbon) { Wall:0.01; Carrier:0.01; activity:1; }
publish rf020 = 31;
adhere_message rf020;
"#;

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-12, "{a} != {b}");
}

fn stores(c: &Cell) -> [f64; 3] { [c.energy, c.nutrient_stored, c.organic_carbon] }

fn set_fills(c: &mut Cell, fills: [f64; 3]) {
    let cap = c.body.capacities();
    c.energy = fills[0] * cap.energy;
    c.nutrient_stored = fills[1] * cap.minerals;
    c.organic_carbon = fills[2] * cap.energy / CHEMICAL_ENERGY_PER_CARBON;
}

fn colony(positions: &[(f64, f64)]) -> (World, Vec<Genome>) {
    colony_with(positions, CHANNELS)
}

fn colony_with(positions: &[(f64, f64)], source: &str) -> (World, Vec<Genome>) {
    let (mut world, genomes) = equipped(source);
    genomes[0].program.initialize_message(&mut world.cells[0].memory);
    for (i, &(x, y)) in positions.iter().enumerate() {
        if i > 0 {
            // Give each clone its own complete founder budget and apply the same
            // paid construction as the first cell.
            world.seed_cells(1, 1, x, y);
            let id = world.cells[i].id;
            world.cells[i] = world.cells[0].clone();
            world.cells[i].id = id;
        }
        let cell = &mut world.cells[i];
        (cell.x, cell.y) = (x, y);
    }
    (world, genomes)
}

fn phase(world: &mut World, genomes: &[Genome]) {
    let mut ws = PhaseWorkspace::default();
    world.freeze_contact_signals(genomes, &mut ws.signals);
    ws.geometry.rebuild(&world.cells, false, true);
    world.apply_adhesion_in(&mut ws);
}

#[test]
fn matched_junctions_exchange_three_real_stores_across_a_seam() {
    let (mut world, genomes) = colony(&[(WIDTH-7.0,100.0),(7.0,100.0)]);
    set_fills(&mut world.cells[0], [0.9,0.1,0.9]);
    set_fills(&mut world.cells[1], [0.1,0.9,0.1]);
    for assembly in &mut world.cells[1].body.cellular.as_mut().unwrap().assemblies {
        if assembly.recipe.kind == AssemblyKind::Mechanism(MechanismKind::JunctionCarbon) {
            assembly.activity = 0.5;
        }
    }
    let before = [stores(&world.cells[0]), stores(&world.cells[1])];
    phase(&mut world, &genomes);
    let expected = [0.8*0.02*DT*JUNCTION_RATES[0], -0.8*0.02*DT*JUNCTION_RATES[1],
                    0.8*0.01*DT*JUNCTION_RATES[2]];
    for k in 0..3 {
        close(world.cells[1].junction_net[k], expected[k]);
        close(world.cells[0].junction_net[k], -expected[k]);
        close(stores(&world.cells[0])[k], before[0][k] - expected[k]);
        close(stores(&world.cells[1])[k], before[1][k] + expected[k]);
    }
    assert!(world.cells.iter().all(|c| c.adhered_neighbors == 1));
    let body = world.body_json(world.cells[1].id, &genomes).unwrap();
    assert!(body.contains("\"colony\":{\"neighbors\":1,"));
    assert!(body.contains(&format!("\"mineralNet\":{}", world.cells[1].junction_net[1])));
}

#[test]
fn junctions_need_an_active_complete_matching_port_and_reciprocal_open_contact() {
    for condition in ["wall", "private", "separated", "no glue", "inactive", "broken", "incomplete", "absent"] {
        let (mut world, genomes) = colony(&[(WIDTH-7.0,100.0),(7.0,100.0)]);
        set_fills(&mut world.cells[0], [0.9; 3]);
        set_fills(&mut world.cells[1], [0.1; 3]);
        let slots = &mut world.cells[1].body.cellular.as_mut().unwrap().assemblies;
        let e = slots.iter().position(|a| a.recipe.kind == AssemblyKind::Mechanism(MechanismKind::JunctionEnergy)).unwrap();
        match condition {
            "wall" => { assert!(world.environment.add_barrier(0.0,100.0,30.0,10.0,PI*0.5,false)>0); }
            "private" => world.cells[1].memory = Registers::default(),
            "separated" => world.cells[1].x += 4.0,
            "no glue" => slots.retain(|a| a.recipe.kind != AssemblyKind::Mechanism(MechanismKind::Adhere)),
            "inactive" => slots[e].activity = 0.0,
            "broken" => slots[e].integrity = 0.0,
            "incomplete" => slots[e].ingredients[0].mass *= 0.5,
            "absent" => { slots.remove(e); }
            _ => unreachable!(),
        }
        let before = world.cells[1].energy;
        phase(&mut world, &genomes);
        assert_eq!(world.cells[1].energy, before, "{condition}");
        assert_eq!(world.cells[1].junction_net[0], 0.0, "{condition}");
        let other_ports_open = matches!(condition, "inactive" | "broken" | "incomplete" | "absent");
        assert_eq!(world.cells[1].adhered_neighbors, u32::from(other_ports_open), "{condition}");
        assert_eq!(world.cells[1].junction_net[2] > 0.0, other_ports_open, "{condition}");
    }
}

#[test]
fn empty_middle_cell_cannot_relay_new_carbon_until_the_next_tick() {
    let (mut world, genomes) = colony(&[(100.0,100.0),(114.0,100.0),(128.0,100.0)]);
    for (i, c) in world.cells.iter_mut().enumerate() {
        set_fills(c, [0.5,0.5,if i==0 {1.0} else {0.0}]);
    }
    let before = world.cells[0].organic_carbon;
    phase(&mut world, &genomes);
    assert_eq!(world.cells.iter().map(|c| c.adhered_neighbors).collect::<Vec<_>>(), [1,2,1]);
    assert!(world.cells[1].organic_carbon > 0.0);
    assert_eq!(world.cells[2].organic_carbon, 0.0);
    phase(&mut world, &genomes);
    assert!(world.cells[2].organic_carbon > 0.0);
    close(world.cells.iter().map(|c| c.organic_carbon).sum(), before);
    world.cells[2].x = 200.0;
    phase(&mut world, &genomes);
    assert_eq!((world.cells[2].adhered_neighbors, world.cells[2].junction_net), (0, [0.0; 3]));
}

#[test]
fn several_neighbors_share_one_ports_total_throughput_in_stable_id_order() {
    // Four cells surround a central recipient. Only the middle has four contacts.
    let positions = [(100.0,100.0),(86.0,100.0),(114.0,100.0),(100.0,86.0),(100.0,114.0)];
    let run = |reverse:bool, outward:bool| {
        let (mut world, genomes) = colony(&positions);
        for (i, cell) in world.cells.iter_mut().enumerate() {
            let high = (i!=0) ^ outward;
            set_fills(cell, [0.5,0.5,if high {1.0} else {0.0}]);
        }
        if reverse { world.cells.reverse(); }
        phase(&mut world,&genomes);
        world.cells.sort_unstable_by_key(|c|c.id);
        let direction = if outward {-1.0} else {1.0};
        let max = JUNCTION_RATES[2]*0.02*DT;
        assert_eq!(world.cells[0].adhered_neighbors,4);
        close(world.cells[0].junction_net[2],direction*max);
        for c in &world.cells[1..] {
            assert_eq!(c.adhered_neighbors,1);
            close(c.junction_net[2], -direction*max/4.0);
        }
        world.cells.iter().map(|c|(stores(c),c.junction_net)).collect::<Vec<_>>()
    };
    for outward in [false,true] { assert_eq!(run(false,outward),run(true,outward)); }
}

#[test]
fn junction_planning_equalizes_fill_and_obeys_scarce_stock_and_headroom() {
    // Different sized stores at 50% fill give no exchange.
    assert_eq!(Store::new(0.5,1.0).plan(&Store::new(2.0,4.0),100.0),0.0);
    // One fast channel cannot invert the two concentrations.
    close(Store::new(1.0,1.0).plan(&Store::new(0.0,3.0),100.0),0.75);
    // Endpoints with no physical space cannot participate.
    assert_eq!(Store::new(0.0,0.0).plan(&Store::new(3.0,4.0),100.0),0.0);
    let mut store = Store::new(0.2,0.5);
    store.requested_in=6.0;
    store.requested_out=2.0;
    store.set_scales(100.0);
    close(store.scale_in*6.0,0.3);
    close(store.scale_out*2.0,0.2);
    store.set_scales(0.08);
    close(6.0*store.scale_in+2.0*store.scale_out,0.08);
}

#[test]
fn exchanged_junction_state_roundtrips_and_v10_rejects_new_body_tags() {
    let (mut world, genomes) = colony(&[(100.0,100.0),(114.0,100.0)]);
    // Spill after replacing the universal core so the fixture respects its new storage capacities.
    let mut phases = PhaseWorkspace::default();
    world.spill_excess_stores_in(&mut phases,true);
    // Create a gradient using a paid ATP expenditure.
    world.cells[1].energy *= 0.6;
    phase(&mut world,&genomes);
    assert!(world.cells[1].junction_net[0] > 0.0);
    let mut writer=Writer::new();
    world.cells[0].body.write_checkpoint(&mut writer);
    assert!(Body::read_checkpoint_version(&mut Reader::new(&writer.0),10).is_err());
    assert!(Body::read_checkpoint_version(&mut Reader::new(&writer.0),11).is_ok());

    let mut engine=crate::Engine::new();
    engine.genomes=genomes;
    engine.world=world;
    let bytes=checkpoint::save(&engine).unwrap();
    let mut restored=checkpoint::restore(&bytes).unwrap();
    assert_eq!(checkpoint::save(&restored).unwrap(),bytes);
    let mut disconnected = checkpoint::restore(&bytes).unwrap();
    for cell in &mut disconnected.world.cells { cell.memory = Registers::default(); }
    phase(&mut disconnected.world, &disconnected.genomes);
    assert!(!disconnected.world.had_adhesive_contacts);
    assert!(disconnected.world.cells.iter().all(|c| c.adhered_neighbors == 0 && c.junction_net == [0.0;3]));
    assert_eq!(engine.world.body_json(1,&engine.genomes),restored.world.body_json(1,&restored.genomes));
    for _ in 0..3 {
        engine.world.step(&mut engine.genomes);
        restored.world.step(&mut restored.genomes);
    }
    assert_eq!(checkpoint::save(&engine).unwrap(),checkpoint::save(&restored).unwrap());
}


#[test]
fn fission_keeps_the_parents_completed_exchange_and_starts_the_daughter_at_zero() {
    let source = format!("{CHANNELS}\nassembly SecondFrame: support() {{ Frame:0.04; }}\nassembly SecondStore: store(energy) {{ Wall:0.04; }}");
    let (mut world, mut genomes) = colony_with(&[(100.0,100.0),(114.0,100.0)], &source);
    let parent = &mut world.cells[0];
    parent.birth_area = parent.body.area * 0.5;
    parent.cooldown = 0.0;
    let replica_mass = 0.01 * parent.body.area;
    parent.nutrient_stored -= replica_mass;
    parent.organic_carbon -= replica_mass;
    parent.energy -= 80.0 * replica_mass + 8.0 * parent.body.area;
    let state = parent.body.cellular.as_mut().unwrap();
    state.replica_mass = replica_mass;
    state.replica_energy = 80.0 * replica_mass * crate::body::EMBODIED_FRACTION;
    state.constriction = 1.0;
    assert!(parent.division_ready());
    world.cells[1].energy *= 0.4;
    world.step(&mut genomes);
    assert_eq!(world.births,1);
    let [parent, peer, daughter] = world.cells.as_slice() else { panic!("expected two parents and one daughter") };
    assert_eq!((parent.adhered_neighbors, peer.adhered_neighbors), (1,1));
    assert!(parent.junction_net[0] < 0.0);
    assert_eq!(peer.junction_net[0],-parent.junction_net[0]);
    assert_eq!((daughter.parent_id,daughter.adhered_neighbors,daughter.junction_net),
        (parent.id,0,[0.0;3]));
}

#[test]
fn isolated_peers_allocate_no_transfer_account_and_dead_neighbors_drop_out() {
    let (mut world, genomes) = colony(&[(100.0,100.0),(114.0,100.0),(300.0,300.0)]);
    let mut phases = PhaseWorkspace::default();
    world.freeze_contact_signals(&genomes,&mut phases.signals);
    phases.geometry.rebuild(&world.cells,false,true);
    world.apply_adhesion_in(&mut phases);
    assert_eq!(phases.junctions.accounts.len(),2);
    assert_eq!(world.cells[2].adhered_neighbors,0);
    world.cells[0].energy=0.0;
    world.cells.retain(Cell::alive);
    world.freeze_contact_signals(&genomes,&mut phases.signals);
    phases.geometry.rebuild(&world.cells,false,true);
    world.apply_adhesion_in(&mut phases);
    assert_eq!(phases.junctions.accounts.len(),0);
    assert!(!world.had_adhesive_contacts);
    assert!(world.cells.iter().all(|c| c.adhered_neighbors == 0 && c.junction_net == [0.0;3]));
}
