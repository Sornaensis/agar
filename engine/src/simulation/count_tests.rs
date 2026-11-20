use super::*;
use crate::{checkpoint, Engine};
use crate::counts::{CellCounts, CountMeal, FoodCounts, units};

fn counted_engine() -> Engine {
    counted_engine_with("cellular; product Blank=scaffold(); gene make: Blank=1;")
}
fn counted_engine_with(source: &str) -> Engine {
    let mut e=Engine::new();
    let id=e.compile(source);
    assert_eq!(id,1);
    e.world.environment.set_initial_nutrients(1.0);
    e.world.environment.set_initial_gases(2.0,2.0);
    e.world.reset_empty(41);
    e.world.set_count_chemistry(true).unwrap();
    e.world.place_cell(id,100.0,100.0,&e.genomes).unwrap();
    e
}
fn balanced(w:&World) {
    let a=w.count_audit().unwrap();
    for i in [0,3,6]{assert_eq!(a[i]+a[i+2],a[i+1],"tick {}: {a:?}",w.ticks);}
    let lost: i128 = w.energy_loss_counts().unwrap().into_iter().map(i128::from).sum();
    assert_eq!(i128::from(a[9])+i128::from(a[11])+lost, i128::from(a[10]), "tick {}: {a:?}",w.ticks);
}
#[test]
fn count_living_stores_edits_reactions_and_lysis_replay_exactly() {
    let mut e=counted_engine();
    balanced(&e.world);
    for _ in 0..90 {e.world.step(&mut e.genomes);balanced(&e.world);}
    let canonical=checkpoint::save(&e).unwrap();
    let mut copy=checkpoint::restore(&canonical).unwrap();
    assert_eq!(canonical,checkpoint::save(&copy).unwrap());
    for _ in 0..5 {e.world.step(&mut e.genomes);copy.world.step(&mut copy.genomes);}
    assert_eq!(checkpoint::save(&e).unwrap(),checkpoint::save(&copy).unwrap());
    e.world.set_cell_stores(1,Some(42.1234),Some(0.2345)).unwrap();
    assert_eq!(e.world.cells[0].counted.as_ref().unwrap().atp,421234);
    balanced(&e.world);
    let before=e.world.count_audit().unwrap();
    e.world.set_cell_stores(1,Some(0.0),None).unwrap();
    assert!(e.world.cells.is_empty()&&!e.world.food.is_empty());
    let after=e.world.count_audit().unwrap();
    assert_eq!(after[9]+421234,before[9]);
    balanced(&e.world);
    for _ in 0..90 {e.world.step(&mut e.genomes);balanced(&e.world);}
    let saved=checkpoint::save(&e).unwrap();
    assert_eq!(saved,checkpoint::save(&checkpoint::restore(&saved).unwrap()).unwrap());
}
#[test]
fn count_fixation_and_respiration_fire_integer_stoichiometry() {
    let mut e=counted_engine();
    let w=&mut e.world;let patch=w.environment.patch_index(100.0,100.0);
    let oxygen=w.environment.take_oxygen_counts(patch,17);
    let dic=w.environment.take_inorganic_carbon_counts(patch,13);
    let c=&mut w.cells[0];let q=c.counted.as_mut().unwrap();
    q.oxygen+=oxygen;q.dic+=dic;c.project_counts();
    let mut cap=c.body.capacities();cap.respiration=100.0;cap.basal_respiration=0.0;
    let before=c.counted.clone().unwrap();
    let fixed=c.count_fix(80,&cap);
    w.count_flux.fixed+=u64::from(fixed);
    assert_eq!(fixed,4);
    let q=c.counted.as_ref().unwrap();
    assert_eq!((q.carbon-before.carbon,before.dic-q.dic,q.oxygen-before.oxygen),(4,4,4));
    w.count_respire(&[cap]);
    let q=w.cells[0].counted.as_ref().unwrap();
    assert_eq!(q.atp-before.atp,(oxygen+fixed)*16);
    assert_eq!(q.oxygen,0);
    balanced(w);
}
#[test]
fn scarce_capture_keeps_all_carriers_and_attached_energy_in_the_two_inventories() {
    let mut e=counted_engine();
    let c=&mut e.world.cells[0];
    let mut cap=c.body.capacities();cap.gut_energy=units(145);cap.gut_minerals=1.0;
    let original=CountMeal{carbon:17,mineral:31,extra:25};
    let mut f=Food{composition:Composition::pool(1),id:1,kind:OrganicKind::Tissue,x:c.x,y:c.y,energy:0.0,nutrients:0.0,extra_energy:0.0,counted:Some(FoodCounts{meal:original,decay_credit:0})};
    c.count_capture(&mut f,&cap,1.0,1,0);
    let p=f.counted.unwrap().meal;let g=c.counted.as_ref().unwrap().gut[OrganicKind::Tissue as usize];
    assert!(g.carbon>0&&p.carbon>0);
    assert_eq!((p.carbon+g.carbon,p.mineral+g.mineral,p.extra+g.extra),(17,31,25));
    assert!(g.energy()<=145);
}
#[test]
fn odd_store_partitions_and_fractional_work_are_not_duplicated() {
    let mut e=counted_engine();
    let c=&mut e.world.cells[0];let mut daughter=c.clone();
    let q=c.counted.as_mut().unwrap();
    *q=CellCounts{atp:209,mineral:13,carbon:15,oxygen:7,dic:9,reserve_extra:13,structural_extra:15,..Default::default()};
    q.gut[1]=CountMeal{carbon:7,mineral:11,extra:9};q.rate_credit[0]=37;
    q.gut[LIGAND] = CountMeal {carbon:13, mineral:17, extra:5};
    c.count_partition(&mut daughter,units(7));
    let (a,b)=(c.counted.as_ref().unwrap(),daughter.counted.as_ref().unwrap());
    assert_eq!((a.atp+b.atp,a.carbon+b.carbon,a.mineral+b.mineral,a.oxygen+b.oxygen,a.dic+b.dic),(202,15,13,7,9));
    assert_eq!((a.reserve_extra+b.reserve_extra,a.structural_extra+b.structural_extra),(13,15));
    assert_eq!((a.gut[1].carbon+b.gut[1].carbon,a.gut[1].mineral+b.gut[1].mineral,a.gut[1].extra+b.gut[1].extra),(7,11,9));
    let mut ligand = a.gut[LIGAND];
    ligand.join(b.gut[LIGAND]);
    assert_eq!(ligand, CountMeal {carbon:13, mineral:17, extra:5});
    assert_eq!(a.rate_credit[0]+b.rate_credit[0],37);
}

#[test]
fn one_patch_shares_its_actual_whole_photon_supply_between_two_cells() {
    let mut engine = counted_engine();
    engine.world.place_cell(1, 150.0, 150.0, &engine.genomes).unwrap();
    engine.world.environment.configure(0.5, 0.7, 0.0);
    let patch = engine.world.environment.patch_index(100.0, 100.0);
    let light = engine.world.environment.patch_light(patch);
    assert!(light > 0.0);
    let mut patches = std::collections::HashMap::new();
    let mut receipts = Vec::new();
    let mut supplied = 0;
    let mut received = [0_u32; 2];
    let per_tick = PHOTO_HABITAT_POWER / PATCHES as f64 * light * DT;
    for _ in 0..10 {
        let mut requests = [(0, patch, 1.0), (1, patch, 1.0)];
        engine.world.share_photons(&mut requests, &mut patches, &mut receipts);
        assert!(receipts[0].abs_diff(receipts[1]) <= 1);
        supplied += receipts.iter().sum::<u32>();
        for (total, &grant) in received.iter_mut().zip(&receipts) { *total += grant; }
    }
    assert_eq!(supplied, crate::counts::floor(per_tick * 10.0));
    assert!(received.iter().all(|&n| n > 0));
    let state = checkpoint::save(&engine).unwrap();
    let restored = checkpoint::restore(&state).unwrap();
    assert_eq!(engine.world.photon_credit, restored.world.photon_credit);
}

#[test]
fn an_indivisible_food_carrier_can_be_taken_after_four_small_work_pulses() {
    let mut engine = counted_engine();
    let c = &mut engine.world.cells[0];
    let mut cap = c.body.capacities();
    cap.gut_energy = 1.0;
    let mut f = Food {
        composition: Composition::pool(1),
        id: 1, kind: OrganicKind::Tissue, x: c.x, y: c.y,
        energy: units(20), nutrients: 0.0, extra_energy: 0.0,
        counted: Some(FoodCounts {
            meal: CountMeal { carbon: 1, ..Default::default() },
            decay_credit: 0,
        }),
    };
    for tick in 1..=4 {
        assert_eq!(c.count_capture(&mut f, &cap, units(5),1,tick), units(5));
        assert_eq!(f.counted.unwrap().meal.carbon, u32::from(tick != 4));
    }
    assert_eq!(c.counted.as_ref().unwrap().gut[OrganicKind::Tissue as usize].carbon, 1);
}

#[test]
fn scarce_quanta_on_spaced_ticks_reach_both_equal_requesters() {
    let mut engine = counted_engine();
    let w = &mut engine.world;
    w.place_cell(1, 150.0, 150.0, &engine.genomes).unwrap();
    let (x, y) = (w.cells[0].x, w.cells[0].y);
    (w.cells[1].x, w.cells[1].y) = (x + 1.0, y);
    let patch = w.environment.patch_index(x, y);
    let per_unit_light = PHOTO_HABITAT_POWER / PATCHES as f64 * DT * f64::from(crate::counts::SCALE);
    w.environment.configure(0.5, 0.50000001 / per_unit_light, 0.0);
    w.environment.take_mineral_counts(patch, u32::MAX);
    let initial = [w.cells[0].counted.as_ref().unwrap().mineral, w.cells[1].counted.as_ref().unwrap().mineral];
    let mut caps = vec![w.cells[0].body.capacities(); 2];
    for cap in &mut caps { cap.uptake = 10.0; cap.minerals = 20.0; }
    let outputs = vec![Outputs { absorb: 1.0, ..Outputs::default() }; 2];
    let mut workspace = PhaseWorkspace::default();
    let mut patches = std::collections::HashMap::new();
    let mut receives = Vec::new();
    let mut light = [0; 2];
    for tick in 0..32 {
        w.ticks = tick;
        let mut requests = [(0, patch, 1.0), (1, patch, 1.0)];
        w.share_photons(&mut requests, &mut patches, &mut receives);
        assert!(receives.iter().sum::<u32>() <= 1);
        for (total, &grant) in light.iter_mut().zip(&receives) { *total += grant; }
        if tick % 2 == 1 {
            assert_eq!(w.environment.release_mineral_counts(patch, 1), 1);
            w.count_uptake(&outputs, &caps, &mut workspace);
            assert_eq!(w.environment.mineral_count(patch), 0);
        }
    }
    let mineral = [w.cells[0].counted.as_ref().unwrap().mineral - initial[0],
                   w.cells[1].counted.as_ref().unwrap().mineral - initial[1]];
    for grants in [light, mineral] {
        assert_eq!(grants[0] + grants[1], 16);
        assert!(grants.iter().all(|&n| n > 0), "{grants:?}");
    }
}

#[test]
fn equal_membranes_share_repeated_one_quantum_dissolved_offers() {
    let mut engine = counted_engine();
    let w = &mut engine.world;
    w.place_cell(1, 150.0, 150.0, &engine.genomes).unwrap();
    let (x, y) = (w.cells[0].x, w.cells[0].y);
    (w.cells[1].x, w.cells[1].y) = (x + 1.0, y);
    let patch = w.environment.patch_index(x, y);
    w.environment.take_oxygen_counts(patch, u32::MAX);
    w.environment.take_inorganic_carbon_counts(patch, u32::MAX);
    w.environment.release_oxygen_counts(patch, 1);
    w.environment.release_inorganic_carbon_counts(patch, 1);
    let mut receipts = [[0; 2]; 2];
    let mut ws = PhaseWorkspace::default();
    for tick in 0..1600 {
        w.ticks = tick;
        w.count_exchange(&mut ws);
        // Return the collected count to keep the two test membranes equally
        // hungry for the next exposure, retaining the physical uptake clocks.
        for (cell, total) in w.cells.iter_mut().zip(&mut receipts) {
            let q = cell.counted.as_mut().unwrap();
            let oxygen = std::mem::take(&mut q.oxygen);
            let dic = std::mem::take(&mut q.dic);
            total[0] += oxygen;
            total[1] += dic;
            assert_eq!(w.environment.release_oxygen_counts(patch, oxygen), oxygen);
            assert_eq!(w.environment.release_inorganic_carbon_counts(patch, dic), dic);
            cell.project_counts();
        }
        assert_eq!(w.environment.oxygen_count(patch), 1);
        assert_eq!(w.environment.inorganic_carbon_count(patch), 1);
    }
    assert!(receipts.iter().flatten().all(|&n| n > 0), "{receipts:?}");
}

#[test]
fn small_bites_yield_the_same_minerals_and_cost_as_one_bulk_digestion() {
    for (kind, expected_minerals) in [(OrganicKind::Tissue, 8), (OrganicKind::Waste, 5)] {
        let mut e = counted_engine();
        let index = kind as usize;
        let mut small = e.world.cells.pop().unwrap();
        let mut bulk = small.clone();
        let cap = small.body.capacities();
        let start = small.counted.as_ref().unwrap().clone();
        let mut emissions = Vec::new();
        for (cell, bite, iterations) in [(&mut small,1,10), (&mut bulk,10,1)] {
            for _ in 0..iterations {
                cell.counted.as_mut().unwrap().gut[index] = CountMeal {carbon:bite, mineral:bite, extra:0};
                cell.project_counts();
                let mut work = [0.0;3];
                work[index] = units(20*bite) / DIGESTION_SPEED[index];
                cell.count_digest(&cap,work,&mut emissions);
            }
            let q = cell.counted.as_ref().unwrap();
            assert_eq!(q.mineral - start.mineral, expected_minerals);
            assert_eq!(start.atp - q.atp, 4);
            assert_eq!(q.gut[index].carbon, 0);
        }
        assert_eq!(small.counted.unwrap().carbon, bulk.counted.unwrap().carbon);
    }
}

#[test]
fn migrated_census_preserves_opening_stocks_without_inventing_history_or_heat() {
    let mut e = counted_engine();
    for _ in 0..12 { e.world.step(&mut e.genomes); }
    let current = checkpoint::save(&e).unwrap();
    let mut appended = crate::checkpoint::Writer::new();
    e.world.census.write(&mut appended);
    e.world.energy_loss.write(&mut appended);
    let mut prior = current[..current.len()-appended.0.len()].to_vec();
    // Reconstruct the v17 single-cell layout, which stored neither keyed
    // partial bites nor individual mouth cooldowns. The body has no mounts.
    let mut q = crate::checkpoint::Writer::new();
    e.world.cells[0].counted.as_ref().unwrap().write(&mut q);
    let start = prior.windows(q.0.len()).position(|row| row == q.0).unwrap();
    let end = start + q.0.len();
    assert_eq!(prior.drain(end-4..end).collect::<Vec<_>>(), [0;4]);
    let fourth_meal = start + 7 * 4 + 3 * 12;
    assert_eq!(prior.drain(fourth_meal..fourth_meal+12).collect::<Vec<_>>(), [0; 12]);
    let mut env = crate::checkpoint::Writer::new();
    e.world.environment.write_checkpoint(&mut env);
    let start = prior.windows(env.0.len()).position(|row| row == env.0).unwrap();
    let mouth = start + env.0.len() + 4 + 16 + 24 * 8;
    let fourth_projection = mouth + 4 + 3 * 24;
    assert_eq!(prior.drain(fourth_projection..fourth_projection+24).collect::<Vec<_>>(), [0; 24]);
    assert_eq!(prior.drain(mouth..mouth+4).collect::<Vec<_>>(), [0;4]);
    prior[8..12].copy_from_slice(&17_u32.to_le_bytes());
    let size = prior.len() as u32;
    prior[12..16].copy_from_slice(&size.to_le_bytes());
    let checksum = checkpoint::checksum(&prior[20..]);
    prior[16..20].copy_from_slice(&checksum.to_le_bytes());
    let mut restored = checkpoint::restore(&prior).unwrap();
    let mut census = Vec::new();
    restored.world.ecology_snapshot(&restored.genomes, &mut census);
    assert_eq!(census[0], 12.0 * DT);
    assert_eq!(&census[1..12], &[1.,1.,1.,1.,0.,0.,0.,0.,0.,0.,-1.]);
    let losses = restored.world.energy_loss_counts().unwrap();
    assert!(losses[0] > 0);
    assert_eq!(&losses[1..], &[0;6]);
    balanced(&restored.world);
    restored.world.set_cell_stores(1,Some(0.0),None).unwrap();
    restored.world.ecology_snapshot(&restored.genomes,&mut census);
    assert_eq!(&census[3..10], &[0.,1.,0.,1.,0.,0.,1.], "manual withdrawal is a separate non-starvation loss");
    let bytes = checkpoint::save(&restored).unwrap();
    assert_eq!(bytes,checkpoint::save(&checkpoint::restore(&bytes).unwrap()).unwrap());
}

#[test]
fn checkpoint_rejects_single_count_of_unexplained_energy_loss() {
    let mut e = counted_engine();
    e.world.cells[0].counted.as_mut().unwrap().atp -= 1;
    e.world.cells[0].project_counts();
    let bytes = checkpoint::save(&e).unwrap();
    assert!(checkpoint::restore(&bytes).err().unwrap().contains("measured losses"));
}

#[test]
fn aggregate_metrics_include_paid_inventory_and_pending_extracellular_deposits() {
    let mut e = counted_engine();
    for _ in 0..120 { e.world.step(&mut e.genomes); }
    let w = &mut e.world;
    let patch = w.environment.patch_index(100.0,100.0);
    let quantities = [w.environment.take_mineral_counts(patch,17),
        w.environment.take_oxygen_counts(patch,13), w.environment.take_inorganic_carbon_counts(patch,19)];
    assert_eq!(quantities,[17,13,19]);
    w.deposits.push(counted::LocalDeposit {x:100.0,y:100.0,quantities});
    let mut metrics = Vec::new();
    w.metrics_snapshot(&e.genomes,&mut metrics);
    let a = w.count_audit().unwrap();
    let counts = |v: f64| (v*f64::from(crate::counts::SCALE)).round() as u64;
    assert_eq!(counts(metrics[18]),a[9]);
    assert_eq!(counts(metrics[20]),a[0]);
    assert_eq!(counts(metrics[22]+metrics[23]+metrics[24]),a[0]);
    assert_eq!(counts(metrics[38]+metrics[39]+metrics[46]+metrics[50]),a[3]);
    assert_eq!(counts(metrics[44]+metrics[45]),a[6]);
    assert_eq!(counts(metrics[44]),u64::from(w.cells[0].counted.as_ref().unwrap().oxygen));
}

#[test]
fn census_groups_mutant_introductions_and_remembers_the_extinct_root() {
    let mut e = counted_engine();
    let source = "cellular; product Blank=scaffold(); gene make: Blank=0;";
    e.genomes.push(Genome::new(source.into(),Program::compile(source).unwrap(),1,1));
    let mutant = e.world.place_cell(2,140.0,100.0,&e.genomes).unwrap();
    e.world.set_cell_stores(mutant,None,Some(0.0)).unwrap();
    let mut living = Vec::new();
    e.world.ecology_snapshot(&e.genomes,&mut living);
    let founder = &e.world.cells[0];
    assert_eq!(living.len(), 2+20);
    assert_eq!(&living[2+15..], &[1., founder.nutrient_stored, 2.0*founder.organic_carbon, 0., 0.]);
    e.world.set_cell_stores(mutant,Some(0.0),None).unwrap();
    e.world.set_cell_stores(1,Some(0.0),None).unwrap();
    let mut data = Vec::new();
    e.world.ecology_snapshot(&e.genomes,&mut data);
    assert_eq!(&data[..12], &[0.,1.,1.,0.,2.,0.,2.,0.,0.,2.,0.,-1.]);
    let bytes = checkpoint::save(&e).unwrap();
    let loaded = checkpoint::restore(&bytes).unwrap();
    let mut replay = Vec::new();
    loaded.world.ecology_snapshot(&loaded.genomes,&mut replay);
    assert_eq!(data,replay);
}


#[test]
fn low_atp_with_reserve_fuel_uses_both_inclusive_thresholds() {
    let mut e = counted_engine();
    let capacity = crate::counts::floor(e.world.cells[0].energy_capacity());
    let atp_limit = capacity / 10;
    let min_carbon = capacity.div_ceil(4 * CHEMICAL_ENERGY_PER_CARBON as u32);
    let mut row = Vec::new();
    for (atp, carbon, expected) in [
        (atp_limit, min_carbon, 1.0),
        (atp_limit + 1, min_carbon, 0.0),
        (atp_limit, min_carbon - 1, 0.0),
    ] {
        let cell = &mut e.world.cells[0];
        let q = cell.counted.as_mut().unwrap();
        q.atp = atp;
        q.carbon = carbon;
        cell.project_counts();
        e.world.ecology_snapshot(&e.genomes, &mut row);
        assert_eq!(row[2 + 19], expected);
    }
    // Continuous worlds use the same thresholds. Enriched gut and stored
    // tissue energy do not stand in for the required reserve carbon.
    let cell = &mut e.world.cells[0];
    cell.counted = None;
    let capacity = cell.energy_capacity();
    cell.energy = capacity / 10.0;
    cell.organic_carbon = capacity / (4.0 * CHEMICAL_ENERGY_PER_CARBON);
    e.world.ecology_snapshot(&e.genomes, &mut row);
    assert_eq!(row[2 + 19], 1.0);
    let cell = &mut e.world.cells[0];
    cell.organic_carbon = 0.0;
    cell.gut_energy = capacity;
    cell.reserve_enrichment = capacity;
    e.world.ecology_snapshot(&e.genomes, &mut row);
    assert_eq!(row[2 + 19], 0.0);
}

#[test]
fn phototroph_fission_requires_provisioned_machinery_and_mineral_storage() {
    let program = Program::compile(include_str!("../../../examples/phototroph.cell")).unwrap();
    let recipes = &program.cellular().unwrap().assemblies;
    let index = |name| recipes.iter().position(|r| r.name == name).unwrap();
    let essential = [
        ("Solar", 0.12),
        ("Respiration", 0.04),
        ("Minerals", 0.025),
        ("Translation", 0.04),
    ];
    let mut input = Inputs {
        body_area: 2.0,
        body_birth_area: 1.0,
        energy: 110.0,
        ..Inputs::default()
    };
    input.assemblies.resize(recipes.len(), 0.0);
    let active = |input: &Inputs| {
        program
            .evaluate_cell(input, &mut Registers::default(), &mut EvalScratch::default())
            .cellular
            .unwrap()
            .activity[index("Division")] > 0.0
    };
    for (name, mass) in essential {
        input.assemblies[index(name)] = 2.0 * mass;
    }
    input.assemblies[index("MineralStore")] = 0.04;
    assert!(!active(&input), "two small compartments do not satisfy the stock provisioning rule");
    input.assemblies[index("MineralStore")] = 0.06;
    assert!(active(&input));
    for (name, mass) in essential {
        input.assemblies[index(name)] = mass;
        assert!(!active(&input), "a single working {name} cannot feed both halves");
        input.assemblies[index(name)] = 2.0 * mass;
    }
}

#[test]
fn mineral_import_below_five_percent_pays_only_for_the_actual_grant() {
    let mut engine = counted_engine();
    let world = &mut engine.world;
    let mut cap = world.cells[0].body.capacities();
    cap.uptake = 10.0;
    let q = world.cells[0].counted.as_mut().unwrap();
    q.atp = 3;
    q.mineral = 0;
    world.cells[0].project_counts();
    world.count_uptake(&[Outputs { absorb: 1.0, ..Default::default() }], &[cap], &mut PhaseWorkspace::default());
    let q = world.cells[0].counted.as_ref().unwrap();
    assert_eq!((q.atp, q.mineral), (0, 3));
}

#[test]
fn capture_work_stays_at_one_opening_on_one_particle() {
    let mut engine = counted_engine();
    let cell = &mut engine.world.cells[0];
    let mut cap = cell.body.capacities();
    cap.gut_energy = 1.0;
    let packet = |id| Food { id, kind:OrganicKind::Waste, x:0.0,y:0.0, energy:units(20),
        nutrients:0.0, extra_energy:0.0, composition:Composition::SOLUBLE,
        counted:Some(FoodCounts { meal:CountMeal { carbon:1, ..Default::default() }, decay_credit:0 }) };
    let mut a = packet(27);
    let mut b = packet(28);
    for tick in 0..3 { cell.count_capture(&mut a, &cap, units(5), 1, tick); }
    cell.count_capture(&mut a, &cap, units(5), 2, 3);
    cell.count_capture(&mut b, &cap, units(5), 1, 3);
    assert_eq!(cell.counted.as_ref().unwrap().gut[0].carbon, 0);
    for tick in 4..7 { cell.count_capture(&mut b, &cap, units(5), 1, tick); }
    assert_eq!(cell.counted.as_ref().unwrap().gut[0].carbon, 1);
    assert_eq!(a.counted.unwrap().meal.carbon, 1);
}

#[test]
fn harvest_retains_every_fixed_count() {
    let mut e = counted_engine();
    let c = &mut e.world.cells[0];
    let cap = c.body.capacities();
    let q = c.counted.as_mut().unwrap();
    q.dic = 250;
    let before = q.carbon;
    assert_eq!(c.count_fix(5000, &cap), 250);
    let q = c.counted.as_ref().unwrap();
    assert_eq!(q.carbon - before, 250);
    assert_eq!(q.dic, 0);
}

#[test]
fn counted_ligand_retains_carbon_enrichment_minerals_and_work_through_capture_conversion_and_lysis() {
    let mut e = counted_engine_with("cellular;product Wall=membrane();gene supply:Wall=1;
        assembly Gut:store(food){Wall:0.04;start_when:built.Gut < 0.039;}");
    for _ in 0..900 {
        if e.world.cells[0].body.capacities().gut_energy >= 30.0 { break; }
        e.world.step(&mut e.genomes);
    }
    let mut cap = e.world.cells[0].body.capacities();
    assert!(cap.gut_energy >= 30.0);
    cap.isomerization = [0.3, 0.0];
    let q = e.world.cells[0].counted.as_mut().unwrap();
    // Move already funded resources into the reserve's enrichment and then the packet.
    let bonus = q.atp / 8;
    q.atp -= bonus;
    q.reserve_extra += bonus;
    e.world.cells[0].project_counts();
    e.world.isomerize_in(&[cap]);
    let q = e.world.cells[0].counted.as_mut().unwrap();
    let moved = q.gut[LIGAND].carbon;
    assert!(moved >= 99 && q.gut[LIGAND].extra > 0);
    q.mineral -= 3;
    q.gut[LIGAND].mineral += 3;
    let packet = q.gut[LIGAND].take(moved);
    e.world.cells[0].project_counts();
    let (x,y) = (e.world.cells[0].x,e.world.cells[0].y);
    let mut ws = PhaseWorkspace::default();
    ws.emissions.push(packet.emission(OrganicKind::Tissue,x,y).material(Composition::pool(LIGAND)));
    e.world.emit_organic_pending(&mut ws);
    balanced(&e.world);
    let cell = &mut e.world.cells[0];
    cell.count_capture(&mut e.world.food[0], &cap, 1e6, 0, 1);
    assert_eq!(cell.counted.as_ref().unwrap().gut[LIGAND], packet);
    e.world.food.retain(|f| f.energy > 0.0);
    balanced(&e.world);
    cap.isomerization = [0.0, 0.12];
    e.world.isomerize_in(&[cap]);
    let remaining = e.world.cells[0].counted.as_ref().unwrap().gut[LIGAND];
    assert!(remaining.carbon > 0 && remaining.carbon < moved);
    balanced(&e.world);
    let saved = checkpoint::save(&e).unwrap();
    let restored = checkpoint::restore(&saved).unwrap();
    assert_eq!(checkpoint::save(&restored).unwrap(), saved);
    let cell = &mut e.world.cells[0];
    cell.integrity = 0.0;
    e.world.remove_dead_in(&mut ws, None);
    let released = e.world.food.iter().map(|f|f.composition.counts(u64::from(f.counted.unwrap().meal.carbon))[LIGAND]).sum::<u64>();
    assert_eq!(released, u64::from(remaining.carbon));
    balanced(&e.world);
}
