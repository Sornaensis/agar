//! Bounded fields on the same torus as the cells. Currents are prescribed;
//! temperature is persistent and advected/diffused, while light is instantaneous.
use crate::simulation::{DT, HEIGHT, WIDTH};
use std::f64::consts::TAU;
mod checkpoint;

pub const COLUMNS: usize = 60;
pub const ROWS: usize = 38;
pub const PATCHES: usize = COLUMNS * ROWS;
pub const MAX_SOURCES: usize = 16;
pub const PATCH_AREA: f64 = WIDTH / COLUMNS as f64 * HEIGHT / ROWS as f64;
pub const DEFAULT_INITIAL_NUTRIENTS: f64 = 0.3;
const NUTRIENT_DIFFUSION: f64 = 12.0;
const NUTRIENT_SOURCE_RATE: f64 = 1.5;

#[derive(Clone, Debug)]
pub struct Source {
    pub id: u32,
    pub kind: u32,
    pub x: f64,
    pub y: f64,
    pub strength: f64,
    pub radius: f64,
}

/// Reusable work buffers, never serialized and never read before initialization.
#[derive(Clone, Default)]
struct FieldScratch {
    advected: Vec<f64>,
    patch_flow: Vec<(f64, f64)>,
    transfers: Vec<(usize, usize, f64)>,
    outgoing: Vec<f64>,
    limits: Vec<f64>,
    remaining: Vec<f64>,
    incoming: Vec<f64>,
}

#[derive(Clone)]
pub struct Environment {
    pub ambient_temperature: f64,
    pub ambient_light: f64,
    pub current_strength: f64,
    /// Mineral mass initially assigned to each 20×20 patch on reset.
    pub initial_nutrients: f64,
    pub sources: Vec<Source>,
    temperature: Vec<f64>,
    light: Vec<f64>,
    heating: Vec<f64>,
    nutrients: Vec<f64>,
    nutrient_injection: Vec<f64>,
    organic: Vec<f64>,
    nutrient_inputs: f64,
    next_source_id: u32,
    scratch: FieldScratch,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            ambient_temperature: 0.5,
            ambient_light: 0.0,
            current_strength: 0.0,
            initial_nutrients: DEFAULT_INITIAL_NUTRIENTS,
            sources: Vec::new(),
            temperature: vec![0.5; PATCHES],
            light: vec![0.0; PATCHES],
            heating: vec![0.0; PATCHES],
            nutrients: vec![DEFAULT_INITIAL_NUTRIENTS; PATCHES],
            nutrient_injection: vec![0.0; PATCHES],
            organic: vec![0.0; PATCHES],
            nutrient_inputs: DEFAULT_INITIAL_NUTRIENTS * PATCHES as f64,
            next_source_id: 1,
            scratch: FieldScratch::default(),
        }
    }
}

impl Environment {
    pub fn reset_dynamic(&mut self) {
        self.temperature.fill(self.ambient_temperature);
        self.nutrients.fill(self.initial_nutrients);
        self.organic.fill(0.0);
        self.nutrient_inputs = self.initial_nutrients * PATCHES as f64;
    }

    /// This changes the next reset's recipe, never overwrites a live field.
    pub fn set_initial_nutrients(&mut self, level: f64) {
        self.initial_nutrients = bounded(level, 0.0, 5.0, DEFAULT_INITIAL_NUTRIENTS);
    }

    pub fn nutrient_at(&self, x: f64, y: f64) -> f64 {
        sample(&self.nutrients, x, y)
    }
    pub fn nutrient_gradient(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.nutrient_at(x + 10.0, y) - self.nutrient_at(x - 10.0, y),
            self.nutrient_at(x, y + 10.0) - self.nutrient_at(x, y - 10.0),
        )
    }
    /// A dimensionless local chemical cue, not an energy or mineral store.
    /// It is emitted by decay/leakage and transported with the water.
    pub fn organic_at(&self, x: f64, y: f64) -> f64 {
        sample(&self.organic, x, y)
    }
    pub fn organic_gradient(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.organic_at(x + 10.0, y) - self.organic_at(x - 10.0, y),
            self.organic_at(x, y + 10.0) - self.organic_at(x, y - 10.0),
        )
    }
    pub fn emit_organic(&mut self, x: f64, y: f64, amount: f64) {
        if amount.is_finite() && amount > 0.0 {
            let i = Self::patch(x, y);
            self.organic[i] = (self.organic[i] + amount).min(1e6);
        }
    }
    fn step_organic(&mut self) {
        if self.organic.iter().all(|&v| v == 0.0) {
            return;
        }
        self.scratch.advected.resize(PATCHES, 0.0);
        for i in 0..PATCHES {
            let (x, y) = Self::patch_center(i);
            let (vx, vy) = self.scratch.patch_flow[i];
            self.scratch.advected[i] = self.organic_at(x - vx * DT, y - vy * DT);
        }
        let advected = &self.scratch.advected;
        let diffusion = 90.0 * DT / PATCH_AREA;
        let retained = (-0.18 * DT).exp();
        for i in 0..PATCHES {
            let col = i % COLUMNS;
            let row = i / COLUMNS;
            let neighbors = [
                row * COLUMNS + (col + COLUMNS - 1) % COLUMNS,
                row * COLUMNS + (col + 1) % COLUMNS,
                ((row + ROWS - 1) % ROWS) * COLUMNS + col,
                ((row + 1) % ROWS) * COLUMNS + col,
            ];
            self.organic[i] = ((advected[i]
                + diffusion
                    * (neighbors.iter().map(|&j| advected[j]).sum::<f64>() - 4.0 * advected[i]))
                * retained)
                .max(0.0);
        }
    }
    pub fn nutrient_patch(&self, index: usize) -> f64 {
        self.nutrients.get(index).copied().unwrap_or(0.0)
    }
    pub fn nutrient_total(&self) -> f64 {
        self.nutrients.iter().sum()
    }
    pub fn nutrient_input_total(&self) -> f64 {
        self.nutrient_inputs
    }

    pub fn take_nutrients(&mut self, index: usize, requested: f64) -> f64 {
        if !requested.is_finite() || requested <= 0.0 {
            return 0.0;
        }
        let Some(balance) = self.nutrients.get_mut(index) else {
            return 0.0;
        };
        let taken = requested.min(*balance);
        *balance -= taken;
        taken
    }

    /// Internal recycling is separate from externally supplied material.
    pub fn recycle_nutrients(&mut self, x: f64, y: f64, amount: f64) {
        if !amount.is_finite() || amount <= 0.0 {
            return;
        }
        let x = if x.is_finite() { x } else { 0.0 };
        let y = if y.is_finite() { y } else { 0.0 };
        self.nutrients[Self::patch(x, y)] += amount;
    }

    pub fn add_nutrients(&mut self, x: f64, y: f64, amount: f64) -> f64 {
        if !amount.is_finite() || amount <= 0.0 {
            return 0.0;
        }
        let amount = amount.min(1000.0);
        if x < 0.0 || y < 0.0 || !x.is_finite() || !y.is_finite() {
            let share = amount / PATCHES as f64;
            for balance in &mut self.nutrients {
                *balance += share;
            }
        } else {
            self.recycle_nutrients(x, y, amount);
        }
        self.nutrient_inputs += amount;
        amount
    }

    pub fn configure(&mut self, temperature: f64, light: f64, current: f64) {
        let temperature = bounded(temperature, 0.0, 1.0, 0.5);
        let change = temperature - self.ambient_temperature;
        self.ambient_temperature = temperature;
        self.ambient_light = bounded(light, 0.0, 1.0, 0.0);
        self.current_strength = bounded(current, 0.0, 30.0, 0.0);
        for value in &mut self.temperature {
            *value = (*value + change).clamp(0.0, 1.0);
        }
        self.rebuild_sources();
    }

    pub fn add_source(&mut self, kind: u32, x: f64, y: f64, strength: f64, radius: f64) -> u32 {
        if kind > 3
            || self.sources.len() >= MAX_SOURCES
            || !x.is_finite()
            || !y.is_finite()
            || !strength.is_finite()
            || !radius.is_finite()
        {
            return 0;
        }
        let id = self.next_source_id;
        let Some(next) = id.checked_add(1) else {
            return 0;
        };
        self.next_source_id = next;
        self.sources.push(Source {
            id,
            kind,
            x: x.rem_euclid(WIDTH),
            y: y.rem_euclid(HEIGHT),
            strength: strength.clamp(if kind == 0 || kind == 3 { 0.0 } else { -1.0 }, 1.0),
            radius: radius.clamp(40.0, 400.0),
        });
        self.rebuild_sources();
        id
    }

    pub fn remove_source(&mut self, id: u32) -> bool {
        let before = self.sources.len();
        self.sources.retain(|s| s.id != id);
        if before == self.sources.len() {
            return false;
        }
        self.rebuild_sources();
        true
    }

    pub fn clear_sources(&mut self) {
        self.sources.clear();
        self.rebuild_sources();
    }

    fn rebuild_sources(&mut self) {
        self.nutrient_injection.fill(0.0);
        for i in 0..PATCHES {
            let (x, y) = Self::patch_center(i);
            let mut light = self.ambient_light;
            let mut heating = 0.0;
            for source in &self.sources {
                let d2 = delta(x - source.x, WIDTH).powi(2) + delta(y - source.y, HEIGHT).powi(2);
                let weight = (-2.0 * d2 / source.radius.powi(2)).exp();
                if source.kind == 0 {
                    light += source.strength * weight;
                }
                if source.kind == 1 {
                    heating += source.strength * weight;
                }
            }
            self.light[i] = light.clamp(0.0, 1.0);
            self.heating[i] = heating.clamp(-2.0, 2.0);
        }
        // Each mineral source has a total production rate independent of its
        // radius. Normalized weights distribute that finite input on the torus.
        for source in self
            .sources
            .iter()
            .filter(|source| source.kind == 3 && source.strength > 0.0)
        {
            let weights: Vec<f64> = (0..PATCHES)
                .map(|i| {
                    let (x, y) = Self::patch_center(i);
                    let d2 =
                        delta(x - source.x, WIDTH).powi(2) + delta(y - source.y, HEIGHT).powi(2);
                    (-2.0 * d2 / source.radius.powi(2)).exp()
                })
                .collect();
            let total: f64 = weights.iter().sum();
            for (rate, weight) in self.nutrient_injection.iter_mut().zip(weights) {
                *rate += NUTRIENT_SOURCE_RATE * source.strength * weight / total;
            }
        }
    }

    pub fn patch(x: f64, y: f64) -> usize {
        let col = (x.rem_euclid(WIDTH) / WIDTH * COLUMNS as f64) as usize;
        let row = (y.rem_euclid(HEIGHT) / HEIGHT * ROWS as f64) as usize;
        row.min(ROWS - 1) * COLUMNS + col.min(COLUMNS - 1)
    }

    pub fn patch_center(index: usize) -> (f64, f64) {
        (
            ((index % COLUMNS) as f64 + 0.5) * WIDTH / COLUMNS as f64,
            ((index / COLUMNS) as f64 + 0.5) * HEIGHT / ROWS as f64,
        )
    }

    pub fn patch_light(&self, index: usize) -> f64 {
        self.light[index]
    }
    pub fn light_at(&self, x: f64, y: f64) -> f64 {
        sample(&self.light, x, y)
    }
    pub fn temperature_at(&self, x: f64, y: f64) -> f64 {
        sample(&self.temperature, x, y)
    }

    pub fn light_gradient(&self, x: f64, y: f64) -> (f64, f64) {
        let h = 10.0;
        (
            self.light_at(x + h, y) - self.light_at(x - h, y),
            self.light_at(x, y + h) - self.light_at(x, y - h),
        )
    }

    pub fn temperature_gradient(&self, x: f64, y: f64) -> (f64, f64) {
        let h = 10.0;
        (
            self.temperature_at(x + h, y) - self.temperature_at(x - h, y),
            self.temperature_at(x, y + h) - self.temperature_at(x, y - h),
        )
    }

    pub fn gradients(&self, x: f64, y: f64) -> ((f64, f64), (f64, f64)) {
        (self.light_gradient(x, y), self.temperature_gradient(x, y))
    }

    pub fn flow_at(&self, x: f64, y: f64, time: f64) -> (f64, f64) {
        let phase = time * 0.04;
        let a = TAU * x / WIDTH + phase;
        let b = TAU * y / HEIGHT - phase;
        let mut vx = self.current_strength * b.sin() * a.cos();
        let mut vy = -self.current_strength * HEIGHT / WIDTH * b.cos() * a.sin();
        for source in &self.sources {
            if source.kind != 2 {
                continue;
            }
            let dx = delta(x - source.x, WIDTH);
            let dy = delta(y - source.y, HEIGHT);
            let weight = source.strength * 60.0 / source.radius
                * (-2.0 * (dx * dx + dy * dy) / source.radius.powi(2)).exp();
            vx -= dy * weight;
            vy += dx * weight;
        }
        let speed = vx.hypot(vy);
        if speed > 60.0 {
            (vx * 60.0 / speed, vy * 60.0 / speed)
        } else {
            (vx, vy)
        }
    }

    pub fn step(&mut self, time: f64) {
        self.step_nutrients(time);
        let organic_active = self.organic.iter().any(|&v| v != 0.0);
        let heat_active = !(self.heating.iter().all(|&v| v == 0.0)
            && self
                .temperature
                .iter()
                .all(|&v| v == self.ambient_temperature));
        // Both semi-Lagrangian fields sample flow at the same patch centers and
        // time. Recompute this derived table each step so source/config changes
        // cannot leave stale velocities. Mineral fluxes sample different faces.
        if organic_active || heat_active {
            self.scratch.patch_flow.resize(PATCHES, (0.0, 0.0));
            for i in 0..PATCHES {
                let (x, y) = Self::patch_center(i);
                self.scratch.patch_flow[i] = self.flow_at(x, y, time);
            }
        }
        self.step_organic();
        // A neutral field stays exactly neutral and incurs no grid solve.
        if !heat_active {
            return;
        }
        self.scratch.advected.resize(PATCHES, 0.0);
        for i in 0..PATCHES {
            let (x, y) = Self::patch_center(i);
            let (vx, vy) = self.scratch.patch_flow[i];
            self.scratch.advected[i] = self.temperature_at(x - vx * DT, y - vy * DT);
        }
        let advected = &self.scratch.advected;
        // Grid spacing is 20 world units: 60 units²/s gives a stable diffusion
        // coefficient of 0.005 per neighbor at the fixed 1/30 s timestep.
        let diffusion = 60.0 * DT / 400.0;
        for i in 0..PATCHES {
            let col = i % COLUMNS;
            let row = i / COLUMNS;
            let left = row * COLUMNS + (col + COLUMNS - 1) % COLUMNS;
            let right = row * COLUMNS + (col + 1) % COLUMNS;
            let up = (row + ROWS - 1) % ROWS * COLUMNS + col;
            let down = (row + 1) % ROWS * COLUMNS + col;
            let diffusion_delta = diffusion
                * (advected[left] + advected[right] + advected[up] + advected[down]
                    - 4.0 * advected[i]);
            let forcing =
                (self.heating[i] * 0.35 + (self.ambient_temperature - advected[i]) * 0.08) * DT;
            self.temperature[i] = (advected[i] + diffusion_delta + forcing).clamp(0.0, 1.0);
        }
    }

    fn step_nutrients(&mut self, time: f64) {
        let has_flow = self.current_strength > 0.0
            || self
                .sources
                .iter()
                .any(|source| source.kind == 2 && source.strength != 0.0);
        let nonuniform = self.nutrients.windows(2).any(|pair| pair[0] != pair[1]);
        if has_flow || nonuniform {
            // Paired finite-volume fluxes move mass between wrapped neighbors.
            // Semi-Lagrangian interpolation is deliberately reserved for heat.
            self.scratch.transfers.clear();
            self.scratch.transfers.reserve(PATCHES * 2);
            self.scratch.outgoing.resize(PATCHES, 0.0);
            self.scratch.outgoing.fill(0.0);
            let diffusion = NUTRIENT_DIFFUSION * DT / PATCH_AREA;
            let advection = DT / 20.0;
            for i in 0..PATCHES {
                let column = i % COLUMNS;
                let row = i / COLUMNS;
                let right = row * COLUMNS + (column + 1) % COLUMNS;
                let down = (row + 1) % ROWS * COLUMNS + column;
                let (x, y) = Self::patch_center(i);
                let vx = if has_flow {
                    self.flow_at(x + 10.0, y, time).0
                } else {
                    0.0
                };
                let vy = if has_flow {
                    self.flow_at(x, y + 10.0, time).1
                } else {
                    0.0
                };
                for (neighbor, velocity) in [(right, vx), (down, vy)] {
                    let donor = if velocity >= 0.0 {
                        self.nutrients[i]
                    } else {
                        self.nutrients[neighbor]
                    };
                    let flux = velocity * advection * donor
                        + diffusion * (self.nutrients[i] - self.nutrients[neighbor]);
                    let (from, to, amount) = if flux >= 0.0 {
                        (i, neighbor, flux)
                    } else {
                        (neighbor, i, -flux)
                    };
                    if amount > 0.0 {
                        self.scratch.outgoing[from] += amount;
                        self.scratch.transfers.push((from, to, amount));
                    }
                }
            }
            let FieldScratch {
                transfers,
                outgoing,
                limits,
                remaining,
                incoming,
                ..
            } = &mut self.scratch;
            limits.clear();
            limits.extend(
                outgoing
                    .iter()
                    .zip(&self.nutrients)
                    .map(|(&requested, &available)| {
                        if requested > available {
                            available / requested
                        } else {
                            1.0
                        }
                    }),
            );
            remaining.clone_from(&self.nutrients);
            incoming.resize(PATCHES, 0.0);
            incoming.fill(0.0);
            for &(from, to, requested) in transfers.iter() {
                // Budget against the old donor balance: same-step arrivals
                // cannot be sent again, and no positivity clamp destroys mass.
                let actual = (requested * limits[from]).min(remaining[from]);
                remaining[from] -= actual;
                incoming[to] += actual;
            }
            for i in 0..PATCHES {
                self.nutrients[i] = remaining[i] + incoming[i];
            }
        }
        let mut supplied = 0.0;
        for (balance, rate) in self.nutrients.iter_mut().zip(&self.nutrient_injection) {
            let amount = rate * DT;
            *balance += amount;
            supplied += amount;
        }
        self.nutrient_inputs += supplied;
    }

    pub fn snapshot(&self, time: f64, target: &mut Vec<f32>) {
        target.clear();
        target.extend_from_slice(&[
            COLUMNS as f32,
            ROWS as f32,
            self.ambient_temperature as f32,
            self.ambient_light as f32,
            self.current_strength as f32,
            self.sources.len() as f32,
            self.initial_nutrients as f32,
        ]);
        for i in 0..PATCHES {
            let (x, y) = Self::patch_center(i);
            let (vx, vy) = self.flow_at(x, y, time);
            target.extend_from_slice(&[
                self.light[i] as f32,
                self.temperature[i] as f32,
                vx as f32,
                vy as f32,
                self.nutrients[i] as f32,
            ]);
        }
        for source in &self.sources {
            target.extend_from_slice(&[
                source.id as f32,
                source.kind as f32,
                source.x as f32,
                source.y as f32,
                source.strength as f32,
                source.radius as f32,
            ]);
        }
    }
}

fn bounded(value: f64, lo: f64, hi: f64, default: f64) -> f64 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        default
    }
}
fn delta(value: f64, size: f64) -> f64 {
    (value + size * 0.5).rem_euclid(size) - size * 0.5
}

fn sample(values: &[f64], x: f64, y: f64) -> f64 {
    let gx = x.rem_euclid(WIDTH) / WIDTH * COLUMNS as f64 - 0.5;
    let gy = y.rem_euclid(HEIGHT) / HEIGHT * ROWS as f64 - 0.5;
    let ix = gx.floor() as isize;
    let iy = gy.floor() as isize;
    let fx = gx - gx.floor();
    let fy = gy - gy.floor();
    let at = |x: isize, y: isize| {
        values[y.rem_euclid(ROWS as isize) as usize * COLUMNS
            + x.rem_euclid(COLUMNS as isize) as usize]
    };
    let a = at(ix, iy) * (1.0 - fx) + at(ix + 1, iy) * fx;
    let b = at(ix, iy + 1) * (1.0 - fx) + at(ix + 1, iy + 1) * fx;
    a * (1.0 - fy) + b * fy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_bounds_and_wrapped_fields() {
        let mut env = Environment::default();
        assert_eq!(env.add_source(9, 0.0, 0.0, 1.0, 100.0), 0);
        assert_eq!(env.add_source(0, f64::NAN, 0.0, 1.0, 100.0), 0);
        let id = env.add_source(0, 2.0, 2.0, 1.0, 100.0);
        assert!(id > 0);
        assert!(env.light_at(WIDTH - 2.0, 2.0) > 0.9);
        assert!((env.light_at(1.0, 3.0) - env.light_at(WIDTH + 1.0, HEIGHT + 3.0)).abs() < 1e-12);
        for _ in 1..MAX_SOURCES {
            assert!(env.add_source(0, 0.0, 0.0, 1.0, 1.0) > 0);
        }
        assert_eq!(env.add_source(0, 0.0, 0.0, 1.0, 1.0), 0);
        assert!(env.remove_source(id));
        assert!(!env.remove_source(id));
        env.clear_sources();
        assert!(env.sources.is_empty());
        assert_eq!(env.light_at(2.0, 2.0), 0.0);
    }

    #[test]
    fn heat_persists_diffuses_and_reset_replays() {
        let mut env = Environment::default();
        env.add_source(1, 10.0, 10.0, 1.0, 80.0);
        for i in 0..90 {
            env.step(i as f64 * DT);
        }
        assert!(env.temperature_at(10.0, 10.0) > 0.8);
        assert!(env.temperature_at(WIDTH - 10.0, 10.0) > 0.7);
        let first = env.temperature.clone();
        env.reset_dynamic();
        for i in 0..90 {
            env.step(i as f64 * DT);
        }
        assert_eq!(env.temperature, first);
        env.clear_sources();
        assert!(
            env.temperature_at(10.0, 10.0) > 0.8,
            "heat outlasts its source"
        );
        for i in 90..180 {
            env.step(i as f64 * DT);
        }
        assert!(env.temperature_at(10.0, 10.0) < first[0]);
        assert!(env
            .temperature
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
    }

    #[test]
    fn reused_work_buffers_match_fresh_checkpoint_restores_across_reset() {
        use crate::checkpoint::{Reader, Writer};

        fn state(env: &Environment) -> Vec<u8> {
            let mut writer = Writer::new();
            env.write_checkpoint(&mut writer);
            assert!(!writer.failed());
            writer.0
        }

        let mut reused = Environment::default();
        reused.configure(0.45, 0.2, 18.0);
        reused.add_source(1, 10.0, 10.0, 0.8, 80.0);
        reused.add_source(2, 1195.0, 755.0, -0.7, 120.0);
        reused.add_source(3, 30.0, 750.0, 0.4, 100.0);
        reused.add_nutrients(1195.0, 5.0, 250.0);
        reused.emit_organic(5.0, 755.0, 4.0);

        for tick in 0..36 {
            if tick == 12 {
                // Leave stale work buffers in a neutral world for several ticks.
                reused.clear_sources();
                reused.configure(0.4, 0.0, 0.0);
                reused.set_initial_nutrients(0.07);
                reused.reset_dynamic();
            } else if tick == 18 {
                reused.configure(0.4, 0.1, 27.0);
                reused.add_source(1, 600.0, 380.0, -0.6, 110.0);
                reused.add_source(2, 800.0, 100.0, 0.9, 60.0);
                reused.add_nutrients(600.0, 380.0, 12.0);
                reused.emit_organic(1195.0, 5.0, 8.0);
            }

            let bytes = state(&reused);
            let mut reader = Reader::new(&bytes);
            let mut fresh = Environment::read_checkpoint(&mut reader).unwrap();
            assert!(reader.finished());
            assert_eq!(state(&fresh), bytes);

            let time = tick as f64 * DT;
            reused.step(time);
            fresh.step(time);
            assert_eq!(state(&reused), state(&fresh), "tick {tick}");
        }
    }

    #[test]
    fn neutral_environment_is_exact_and_flow_is_periodic_and_bounded() {
        let mut env = Environment::default();
        env.step(0.0);
        assert_eq!(env.temperature_at(100.0, 20.0), 0.5);
        assert_eq!(env.flow_at(100.0, 20.0, 3.0), (0.0, 0.0));
        env.configure(0.5, 0.2, 30.0);
        env.add_source(2, 3.0, 2.0, 1.0, 100.0);
        let a = env.flow_at(40.0, 30.0, 3.0);
        let b = env.flow_at(WIDTH + 40.0, HEIGHT + 30.0, 3.0);
        assert!((a.0 - b.0).abs() < 1e-10 && (a.1 - b.1).abs() < 1e-10);
        assert!(a.0.hypot(a.1) <= 60.0);
        let mut snapshot = Vec::new();
        env.snapshot(3.0, &mut snapshot);
        assert_eq!(snapshot.len(), 7 + PATCHES * 5 + 6);
        assert!(snapshot.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn mineral_diffusion_crosses_both_seams_without_creating_or_losing_mass() {
        let mut env = Environment::default();
        env.set_initial_nutrients(0.0);
        env.reset_dynamic();
        assert_eq!(env.add_nutrients(10.0, 10.0, 100.0), 100.0);
        env.step(0.0);
        assert!(env.nutrient_patch(COLUMNS - 1) > 0.0);
        assert!(env.nutrient_patch(PATCHES - COLUMNS) > 0.0);
        for tick in 1..300 {
            env.step(tick as f64 * DT);
        }
        assert!((env.nutrient_total() - 100.0).abs() < 1e-9);
        assert_eq!(env.nutrient_input_total(), 100.0);
        assert!(env
            .nutrients
            .iter()
            .all(|mass| mass.is_finite() && *mass >= 0.0));
        assert!(
            (env.nutrient_at(1.0, 3.0) - env.nutrient_at(WIDTH + 1.0, HEIGHT + 3.0)).abs() < 1e-12
        );
    }

    #[test]
    fn conservative_flow_keeps_extreme_mineral_gradients_finite_and_nonnegative() {
        let mut env = Environment::default();
        env.set_initial_nutrients(0.0);
        env.reset_dynamic();
        env.configure(0.5, 0.0, 30.0);
        for (x, y, strength) in [(5.0, 5.0, 1.0), (1180.0, 740.0, -1.0), (600.0, 380.0, 1.0)] {
            env.add_source(2, x, y, strength, 80.0);
        }
        env.recycle_nutrients(1.0, 1.0, 1_000_000.0);
        env.recycle_nutrients(600.0, 380.0, 1e-9);
        let initial = env.nutrient_total();
        let original_patch = env.nutrient_patch(0);
        for tick in 0..600 {
            env.step(tick as f64 * DT);
            assert!(env
                .nutrients
                .iter()
                .all(|mass| mass.is_finite() && *mass >= 0.0));
        }
        assert!(env.nutrient_patch(0) < original_patch);
        assert!((env.nutrient_total() - initial).abs() < initial * 1e-11);
        assert_eq!(
            env.nutrient_input_total(),
            0.0,
            "internal transfers are not external inputs"
        );
    }

    #[test]
    fn mineral_sources_have_finite_radius_independent_input_and_replay() {
        for radius in [40.0, 400.0] {
            let mut env = Environment::default();
            env.set_initial_nutrients(0.0);
            env.reset_dynamic();
            env.configure(0.5, 0.0, 20.0);
            let id = env.add_source(3, 1195.0, 755.0, 0.4, radius);
            assert!(id > 0);
            for tick in 0..90 {
                env.step(tick as f64 * DT);
            }
            let expected = NUTRIENT_SOURCE_RATE * 0.4 * 90.0 * DT;
            assert!((env.nutrient_total() - expected).abs() < 1e-10);
            assert!((env.nutrient_input_total() - expected).abs() < 1e-10);
            let first = env.nutrients.clone();
            env.reset_dynamic();
            assert_eq!(env.sources[0].id, id);
            assert_eq!(env.nutrient_total(), 0.0);
            for tick in 0..90 {
                env.step(tick as f64 * DT);
            }
            assert_eq!(env.nutrients, first);
            env.remove_source(id);
            let before = env.nutrient_total();
            for tick in 90..120 {
                env.step(tick as f64 * DT);
            }
            assert!((env.nutrient_total() - before).abs() < 1e-10);
        }
    }

    #[test]
    fn nutrient_reset_recipe_and_user_additions_are_explicit() {
        let mut env = Environment::default();
        let original = env.nutrient_total();
        env.set_initial_nutrients(0.1);
        assert_eq!(
            env.nutrient_total(),
            original,
            "editing the recipe preserves the live field"
        );
        assert_eq!(env.add_nutrients(-1.0, -1.0, 8.0), 8.0);
        assert!((env.nutrient_total() - original - 8.0).abs() < 1e-9);
        env.reset_dynamic();
        assert!((env.nutrient_total() - 0.1 * PATCHES as f64).abs() < 1e-9);
        assert_eq!(env.nutrient_input_total(), 0.1 * PATCHES as f64);
        assert_eq!(env.add_nutrients(10.0, 10.0, f64::NAN), 0.0);
        assert_eq!(env.add_nutrients(10.0, 10.0, -1.0), 0.0);
        assert_eq!(env.add_nutrients(10.0, 10.0, 5000.0), 1000.0);
        env.set_initial_nutrients(999.0);
        assert_eq!(env.initial_nutrients, 5.0);
        env.set_initial_nutrients(f64::NAN);
        assert_eq!(env.initial_nutrients, DEFAULT_INITIAL_NUTRIENTS);
    }

    #[test]
    fn withdrawals_and_recycling_never_overdraw_or_count_twice() {
        let mut env = Environment::default();
        env.set_initial_nutrients(0.0);
        env.reset_dynamic();
        env.add_nutrients(10.0, 10.0, 2.0);
        let input = env.nutrient_input_total();
        assert_eq!(env.take_nutrients(0, 1.5), 1.5);
        assert_eq!(env.take_nutrients(0, 99.0), 0.5);
        assert_eq!(env.take_nutrients(0, 1.0), 0.0);
        assert_eq!(env.take_nutrients(PATCHES, 1.0), 0.0);
        assert_eq!(env.take_nutrients(0, f64::NAN), 0.0);
        env.recycle_nutrients(WIDTH - 1.0, HEIGHT - 1.0, 2.0);
        assert_eq!(env.nutrient_total(), 2.0);
        assert_eq!(env.nutrient_patch(PATCHES - 1), 2.0);
        assert_eq!(env.nutrient_input_total(), input);
    }
}
