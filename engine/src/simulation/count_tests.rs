use super::*;
use crate::{checkpoint, Engine};
use crate::counts::{CellCounts, CountMeal, FoodCounts, units};

fn counted_engine() -> Engine {
    let mut e=Engine::new();
    let id=e.compile("cellular; product Blank=scaffold(); gene make: Blank=1;");
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
    assert!(a[9]+a[11]<=a[10], "tick {}: {a:?}",w.ticks);
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
    let (fixed,exuded)=c.count_fix(80,&cap);
    w.count_flux.fixed+=u64::from(fixed);
    assert_eq!((fixed,exuded),(4,0));
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
    let mut f=Food{id:1,kind:OrganicKind::Tissue,x:c.x,y:c.y,energy:0.0,nutrients:0.0,extra_energy:0.0,counted:Some(FoodCounts{meal:original,decay_credit:0})};
    c.count_capture(&mut f,&cap,1.0);
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
    c.count_partition(&mut daughter,units(7));
    let (a,b)=(c.counted.as_ref().unwrap(),daughter.counted.as_ref().unwrap());
    assert_eq!((a.atp+b.atp,a.carbon+b.carbon,a.mineral+b.mineral,a.oxygen+b.oxygen,a.dic+b.dic),(202,15,13,7,9));
    assert_eq!((a.reserve_extra+b.reserve_extra,a.structural_extra+b.structural_extra),(13,15));
    assert_eq!((a.gut[1].carbon+b.gut[1].carbon,a.gut[1].mineral+b.gut[1].mineral,a.gut[1].extra+b.gut[1].extra),(7,11,9));
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
        id: 1, kind: OrganicKind::Tissue, x: c.x, y: c.y,
        energy: units(20), nutrients: 0.0, extra_energy: 0.0,
        counted: Some(FoodCounts {
            meal: CountMeal { carbon: 1, ..Default::default() },
            decay_credit: 0,
        }),
    };
    for tick in 1..=4 {
        assert_eq!(c.count_capture(&mut f, &cap, units(5)), units(5));
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
