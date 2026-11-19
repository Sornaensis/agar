use super::*;
use crate::checkpoint::{Reader, Writer};
use std::f64::consts::FRAC_PI_2;

fn state(env: &Environment) -> Vec<u8> {
    let mut w = Writer::new();
    env.write_checkpoint(&mut w);
    w.0
}
fn resume(env: &Environment) -> Environment {
    Environment::read_checkpoint(&mut Reader::new(&state(env)), 17).unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}
fn empty() -> Environment {
    let mut env = Environment::default();
    assert!(env.set_initial_gases(0.0, 0.0));
    env.reset_dynamic();
    env
}

#[test]
fn sealed_gases_use_conservative_nonnegative_transport_and_internal_transfers_have_no_external_charge(
) {
    let mut env = empty();
    env.release_oxygen(1190.0, 750.0, 20.0);
    env.release_inorganic_carbon(10.0, 10.0, 30.0);
    let i = env.patch_index(1190.0, 750.0);
    let moved = env.take_oxygen(i, 100.0);
    assert_eq!(moved, 20.0);
    env.release_oxygen(10.0, 10.0, moved);
    assert_eq!(env.take_oxygen(i, f64::NAN), 0.0);
    assert_eq!(env.take_inorganic_carbon(usize::MAX, 1.0), 0.0);
    env.add_source(2, 10.0, 10.0, 1.0, 120.0);
    let band = env.add_source(6, 600.0, 380.0, 1.0, 250.0);
    env.update_source(band, 6, 600.0, 380.0, 1.0, 250.0, 0.7, 1200.0);
    for tick in 0..60 {
        env.step(tick as f64 * DT);
    }
    close(env.oxygen_total(), 20.0);
    close(env.inorganic_carbon_total(), 30.0);
    assert!(env
        .gases
        .oxygen
        .iter()
        .chain(&env.gases.carbon)
        .all(|&n| n >= 0.0 && n.is_finite()));
    assert_eq!(
        (
            env.oxygen_input_total(),
            env.oxygen_output_total(),
            env.carbon_input_total(),
            env.carbon_output_total()
        ),
        (0.0, 0.0, 0.0, 0.0)
    );
    assert!(
        env.oxygen_patch(env.patch_index(1190.0, 10.0)) > 0.0,
        "diffusion must wrap the expanded fields"
    );
}

#[test]
fn gas_emitters_and_absorbers_reconcile_exact_actual_external_amounts() {
    let mut env = empty();
    let oxygen = env.add_source(8, 610.0, 390.0, 0.6, 80.0);
    let carbon = env.add_source(10, 610.0, 390.0, 0.6, 80.0);
    for tick in 0..30 {
        env.step(tick as f64 * DT);
    }
    close(env.oxygen_total(), 0.9);
    close(env.inorganic_carbon_total(), 0.9);
    close(env.oxygen_input_total(), 0.9);
    close(env.carbon_input_total(), 0.9);
    assert!(env.remove_source(oxygen));
    assert!(env.remove_source(carbon));
    let oxygen = env.add_source(9, 610.0, 390.0, 1.0, 60.0);
    let carbon = env.add_source(11, 610.0, 390.0, 1.0, 60.0);
    for tick in 30..90 {
        env.step(tick as f64 * DT);
    }
    assert!(env.oxygen_output_total() > 0.0 && env.carbon_output_total() > 0.0);
    close(
        env.oxygen_total() + env.oxygen_output_total(),
        env.oxygen_input_total(),
    );
    close(
        env.inorganic_carbon_total() + env.carbon_output_total(),
        env.carbon_input_total(),
    );
    assert!(env
        .gases
        .oxygen
        .iter()
        .chain(&env.gases.carbon)
        .all(|&n| n >= 0.0));
    env.update_source(oxygen, 9, 610.0, 390.0, 0.0, 60.0, 0.0, 600.0);
    env.remove_source(carbon);
    let outputs = (env.oxygen_output_total(), env.carbon_output_total());
    env.step(3.0);
    assert_eq!(
        outputs,
        (env.oxygen_output_total(), env.carbon_output_total())
    );
    assert_eq!(state(&env), state(&resume(&env)));
}

#[test]
fn open_exchange_is_signed_finite_exponential_and_configuration_never_replaces_live_gases() {
    let mut env = empty();
    assert!(env.configure_gas_exchange(true, 0.2, 2.0, 3.0));
    for tick in 0..30 {
        env.step(tick as f64 * DT);
    }
    let o = 2.0 * (1.0 - (-0.2_f64).exp());
    let c = 3.0 * (1.0 - (-0.2_f64).exp());
    close(env.oxygen_patch(0), o);
    close(env.inorganic_carbon_patch(0), c);
    close(env.oxygen_total(), env.oxygen_input_total());
    close(env.inorganic_carbon_total(), env.carbon_input_total());
    let original = (env.oxygen_total(), env.inorganic_carbon_total());
    assert!(env.set_initial_gases(80.0, 90.0));
    assert_eq!(original, (env.oxygen_total(), env.inorganic_carbon_total()));
    assert!(env.configure_gas_exchange(true, 0.2, 0.1, 0.2));
    assert_eq!(original, (env.oxygen_total(), env.inorganic_carbon_total()));
    for tick in 30..60 {
        env.step(tick as f64 * DT);
    }
    close(env.oxygen_patch(0), 0.1 + (o - 0.1) * (-0.2_f64).exp());
    close(
        env.inorganic_carbon_patch(0),
        0.2 + (c - 0.2) * (-0.2_f64).exp(),
    );
    assert!(env.oxygen_output_total() > 0.0 && env.carbon_output_total() > 0.0);
    close(
        env.oxygen_total() + env.oxygen_output_total(),
        env.oxygen_input_total(),
    );
    close(
        env.inorganic_carbon_total() + env.carbon_output_total(),
        env.carbon_input_total(),
    );
    let before = state(&env);
    assert!(!env.configure_gas_exchange(true, f64::NAN, 1.0, 1.0));
    assert!(!env.set_initial_gases(101.0, 1.0));
    assert_eq!(state(&env), before);
    env.configure_gas_exchange(false, 0.2, 0.1, 0.2);
    let inputs = (env.oxygen_input_total(), env.carbon_input_total());
    let outputs = (env.oxygen_output_total(), env.carbon_output_total());
    env.step(2.0);
    assert_eq!(inputs, (env.oxygen_input_total(), env.carbon_input_total()));
    assert_eq!(
        outputs,
        (env.oxygen_output_total(), env.carbon_output_total())
    );
}

#[test]
fn aerator_is_explicit_local_signed_connection_and_solid_patches_have_no_air_surface() {
    let mut env = empty();
    let aerator = env.add_source(12, 610.0, 390.0, 1.0, 60.0);
    assert!(aerator > 0);
    env.step(0.0);
    let near = env.patch_index(610.0, 390.0);
    let far = env.patch_index(10.0, 10.0);
    assert!(env.oxygen_patch(near) > env.oxygen_patch(far) * 100.0);
    assert!(env.oxygen_input_total() > 0.0 && env.carbon_input_total() > 0.0);
    env.set_initial_gases(3.0, 3.0);
    env.reset_dynamic();
    env.step(DT);
    assert!(env.oxygen_output_total() > 0.0 && env.carbon_output_total() > 0.0);
    close(
        env.oxygen_total() + env.oxygen_output_total(),
        env.oxygen_input_total(),
    );
    close(
        env.inorganic_carbon_total() + env.carbon_output_total(),
        env.carbon_input_total(),
    );
    env.clear_sources();
    env.set_initial_gases(0.0, 0.0);
    env.reset_dynamic();
    assert!(env.add_barrier(610.0, 390.0, 80.0, 80.0, 0.0, false) > 0);
    env.configure_gas_exchange(true, 1.0, 1.0, 1.0);
    env.step(2.0 * DT);
    assert_eq!(env.oxygen_patch(near), 0.0);
    assert_eq!(env.inorganic_carbon_patch(near), 0.0);
    assert!(env.oxygen_patch(far) > 0.0);
}

#[test]
fn gases_obey_wall_topology_and_resize_preserves_amounts_and_exact_continuation() {
    let mut env = empty();
    env.add_barrier(400.0, 380.0, 760.0, 20.0, FRAC_PI_2, false);
    env.add_barrier(600.0, 380.0, 760.0, 20.0, FRAC_PI_2, false);
    env.release_oxygen(510.0, 390.0, 10.0);
    env.release_inorganic_carbon(510.0, 390.0, 15.0);
    let jet = env.add_source(6, 500.0, 380.0, 1.0, 500.0);
    env.update_source(jet, 6, 500.0, 380.0, 1.0, 500.0, 0.0, 1200.0);
    for tick in 0..90 {
        env.step(tick as f64 * DT);
    }
    for i in 0..env.patch_count() {
        let (x, _) = env.patch_position(i);
        if !(400.0..600.0).contains(&x) {
            assert_eq!(env.oxygen_patch(i), 0.0);
            assert_eq!(env.inorganic_carbon_patch(i), 0.0);
        }
    }
    close(env.oxygen_total(), 10.0);
    close(env.inorganic_carbon_total(), 15.0);
    env.clear_barriers();
    assert!(env.set_size(2380.0, 1500.0));
    close(env.oxygen_total(), 10.0);
    close(env.inorganic_carbon_total(), 15.0);
    env.add_source(12, 1200.0, 750.0, 0.5, 100.0);
    env.add_barrier(800.0, 600.0, 120.0, 20.0, 0.2, true);
    let mut copy = resume(&env);
    assert_eq!(state(&env), state(&copy));
    for tick in 90..105 {
        env.step(tick as f64 * DT);
        copy.step(tick as f64 * DT);
    }
    assert_eq!(state(&env), state(&copy));
}

#[test]
fn version_eight_environment_migration_establishes_explicit_initial_gas_ledgers() {
    let mut env = Environment::default();
    env.add_source(3, 100.0, 200.0, 0.3, 80.0);
    env.add_barrier(300.0, 300.0, 120.0, 30.0, 0.2, true);
    let mut old = state(&env);
    old.remove(0); // v8 preceded the v17 chemistry-mode tag.
    let mut gas = Writer::new();
    env.gases.write_checkpoint(&mut gas);
    old.truncate(old.len() - gas.0.len());
    let mut reader = Reader::new(&old);
    let migrated = Environment::read_checkpoint(&mut reader, 8).unwrap();
    assert!(reader.finished());
    assert_eq!(migrated.sources.len(), 1);
    assert_eq!(migrated.barriers.len(), 1);
    assert_eq!(migrated.oxygen_total(), migrated.patch_count() as f64);
    assert_eq!(
        migrated.inorganic_carbon_total(),
        migrated.patch_count() as f64
    );
    assert_eq!(migrated.oxygen_input_total(), migrated.oxygen_total());
    assert_eq!(
        migrated.carbon_input_total(),
        migrated.inorganic_carbon_total()
    );
    assert_eq!(
        (
            migrated.oxygen_output_total(),
            migrated.carbon_output_total()
        ),
        (0.0, 0.0)
    );
}

#[test]
fn all_six_chemical_ports_act_on_the_wrapped_disk_and_account_actual_exchange() {
    let mut env = empty();
    env.set_initial_nutrients(0.0);
    env.reset_dynamic();
    let near = env.patch_index(10.0,210.0);
    let across = env.patch_index(env.width - 10.0,210.0);
    let outside = env.patch_index(90.0,210.0);
    let stocks = |e:&Environment,i| [e.nutrient_patch(i),e.oxygen_patch(i),e.inorganic_carbon_patch(i)];
    let inputs = |e:&Environment| [e.nutrient_input_total(),e.oxygen_input_total(),e.carbon_input_total()];
    let outputs = |e:&Environment| [e.nutrient_removed_total(),e.oxygen_output_total(),e.carbon_output_total()];
    for kind in [3,8,10] { assert_ne!(env.add_source(kind,0.0,210.0,0.8,40.0),0); }
    let barrier = env.add_barrier(0.0,210.0,100.0,100.0,0.0,false);
    assert!(barrier > 0);
    env.step(0.0);
    assert_eq!(inputs(&env),[0.0;3], "a completely covered port introduces no material");
    assert!(env.remove_barrier(barrier));
    env.step(DT);
    for n in inputs(&env) { close(n,1.5*0.8*DT); }
    assert_eq!(stocks(&env,outside),[0.0;3]);
    for k in 0..3 {
        assert!(stocks(&env,near)[k]>0.0);
        close(stocks(&env,near)[k],stocks(&env,across)[k]);
    }

    env.clear_sources();
    env.set_initial_nutrients(1.0);
    assert!(env.set_initial_gases(1.0,1.0));
    env.reset_dynamic();
    for kind in [4,9,11] { assert_ne!(env.add_source(kind,0.0,210.0,0.8,40.0),0); }
    env.step(0.0);
    for n in outputs(&env) { close(n,1.5*0.8*DT); }
    assert_eq!(stocks(&env,outside),[1.0;3]);
    assert!(stocks(&env,near).iter().all(|n| *n<1.0));
    for (amount,removed) in [env.nutrient_total(),env.oxygen_total(),env.inorganic_carbon_total()]
        .into_iter().zip(outputs(&env)) {
        close(amount + removed, env.patch_count() as f64);
    }
    env.set_initial_nutrients(0.0);
    assert!(env.set_initial_gases(0.0,0.0));
    env.reset_dynamic(); env.step(0.0);
    assert_eq!(outputs(&env),[0.0;3], "an empty patch cannot fund a sink");
}
