use super::*;
use crate::cellular::{AssemblyInstance, Ingredient, ProductDef, Recipe, RecipePart};

fn recipe(name: &str, kind: MechanismKind, mass: f64, at: f64) -> std::rc::Rc<Recipe> {
    let roles = kind.required_roles();
    std::rc::Rc::new(Recipe {
        name: name.into(), kind: AssemblyKind::Mechanism(kind), at, direction: 0.0,
        parts: roles.iter().map(|&role| RecipePart {
            product: ProductDef { name: role.name().into(), kind: role, half_life: 0.0 },
            required_mass: mass / roles.len() as f64,
        }).collect(),
    })
}

fn append(body: &mut Body, recipe: &std::rc::Rc<Recipe>, fraction: f64, integrity: f64) {
    let state = body.cellular.get_or_insert_with(CellularState::default);
    state.assemblies.push(AssemblyInstance { inherited_at: None,
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
    for a in &mut actual { a.inherited_at = old.iter().find(|o| o.id == a.id).unwrap().inherited_at; }
    assert_eq!(actual, old, "Whole physical records retain their IDs, ingredients, energy, activity and integrity");
    for child in [left, right] {
        assert!(child.space().used <= child.space().capacity + 1e-9);
        assert!(child.surface().used <= child.surface().capacity + 1e-9);
    }
}

#[test]
fn constrictor_sets_axis_and_mounted_copies_follow_their_hemisphere() {
    use MechanismKind::*;
    use std::f64::consts::{PI,FRAC_PI_2};
    let mut body = dividing_body();
    append(&mut body,&recipe("Division",Constrict,0.02,FRAC_PI_2),1.0,1.0);
    let upper=recipe("Upper",SenseContact,0.01,FRAC_PI_2);
    let lower=recipe("Lower",SenseContact,0.01,-FRAC_PI_2);
    let edge=recipe("Edge",SenseContact,0.01,0.02);
    append(&mut body,&upper,1.0,1.0); append(&mut body,&upper,1.0,1.0);
    append(&mut body,&lower,0.5,1.0); append(&mut body,&lower,1.0,0.0);
    append(&mut body,&edge,1.0,1.0);
    assert_eq!(body.cleavage_axis(),Some(FRAC_PI_2));
    let (left,right)=body.partition().unwrap();
    assert_conserved(&body,&left,&right);
    let half=|b:&Body,name:&str| b.cellular.as_ref().unwrap().assemblies.iter()
        .filter(|a|a.recipe.name==name).count();
    assert_eq!([half(&left,"Upper"),half(&right,"Upper")],[0,2]);
    assert_eq!([half(&left,"Lower"),half(&right,"Lower")],[2,0]);
    let septal=right.cellular.as_ref().unwrap().assemblies.iter().find(|a|a.recipe.name=="Edge").unwrap();
    assert!((septal.at()-(-PI/2.0+0.04)).abs()<1e-12);
    let mut checkpoint = Writer::new();
    right.write_checkpoint(&mut checkpoint);
    let restored = Body::read_checkpoint_version(&mut Reader::new(&checkpoint.0), crate::checkpoint::VERSION).unwrap();
    assert_eq!(restored,right,"Inherited attachment angles survive a checkpoint");
    body.cellular.as_mut().unwrap().assemblies.reverse();
    assert_eq!(body.partition(),Some((left,right)));
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


#[test]
fn a_paid_furrow_keeps_its_plane_after_the_constrictor_switches_off() {
    let mut body = dividing_body();
    append(&mut body,&recipe("Ring",MechanismKind::Constrict,0.02,0.7),1.0,1.0);
    let state = body.cellular.as_mut().unwrap();
    state.constriction_axis = state.active_constrictor_axis();
    state.constriction = 0.5;
    state.assemblies.last_mut().unwrap().activity = 0.0;
    assert_eq!(body.cleavage_axis(), Some(0.7));
    let mut bytes = Writer::new();
    body.write_checkpoint(&mut bytes);
    let restored = Body::read_checkpoint_version(&mut Reader::new(&bytes.0), crate::checkpoint::VERSION).unwrap();
    assert_eq!(restored.cleavage_axis(), body.cleavage_axis());
    assert_eq!(restored.partition(), body.partition());
}
