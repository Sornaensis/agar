use super::*;
use crate::cellular::{AssemblyInstance, Ingredient, ProductDef, Recipe, RecipePart};

fn recipe(name: &str, kind: MechanismKind, mass: f64, at: f64) -> Recipe {
    let roles = kind.required_roles();
    Recipe {
        name: name.into(), kind: AssemblyKind::Mechanism(kind), at, direction: 0.0,
        parts: roles.iter().map(|&role| RecipePart {
            product: ProductDef { name: role.name().into(), kind: role, half_life: 0.0 },
            required_mass: mass / roles.len() as f64,
        }).collect(),
    }
}

fn append(body: &mut Body, recipe: &Recipe, fraction: f64, integrity: f64) {
    let state = body.cellular.get_or_insert_with(CellularState::default);
    state.assemblies.push(AssemblyInstance {
        id: state.next_assembly_id, recipe: recipe.clone(), integrity, activity: 1.0,
        ingredients: recipe.parts.iter().map(|part| Ingredient {
            mass: part.required_mass * fraction,
            embodied_energy: part.required_mass * fraction * part.product.kind.synthesis_cost() * EMBODIED_FRACTION,
        }).collect(),
    });
    state.next_assembly_id += 1;
}

fn dividing_body() -> Body {
    let (mut body, _) = Body::founder();
    body.area = 2.0;
    body.scaffold_energy *= 2.0;
    let support = recipe("Support", MechanismKind::Support, 0.06, 0.0);
    append(&mut body, &support, 1.0, 1.0);
    append(&mut body, &support, 1.0, 1.0);
    body
}

fn assert_conserved(before: &Body, left: &Body, right: &Body) {
    before.validate().unwrap();
    left.validate().unwrap();
    right.validate().unwrap();
    assert!(left.viable() && right.viable());
    assert!((before.structural_mass() - left.structural_mass() - right.structural_mass()).abs() < 1e-9);
    assert!((before.embodied_energy() - left.embodied_energy() - right.embodied_energy()).abs() < 1e-9);
    let mut old = before.cellular.as_ref().unwrap().assemblies.clone();
    let mut actual: Vec<_> = [&left, &right].into_iter()
        .flat_map(|body| body.cellular.as_ref().unwrap().assemblies.iter().cloned()).collect();
    old.sort_by_key(|assembly| assembly.id);
    actual.sort_by_key(|assembly| assembly.id);
    assert_eq!(actual, old, "Whole physical records retain their IDs, ingredients, energy, activity and integrity");
    for child in [left, right] {
        assert!(child.space().used <= child.space().capacity + 1e-9);
        assert!(child.surface().used <= child.surface().capacity + 1e-9);
    }
}

#[test]
fn repeated_physical_recipes_are_distributed_without_cloning_or_retyping() {
    use MechanismKind::*;
    let mut body = dividing_body();
    let recipes = [
        recipe("Food", FoodStorage, 0.06, 0.0),
        recipe("Capture", ParticleCapture, 0.03, 0.0),
        recipe("Exudate", DigestExudate, 0.04, 0.0),
        recipe("Tissue", DigestTissue, 0.04, 0.0),
        recipe("Energy", EnergyStorage, 0.08, 0.0),
        recipe("Minerals", MineralStorage, 0.02, 0.0),
        recipe("Translation", Translation, 0.04, 0.0),
        // Same label and mechanism, different physical placement. Matching only
        // the family or name incorrectly puts both left motors on one daughter.
        recipe("Motor", Propulsion, 0.025, std::f64::consts::FRAC_PI_2),
        recipe("Motor", Propulsion, 0.025, -std::f64::consts::FRAC_PI_2),
        recipe("Eye", SenseOptical, 0.02, 0.0),
        recipe("Touch", SenseContact, 0.01, 0.0),
        recipe("Weapon", Pierce, 0.05, 0.0),
        recipe("Transport", MineralImport, 0.025, 0.0),
        recipe("Envelope", Grow, 0.02, 0.0),
        recipe("Copy", Replicate, 0.02, 0.0),
        recipe("Division", Constrict, 0.02, 0.0),
        recipe("Engulf", Engulf, 0.025, 0.0),
    ];
    for _ in 0..2 { for recipe in &recipes { append(&mut body, recipe, 1.0, 1.0); } }
    let (left, right) = body.partition().unwrap();
    assert_conserved(&body, &left, &right);
    for child in [&left, &right] {
        for recipe in &recipes {
            assert_eq!(child.cellular.as_ref().unwrap().assemblies.iter()
                .filter(|assembly| &assembly.recipe == recipe && assembly.functional_mass() > 0.0).count(), 1,
                "One existing complete copy of {} at {} should reach each daughter", recipe.name, recipe.at);
        }
    }
    body.cellular.as_mut().unwrap().assemblies.reverse();
    assert_eq!(body.partition(), Some((left, right)), "Physical ID ordering makes replay independent of vector order");
}

#[test]
fn partial_or_disabled_copies_do_not_replace_functioning_inherited_machinery() {
    let mut body = dividing_body();
    let machine = recipe("Enzyme", MechanismKind::DigestTissue, 0.06, 0.0);
    // Broken complete material gets an earlier ID; it must not satisfy the
    // working-copy balance when later complete instances are placed.
    append(&mut body, &machine, 1.0, 0.0);
    append(&mut body, &machine, 0.7, 1.0);
    append(&mut body, &machine, 1.0, 1.0);
    append(&mut body, &machine, 1.0, 1.0);
    let mut changed_product = machine.clone();
    changed_product.parts[0].product.half_life = 8.0;
    append(&mut body, &changed_product, 1.0, 1.0);
    append(&mut body, &changed_product, 1.0, 1.0);
    let (left, right) = body.partition().unwrap();
    assert_conserved(&body, &left, &right);
    for child in [&left, &right] {
        for recipe in [&machine, &changed_product] {
            assert_eq!(child.cellular.as_ref().unwrap().assemblies.iter()
                .filter(|assembly| &assembly.recipe == recipe && assembly.functional_mass() > 0.0).count(), 1);
        }
    }
}

#[test]
fn capacity_limits_take_priority_over_equal_recipe_counts() {
    let mut body = dividing_body();
    let larger = recipe("Large", MechanismKind::FoodStorage, 0.12, 0.0);
    let duplicated = recipe("Reserve", MechanismKind::EnergyStorage, 0.25 / 2.4, 0.0);
    for _ in 0..3 { append(&mut body, &larger, 1.0, 1.0); }
    for _ in 0..2 { append(&mut body, &duplicated, 1.0, 1.0); }
    append(&mut body, &recipe("Translation", MechanismKind::Translation, 0.1, 0.0), 1.0, 1.0);
    let (left, right) = body.partition().unwrap();
    assert_conserved(&body, &left, &right);
    let mut counts = [&left, &right].map(|child| child.cellular.as_ref().unwrap().assemblies
        .iter().filter(|assembly| assembly.recipe == duplicated).count());
    counts.sort();
    assert_eq!(counts, [0, 2], "The daughter holding two large compartments cannot also fit a reserve; whole instances must fit");
}
