use super::*;

const SOLAR: &str = include_str!("../../../examples/solar.cell");

fn solar_world() -> (World, Vec<Genome>) {
    let program = Program::compile(SOLAR).unwrap();
    let genomes = vec![Genome::new(SOLAR.into(), program, 0, 0)];
    let mut world = World::empty(42);
    world.environment.configure(0.5, 0.85, 0.0);
    world.environment.set_initial_nutrients(2.0);
    world.reset_empty(42);

    world.set_mutation_rate(0.0);
    world.seed_cells(1, 1, 600.0, 380.0);
    (world, genomes)
}

#[test]
fn cellular_expression_precedes_physical_assembly_and_light_harvesting() {
    let (mut world, mut genomes) = solar_world();
    let mineral_total = world.nutrient_total();
    let initial_energy = world.cells[0].energy;
    assert_eq!(world.cells[0].body.capacities().light_harvest, 0.0);
    world.step(&mut genomes);
    let cell = &world.cells[0];
    let (pools, built) = cell.body.cellular_inputs(genomes[0].program.cellular().unwrap());
    assert!(pools.iter().sum::<f64>() > 0.0);
    assert_eq!(built.iter().sum::<f64>(), 0.0);
    assert_eq!(world.photosynthesis_total, 0.0);
    assert!(cell.energy < initial_energy && cell.nutrient_stored < 0.6);
    for _ in 0..600 {
        world.step(&mut genomes);
        assert!((world.nutrient_total() - mineral_total).abs() < 1e-7);
        assert!(world.energy_total() <= world.energy_input_total() + world.photosynthesis_total + 1e-7);
    }
    assert_eq!(world.cells.len(), 1);
    assert!(world.cells[0].body.capacities().light_harvest > 0.0);
    assert_eq!(world.cells[0].body.cellular.as_ref().unwrap().assemblies.iter()
        .filter(|assembly| assembly.complete()).count(), 1,
        "Small stock buffers must not fund a second complete solar machine");
    assert!(world.photosynthesis_total > 0.0);
    assert_eq!(world.births, 0, "An energy source cannot trigger division without replication machinery");
}

#[test]
fn a_missing_typed_ingredient_prevents_physical_function() {
    let source = SOLAR.replace(
        "if built.Solar + 0.000001 < 0.04 && pool.Pigment < 0.002 then 1 else 0",
        "0",
    );
    let (mut world, _) = solar_world();
    let mut genomes = vec![Genome::new(source.clone(), Program::compile(&source).unwrap(), 0, 0)];
    for _ in 0..600 { world.step(&mut genomes); }
    assert_eq!(world.cells.len(), 1);
    let (pools, _) = world.cells[0].body.cellular_inputs(genomes[0].program.cellular().unwrap());
    assert_eq!(pools[1], 0.0);
    assert_eq!(world.cells[0].body.capacities().light_harvest, 0.0);
    assert_eq!(world.photosynthesis_total, 0.0);
}
