//! Dissolved oxygen and effective inorganic carbon. Units are abstract amounts
//! in one water-volume unit per physical 20 × 20 field patch.
use super::*;
use crate::checkpoint::{ensure, Reader, Result, Writer};

pub const PATCH_WATER_VOLUME: f64 = 1.0;
pub const DEFAULT_INITIAL_OXYGEN: f64 = 1.0;
pub const DEFAULT_INITIAL_CARBON: f64 = 1.0;
const OXYGEN_DIFFUSION: f64 = 80.0;
const CARBON_DIFFUSION: f64 = 60.0;
const GAS_SOURCE_RATE: f64 = 1.5;
// A unit-strength aerator exchanges five abstract water volumes per second,
// distributed over its normalized spatial footprint.
const AERATOR_VOLUME_RATE: f64 = 5.0;

#[derive(Clone)]
pub(super) struct GasState {
    pub initial_oxygen: f64,
    pub initial_carbon: f64,
    pub atmosphere_open: bool,
    pub exchange_rate: f64,
    pub oxygen_equilibrium: f64,
    pub carbon_equilibrium: f64,
    pub oxygen: Vec<f64>,
    pub carbon: Vec<f64>,
    pub oxygen_inputs: f64,
    pub oxygen_outputs: f64,
    pub carbon_inputs: f64,
    pub carbon_outputs: f64,
    oxygen_injection: Vec<f64>,
    oxygen_absorption: Vec<f64>,
    carbon_injection: Vec<f64>,
    carbon_absorption: Vec<f64>,
    aeration: Vec<f64>,
    /// Derived from the atmosphere and aerators, never serialized.
    exchange_fraction: Vec<f64>,
}
impl GasState {
    pub fn new(patches: usize) -> Self {
        Self {
            initial_oxygen: DEFAULT_INITIAL_OXYGEN,
            initial_carbon: DEFAULT_INITIAL_CARBON,
            atmosphere_open: false,
            exchange_rate: 0.02,
            oxygen_equilibrium: 1.0,
            carbon_equilibrium: 1.0,
            oxygen: vec![DEFAULT_INITIAL_OXYGEN; patches],
            carbon: vec![DEFAULT_INITIAL_CARBON; patches],
            oxygen_inputs: DEFAULT_INITIAL_OXYGEN * patches as f64,
            oxygen_outputs: 0.0,
            carbon_inputs: DEFAULT_INITIAL_CARBON * patches as f64,
            carbon_outputs: 0.0,
            oxygen_injection: vec![0.0; patches],
            oxygen_absorption: vec![0.0; patches],
            carbon_injection: vec![0.0; patches],
            carbon_absorption: vec![0.0; patches],
            aeration: vec![0.0; patches],
            exchange_fraction: vec![0.0; patches],
        }
    }
    pub fn reset(&mut self) {
        self.oxygen.fill(self.initial_oxygen);
        self.carbon.fill(self.initial_carbon);
        self.oxygen_inputs = self.initial_oxygen * self.oxygen.len() as f64;
        self.oxygen_outputs = 0.0;
        self.carbon_inputs = self.initial_carbon * self.carbon.len() as f64;
        self.carbon_outputs = 0.0;
    }
    pub fn write_checkpoint(&self, w: &mut Writer) {
        for n in [
            self.initial_oxygen,
            self.initial_carbon,
            self.exchange_rate,
            self.oxygen_equilibrium,
            self.carbon_equilibrium,
            self.oxygen_inputs,
            self.oxygen_outputs,
            self.carbon_inputs,
            self.carbon_outputs,
        ] {
            w.f64(n);
        }
        w.u8(u8::from(self.atmosphere_open));
        for field in [&self.oxygen, &self.carbon] {
            w.count(field.len());
            for n in field {
                w.f64(*n);
            }
        }
    }
    pub fn read_checkpoint(r: &mut Reader<'_>, patches: usize) -> Result<Self> {
        let mut result = Self::new(patches);
        result.initial_oxygen = r.range(0.0, 100.0)?;
        result.initial_carbon = r.range(0.0, 100.0)?;
        result.exchange_rate = r.range(0.0, 1.0)?;
        result.oxygen_equilibrium = r.range(0.0, 100.0)?;
        result.carbon_equilibrium = r.range(0.0, 100.0)?;
        result.oxygen_inputs = r.range(0.0, 1e100)?;
        result.oxygen_outputs = r.range(0.0, 1e100)?;
        result.carbon_inputs = r.range(0.0, 1e100)?;
        result.carbon_outputs = r.range(0.0, 1e100)?;
        let open = r.u8()?;
        ensure(open <= 1, "invalid atmosphere boundary")?;
        result.atmosphere_open = open == 1;
        for field in [&mut result.oxygen, &mut result.carbon] {
            ensure(r.count(patches)? == patches, "incorrect gas field size")?;
            for n in field {
                *n = r.range(0.0, 1e100)?;
            }
        }
        Ok(result)
    }
}

impl Environment {
    /// Initial amounts are a recipe for the next explicit reset, not live stock.
    pub fn set_initial_gases(&mut self, oxygen: f64, carbon: f64) -> bool {
        if ![oxygen, carbon]
            .iter()
            .all(|n| n.is_finite() && (0.0..=100.0).contains(n))
        {
            return false;
        }
        self.gases.initial_oxygen = oxygen;
        self.gases.initial_carbon = carbon;
        true
    }
    pub fn configure_gas_exchange(
        &mut self,
        open: bool,
        rate: f64,
        oxygen_equilibrium: f64,
        carbon_equilibrium: f64,
    ) -> bool {
        if !rate.is_finite()
            || !(0.0..=1.0).contains(&rate)
            || ![oxygen_equilibrium, carbon_equilibrium]
                .iter()
                .all(|n| n.is_finite() && (0.0..=100.0).contains(n))
        {
            return false;
        }
        self.gases.atmosphere_open = open;
        self.gases.exchange_rate = rate;
        self.gases.oxygen_equilibrium = oxygen_equilibrium;
        self.gases.carbon_equilibrium = carbon_equilibrium;
        self.rebuild_exchange_fractions();
        true
    }
    pub fn oxygen_patch(&self, index: usize) -> f64 {
        self.gases.oxygen.get(index).copied().unwrap_or(0.0)
    }
    pub fn inorganic_carbon_patch(&self, index: usize) -> f64 {
        self.gases.carbon.get(index).copied().unwrap_or(0.0)
    }
    pub fn oxygen_at(&self, x: f64, y: f64) -> f64 {
        self.sample(&self.gases.oxygen, x, y) / PATCH_WATER_VOLUME
    }
    pub fn inorganic_carbon_at(&self, x: f64, y: f64) -> f64 {
        self.sample(&self.gases.carbon, x, y) / PATCH_WATER_VOLUME
    }
    pub fn oxygen_total(&self) -> f64 {
        self.gases.oxygen.iter().sum()
    }
    pub fn inorganic_carbon_total(&self) -> f64 {
        self.gases.carbon.iter().sum()
    }
    pub fn oxygen_input_total(&self) -> f64 {
        self.gases.oxygen_inputs
    }
    pub fn oxygen_output_total(&self) -> f64 {
        self.gases.oxygen_outputs
    }
    pub fn carbon_input_total(&self) -> f64 {
        self.gases.carbon_inputs
    }
    pub fn carbon_output_total(&self) -> f64 {
        self.gases.carbon_outputs
    }
    pub fn oxygen_removed_total(&self) -> f64 {
        self.gases.oxygen_outputs
    }
    pub fn carbon_removed_total(&self) -> f64 {
        self.gases.carbon_outputs
    }
    /// Internal transfers leave external ledgers unchanged. Callers collect and
    /// jointly allocate competing requests before committing these withdrawals.
    pub fn take_oxygen(&mut self, index: usize, requested: f64) -> f64 {
        take(&mut self.gases.oxygen, index, requested)
    }
    pub fn take_inorganic_carbon(&mut self, index: usize, requested: f64) -> f64 {
        take(&mut self.gases.carbon, index, requested)
    }
    pub fn release_oxygen(&mut self, x: f64, y: f64, amount: f64) {
        if amount.is_finite() && amount > 0.0 && x.is_finite() && y.is_finite() {
            let i = self.patch_index(x, y);
            self.release_oxygen_patch(i, amount);
        }
    }
    pub fn release_inorganic_carbon(&mut self, x: f64, y: f64, amount: f64) {
        if amount.is_finite() && amount > 0.0 && x.is_finite() && y.is_finite() {
            let i = self.patch_index(x, y);
            self.release_inorganic_carbon_patch(i, amount);
        }
    }
    pub fn release_oxygen_patch(&mut self, index: usize, amount: f64) {
        if amount.is_finite() && amount > 0.0 {
            if let Some(balance) = self.gases.oxygen.get_mut(index) {
                *balance += amount;
            }
        }
    }
    pub fn release_inorganic_carbon_patch(&mut self, index: usize, amount: f64) {
        if amount.is_finite() && amount > 0.0 {
            if let Some(balance) = self.gases.carbon.get_mut(index) {
                *balance += amount;
            }
        }
    }
    pub(super) fn rebuild_gas_sources(&mut self) {
        let count = self.patch_count();
        for rates in [
            &mut self.gases.oxygen_injection,
            &mut self.gases.oxygen_absorption,
            &mut self.gases.carbon_injection,
            &mut self.gases.carbon_absorption,
            &mut self.gases.aeration,
        ] {
            rates.resize(count, 0.0);
            rates.fill(0.0);
        }
        for source in self
            .sources
            .iter()
            .filter(|source| (8..=12).contains(&source.kind) && source.strength > 0.0)
        {
            let weights = self.chemical_footprint(source);
            let (rates, rate) = match source.kind {
                8 => (&mut self.gases.oxygen_injection, GAS_SOURCE_RATE),
                9 => (&mut self.gases.oxygen_absorption, GAS_SOURCE_RATE),
                10 => (&mut self.gases.carbon_injection, GAS_SOURCE_RATE),
                11 => (&mut self.gases.carbon_absorption, GAS_SOURCE_RATE),
                _ => (
                    &mut self.gases.aeration,
                    AERATOR_VOLUME_RATE / PATCH_WATER_VOLUME,
                ),
            };
            for (value, weight) in rates.iter_mut().zip(weights) {
                *value += rate * source.strength * weight;
            }
        }
        self.rebuild_exchange_fractions();
    }
    fn rebuild_exchange_fractions(&mut self) {
        let gas = &mut self.gases;
        gas.exchange_fraction.resize(gas.oxygen.len(), 0.0);
        let ambient = if gas.atmosphere_open {
            gas.exchange_rate
        } else {
            0.0
        };
        for (fraction, aeration) in gas.exchange_fraction.iter_mut().zip(&gas.aeration) {
            let rate = ambient + aeration;
            *fraction = if rate <= 0.0 {
                0.0
            } else {
                -(-rate * DT).exp_m1()
            };
        }
    }
    pub(super) fn step_gases(&mut self, has_flow: bool) {
        transport_amounts(
            &mut self.gases.oxygen,
            &self.grid.neighbors,
            &self.blocked_right,
            &self.blocked_down,
            OXYGEN_DIFFUSION,
            has_flow,
            &mut self.scratch,
        );
        transport_amounts(
            &mut self.gases.carbon,
            &self.grid.neighbors,
            &self.blocked_right,
            &self.blocked_down,
            CARBON_DIFFUSION,
            has_flow,
            &mut self.scratch,
        );
        let gas = &mut self.gases;
        for i in 0..gas.oxygen.len() {
            // Sources, sinks and surface exchange operate only in exposed water.
            if self.solid_patches[i] { continue; }
            let oxygen_input = gas.oxygen_injection[i] * DT;
            let carbon_input = gas.carbon_injection[i] * DT;
            gas.oxygen[i] += oxygen_input;
            gas.carbon[i] += carbon_input;
            gas.oxygen_inputs += oxygen_input;
            gas.carbon_inputs += carbon_input;
            let oxygen_output = (gas.oxygen_absorption[i] * DT).min(gas.oxygen[i]);
            let carbon_output = (gas.carbon_absorption[i] * DT).min(gas.carbon[i]);
            gas.oxygen[i] -= oxygen_output;
            gas.carbon[i] -= carbon_output;
            gas.oxygen_outputs += oxygen_output;
            gas.carbon_outputs += carbon_output;
            // A local aerator can open its own footprint in a sealed scene.
            let rate = if gas.atmosphere_open {
                gas.exchange_rate
            } else {
                0.0
            } + gas.aeration[i];
            if rate <= 0.0 {
                continue;
            }
            let fraction = gas.exchange_fraction[i];
            // Deliberately abstract saturation curve: the temperature signal
            // has no Celsius/depth/pressure calibration in this simulator.
            let oxygen_target =
                gas.oxygen_equilibrium * (1.25 - 0.5 * self.temperature[i]) * PATCH_WATER_VOLUME;
            let carbon_target = gas.carbon_equilibrium * PATCH_WATER_VOLUME;
            exchange(
                &mut gas.oxygen[i],
                oxygen_target,
                fraction,
                &mut gas.oxygen_inputs,
                &mut gas.oxygen_outputs,
            );
            exchange(
                &mut gas.carbon[i],
                carbon_target,
                fraction,
                &mut gas.carbon_inputs,
                &mut gas.carbon_outputs,
            );
        }
    }
}
fn take(field: &mut [f64], index: usize, requested: f64) -> f64 {
    if !requested.is_finite() || requested <= 0.0 {
        return 0.0;
    }
    let Some(balance) = field.get_mut(index) else {
        return 0.0;
    };
    let actual = requested.min(*balance);
    *balance -= actual;
    actual
}
fn exchange(balance: &mut f64, target: f64, fraction: f64, input: &mut f64, output: &mut f64) {
    let change = (target - *balance) * fraction;
    if change >= 0.0 {
        *balance += change;
        *input += change;
    } else {
        let actual = (-change).min(*balance);
        *balance -= actual;
        *output += actual;
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use crate::checkpoint::{Reader, Writer};
    #[test]
    fn exchange_fractions_match_direct_formula_through_all_configuration_paths() {
        let mut env = Environment::default();
        let aerator = env.add_source(12, 1190.0, 20.0, 0.8, 100.0);
        for revision in 0..7 {
            match revision {
                1 => {
                    env.configure_gas_exchange(true, 0.2, 2.0, 3.0);
                }
                2 => {
                    env.update_source(aerator, 12, 10.0, 300.0, 1.0, 300.0, 0.0, 600.0);
                }
                3 => {
                    env.set_size(1180.0, 740.0);
                }
                4 => {
                    let mut writer = Writer::new();
                    env.write_checkpoint(&mut writer);
                    env = Environment::read_checkpoint(&mut Reader::new(&writer.0), 9).unwrap();
                }
                5 => {
                    env.configure_gas_exchange(false, 0.0, 1.0, 1.0);
                    env.reset_dynamic();
                }
                6 => {
                    env.clear_sources();
                }
                _ => {}
            }
            for i in 0..env.patch_count() {
                let gas = &env.gases;
                let rate = if gas.atmosphere_open {
                    gas.exchange_rate
                } else {
                    0.0
                } + gas.aeration[i];
                let expected = if rate <= 0.0 {
                    0.0
                } else {
                    -(-rate * DT).exp_m1()
                };
                assert_eq!(gas.exchange_fraction[i].to_bits(), expected.to_bits());
            }
        }
    }
}
