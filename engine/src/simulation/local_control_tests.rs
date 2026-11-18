//! Behavioral tests cross the compiler, paid physical machinery and world phases.
use super::*;
use super::mechanism_tests::equipped as paid_equipped;
use crate::checkpoint::Writer;

fn equipped(source: &str) -> (World, Vec<Genome>) {
    let (mut world, genomes) = paid_equipped(source);
    world.cells[0].x = 600.0;
    world.cells[0].y = 380.0;
    (world, genomes)
}

fn reading(world: &World, genomes: &[Genome], name: &str) -> ReceptorInput {
    let program = &genomes[0].program;
    let index = program.cellular().unwrap().assemblies.iter().position(|recipe| recipe.name == name).unwrap();
    world.sense_program(1, program).unwrap().receptors[index]
}

fn particle(world: &mut World, x: f64, y: f64) {
    assert_eq!(world.add_food(x, y, 1), 1);
    let last = world.food.last_mut().unwrap();
    last.x = x;
    last.y = y;
}

fn bytes(world: &World) -> Vec<u8> {
    let mut writer = Writer::new();
    world.write_checkpoint(&mut writer);
    writer.0
}

const EYE: &str = r#"
assembly Eye: sense(optical) { Mount:0.005; Detector:0.01; Pigment:0.005; at:0; direction:0; }
gene watch: Catalyst = if sensed.Eye.valid then 0.1 else 0;
"#;

#[test]
fn primitive_optics_actually_see_and_named_readings_obey_orientation_occlusion_and_damage() {
    let (mut world, genomes) = equipped(EYE);
    world.environment.configure(0.5, 1.0, 0.0);
    particle(&mut world, 640.0, 380.0);
    let before = bytes(&world);
    let legacy = world.sense_cell(1).unwrap();
    assert!(legacy.object_available && legacy.object_valid, "New optical machines must have a real viewing width");
    let eye = reading(&world, &genomes, "Eye");
    assert!(eye.available && eye.valid);
    assert!(eye.offset[0] > 25.0 && eye.offset[0] < 40.0, "Offset begins at the receptor, not the cell center");
    assert_eq!(bytes(&world), before, "Inspection must not commit state");
    world.cells[0].angle = PI;
    assert!(!reading(&world, &genomes, "Eye").valid);
    world.cells[0].angle = 0.0;
    world.try_seed_fixture(1, 1, 620.0, 380.0, BodyFixture::Bare).unwrap();
    world.cells[1].x = 620.0;
    world.cells[1].y = 380.0;
    let occluded = reading(&world, &genomes, "Eye");
    assert!(occluded.valid && occluded.area > 0.5, "The nearer body occludes the particle");
    let sensor = world.cells[0].body.cellular.as_mut().unwrap().assemblies.iter_mut()
        .find(|assembly| assembly.recipe.name == "Eye").unwrap();
    sensor.integrity = 0.0;
    assert_eq!(reading(&world, &genomes, "Eye"), ReceptorInput::default());
}

#[test]
fn named_fields_sample_attachment_points_rotate_and_wrap_without_free_gradients() {
    let source = r#"
assembly Front: sense(light) { Mount:0.005; Detector:0.01; Pigment:0.005; at:0; }
assembly Back: sense(light) { Mount:0.005; Detector:0.01; Pigment:0.005; at:3.141592653589793; }
gene compare: Catalyst = activate(sensed.Front.level - sensed.Back.level, 0.1);
"#;
    let (mut world, genomes) = equipped(source);
    world.environment.configure(0.5, 0.0, 0.0);
    world.environment.add_source(0, 630.0, 380.0, 1.0, 40.0);
    let front = reading(&world, &genomes, "Front").level;
    let back = reading(&world, &genomes, "Back").level;
    assert!(front > back + 0.01);
    world.cells[0].angle = PI;
    assert_eq!(reading(&world, &genomes, "Front").level, back);
    assert_eq!(reading(&world, &genomes, "Back").level, front);
    world.cells[0].angle = 0.0;
    world.cells[0].x = WIDTH - 2.0;
    world.environment.sources.clear();
    world.environment.add_source(0, 20.0, 380.0, 1.0, 40.0);
    assert!(reading(&world, &genomes, "Front").level > reading(&world, &genomes, "Back").level);
}

#[test]
fn contact_is_local_but_flow_remains_observable_without_a_touching_object() {
    let (mut world, genomes) = equipped(r#"
product Hair=filament();
assembly Touch:sense(contact) { Mount:0.002; Detector:0.006; Hair:0.002; at:0; }
gene respond:Catalyst = if sensed.Touch.valid then sensed.Touch.resistance else sensed.Touch.speed;
"#);
    world.cells[0].vx = 5.0;
    let empty = reading(&world, &genomes, "Touch");
    assert!(empty.available && !empty.valid && empty.speed > 0.0);
    let program = &genomes[0].program;
    let output = program.evaluate(&world.sense_program(1, program).unwrap());
    assert!(output.cellular.unwrap().expression.iter().sum::<f64>() > 0.0);
    particle(&mut world, 609.0, 380.0);
    assert!(reading(&world, &genomes, "Touch").valid);
    world.cells[0].angle = PI;
    assert!(!reading(&world, &genomes, "Touch").valid, "Front contact cannot teleport to a rear receptor");
}

#[test]
fn sensor_activity_changes_the_following_snapshot_and_unused_readings_are_not_sampled() {
    let (mut world, mut genomes) = equipped(&EYE.replace("at:0;", "activity:0; at:0;"));
    particle(&mut world, 640.0, 380.0);
    assert!(reading(&world, &genomes, "Eye").available, "Already stored active state is the observation snapshot");
    world.step(&mut genomes);
    assert!(!reading(&world, &genomes, "Eye").available);
    let unused = Program::compile(&genomes[0].source.replace("gene watch: Catalyst = if sensed.Eye.valid then 0.1 else 0;", "gene watch:Catalyst=0;")).unwrap();
    assert!(unused.receptor_reads().is_empty());
    assert!(world.sense_program(1, &unused).unwrap().receptors.iter().all(|input| !input.available));
}

#[test]
fn rear_piercers_hit_the_rear_and_shields_protect_their_own_surface() {
    fn scenario(shield_at: f64) -> (f64, f64) {
        let source = format!(r#"
product Barb=filament();
assembly Sting:pierce() {{ Mount:0.01; Barb:0.02; Catalyst:0.01; at:3.141592653589793; direction:3.141592653589793; }}
assembly Armour:shield() {{ Frame:0.04; at:{shield_at}; }}
"#);
        let (mut world, genomes) = equipped(&source);
        // A second paid physical specimen is placed behind the attacker.
        let (mut victim_world, _) = equipped(&source);
        let mut victim = victim_world.cells.remove(0);
        victim.id = 2;
        victim.x = 585.0;
        victim.y = 380.0;
        victim.angle = 0.0;
        // Disable the victim's weapon without changing its mass.
        victim.body.cellular.as_mut().unwrap().assemblies.iter_mut()
            .find(|assembly| assembly.recipe.name == "Sting").unwrap().activity = 0.0;
        world.cells.push(victim);
        let capacities: Vec<_> = world.cells.iter().map(|cell| cell.body.capacities()).collect();
        let mut outputs: Vec<_> = world.cells.iter().map(|cell| genomes[0].program.evaluate(
            &world.sense_program(cell.id, &genomes[0].program).unwrap())).collect();
        outputs[0].bite = 1.0;
        outputs[1].bite = 0.0;
        let mut workspace = PhaseWorkspace::default();
        workspace.geometry.rebuild(&world.cells, false, true);
        world.resolve_predation_in(&outputs, &capacities, &mut workspace);
        assert!(world.attacks > 0, "A rear-mounted weapon must reach a rear target");
        assert_eq!(world.cells[0].gut_energy, 0.0, "Primitive piercing cannot extract living reserves");
        let integrity = world.cells[1].integrity;
        let armour = world.cells[1].body.cellular.as_ref().unwrap().assemblies.iter()
            .find(|assembly| assembly.recipe.name == "Armour").unwrap().integrity;
        (integrity, armour)
    }
    let front = scenario(0.0);
    let back = scenario(PI);
    assert!(front.0 > back.0, "Shield facing the incoming rear-mounted attacker protects this contact");
    assert!(front.1 < 1.0 && back.1 == 1.0, "Surface damage follows the contact location");
}

#[test]
fn primitive_shield_activity_cannot_change_legacy_cortex_regulation() {
    let (mut world, genomes) = equipped(r#"
assembly Cortex:cortex { Frame:0.04; activity:1; }
assembly Rear:shield() { Frame:0.04; at:3.141592653589793; activity:0; }
"#);
    let mut outputs = genomes[0].program.evaluate(&world.sense_program(1, &genomes[0].program).unwrap());
    let controls = outputs.cellular.as_mut().unwrap();
    let rear = genomes[0].program.cellular().unwrap().assemblies.iter().position(|recipe| recipe.name == "Rear").unwrap();
    let mut protections = Vec::new();
    for activity in [0.0, 1.0] {
        controls.activity[rear] = activity;
        outputs.defend = world.cells[0].body.cellular_actions(genomes[0].program.cellular().unwrap(), controls).defend;
        let caps = vec![world.cells[0].body.capacities()];
        // The rear shield cannot cover this front contact or change the cortex's effort.
        let old_effort = world.legacy_shield_effort(0, &Outputs { cellular: Some(controls.clone()), defend: outputs.defend, ..Outputs::default() });
        protections.push(world.contact_protection(0, 0.0, old_effort, &caps));
    }
    assert_eq!(protections[0], protections[1]);
    assert!((protections[0] - 0.24).abs() < 1e-12);
}

#[test]
fn inactive_primitive_weapons_cannot_extend_legacy_piercing_reach() {
    let (mut world, genomes) = equipped(r#"
product Barb=filament();
assembly Old:piercer { Mount:0.01; Barb:0.04; Catalyst:0.01; }
assembly Rear:pierce() { Mount:0.01; Barb:0.04; Catalyst:0.01; at:3.141592653589793; direction:3.141592653589793; activity:0; }
"#);
    world.cells[0].body.cellular.as_mut().unwrap().assemblies.iter_mut()
        .find(|assembly| assembly.recipe.name == "Rear").unwrap().activity = 0.0;
    world.try_seed_fixture(1, 1, 618.3, 380.0, BodyFixture::Bare).unwrap();
    world.cells[1].x = 618.3;
    world.cells[1].y = 380.0;
    let caps: Vec<_> = world.cells.iter().map(|cell| cell.body.capacities()).collect();
    let mut output = genomes[0].program.evaluate(&world.sense_program(1, &genomes[0].program).unwrap());
    output.bite = 1.0;
    let outputs = vec![output, Outputs::default()];
    let mut workspace = PhaseWorkspace::default();
    workspace.geometry.rebuild(&world.cells, false, true);
    world.resolve_predation_in(&outputs, &caps, &mut workspace);
    assert_eq!(world.attacks, 0, "Inactive primitive material must not extend the old weapon's range");
    world.cells[1].x = 617.5;
    workspace.geometry.rebuild(&world.cells, false, true);
    world.resolve_predation_in(&outputs, &caps, &mut workspace);
    assert!(world.attacks > 0, "The paid old weapon still works at its own range");
}

#[test]
fn cellular_tick_buffers_reuse_allocations_and_v7_continuation_is_exact() {
    let (mut world, mut genomes) = equipped(EYE);
    particle(&mut world, 640.0, 380.0);
    world.step(&mut genomes);
    let input = &world.workspace.observations.inputs[0];
    let input_ptrs = (input.products.as_ptr(), input.assemblies.as_ptr(), input.receptors.as_ptr());
    let controls = world.workspace.outputs[0].cellular.as_ref().unwrap();
    let control_ptrs = (controls.expression.as_ptr(), controls.activity.as_ptr(), controls.start.as_ptr(), controls.recycle.as_ptr());
    world.step(&mut genomes);
    let input = &world.workspace.observations.inputs[0];
    assert_eq!(input_ptrs, (input.products.as_ptr(), input.assemblies.as_ptr(), input.receptors.as_ptr()));
    let controls = world.workspace.outputs[0].cellular.as_ref().unwrap();
    assert_eq!(control_ptrs, (controls.expression.as_ptr(), controls.activity.as_ptr(), controls.start.as_ptr(), controls.recycle.as_ptr()));
    let mut original = crate::Engine { world, genomes, ..crate::Engine::new() };
    let saved = crate::checkpoint::save(&original).unwrap();
    assert_eq!(u32::from_le_bytes(saved[8..12].try_into().unwrap()), 9);
    let mut restored = crate::checkpoint::restore(&saved).unwrap();
    assert_eq!(crate::checkpoint::save(&restored).unwrap(), saved);
    for _ in 0..60 {
        original.world.step(&mut original.genomes);
        restored.world.step(&mut restored.genomes);
    }
    assert_eq!(crate::checkpoint::save(&restored).unwrap(), crate::checkpoint::save(&original).unwrap());
    // Switching to a shorter legacy program cannot leak the previous cellular packet.
    let legacy = "thrust=0;turn=0;divide=false;feed=0;";
    original.genomes[0] = Genome::new(legacy.into(), Program::compile(legacy).unwrap(), 0, 0);
    original.world.step(&mut original.genomes);
    assert!(original.world.workspace.observations.inputs[0].products.is_empty());
    assert!(original.world.workspace.observations.inputs[0].receptors.is_empty());
    assert!(original.world.workspace.outputs[0].cellular.is_none());
}
