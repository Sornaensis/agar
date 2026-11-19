use super::*;
use crate::checkpoint::Writer;

const IDLE: &str = "thrust=0;turn=0;divide=false;feed=0;absorb=0;";

fn scene(recipe: BodyFixture, source: &str, light: f64) -> (World, Vec<Genome>) {
    let program = Program::compile(source).unwrap();
    let genomes = vec![Genome::new(source.to_owned(), program, 0, 0)];
    let mut world = World::empty(107);
    world.environment.configure(0.5, light, 0.0);

    world.set_mutation_rate(0.0);
    add_cell(&mut world, recipe, 110.0, 110.0);
    (world, genomes)
}

fn add_cell(world: &mut World, recipe: BodyFixture, x: f64, y: f64) -> usize {
    let index = world.cells.len();
    world.try_seed_fixture(1, 1, x, y, recipe).unwrap();
    let cell = &mut world.cells[index];
    cell.x = x;
    cell.y = y;
    cell.vx = 0.0;
    cell.vy = 0.0;
    cell.angle = 0.0;
    index
}

fn add_particle(world: &mut World, x: f64, y: f64) -> usize {
    let index = world.food.len();
    assert_eq!(world.fixture_exudate(x, y, 1), 1);
    world.food[index].x = x;
    world.food[index].y = y;
    index
}

fn state(world: &World) -> Vec<u8> {
    let mut writer = Writer::new();
    world.write_checkpoint(&mut writer);
    assert!(!writer.failed());
    writer.0
}

fn grow_for_division(cell: &mut Cell) {
    let controls = BodyControls::default();
    for _ in 0..32 {
        let mass = cell.body.structural_mass();
        cell.body.work(
            1.0,
            &controls,
            0.0,
            &mut cell.energy,
            &mut cell.nutrient_stored,
            1.0,
            MAX_AREA,
        );
        cell.organic_carbon -= cell.body.structural_mass() - mass;
    }
    cell.cooldown = 0.0;
    assert!(
        cell.division_ready(),
        "paid frame growth must produce a physically eligible fixture"
    );
}

#[test]
fn ancestry_exhaustion_rejects_before_any_tick_state_is_committed() {
    for scenario in 0..3 {
        let source = "thrust=0;turn=0;divide=true;feed=0;absorb=0;ri001=ri001+1;";
        let (mut world, genomes) = scene(BodyFixture::Bare, source, 1.0);
        match scenario {
            0 => {
                grow_for_division(&mut world.cells[0]);
                world.next_id = u32::MAX;
            }
            1 => {
                let second = add_cell(&mut world, BodyFixture::Bare, 400.0, 200.0);
                grow_for_division(&mut world.cells[0]);
                grow_for_division(&mut world.cells[second]);
                world.next_id = u32::MAX - 1; // One address, two possible daughters.
            }
            _ => {
                let second = add_cell(&mut world, BodyFixture::Bare, 400.0, 200.0);
                grow_for_division(&mut world.cells[second]);
                world.cells[second].parent_id = world.cells[0].id;
                world.cells[second].generation = u32::MAX;
            }
        }
        let mut engine = crate::Engine {
            world,
            genomes,
            ..crate::Engine::new()
        };
        let before = crate::checkpoint::save(&engine).unwrap();
        assert!(
            crate::checkpoint::restore(&before).is_ok(),
            "fixture must itself be a valid restorable world"
        );
        let error = engine.world.try_step(&mut engine.genomes).unwrap_err();
        assert!(error.contains("ancestry"), "{scenario}: {error}");
        assert_eq!(
            crate::checkpoint::save(&engine).unwrap(),
            before,
            "scenario {scenario} partially advanced fields, registers, bodies, RNG, or births"
        );
    }
}

fn observe_and_retain_tracks(world: &mut World) -> Inputs {
    let (mut inputs, tracks) = world.observations(Some(0));
    world.cells[0].tracking = tracks[0].clone();
    inputs.remove(0)
}

#[test]
fn skipping_unused_pure_sensors_preserves_outputs_and_tracking() {
    for source in [IDLE,
        "thrust=light.level;turn=chemical.organic_bearing;divide=false;feed=flow.speed;absorb=heat.value;",
        "thrust=length(object.motion);turn=angle(contact.offset);divide=false;feed=chemical.mineral;absorb=light.bearing;",
    ] {
        let (mut world, genomes) = scene(BodyFixture::Hunter, source, 0.8);
        world.environment.configure(0.3, 0.8, 8.0);
        world.environment.add_source(0, 125.0, 120.0, 0.6, 80.0);
        world.environment.emit_organic(110.0, 110.0, 12.0);
        add_particle(&mut world, 118.0, 110.0);
        add_cell(&mut world, BodyFixture::Hunter, 143.0, 110.0);
        let caps: Vec<_> = world.cells.iter().map(|cell| cell.body.capacities()).collect();
        let mut complete = ObservationWorkspace::default();
        let mut used = ObservationWorkspace::default();
        let before = state(&world);
        world.observations_into(None, &caps, &mut complete, None);
        world.observations_into(None, &caps, &mut used, Some(&genomes));
        for (all, used) in complete.inputs.iter().zip(&used.inputs) {
            assert_eq!(genomes[0].program.evaluate(all), genomes[0].program.evaluate(used));
        }
        for (all, used) in complete.tracking.iter().zip(&used.tracking) {
            assert_eq!(all.next_serial, used.next_serial);
            assert_eq!(format!("{:?}", all.entries), format!("{:?}", used.entries));
        }
        assert_eq!(before, state(&world), "sampling must remain read-only");
    }
}

#[test]
fn absent_receptors_reveal_no_environment_or_target_properties() {
    let (mut world, _) = scene(BodyFixture::Bare, IDLE, 1.0);
    world.environment.configure(0.3, 1.0, 20.0);
    world.environment.emit_organic(110.0, 110.0, 12.0);
    add_particle(&mut world, 118.0, 110.0);
    add_cell(&mut world, BodyFixture::Hunter, 123.0, 110.0);
    let input = world.sense_cell(1).unwrap();
    for flag in [
        input.light_available,
        input.light_valid,
        input.heat_available,
        input.heat_valid,
        input.chemical_available,
        input.chemical_valid,
        input.flow_available,
        input.flow_valid,
        input.contact_available,
        input.contact_valid,
        input.object_available,
        input.object_valid,
    ] {
        assert!(!flag);
    }
    for number in [
        input.light_level,
        input.light_bearing,
        input.heat_value,
        input.heat_bearing,
        input.chemical_mineral,
        input.chemical_organic,
        input.chemical_mineral_bearing,
        input.chemical_organic_bearing,
        input.flow_speed,
        input.flow_bearing,
        input.contact_pressure,
        input.contact_resistance,
        input.object_area,
        input.object_contrast,
        input.object_pigment,
    ] {
        assert_eq!(number, 0.0);
    }
    assert_eq!(input.flow_vector, [0.0; 2]);
    assert_eq!(input.contact_offset, [0.0; 2]);
    assert_eq!(input.object_offset, [0.0; 2]);
    assert_eq!(input.object_motion, [0.0; 2]);
    assert!(input.contact_token.is_empty() && input.object_token.is_empty());
    assert!(input.energy > 0.0 && input.body_mass > 0.0);
    assert!(
        input.organelle[Kind::Core as usize].count > 0,
        "core self-state remains available"
    );
}

#[test]
fn chemical_receptors_sample_local_fields_not_nearest_food_and_damage_removes_access() {
    let (mut world, _) = scene(BodyFixture::Scavenger, IDLE, 1.0);
    world.environment.emit_organic(110.0, 110.0, 4.0);
    world.environment.add_nutrients(110.0, 110.0, 2.0);
    add_particle(&mut world, 170.0, 110.0);
    let before = world.sense_cell(1).unwrap();
    assert!(before.chemical_available && before.chemical_valid);
    assert!(before.chemical_mineral > 0.3 && before.chemical_organic > 0.0);
    assert!(!before.light_available && !before.heat_available);
    world.food[0].x = 800.0;
    world.food[0].y = 600.0;
    let moved = world.sense_cell(1).unwrap();
    assert_eq!(
        (
            before.chemical_mineral,
            before.chemical_organic,
            before.chemical_organic_bearing
        ),
        (
            moved.chemical_mineral,
            moved.chemical_organic,
            moved.chemical_organic_bearing
        )
    );
    for organ in &mut world.cells[0].body.organs {
        if organ.kind == Kind::Chemoreceptor {
            organ.integrity = 0.1;
        }
    }
    let broken = world.sense_cell(1).unwrap();
    assert!(!broken.chemical_available && !broken.chemical_valid);
    assert_eq!(broken.chemical_mineral, 0.0);
    assert_eq!(broken.chemical_organic, 0.0);
    assert!(world.environment.organic_at(110.0, 110.0) > 0.0);
}

#[test]
fn optical_hardware_requires_illuminated_surfaces_and_loses_range_in_dim_light() {
    let (mut world, _) = scene(BodyFixture::Hunter, IDLE, 0.0);
    add_particle(&mut world, 170.0, 110.0);
    let dark = world.sense_cell(1).unwrap();
    assert!(dark.object_available && !dark.object_valid);
    assert!(dark.object_token.is_empty());
    assert_eq!(dark.object_offset, [0.0; 2]);
    world.environment.configure(0.5, 1.0, 0.0);
    let lit = world.sense_cell(1).unwrap();
    assert!(lit.object_valid && lit.object_contrast > 0.0);
    assert!((lit.object_offset[0] - 60.0).abs() < 1.0);
    world.environment.configure(0.5, 0.25, 0.0);
    assert!(
        !world.sense_cell(1).unwrap().object_valid,
        "dim light shortens hardware reach"
    );
    world.environment.configure(0.5, 0.0, 0.0);
    assert_ne!(world.environment.add_source(0, 170.0, 110.0, 1.0, 40.0), 0);
    assert!(
        world.environment.light_at(110.0, 110.0) < 0.02,
        "the observer is outside the bright patch"
    );
    assert!(world.environment.light_at(170.0, 110.0) > 0.9);
    assert!(
        world.sense_cell(1).unwrap().object_valid,
        "an observer in shadow can see a lit surface"
    );
}

#[test]
fn occlusion_and_toroidal_wrapping_apply_even_to_remembered_targets() {
    let (mut world, _) = scene(BodyFixture::Hunter, IDLE, 1.0);
    world.cells[0].x = WIDTH - 20.0;
    add_particle(&mut world, 20.0, 110.0);
    let first = observe_and_retain_tracks(&mut world);
    assert!(first.object_valid);
    let target = first.object_token;
    world.cells[0].scan_target = target.clone();
    let blocker = add_cell(&mut world, BodyFixture::Bare, 0.0, 110.0);
    let blocked = world.sense_cell(1).unwrap();
    assert!(
        blocked.object_valid,
        "the occluding surface remains visible"
    );
    assert_ne!(
        blocked.object_token, target,
        "a track token cannot see through a cell"
    );
    assert!(blocked.object_area > 0.5);
    world.cells[blocker].y = 150.0;
    assert_eq!(world.sense_cell(1).unwrap().object_token, target);
    world.cells[blocker].x = 20.0;
    world.cells[blocker].y = 110.0;
    assert_ne!(
        world.sense_cell(1).unwrap().object_token,
        target,
        "a particle inside another cell is hidden"
    );
}

#[test]
fn flow_bearing_cannot_recover_direction_below_vector_resolution() {
    let (mut world, _) = scene(BodyFixture::Scavenger, IDLE, 1.0);
    world.cells[0].vx = -0.001;
    world.cells[0].vy = -0.003;
    let low = world.sense_cell(1).unwrap();
    assert!(low.flow_available && low.flow_valid);
    assert_eq!(low.flow_vector, [0.0; 2]);
    assert_eq!(low.flow_speed, 0.0);
    assert_eq!(low.flow_bearing, 0.0);
    world.cells[0].vx = -1.23;
    world.cells[0].vy = -2.34;
    let measured = world.sense_cell(1).unwrap();
    assert_eq!(
        measured.flow_bearing,
        measured.flow_vector[1].atan2(measured.flow_vector[0])
    );
}

#[test]
fn tracks_expire_without_aliasing_and_remembered_tokens_grant_no_remote_access() {
    let (mut world, _) = scene(BodyFixture::Hunter, IDLE, 1.0);
    add_particle(&mut world, 160.0, 110.0);
    let original = observe_and_retain_tracks(&mut world).object_token;
    world.ticks = TRACK_TTL;
    assert_eq!(observe_and_retain_tracks(&mut world).object_token, original);
    world.cells[0].scan_target = original.clone();
    world.food[0].x = 700.0;
    let remote = world.sense_cell(1).unwrap();
    assert!(remote.object_available && !remote.object_valid);
    assert!(remote.object_token.is_empty());
    assert_eq!(remote.object_offset, [0.0; 2]);
    world.ticks += TRACK_TTL + 1;
    world.food[0].x = 160.0;
    let reacquired = observe_and_retain_tracks(&mut world).object_token;
    assert!(!reacquired.is_empty() && reacquired != original);
    world.food[0].x = 700.0;
    add_particle(&mut world, 160.0, 110.0);
    let replacement = observe_and_retain_tracks(&mut world).object_token;
    assert!(replacement != reacquired && replacement != original);
    assert!(world.cells[0].tracking.entries.len() <= TRACK_LIMIT);
}

#[test]
fn scan_requests_change_the_next_sample_without_rewriting_current_memory_reads() {
    let source = "rb001=object.valid;rs001=object.token;scan.bearing=3.141592653589793;scan.width=1;thrust=0;turn=0;divide=false;feed=0;absorb=0;";
    let (mut world, mut genomes) = scene(BodyFixture::Hunter, source, 1.0);
    add_particle(&mut world, 160.0, 110.0);
    assert!(world.sense_cell(1).unwrap().object_valid);
    world.try_step(&mut genomes).unwrap();
    assert_eq!(
        world.cells[0].memory.value("rb001"),
        Some(&crate::genome::Value::Boolean(true))
    );
    assert!(!world.sense_cell(1).unwrap().object_valid);
    world.try_step(&mut genomes).unwrap();
    assert_eq!(
        world.cells[0].memory.value("rb001"),
        Some(&crate::genome::Value::Boolean(false))
    );
}

#[test]
fn inspection_repeats_without_committing_tracks_memory_or_random_state() {
    let source = "ri001=ri001+1;rs001=object.token;thrust=0;turn=0;divide=false;feed=0;absorb=0;";
    let (mut world, mut genomes) = scene(BodyFixture::Hunter, source, 1.0);
    add_particle(&mut world, 160.0, 110.0);
    world.try_step(&mut genomes).unwrap();
    let before = state(&world);
    let mut previous = None;
    for _ in 0..4 {
        let input = world.sense_cell(1).unwrap();
        assert!(input.object_valid);
        let preview =
            Program::evaluation_json(source, &input, &world.cells[0].memory, 1, 1, world.ticks)
                .unwrap();
        if let Some(previous) = &previous {
            assert_eq!(&preview, previous);
        }
        previous = Some(preview);
        world.sense_all();
        assert_eq!(state(&world), before);
    }
}

#[test]
fn optical_return_does_not_depend_on_hidden_target_energy_genome_or_cell_integrity() {
    let (mut world, _) = scene(BodyFixture::Hunter, IDLE, 1.0);
    let target = add_cell(&mut world, BodyFixture::Bare, 160.0, 110.0);
    let before = observe_and_retain_tracks(&mut world);
    assert!(before.object_valid);
    world.cells[target].energy = 0.001;
    world.cells[target].integrity = 0.25;
    world.cells[target].generation = 200;
    world.cells[target].genome_id = 2;
    let after = world.sense_cell(1).unwrap();
    assert_eq!(
        (
            before.object_token,
            before.object_offset,
            before.object_area,
            before.object_contrast,
            before.object_pigment,
            before.object_motion
        ),
        (
            after.object_token,
            after.object_offset,
            after.object_area,
            after.object_contrast,
            after.object_pigment,
            after.object_motion
        )
    );
}
