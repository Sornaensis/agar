//! Whole dissolved units. Fractional phases schedule discrete transfers and
//! never enter inventories or conservation ledgers.
use super::*;
use crate::checkpoint::{ensure, Reader, Result, Writer};
use crate::counts::{SCALE, nearest as counts};
pub(super) use crate::counts::wide_units as units;

// Leave headroom for the World to add cell introductions, respiratory losses
// and fixed carbon to environmental budgets in a single exact u64 audit.
const BUDGET_LIMIT: u64 = u64::MAX / 128;
const MINERAL: usize = 0;
const OXYGEN: usize = 1;
const CARBON: usize = 2;
const DIFFUSION: [f64; 3] =
    [NUTRIENT_DIFFUSION, gases::OXYGEN_DIFFUSION, gases::CARBON_DIFFUSION];

#[derive(Clone)]
struct Port {
    id: u32,
    kind: u32,
    rate: f64,
    // Whole counts are due each time this rate phase reaches one tick frequency.
    phase: f64,
    cursor: u32,
    footprint: Vec<(f64, usize)>,
}

#[derive(Clone)]
pub(super) struct CountWater {
    pub(super) stocks: [Vec<u32>; 3],
    pub(super) inputs: [u64; 3],
    pub(super) outputs: [u64; 3],
    faces: [Vec<[f64; 2]>; 3],
    surface: [Vec<f64>; 2],
    ports: Vec<Port>,
    // Scratch space for a synchronous transport step; never serialized.
    start: Vec<u32>,
    outgoing: Vec<u32>,
    eligible: Vec<(f64, usize)>,
}

impl CountWater {
    fn new(n: usize, initial: [f64; 3]) -> Self {
        let stocks = initial.map(|value| vec![counts(value); n]);
        Self {
            stocks,
            inputs: initial.map(|v| u64::from(counts(v)) * n as u64),
            outputs: [0; 3],
            faces: std::array::from_fn(|_| vec![[0.0; 2]; n]),
            surface: std::array::from_fn(|_| vec![0.0; n]),
            ports: vec![],
            start: vec![],
            outgoing: vec![],
            eligible: Vec::new(),
        }
    }

    fn transfer(&mut self, channel: usize, i: usize, amount: u32, add: bool) -> u32 {
        let Some(stock) = self.stocks[channel].get_mut(i) else { return 0; };
        if add {
            let n = amount.min(u32::MAX - *stock);
            *stock += n;
            n
        } else {
            let n = amount.min(*stock);
            *stock -= n;
            n
        }
    }

    fn external(&mut self, channel: usize, i: usize, amount: u32, add: bool) -> u32 {
        let budget = if add { self.inputs[channel] } else { self.outputs[channel] };
        let amount = u64::from(amount).min(BUDGET_LIMIT - budget) as u32;
        let actual = self.transfer(channel, i, amount, add);
        if add {
            self.inputs[channel] += u64::from(actual);
        } else { self.outputs[channel] += u64::from(actual); }
        actual
    }

    // Use only the start-of-tick donor stock. Transfers can never overdraw
    // that stock, nor can a receiver overflow even under convergent flow.
    fn transport(&mut self, channel: usize, grid: &PatchGeometry,
        right: &[bool], down: &[bool], flow: Option<&[(f64, f64)]>) {
        let values = &mut self.stocks[channel];
        if flow.is_none() && values.windows(2).all(|pair| pair[0] == pair[1]) {
            return;
        }
        self.start.clone_from(values);
        self.outgoing.resize(values.len(), 0);
        self.outgoing.fill(0);
        let diffusion = DIFFUSION[channel] * DT / PATCH_AREA;
        let advection = DT / PATCH_SIZE;
        for i in 0..values.len() {
            let velocity = flow.map_or((0.0, 0.0), |f| f[i]);
            let faces = [
                (grid.neighbors[i][1], velocity.0, right[i]),
                (grid.neighbors[i][3], velocity.1, down[i]),
            ];
            for (axis, (j, speed, blocked)) in faces.into_iter().enumerate() {
                let phase = &mut self.faces[channel][i][axis];
                if blocked { continue; }
                let a = f64::from(self.start[i]);
                let b = f64::from(self.start[j]);
                let upstream = if speed >= 0.0 { a } else { b };
                let desired = *phase + speed * advection * upstream + diffusion * (a - b);
                let whole = desired.trunc();
                *phase = desired - whole;
                if whole == 0.0 { continue; }
                let (from, to) = if whole > 0.0 { (i, j) } else { (j, i) };
                let n = (whole.abs() as u32)
                    .min(self.start[from] - self.outgoing[from])
                    .min(u32::MAX - values[to]);
                self.outgoing[from] += n;
                values[from] -= n;
                values[to] += n;
            }
        }
    }

    fn drive_ports(&mut self, solid: &[bool]) {
        // Emission precedes absorption, independent of recipe/render ordering.
        for adding in [true, false] {
            for p in 0..self.ports.len() {
                let port = &self.ports[p];
                let (channel, add) =
                    match port.kind {
                        3 => (MINERAL, true),
                        4 => (MINERAL, false),
                        8 => (OXYGEN, true),
                        9 => (OXYGEN, false),
                        10 => (CARBON, true),
                        11 => (CARBON, false),
                        _ => unreachable!(),
                    };
                if adding != add { continue; }
                let port = &mut self.ports[p];
                if port.rate == 0.0 { continue; }
                // Accrue work only during open, funded time. A moving barrier
                // cannot erase preceding fractional work or make an empty
                // patch consume the quota of a different wetted patch.
                self.eligible.clear();
                let mut previous = 0.0;
                let mut fraction = 0.0;
                for &(end, patch) in &port.footprint {
                    let weight = end - previous;
                    previous = end;
                    let available = if add {
                        self.stocks[channel][patch] < u32::MAX
                    } else {
                        self.stocks[channel][patch] > 0
                    };
                    if !solid[patch] && available && weight > 0.0 {
                        fraction += weight;
                        self.eligible.push((fraction, patch));
                    }
                }
                if self.eligible.is_empty() { continue; }
                if self.eligible.len() == port.footprint.len() {
                    fraction = 1.0;
                    self.eligible.last_mut().unwrap().0 = fraction;
                }
                let frequency = 1.0 / DT;
                let accumulated = port.phase + port.rate * fraction;
                let due = (accumulated / frequency).floor() as u32;
                port.phase = (accumulated - f64::from(due) * frequency).max(0.0);
                for _ in 0..due {
                    // An additive full-period walk samples even very broad
                    // ports at one count/s without splitting any inventory.
                    let port = &mut self.ports[p];
                    port.cursor = port.cursor.wrapping_add(0x9e37_79b9);
                    let quantile = fraction * (f64::from(port.cursor) / 4294967296.0);
                    let at = self.eligible.partition_point(|(end, _)| *end <= quantile);
                    let patch = self.eligible[at].1;
                    self.external(channel, patch, 1, add);
                }
            }
        }
    }

    // Distribute each source patch by its exact normalized overlap area. The
    // difference of successive cumulative integer quotas sums to its full stock.
    pub(super) fn resized(&self, old_cols: usize, old_rows: usize,
        cols: usize, rows: usize) -> Option<[Vec<u32>; 3]> {
        let mut stocks: [Vec<u32>; 3] =
            std::array::from_fn(|_| vec![0; cols * rows]);
        let denominator = (cols * rows) as u64;
        for i in 0..old_cols * old_rows {
            let (x, y) = (i % old_cols, i / old_cols);
            let (left, right, top, bottom) =
                (x * cols, (x + 1) * cols, y * rows, (y + 1) * rows);
            let mut covered = 0u64;
            for row in top / old_rows..bottom.div_ceil(old_rows) {
                for col in left / old_cols..right.div_ceil(old_cols) {
                    let dx =
                        right.min((col + 1) * old_cols) - left.max(col * old_cols);
                    let dy =
                        bottom.min((row + 1) * old_rows) - top.max(row * old_rows);
                    let end = covered + (dx * dy) as u64;
                    for (dest, src) in stocks.iter_mut().zip(&self.stocks) {
                        let amount = u64::from(src[i]);
                        let share =
                            (amount * end / denominator -
                                        amount * covered / denominator) as u32;
                        let slot = &mut dest[row * cols + col];
                        *slot = slot.checked_add(share)?;
                    }
                    covered = end;
                }
            }
        }
        Some(stocks)
    }

    pub(super) fn install_resized(&mut self, stocks: [Vec<u32>; 3]) {
        let n = stocks[0].len();
        self.stocks = stocks;
        self.faces = std::array::from_fn(|_| vec![[0.0; 2]; n]);
        self.surface = std::array::from_fn(|_| vec![0.0; n]);
    }

    pub(super) fn write(&self, w: &mut Writer) {
        for c in 0..3 {
            w.u64(self.inputs[c]);
            w.u64(self.outputs[c]);
            for &n in &self.stocks[c] { w.u32(n) }
            for &[x, y] in &self.faces[c] { w.f64(x); w.f64(y) }
        }
        for series in &self.surface { for &n in series { w.f64(n) } }
        w.count(self.ports.len());
        for port in &self.ports {
            w.u32(port.id);
            w.f64(port.phase);
            w.u32(port.cursor)
        }
    }
    pub(super) fn read(&mut self, r: &mut Reader<'_>) -> Result<()> {
        fn remainder(r: &mut Reader<'_>) -> Result<f64> {
            let n = r.range(-1.0, 1.0)?;
            ensure(n.abs() < 1.0, "invalid transfer phase")?;
            Ok(n)
        }
        for c in 0..3 {
            self.inputs[c] = r.u64()?;
            self.outputs[c] = r.u64()?;
            ensure(self.inputs[c] <= BUDGET_LIMIT && self.outputs[c] <= BUDGET_LIMIT,
                "external count budget outside safe range")?;
            for n in &mut self.stocks[c] { *n = r.u32()? }
            for face in &mut self.faces[c] {
                *face = [remainder(r)?, remainder(r)?]
            }
        }
        for series in &mut self.surface {
            for n in series { *n = remainder(r)? }
        }
        ensure(r.count(MAX_SOURCES)? == self.ports.len(),
                "incorrect counted chemical port count")?;
        for port in &mut self.ports {
            ensure(r.u32()? == port.id,
                    "invalid counted chemical port identifier")?;
            port.phase = r.range(0.0, 1.0 / DT)?;
            ensure(port.phase < 1.0 / DT, "invalid emission phase")?;
            port.cursor = r.u32()?;
        }
        Ok(())
    }
}

impl Environment {
    pub fn uses_count_chemistry(&self) -> bool { self.counted.is_some() }
    // The World API enforces the fresh, empty-world gate before changing mode.
    pub fn set_count_chemistry(&mut self, enabled: bool) -> bool {
        if enabled == self.uses_count_chemistry() { return true; }
        self.reset_chemistry_mode(enabled);
        true
    }
    pub(super) fn reset_chemistry_mode(&mut self, enabled: bool) {
        let n = self.patch_count();
        if enabled {
            self.nutrients = Vec::new();
            self.gases.oxygen = Vec::new();
            self.gases.carbon = Vec::new();
            self.nutrient_inputs = 0.0;
            self.nutrient_removed = 0.0;
            self.gases.oxygen_inputs = 0.0;
            self.gases.oxygen_outputs = 0.0;
            self.gases.carbon_inputs = 0.0;
            self.gases.carbon_outputs = 0.0;
            self.counted =
                Some(CountWater::new(n,
                        [self.initial_nutrients, self.gases.initial_oxygen,
                                self.gases.initial_carbon]));
            self.rebuild_count_ports();
        } else {
            self.counted = None;
            self.nutrients.resize(n, 0.0);
            self.nutrients.fill(self.initial_nutrients);
            self.nutrient_inputs = self.initial_nutrients * n as f64;
            self.nutrient_removed = 0.0;
            self.gases.oxygen.resize(n, 0.0);
            self.gases.carbon.resize(n, 0.0);
            self.gases.reset();
        }
    }
    pub(super) fn rebuild_count_ports(&mut self) {
        let Some(water) = self.counted.as_mut() else { return; };
        let old = std::mem::take(&mut water.ports);
        let mut ports = Vec::new();
        for source in self.sources.iter().filter(|s| matches!(s.kind, 3 | 4 | 8..=11)) {
            let previous = old.iter().find(|p| p.id == source.id && p.kind == source.kind);
            let mut cumulative = 0.0;
            let mut footprint = Vec::new();
            for (i, weight) in self.chemical_footprint(source).into_iter().enumerate() {
                if weight > 0.0 {
                    cumulative += weight;
                    footprint.push((cumulative, i));
                }
            }
            footprint.last_mut().expect("each port covers a patch").0 = 1.0;
            // Snapshot-based object edits may round the normalized strength to
            // f32. Preserve exact whole-count/s rates at that precision.
            let rate = source.strength * f64::from(SCALE) * NUTRIENT_SOURCE_RATE;
            let nearest = rate.round();
            let rate = if (rate - nearest).abs() <= rate * f64::from(f32::EPSILON) {
                nearest
            } else {
                rate
            };
            ports.push(Port {
                id: source.id,
                kind: source.kind,
                rate,
                phase: previous.map_or(0.0, |p| p.phase),
                cursor: previous.map_or(source.id.wrapping_mul(0x85ebca6b), |p| p.cursor),
                footprint,
            });
        }
        self.counted.as_mut().unwrap().ports = ports;
    }
    pub(super) fn step_counted(&mut self, has_flow: bool) {
        let water = self.counted.as_mut().unwrap();
        let flow = has_flow.then_some(self.scratch.face_flow.as_slice());
        for channel in 0..3 {
            water.transport(channel, &self.grid, &self.blocked_right,
                &self.blocked_down, flow)
        }
        water.drive_ports(&self.solid_patches);
        for (i, &fraction) in self.gases.exchange_fraction.iter().enumerate() {
            if fraction == 0.0 || self.solid_patches[i] { continue; }
            let targets =
                [self.gases.oxygen_equilibrium *
                            (1.25 - 0.5 * self.temperature[i]),
                        self.gases.carbon_equilibrium];
            for (axis, target) in targets.into_iter().enumerate() {
                let c = axis + 1;
                // Temperature and equilibrium set a driving concentration.
                // The schedule may approach a fractional target while every
                // realized atmospheric exchange remains one or more counts.
                let difference =
                    target * f64::from(SCALE) - f64::from(water.stocks[c][i]);
                let scheduled =
                    water.surface[axis][i] + difference * fraction;
                let whole = scheduled.trunc();
                water.surface[axis][i] = scheduled - whole;
                if whole != 0.0 {
                    water.external(c, i, whole.abs() as u32, whole > 0.0);
                }
            }
        }
    }
    fn patch_counted(&self, channel: usize, i: usize) -> u32 {
        self.counted.as_ref().and_then(|v|
                        v.stocks[channel].get(i)).copied().unwrap_or(0)
    }
    fn transfer_counted(&mut self, channel: usize, i: usize, n: u32,
        add: bool) -> u32 {
        self.counted.as_mut().map_or(0, |w| w.transfer(channel, i, n, add))
    }
    pub fn mineral_count(&self, i: usize) -> u32 {
        self.patch_counted(MINERAL, i)
    }
    pub fn oxygen_count(&self, i: usize) -> u32 {
        self.patch_counted(OXYGEN, i)
    }
    pub fn inorganic_carbon_count(&self, i: usize) -> u32 {
        self.patch_counted(CARBON, i)
    }
    pub fn take_mineral_counts(&mut self, i: usize, n: u32) -> u32 {
        self.transfer_counted(MINERAL, i, n, false)
    }
    pub fn take_oxygen_counts(&mut self, i: usize, n: u32) -> u32 {
        self.transfer_counted(OXYGEN, i, n, false)
    }
    pub fn take_inorganic_carbon_counts(&mut self, i: usize, n: u32) -> u32 {
        self.transfer_counted(CARBON, i, n, false)
    }
    pub fn release_mineral_counts(&mut self, i: usize, n: u32) -> u32 {
        self.transfer_counted(MINERAL, i, n, true)
    }
    pub fn release_oxygen_counts(&mut self, i: usize, n: u32) -> u32 {
        self.transfer_counted(OXYGEN, i, n, true)
    }
    pub fn release_inorganic_carbon_counts(&mut self, i: usize, n: u32) -> u32 {
        self.transfer_counted(CARBON, i, n, true)
    }
    // Lysis may leave more than fits at the point of release. Only connected
    // exposed water is eligible; callers keep any remainder as a deposit.
    fn release_counts_near(&mut self, channel: usize, i: usize, amount: u32) -> u32 {
        if amount == 0 || i >= self.patch_count() ||
                    !self.uses_count_chemistry() || self.solid_patches[i] {
            return 0
        }
        let mut actual = self.transfer_counted(channel, i, amount, true);
        if actual == amount { return actual; }
        let mut seen = vec![false; self.patch_count()];
        let mut queue = std::collections::VecDeque::from([i]);
        seen[i] = true;
        while let Some(from) = queue.pop_front() {
            for to in self.grid.neighbors[from] {
                if seen[to] || self.solid_patches[to] ||
                        self.edge_blocked(from, to) {
                    continue;
                }
                seen[to] = true;
                actual +=
                    self.transfer_counted(channel, to, amount - actual, true);
                if actual == amount { return actual; }
                queue.push_back(to);
            }
        }
        actual
    }
    pub fn release_mineral_counts_near(&mut self, i: usize, n: u32) -> u32 {
        self.release_counts_near(MINERAL, i, n)
    }
    pub fn release_oxygen_counts_near(&mut self, i: usize, n: u32) -> u32 {
        self.release_counts_near(OXYGEN, i, n)
    }
    pub fn release_inorganic_carbon_counts_near(&mut self, i: usize, n: u32) -> u32 {
        self.release_counts_near(CARBON, i, n)
    }
    /// Historical-unit presentation can round a large budget. These totals are
    /// exact and use channel order mineral, oxygen, inorganic carbon.
    pub fn dissolved_counts(&self) -> [u64; 3] {
        self.counted.as_ref().map_or([0; 3],
            |w|
                std::array::from_fn(|c|
                        w.stocks[c].iter().map(|&n| u64::from(n)).sum()))
    }
    pub fn external_input_counts(&self) -> [u64; 3] {
        self.counted.as_ref().map_or([0; 3], |w| w.inputs)
    }
    pub fn external_removed_counts(&self) -> [u64; 3] {
        self.counted.as_ref().map_or([0; 3], |w| w.outputs)
    }
    pub(super) fn add_counted_minerals(&mut self, x: f64, y: f64, amount: f64) -> f64 {
        let n = crate::counts::floor(amount);
        let i = self.patch_index(x, y);
        let extent = self.patch_count() as u32;
        let water = self.counted.as_mut().unwrap();
        if x >= 0.0 && y >= 0.0 && x.is_finite() && y.is_finite() {
            return units(u64::from(water.external(MINERAL, i, n, true)))
        }
        let mut total = 0;
        for i in 0..extent {
            total +=
                u64::from(water.external(MINERAL, i as usize,
                        n / extent + u32::from(i < n % extent), true))
        }
        units(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> Environment {
        let mut env = Environment::default();
        assert!(env.set_size(400.0, 400.0));
        env.set_initial_nutrients(0.0);
        assert!(env.set_initial_gases(0.0, 0.0));
        assert!(env.set_count_chemistry(true));
        env
    }

    fn state(env: &Environment) -> Vec<u8> {
        let mut w = Writer::new();
        env.write_checkpoint(&mut w);
        w.0
    }

    fn step(env: &mut Environment, n: usize) {
        for i in 0..n { env.step(i as f64 * DT); }
    }

    #[test]
    fn individual_counts_follow_port_clock_and_covered_or_saturated_emission_is_unfunded() {
        let mut env = empty();
        let strength = 1.0 / (SCALE as f64 * NUTRIENT_SOURCE_RATE);
        for kind in [3, 8, 10] {
            assert!(env.add_source(kind,10.0,10.0,strength,180.0)>0);
        }
        step(&mut env, 29);
        assert_eq!(env.dissolved_counts(),[0;3]);
        step(&mut env, 1);
        assert_eq!(env.dissolved_counts(),[1;3]);
        step(&mut env, 90);
        assert_eq!(env.dissolved_counts(),[4;3]);
        assert_eq!(env.external_input_counts(),[4;3]);
        assert_eq!(env.external_removed_counts(),[0;3]);

        env.clear_sources();
        env.reset_dynamic();
        let id = env.add_barrier(10.0, 10.0, 80.0, 80.0, 0.0, false);
        for kind in [3, 8, 10] { env.add_source(kind, 10.0, 10.0, 1.0, 20.0); }
        step(&mut env, 3);
        assert_eq!(env.external_input_counts(),[0;3]);
        env.remove_barrier(id);
        let water = env.counted.as_mut().unwrap();
        for stock in &mut water.stocks { stock.fill(u32::MAX) }
        step(&mut env, 2);
        assert_eq!(env.external_input_counts(),[0;3]);
        env.clear_sources();
        env.reset_dynamic();
        for kind in [4, 9, 11] { env.add_source(kind, 10.0, 10.0, 1.0, 20.0); }
        for c in 0..3 { env.transfer_counted(c, 0, 7, true); }
        step(&mut env, 1);
        assert_eq!(env.external_removed_counts(),[7;3]);
        assert_eq!(env.dissolved_counts(),[0;3]);
        step(&mut env, 3);
        assert_eq!(env.external_removed_counts(),[7;3]);
    }

    #[test]
    fn intermittent_open_water_retains_subcount_transport_and_port_work() {
        let mut env = empty();
        env.release_oxygen_counts(0, 1);
        env.blocked_right.fill(true);
        env.blocked_down.fill(true);
        // At 80 diffusion units, an O2 count needs 150 exposed ticks on
        // this face. Never offer more than one consecutive exposed tick.
        for tick in 0..304 {
            env.blocked_right[0] = tick % 2 == 1;
            env.step_counted(false);
        }
        assert_eq!(env.oxygen_count(1), 1);
        assert_eq!(env.dissolved_counts(), [0, 1, 0]);

        let mut sink = empty();
        let strength = 0.5 / (f64::from(SCALE) * NUTRIENT_SOURCE_RATE);
        sink.add_source(4, 10.0, 10.0, strength, 20.0);
        sink.release_mineral_counts(0, 1);
        sink.blocked_right.fill(true);
        sink.blocked_down.fill(true);
        // Every 60th tick is covered, including the first would-be due
        // tick of an ungated half-count/s clock.
        for tick in 1..=60 {
            sink.solid_patches[0] = tick % 60 == 0;
            sink.step_counted(false);
        }
        assert_eq!(sink.mineral_count(0), 1);
        sink.solid_patches[0] = false;
        sink.step_counted(false);
        assert_eq!(sink.external_removed_counts(), [1, 0, 0]);

        let mut reservoir = empty();
        reservoir.configure_gas_exchange(true, 1.0, 0.0, 0.2 / f64::from(SCALE));
        step(&mut reservoir, 200);
        assert!(reservoir.external_input_counts()[CARBON] > 0);
        assert!(reservoir.external_removed_counts()[CARBON] > 0);
        assert_eq!(reservoir.dissolved_counts()[CARBON] + reservoir.external_removed_counts()[CARBON],
                   reservoir.external_input_counts()[CARBON]);
    }

    #[test]
    fn external_budget_ceiling_only_records_realized_transfers() {
        let mut env = empty();
        let water = env.counted.as_mut().unwrap();
        water.inputs[MINERAL] = BUDGET_LIMIT - 1;
        assert_eq!(water.external(MINERAL, 0, 3, true), 1);
        assert_eq!(water.external(MINERAL, 0, 3, true), 0);
        assert_eq!(water.stocks[MINERAL][0], 1);
        water.outputs[MINERAL] = BUDGET_LIMIT;
        assert_eq!(water.external(MINERAL, 0, 1, false), 0);
        assert_eq!(water.stocks[MINERAL][0], 1);

        water.inputs[MINERAL] = u64::MAX;
        assert!(Environment::read_checkpoint(&mut Reader::new(&state(&env)), 17).is_err());
    }

    #[test]
    fn sparse_dissolved_units_move_by_integer_face_flux_and_obey_walls() {
        let mut env = empty();
        env.release_oxygen_counts(0, 1);
        for i in 0..200 {
            env.step(i as f64 * DT);
            assert_eq!(env.dissolved_counts(),[0,1,0]);
            if env.oxygen_count(0) == 0 { break }
        }
        assert_eq!(env.oxygen_count(0),0,"one isolated count eventually diffuses");
        env.reset_dynamic();
        env.add_barrier(200.0, 200.0, 400.0, 20.0, TAU / 4.0, false);
        env.add_barrier(0.0, 200.0, 400.0, 20.0, TAU / 4.0, false);
        let patch = env.patch_index(170.0, 210.0);
        env.release_oxygen_counts(patch, u32::MAX);
        env.release_inorganic_carbon_counts(patch, 19);
        env.add_source(5, 100.0, 200.0, 1.0, 300.0);
        step(&mut env, 120);
        assert_eq!(env.dissolved_counts(),[0,u64::from(u32::MAX),19]);
        for i in 0..env.patch_count() {
            if env.patch_position(i).0 > 200.0 {
                assert_eq!(env.oxygen_count(i),0);
                assert_eq!(env.inorganic_carbon_count(i),0);
            }
        }
    }

    #[test]
    fn clipping_resize_and_fallback_deposits_never_destroy_a_count() {
        let mut env = empty();
        assert_eq!(env.release_mineral_counts(0,u32::MAX),u32::MAX);
        assert_eq!(env.release_mineral_counts(0,5),0);
        assert_eq!(env.release_mineral_counts_near(0,5),5);
        assert_eq!(env.take_mineral_counts(0,2),2);
        let total = env.dissolved_counts();
        for (w, h) in [(740.0, 420.0), (400.0, 400.0), (420.0, 740.0)] {
            assert!(env.set_size(w,h));
            assert_eq!(env.dissolved_counts(),total);
        }
        let mut crowded = empty();
        assert!(crowded.set_size(800.0,400.0));
        crowded.release_oxygen_counts(0, u32::MAX);
        crowded.release_oxygen_counts(1, 2);
        let original = state(&crowded);
        assert!(!crowded.set_size(400.0,400.0));
        assert_eq!(state(&crowded),original);
    }

    #[test]
    fn checkpoint_preserves_subcount_transport_ports_and_reservoir_phases() {
        let mut env = empty();
        env.configure_gas_exchange(true, 0.011, 0.0023, 0.0012);
        env.add_source(3, 10.0, 20.0, 0.00031, 180.0);
        env.add_source(9, 100.0, 20.0, 0.0031, 30.0);
        env.release_mineral_counts(47, 15);
        env.release_inorganic_carbon_counts(15, 223);
        env.release_oxygen_counts(14, 19);
        step(&mut env, 19);
        let snapshot = state(&env);
        let mut r = Reader::new(&snapshot);
        let mut other = Environment::read_checkpoint(&mut r, 17).unwrap();
        assert!(r.finished());
        assert_eq!(state(&other),snapshot);
        for i in 19..375 {
            env.step(i as f64 * DT);
            other.step(i as f64 * DT);
        }
        assert_eq!(state(&env),state(&other));
        let stocks = env.dissolved_counts();
        let inputs = env.external_input_counts();
        let outputs = env.external_removed_counts();
        for c in 0..3 {
            assert_eq!(stocks[c]+outputs[c],inputs[c]+[15,19,223][c]);
        }
        assert!(inputs[1]>0 && inputs[2]>0);
        assert!(outputs[1]>0);

        let legacy = Environment::default();
        let v17 = state(&legacy);
        let mut r =
            Reader::new(&v17[1..]); // continuous v16 had no mode prefix
        let restored = Environment::read_checkpoint(&mut r, 16).unwrap();
        assert!(r.finished() && !restored.uses_count_chemistry());
        assert_eq!(state(&restored),v17);
    }
}
