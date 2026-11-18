fn recycling_program() -> CellularProgram {
    let mut program = flagellum_program();
    program.assemblies[0].kind = AssemblyKind::Mechanism(MechanismKind::Propulsion);
    program.products.push(product("Protease", ProductKind::Enzyme));
    program.assemblies.push(AssemblyDef {
        name: "Recovery".into(), kind: AssemblyKind::Mechanism(MechanismKind::Recycle),
        parts: vec![(3, 0.02)], at: 0.0, direction: 0.0,
    });
    program
}

#[test]
fn low_energy_construction_depends_on_paid_resources_not_empty_storage_capacity() {
    let program = CellularProgram {
        products: vec![product("Protease", ProductKind::Enzyme), product("Wall", ProductKind::Membrane)],
        assemblies: vec![
            AssemblyDef { name: "TissueEnzyme".into(), kind: AssemblyKind::Mechanism(MechanismKind::DigestTissue),
                parts: vec![(0, 0.02)], at: 0.0, direction: 0.0 },
            AssemblyDef { name: "Battery".into(), kind: AssemblyKind::Mechanism(MechanismKind::EnergyStorage),
                parts: vec![(1, 0.08)], at: 0.0, direction: 0.0 },
        ],
    };
    program.validate().unwrap();
    let mut results = Vec::new();
    for extra_storage in [false, true] {
        let mut body = stocked(&program, &[0.0, 0.0]);
        if extra_storage { add_complete(&mut body, &program, 1); }
        let mut energy = 4.0;
        let mut minerals = 0.2;
        assert!(energy < 0.15 * body.capacities().energy, "The former hidden reserve blocked this affordable enzyme");
        let original_material = body.structural_mass() + minerals;
        let original_energy = body.embodied_energy() + energy;
        let mut heat = 0.0;
        for _ in 0..120 {
            let state = body.cellular.as_ref().unwrap();
            let installed: f64 = state.assemblies.iter().filter(|a| a.recipe.name == "TissueEnzyme")
                .map(AssemblyInstance::mass).sum();
            let complete = state.assemblies.iter().any(|a| a.recipe.name == "TissueEnzyme" && a.complete());
            let mut request = controls(&program, 0.0);
            request.start = vec![!complete, false];
            request.expression[0] = if installed + state.pools[0].mass + 1e-12 < 0.02 { 1.0 } else { 0.0 };
            let available = energy;
            let report = work(&mut body, &program, &request, &mut energy, &mut minerals, 1.0 / 30.0);
            assert!(report.energy_spent <= available + 1e-12 && energy >= 0.0);
            heat += report.dissipated_energy;
            close(original_material, body.structural_mass() + minerals);
            close(original_energy, body.embodied_energy() + energy + heat);
        }
        assert!(body.cellular.as_ref().unwrap().assemblies.iter().any(|a| a.recipe.name == "TissueEnzyme" && a.complete()));
        close(body.capacities().substrate_digestion[1], 65.0 * 0.02);
        assert!(energy > 1.0 && energy < 4.0, "The enzyme is paid, with enough energy left to use it");
        results.push((energy, minerals));
    }
    assert_eq!(results[0], results[1], "Adding empty storage neither locks nor grants construction energy");

    let mut body = stocked(&program, &[0.0, 0.0]);
    let mut energy = 0.0;
    let mut minerals = 0.2;
    let report = work(&mut body, &program, &controls(&program, 1.0), &mut energy, &mut minerals, 1.0);
    assert_eq!(report.energy_spent, 0.0);
    assert_eq!(report.synthesized_mass, 0.0);
    assert_eq!(report.constructed_mass, 0.0);
}

#[test]
fn initiation_gate_controls_new_commitments_without_freezing_partial_machines() {
    let program = flagellum_program();
    let mut body = stocked(&program, &[0.01, 0.02, 0.05]);
    let mut request = controls(&program, 0.0);
    request.start = vec![false];
    let mut energy = 100.0;
    let mut minerals = 1.0;
    let initial = body.structural_mass();
    let report = work(&mut body, &program, &request, &mut energy, &mut minerals, 0.1);
    assert_eq!(report.constructed_mass, 0.0);
    assert!(body.cellular.as_ref().unwrap().assemblies.is_empty());
    close(body.structural_mass(), initial);
    request.start[0] = true;
    work(&mut body, &program, &request, &mut energy, &mut minerals, 0.1);
    let started = body.cellular.as_ref().unwrap().assemblies[0].mass();
    assert!(started > 0.0 && started < 0.04);
    request.start[0] = false;
    for _ in 0..20 { work(&mut body, &program, &request, &mut energy, &mut minerals, 0.1); }
    let state = body.cellular.as_ref().unwrap();
    assert_eq!(state.assemblies.len(), 1);
    assert!(state.assemblies[0].complete());
    assert!(!state.control_reports[0].start_allowed);
    assert!(state.pools.iter().any(|p| p.mass > 0.0), "Stock alone cannot bypass a false initiation gate");
}

#[test]
fn recycling_requires_complete_active_paid_machinery_and_preserves_all_ledgers() {
    let program = recycling_program();
    let mut body = stocked(&program, &[0.0; 4]);
    add_complete(&mut body, &program, 0);
    let mut request = controls(&program, 0.0);
    request.start = vec![false; 2];
    request.recycle = vec![1.0, 0.0];
    let mut energy = 100.0;
    let mut minerals = 1.0;
    let report = work(&mut body, &program, &request, &mut energy, &mut minerals, 1.0);
    assert_eq!(report.recycled_mass, 0.0, "An effort is not a recycling capability");
    add_complete(&mut body, &program, 1);
    let complete_recycler = body.cellular.as_ref().unwrap().assemblies[1].clone();
    let partial = &mut body.cellular.as_mut().unwrap().assemblies[1].ingredients[0];
    partial.mass *= 0.5;
    partial.embodied_energy *= 0.5;
    assert_eq!(work(&mut body, &program, &request, &mut energy, &mut minerals, 1.0).recycled_mass, 0.0);
    body.cellular.as_mut().unwrap().assemblies[1] = complete_recycler;
    request.activity[1] = 0.0;
    assert_eq!(work(&mut body, &program, &request, &mut energy, &mut minerals, 1.0).recycled_mass, 0.0);
    request.activity[1] = 1.0;
    let before_material = body.structural_mass() + minerals;
    let before_energy = body.embodied_energy() + energy;
    let report = work(&mut body, &program, &request, &mut energy, &mut minerals, 1.0);
    close(report.recycled_mass, 0.8 * 0.02);
    assert!(report.energy_spent > 0.0);
    assert_eq!(report.energy_recovered, 0.0);
    assert_eq!(report.constructed_mass, 0.0);
    close(before_material, body.structural_mass() + minerals);
    close(before_energy, body.embodied_energy() + energy + report.dissipated_energy);
    let state = body.cellular.as_ref().unwrap();
    close(state.control_reports[0].recycle_rate, report.recycled_mass);
    for pool in &state.pools {
        assert!(pool.embodied_energy <= pool.mass * pool.product.kind.synthesis_cost() * EMBODIED_FRACTION + 1e-12);
    }
    assert!(body.cellular_json(Some(&program)).contains("\"recycleRate\":0.016"));
}

#[test]
fn recycling_controls_and_original_recipes_survive_checkpoint_seven_exactly() {
    let program = recycling_program();
    let mut body = stocked(&program, &[0.0; 4]);
    add_complete(&mut body, &program, 0);
    add_complete(&mut body, &program, 1);
    let mut request = controls(&program, 0.0);
    request.start = vec![false; 2];
    request.recycle = vec![0.5, 0.0];
    let mut energy = 100.0;
    let mut minerals = 1.0;
    work(&mut body, &program, &request, &mut energy, &mut minerals, 0.25);
    let mut writer = Writer::new();
    body.write_checkpoint(&mut writer);
    let mut reader = Reader::new(&writer.0);
    let mut restored = Body::read_checkpoint_version(&mut reader, 7).unwrap();
    assert!(reader.finished());
    assert_eq!(body, restored);
    assert_eq!(body.cellular_json(Some(&program)), restored.cellular_json(Some(&program)));
    let mut resumed_energy = energy;
    let mut resumed_minerals = minerals;
    for _ in 0..10 {
        work(&mut body, &program, &request, &mut energy, &mut minerals, 0.1);
        work(&mut restored, &program, &request, &mut resumed_energy, &mut resumed_minerals, 0.1);
    }
    assert_eq!(body, restored);
    assert_eq!(energy.to_bits(), resumed_energy.to_bits());
    assert_eq!(minerals.to_bits(), resumed_minerals.to_bits());
    assert!(Body::read_checkpoint_version(&mut Reader::new(&writer.0), 6).is_err(),
        "A v6 tag reader must reject the v7 recycling mechanism");
}

#[test]
fn fractional_recycling_effort_finishes_without_unaccounted_residue() {
    let program = recycling_program();
    let mut body = stocked(&program, &[0.0; 4]);
    add_complete(&mut body, &program, 0);
    add_complete(&mut body, &program, 1);
    let mut request = controls(&program, 0.0);
    request.start = vec![false; 2];
    request.recycle = vec![0.5, 0.0];
    let mut energy = 100.0;
    let mut minerals = 1.0;
    let before = body.structural_mass() + minerals;
    let mut recycled = 0.0;
    for _ in 0..60 {
        recycled += work(&mut body, &program, &request, &mut energy, &mut minerals, 0.1).recycled_mass;
    }
    assert!(!body.cellular.as_ref().unwrap().assemblies.iter().any(|a| a.recipe.name == "Swim"));
    close(recycled, 0.04);
    close(before, body.structural_mass() + minerals);
}

#[test]
fn recovered_ingredients_are_available_only_to_the_following_work_snapshot() {
    let mut program = recycling_program();
    let mut next = program.assemblies[0].clone();
    next.name = "Replacement".into();
    program.assemblies.push(next);
    let mut body = stocked(&program, &[0.0; 4]);
    add_complete(&mut body, &program, 0);
    add_complete(&mut body, &program, 1);
    let mut request = controls(&program, 0.0);
    request.start = vec![false, false, true];
    request.recycle = vec![1.0, 0.0, 0.0];
    let mut energy = 100.0;
    let mut minerals = 1.0;
    let first = work(&mut body, &program, &request, &mut energy, &mut minerals, 1.0);
    assert!(first.recycled_mass > 0.0);
    assert_eq!(first.constructed_mass, 0.0);
    assert!(!body.cellular.as_ref().unwrap().assemblies.iter().any(|a| a.recipe.name == "Replacement"));
    let second = work(&mut body, &program, &request, &mut energy, &mut minerals, 1.0);
    assert!(second.constructed_mass > 0.0);
    assert!(body.cellular.as_ref().unwrap().assemblies.iter().any(|a| a.recipe.name == "Replacement"));
}

#[test]
fn recycling_inherited_recipe_selects_name_but_recovers_original_ingredient_identity() {
    let original = recycling_program();
    let mut body = stocked(&original, &[0.0; 4]);
    add_complete(&mut body, &original, 0);
    add_complete(&mut body, &original, 1);
    let _ = body.cellular_inputs(&original); // Warm the old lookup tables.
    let mut replacement = original.clone();
    replacement.products[1].kind = ProductKind::Scaffold;
    replacement.assemblies[0].kind = AssemblyKind::Mechanism(MechanismKind::Shield);
    replacement.assemblies[0].parts = vec![(1, 0.04)];
    replacement.validate().unwrap();
    let mut request = controls(&replacement, 0.0);
    request.start = vec![false; 2];
    request.recycle = vec![1.0, 0.0];
    let mut energy = 100.0;
    let mut minerals = 1.0;
    let before = body.structural_mass() + minerals;
    let report = work(&mut body, &replacement, &request, &mut energy, &mut minerals, 1.0);
    assert!(report.recycled_mass > 0.0);
    close(body.structural_mass() + minerals, before);
    let state = body.cellular.as_ref().unwrap();
    assert_eq!(state.assemblies[0].recipe.kind, original.assemblies[0].kind);
    assert!(state.pools.iter().any(|p| p.product.name == "Motor" && p.product.kind == ProductKind::Motor && p.mass > 0.0));
    let (products, built) = body.cellular_inputs(&replacement);
    assert_eq!(products[1], 0.0, "Recovered motor matter is not scaffold despite sharing a name");
    assert_eq!(built[0], 0.0, "An inherited motor is not a shield");
}

#[test]
fn full_inherited_pool_table_recovers_minerals_instead_of_discarding_material() {
    let program = recycling_program();
    let mut body = stocked(&program, &[0.0; 4]);
    add_complete(&mut body, &program, 0);
    add_complete(&mut body, &program, 1);
    let state = body.cellular.as_mut().unwrap();
    state.pools = (0..MAX_POOLS).map(|i| ProductPool {
        product: product(&format!("Inherited{i}"), ProductKind::Scaffold),
        mass: 0.0, embodied_energy: 0.0, expression_requested: 0.0, expression_rate: 0.0,
    }).collect();
    let mut request = controls(&program, 0.0);
    request.start = vec![false; 2];
    request.recycle = vec![1.0, 0.0];
    let mut energy = 100.0;
    let mut minerals = 1.0;
    let before = body.structural_mass() + minerals;
    let report = work(&mut body, &program, &request, &mut energy, &mut minerals, 1.0);
    assert!(report.recycled_mass > 0.0);
    close(report.minerals_recovered, report.recycled_mass);
    close(before, body.structural_mass() + minerals);
}

#[test]
fn recycling_needs_room_for_recovered_loose_material_and_does_not_use_freed_space() {
    let mut program = recycling_program();
    program.products.push(product("Filler", ProductKind::Scaffold));
    let mut body = stocked(&program, &[0.0; 5]);
    add_complete(&mut body, &program, 0);
    add_complete(&mut body, &program, 1);
    let free = body.space().free;
    let pool = &mut body.cellular.as_mut().unwrap().pools[4];
    pool.mass = free;
    pool.embodied_energy = free * 40.0 * EMBODIED_FRACTION;
    let mut request = controls(&program, 0.0);
    request.start = vec![false; 2];
    request.recycle = vec![1.0, 0.0];
    let mut energy = 100.0;
    let mut minerals = 1.0;
    let report = work(&mut body, &program, &request, &mut energy, &mut minerals, 1.0);
    close(report.recycled_mass, 0.0);
    assert!(body.space().used <= body.space().capacity + 1e-12);
}

#[test]
fn cached_bindings_reused_buffers_and_phase_summaries_preserve_exact_snapshot_values() {
    let mut program = recycling_program();
    program.products.push(product("BatteryWall", ProductKind::Membrane));
    program.assemblies.push(AssemblyDef { name: "Battery".into(),
        kind: AssemblyKind::Mechanism(MechanismKind::EnergyStorage),
        parts: vec![(4, 0.04)], at: 0.0, direction: 0.0 });
    program.products.push(product("Frame", ProductKind::Scaffold));
    for name in ["LegacyCoreA", "LegacyCoreB"] {
        program.assemblies.push(AssemblyDef { name: name.into(),
            kind: AssemblyKind::Organelle(OrganelleKind::Core),
            parts: vec![(5, 0.03), (3, 0.01)], at: 0.0, direction: 0.0 });
    }
    let mut body = stocked(&program, &[0.01, 0.007, 0.023, 0.003, 0.002, 0.001]);
    for definition in 0..program.assemblies.len() { add_complete(&mut body, &program, definition); }
    let mut old = body.organs[0].clone();
    old.id = body.next_organelle_id;
    body.next_organelle_id += 1;
    old.mass = 0.047;
    old.integrity = 0.83;
    body.organs.push(old.clone());
    old.id = body.next_organelle_id;
    body.next_organelle_id += 1;
    old.kind = OrganelleKind::Reserve;
    body.organs.push(old);
    let mut products = Vec::new();
    let mut built = Vec::new();
    let mut active = Vec::new();
    let mut measures = Vec::new();
    for step in 0..16 {
        body.area = 0.75 + step as f64 * 0.2;
        let state = body.cellular.as_mut().unwrap();
        state.assemblies[step % 3].integrity = 0.25 + step as f64 * 0.04;
        state.assemblies[step % 3].activity = step as f64 / 16.0;
        body.cellular_inputs_into(&program, &mut products, &mut built);
        body.active_recipe_masses_into(&program, &mut active);
        let state = body.cellular.as_ref().unwrap();
        for (i, def) in program.assemblies.iter().enumerate() {
            let expected = state.assemblies.iter().filter(|a| a.recipe.matches(&program, def))
                .map(AssemblyInstance::functional_mass).sum::<f64>();
            assert_eq!(built[i].to_bits(), expected.to_bits());
            let expected = state.assemblies.iter().filter(|a| a.recipe.matches(&program, def))
                .map(|a| a.functional_mass() * a.activity).sum::<f64>();
            assert_eq!(active[i].to_bits(), expected.to_bits());
        }
        let summary = body.work_snapshot(&mut measures);
        assert_eq!(summary.free_space.to_bits(), body.space().free.to_bits());
        assert_eq!(summary.free_surface.to_bits(), body.surface().free.to_bits());
        assert_eq!(summary.support_area.to_bits(), (body.support_mass() / CORE_MASS_PER_AREA).to_bits());
    }
    let pointers = (products.as_ptr(), built.as_ptr(), active.as_ptr());
    let mut before = Writer::new();
    body.write_checkpoint(&mut before);
    body.cellular_inputs_into(&program, &mut products, &mut built);
    body.active_recipe_masses_into(&program, &mut active);
    assert_eq!(pointers, (products.as_ptr(), built.as_ptr(), active.as_ptr()));
    let mut after = Writer::new();
    body.write_checkpoint(&mut after);
    assert_eq!(before.0, after.0, "Derived binding caches are absent from canonical saves");
    program.assemblies[0].direction = 0.5;
    body.cellular_inputs_into(&program, &mut products, &mut built);
    assert_eq!(built[0], 0.0, "Changing geometry requires a new physical recipe identity");
}

#[test]
fn directional_shields_and_contact_damage_follow_attachment_without_losing_matter() {
    let mut program = recycling_program();
    program.products.push(product("Armor", ProductKind::Scaffold));
    program.assemblies.push(AssemblyDef { name: "FrontShield".into(),
        kind: AssemblyKind::Mechanism(MechanismKind::Shield), parts: vec![(4, 0.04)],
        at: 0.0, direction: 0.0 });
    let mut back = program.assemblies[0].clone();
    back.name = "RearMotor".into();
    back.at = std::f64::consts::PI;
    program.assemblies.push(back);
    let mut body = stocked(&program, &[0.0; 5]);
    for definition in 0..4 { add_complete(&mut body, &program, definition); }
    assert!(body.protection_at(0.0, 1.0) > 0.0);
    close(body.protection_at(std::f64::consts::PI, 1.0), 0.0);
    let protected = body.protection_at(0.0, 1.0);
    body.cellular.as_mut().unwrap().assemblies[2].activity = 0.0;
    close(body.protection_at(0.0, 1.0), protected * 0.5);
    let material = body.structural_mass();
    let embodied = body.embodied_energy();
    let used = body.space().used;
    body.damage_attributed(0.3, &[(0.0, 0.2), (std::f64::consts::PI, 0.1)]);
    let state = body.cellular.as_ref().unwrap();
    close(state.assemblies[0].integrity, 0.8);
    close(state.assemblies[1].integrity, 0.7); // Internal recycler.
    close(state.assemblies[2].integrity, 0.8);
    close(state.assemblies[3].integrity, 0.9);
    close(body.organs[0].integrity, 0.7);
    assert_eq!(material.to_bits(), body.structural_mass().to_bits());
    assert_eq!(embodied.to_bits(), body.embodied_energy().to_bits());
    assert_eq!(used.to_bits(), body.space().used.to_bits());
}
