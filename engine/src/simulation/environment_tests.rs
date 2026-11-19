use super::*;
use crate::checkpoint::{Reader, Writer};

const IDLE: &str = "thrust=0;turn=0;divide=false;feed=0;absorb=0;photosynthesize=0;";
fn programs() -> Vec<Genome> {
    vec![Genome::new(
        IDLE.into(),
        Program::compile(IDLE).unwrap(),
        0,
        0,
    )]
}
fn place(world: &mut World, kind: BodyFixture, x: f64, y: f64) -> usize {
    let i = world.cells.len();
    world.try_seed_fixture(1, 1, x, y, kind).unwrap();
    let c = &mut world.cells[i];
    c.x = x;
    c.y = y;
    c.vx = 0.0;
    c.vy = 0.0;
    c.angle = 0.0;
    i
}
fn bytes(world: &World) -> Vec<u8> {
    let mut w = Writer::new();
    world.write_checkpoint(&mut w);
    w.0
}

#[test]
fn enlarged_world_sensing_collisions_spawning_and_snapshots_use_live_dimensions() {
    let mut world = World::empty(42);
    assert!(world.set_world_size(2400.0, 1520.0));
    world.environment.configure(0.5, 1.0, 0.0);

    let observer = place(&mut world, BodyFixture::Hunter, 1205.0, 1000.0);
    let target = place(&mut world, BodyFixture::Bare, 1235.0, 1000.0);
    assert!(
        world
            .sense_cell(world.cells[observer].id)
            .unwrap()
            .object_valid,
        "east-half grid must find nearby targets"
    );
    world.cells[target].x = 5.0;
    assert!(
        !world
            .sense_cell(world.cells[observer].id)
            .unwrap()
            .object_valid,
        "old 1200-wide seam must not alias distant cells"
    );
    world.cells[observer].x = 2390.0;
    world.cells[target].x = 10.0;
    assert!(
        world
            .sense_cell(world.cells[observer].id)
            .unwrap()
            .object_valid,
        "actual expanded seam must wrap sensing"
    );
    world.cells[observer].x = 2399.0;
    world.cells[target].x = 1.0;
    world.resolve_collisions();
    let distance = delta(world.cells[target].x - world.cells[observer].x, 2400.0).hypot(delta(
        world.cells[target].y - world.cells[observer].y,
        1520.0,
    ));
    assert!(distance + 1e-8 >= world.cells[observer].radius() + world.cells[target].radius());
    let mut snapshot = Vec::new();
    world.snapshot(&mut snapshot);
    assert_eq!(&snapshot[7..9], &[2400.0, 1520.0]);
    world.render_snapshot(&mut snapshot);
    assert_eq!(&snapshot[7..9], &[2400.0, 1520.0]);
    world.seed_cells(1, 100, -1.0, -1.0);
    world.fixture_exudate(-1.0, -1.0, 100);
    assert!(
        world
            .cells
            .iter()
            .filter(|c| c.x > 1200.0 && c.y > 760.0)
            .count()
            > 10
    );
    assert!(
        world
            .food
            .iter()
            .filter(|f| f.x > 1200.0 && f.y > 760.0)
            .count()
            > 10
    );
    assert!(world.cells.iter().all(|c| c.x < 2400.0 && c.y < 1520.0));
    assert!(world.food.iter().all(|f| f.x < 2400.0 && f.y < 1520.0));
}

#[test]
fn barriers_stop_live_cells_and_particles_and_occlude_physical_vision() {
    let mut world = World::empty(42);

    world.environment.configure(0.5, 1.0, 0.0);
    let observer = place(&mut world, BodyFixture::Hunter, 550.0, 380.0);
    place(&mut world, BodyFixture::Bare, 650.0, 380.0);
    assert!(
        world
            .sense_cell(world.cells[observer].id)
            .unwrap()
            .object_valid
    );
    let barrier = world
        .environment
        .add_barrier(600.0, 380.0, 760.0, 20.0, PI * 0.5, false);
    assert!(barrier > 0);
    assert!(
        !world
            .sense_cell(world.cells[observer].id)
            .unwrap()
            .object_valid,
        "opaque walls block sight"
    );
    world.cells[observer].vx = 6000.0;
    world.fixture_exudate(550.0, 380.0, 1);
    world.food[0].x = 586.0;
    world.food[0].y = 380.0;
    let flow = world.environment.add_source(6, 600.0, 380.0, 1.0, 100.0);
    world
        .environment
        .update_source(flow, 6, 600.0, 380.0, 1.0, 100.0, 0.0, 1200.0);
    let mut registry = programs();
    world.step(&mut registry);
    assert!(
        world.cells[observer].x < 590.0 - world.cells[observer].radius() + 0.01,
        "fast cell tunneled through wall"
    );
    assert!(world.food[0].x <= 587.51, "particle entered wall");
    let total = world.nutrient_total();
    world.environment.remove_barrier(barrier);
    world.settle_barriers();
    assert_eq!(world.nutrient_total(), total);
}

#[test]
fn live_world_resize_and_absorber_roundtrip_keep_full_material_ledger_and_exact_replay() {
    let mut world = World::empty(91);

    world.set_mutation_rate(0.0);
    place(&mut world, BodyFixture::Bare, 600.0, 380.0);
    world.fixture_exudate(-1.0, -1.0, 10);
    let mass = world.nutrient_total();
    assert!(world.set_world_size(2400.0, 1520.0));
    assert_eq!((world.cells[0].x, world.cells[0].y), (1200.0, 760.0));
    assert!((world.nutrient_total() - mass).abs() < 1e-8);
    world.environment.add_source(4, 1200.0, 760.0, 1.0, 100.0);
    let source = world.environment.add_source(6, 1200.0, 400.0, 0.4, 200.0);
    world
        .environment
        .update_source(source, 6, 1200.0, 400.0, 0.4, 200.0, 0.0, 2400.0);
    world
        .environment
        .add_barrier(700.0, 400.0, 160.0, 30.0, 0.2, true);
    let mut registry = programs();
    for _ in 0..15 {
        world.step(&mut registry);
    }
    let saved = bytes(&world);
    let mut restored = World::read_checkpoint(&mut Reader::new(&saved), registry.len(), crate::checkpoint::VERSION).unwrap();
    assert_eq!(bytes(&restored), saved);
    for _ in 0..15 {
        world.step(&mut registry);
        restored.step(&mut registry);
    }
    assert_eq!(bytes(&world), bytes(&restored));
    assert!(
        (world.nutrient_total() + world.environment.nutrient_removed_total()
            - world.nutrient_input_total())
        .abs()
            < 1e-8
    );
    assert!(world.environment.nutrient_removed_total() > 0.0);
}
