//! A fixed-step, seeded simulation. Genomes only read the pre-step snapshot;
//! material transport, upkeep, uptake, growth, feeding, predation, remodeling,
//! division, and death are resolved by the host in that order.
use crate::genome::{EvalScratch, Inputs, MutationKind, Outputs, Program, Registers};
use crate::environment::{Environment, PATCHES, PATCH_AREA};
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};
mod checkpoint;

pub const WIDTH: f64 = 1200.0;
pub const HEIGHT: f64 = 760.0;
pub const DT: f64 = 1.0 / 30.0;
pub const SENSE_RADIUS: f64 = 120.0;
pub const MAX_CELLS: usize = 384;
pub const MAX_FOOD: usize = 1200;
pub const CELL_RADIUS: f64 = 7.0;
pub const MAX_ENERGY: f64 = 200.0;
pub const MIN_SIZE: f64 = 0.5;
pub const MAX_SIZE: f64 = 4.0;
pub const MAX_GROWTH_RATE: f64 = 0.25;
pub const GROWTH_ENERGY_COST: f64 = 20.0;
pub const BASAL_METABOLISM: f64 = 0.65;
pub const THRUST_ENERGY_COST: f64 = 1.45;
pub const CELL_STRIDE: usize = 53;
pub const MEMORY_REGISTER_COST: f64 = 0.0025;
pub const MAX_GENOME_BYTES: usize = 64 * 1024 * 1024;
const PHOTO_RATE: f64 = 3.0;
const PHOTO_UPKEEP: f64 = 0.18;
const PHOTO_PATCH_POWER: f64 = 0.02;
const BITE_UPKEEP: f64 = 0.9;
const DEFENSE_UPKEEP: f64 = 0.6;
const REPAIR_RATE: f64 = 0.15;
const REPAIR_COST: f64 = 20.0;
const ENGULF_RATIO: f64 = 1.6;
const ENGULF_COST: f64 = 8.0;
const ENGULF_RECOVERY: f64 = 0.8;
const REMAINS_RECOVERY: f64 = 0.6;
const FOOD_NUTRIENTS: f64 = 0.25;
const FOOD_DECAY: f64 = 0.02;
const DIGESTION_RATE: f64 = 90.0;
const UPTAKE_RATE: f64 = 0.25;
const REMODELING_RATE: f64 = 0.08;
const REMODELING_COST: f64 = 12.0;
const PROPULSION: usize = 0;
const PHOTOSYNTHESIS: usize = 1;
const DIGESTION: usize = 2;
const ARMOR: usize = 3;
const SENSING: usize = 4;
pub const MAX_GENOMES: usize = 2048;
const DIVISION_MIN_ENERGY: f64 = 90.0;
const DIVISION_COST: f64 = 12.0;
const DIVISION_COOLDOWN: f64 = 4.0;
const GRID_COLUMNS: usize = 10;
const GRID_ROWS: usize = 6;
const GRID_BUCKETS: usize = GRID_COLUMNS * GRID_ROWS;

#[derive(Clone)]
pub struct Genome {
    pub source: String,
    pub program: Program,
    pub parent_id: u32,
    pub mutation_kind: u32,
    pub bytes: usize,
}

impl Genome {
    pub fn new(source: String, program: Program, parent_id: u32, mutation_kind: u32) -> Self {
        let bytes = std::mem::size_of::<Self>() + source.capacity() + program.estimated_bytes();
        Self { source, program, parent_id, mutation_kind, bytes }
    }
}

pub fn registry_bytes(genomes: &[Genome]) -> usize {
    genomes.iter().map(|genome| genome.bytes).fold(0usize, usize::saturating_add)
}

pub struct MutationResult {
    pub genome_id: u32,
    pub skipped: Option<&'static str>,
}

/// Provenance is part of a genome's identity. Replaying a mutation reuses the
/// same entry, while identical source descended from a different parent does not.
pub fn mutate_registered(
    genomes: &mut Vec<Genome>, parent_id: u32, seed: u64, kind: MutationKind,
) -> MutationResult {
    let Some(parent) = genomes.get(parent_id.wrapping_sub(1) as usize) else {
        return MutationResult { genome_id: 0, skipped: Some("Unknown parent genome.") };
    };
    let Some(mutation) = parent.program.mutate(seed, kind) else {
        return MutationResult { genome_id: parent_id, skipped: Some("No applicable mutation fits this program's limits.") };
    };
    let mutation_kind = match mutation.kind {
        MutationKind::Any => 0,
        MutationKind::Constant => 1,
        MutationKind::Expression => 2,
        MutationKind::Reference => 3,
    };
    if mutation_kind == 0 || mutation.source == parent.source {
        return MutationResult { genome_id: parent_id, skipped: Some("No applicable mutation changes this program.") };
    }
    // Look for a replay before applying the capacity limit: reset must still be
    // able to reproduce already-registered lineages when the registry is full.
    if let Some(index) = genomes.iter().position(|genome| {
        genome.parent_id == parent_id && genome.mutation_kind == mutation_kind && genome.source == mutation.source
    }) {
        return MutationResult { genome_id: index as u32 + 1, skipped: None };
    }
    if genomes.len() >= MAX_GENOMES {
        return MutationResult { genome_id: parent_id, skipped: Some("The 2048-genome limit was reached; the original genome is inherited unchanged.") };
    }
    let genome = Genome::new(mutation.source, mutation.program, parent_id, mutation_kind);
    if registry_bytes(genomes).saturating_add(genome.bytes) > MAX_GENOME_BYTES {
        return MutationResult { genome_id: parent_id, skipped: Some("The 64 MiB genome storage limit was reached; the original genome is inherited unchanged.") };
    }
    genomes.push(genome);
    MutationResult { genome_id: genomes.len() as u32, skipped: None }
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub id: u32,
    pub genome_id: u32,
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub angle: f64,
    pub energy: f64,
    /// Relative biomass and 2D area; mass is conserved when a cell divides.
    pub size: f64,
    pub age: f64,
    pub generation: u32,
    pub parent_id: u32,
    pub cooldown: f64,
    pub memory: Registers,
    pub integrity: f64,
    pub photosynthesis_rate: f64,
    /// Free minerals; each unit of biomass also binds one mineral unit.
    pub nutrient_stored: f64,
    /// Propulsion, photosynthesis, digestion, armor, and sensing fractions.
    pub body: [f64; 5],
    pub uptake_rate: f64,
    pub remodeling_rate: f64,
    pub engulf_cooldown: f64,
}

impl Cell {
    fn alive(&self) -> bool { self.energy > 0.0 && self.integrity > 0.0 }
    pub fn radius(&self) -> f64 { CELL_RADIUS * self.size.sqrt() }
    pub fn energy_capacity(&self) -> f64 { MAX_ENERGY * self.size }
    pub fn nutrient_capacity(&self) -> f64 { 2.0 * self.size }
    pub fn body_upkeep(&self) -> f64 { 0.30 * self.body.iter().sum::<f64>() * self.size }
    pub fn basal_metabolism(&self) -> f64 { 0.35 * self.size + self.body_upkeep() }
    pub fn sense_range(&self) -> f64 { 40.0 + 400.0 * self.body[SENSING] }
    fn capacity_factor(&self, capability: usize) -> f64 { self.body[capability] / 0.2 }

    /// Return overflow for immediate recycling into the environment.
    fn receive_nutrients(&mut self, amount: f64) -> f64 {
        let accepted = amount.min((self.nutrient_capacity() - self.nutrient_stored).max(0.0));
        self.nutrient_stored += accepted;
        amount - accepted
    }

    fn grow(&mut self, signal: f64) {
        if !self.alive() || self.size >= MAX_SIZE || signal <= 0.0 { return; }
        let requested = (signal * MAX_GROWTH_RATE * DT).min(MAX_SIZE - self.size)
            .min(self.nutrient_stored);
        let cost = requested * GROWTH_ENERGY_COST;
        let growth = if cost >= self.energy {
            // Spending all available energy must produce exactly zero, rather
            // than a rounding residue that could let a dead cell eat again.
            let affordable = self.energy / GROWTH_ENERGY_COST;
            self.energy = 0.0;
            affordable
        } else {
            self.energy -= cost;
            requested
        };
        self.nutrient_stored = (self.nutrient_stored - growth).max(0.0);
        self.size = (self.size + growth).min(MAX_SIZE);
    }

    fn remodel(&mut self, output: &Outputs) {
        if !self.alive() { return; }
        let mut target = [output.invest_propulsion, output.invest_photosynthesis,
            output.invest_digestion, output.invest_armor, output.invest_sensing];
        for fraction in &mut target { *fraction = fraction.clamp(0.0, 1.0); }
        let target_sum: f64 = target.iter().sum();
        if target_sum > 1.0 { for fraction in &mut target { *fraction /= target_sum; } }
        let old_core = (1.0 - self.body.iter().sum::<f64>()).max(0.0);
        let new_core = (1.0 - target.iter().sum::<f64>()).max(0.0);
        let distance = 0.5 * ((old_core - new_core).abs()
            + self.body.iter().zip(target).map(|(old, new)| (old - new).abs()).sum::<f64>());
        if distance <= 0.0 { return; }
        let requested = (REMODELING_RATE * self.size * DT).min(distance * self.size);
        let cost = requested * REMODELING_COST;
        let moved = if cost >= self.energy {
            let affordable = self.energy / REMODELING_COST;
            self.energy = 0.0;
            affordable
        } else {
            self.energy -= cost;
            requested
        };
        let fraction = (moved / (distance * self.size)).min(1.0);
        for (current, target) in self.body.iter_mut().zip(target) {
            *current += (target - *current) * fraction;
        }
        // Keep floating-point roundoff from exceeding the shared body budget.
        let sum: f64 = self.body.iter().sum();
        if sum > 1.0 { for current in &mut self.body { *current /= sum; } }
        self.remodeling_rate = moved / DT;
    }
}

#[derive(Clone, Debug)]
pub struct Food {
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    pub nutrients: f64,
}

#[derive(Clone)]
struct Rng(u32);
impl Rng {
    fn new(seed: u32) -> Self { Self(if seed == 0 { 0x9E37_79B9 } else { seed }) }
    fn next(&mut self) -> u32 {
        let mut n = self.0;
        n ^= n << 13;
        n ^= n >> 17;
        n ^= n << 5;
        self.0 = n;
        n
    }
    fn unit(&mut self) -> f64 { self.next() as f64 / (u32::MAX as f64 + 1.0) }
}

/// Wrapped broad-phase queries visit each bucket at most once, even when a
/// specialized cell senses farther than half the world's shorter dimension.
struct Grid {
    buckets: [Vec<usize>; GRID_BUCKETS],
}
impl Grid {
    fn new() -> Self { Self { buckets: std::array::from_fn(|_| Vec::new()) } }
    fn index(x: f64, y: f64) -> usize {
        let col = ((x / WIDTH * GRID_COLUMNS as f64) as usize).min(GRID_COLUMNS - 1);
        let row = ((y / HEIGHT * GRID_ROWS as f64) as usize).min(GRID_ROWS - 1);
        row * GRID_COLUMNS + col
    }
    fn insert(&mut self, x: f64, y: f64, id: usize) { self.buckets[Self::index(x, y)].push(id); }
    fn nearby(&self, x: f64, y: f64) -> impl Iterator<Item = usize> + '_ {
        self.nearby_radius(x, y, SENSE_RADIUS)
    }
    fn nearby_radius(&self, x: f64, y: f64, radius: f64) -> impl Iterator<Item = usize> + '_ {
        let index = Self::index(x, y);
        let col = (index % GRID_COLUMNS) as isize;
        let row = (index / GRID_COLUMNS) as isize;
        let dx = (radius / (WIDTH / GRID_COLUMNS as f64)).ceil() as isize;
        let dy = (radius / (HEIGHT / GRID_ROWS as f64)).ceil() as isize;
        let columns = (2 * dx + 1).min(GRID_COLUMNS as isize);
        let rows = (2 * dy + 1).min(GRID_ROWS as isize);
        (0..rows).flat_map(move |y| {
            (0..columns).flat_map(move |x| {
                let c = (col - dx + x).rem_euclid(GRID_COLUMNS as isize) as usize;
                let r = (row - dy + y).rem_euclid(GRID_ROWS as isize) as usize;
                self.buckets[r * GRID_COLUMNS + c].iter().copied()
            })
        })
    }
}

pub struct World {
    pub cells: Vec<Cell>,
    pub food: Vec<Food>,
    pub ticks: u64,
    pub births: u64,
    pub deaths: u64,
    pub eaten: u64,
    pub food_rate: f64,
    pub mutation_rate: f64,
    pub mutations: u64,
    pub mutations_skipped: u64,
    pub environment: Environment,
    pub predations: u64,
    pub attacks: u64,
    pub photosynthesis_total: f64,
    nutrient_imports: f64,
    food_credit: f64,
    next_id: u32,
    rng: Rng,
    seed: u32,
}

impl World {
    pub fn empty(seed: u32) -> Self {
        Self {
            cells: Vec::new(), food: Vec::new(), ticks: 0,
            births: 0, deaths: 0, eaten: 0, food_rate: 6.0, food_credit: 0.0,
            mutation_rate: 0.10, mutations: 0, mutations_skipped: 0,
            environment: Environment::default(), predations: 0, attacks: 0, photosynthesis_total: 0.0,
            nutrient_imports: 0.0,
            next_id: 1, rng: Rng::new(seed), seed,
        }
    }

    pub fn reset(&mut self, seed: u32, genome_id: u32) {
        let mutation_rate = self.mutation_rate;
        let mut environment = self.environment.clone();
        environment.reset_dynamic();
        *self = Self::empty(seed);
        self.environment = environment;
        self.mutation_rate = mutation_rate;
        self.add_food(-1.0, -1.0, 360);
        self.seed_cells(genome_id, 22, -1.0, -1.0);
    }

    pub fn reset_empty(&mut self, seed: u32, initial_food: usize) {
        let mutation_rate = self.mutation_rate;
        let food_rate = self.food_rate;
        let mut environment = self.environment.clone();
        environment.reset_dynamic();
        *self = Self::empty(seed);
        self.environment = environment;
        self.mutation_rate = mutation_rate;
        self.food_rate = food_rate;
        self.add_food(-1.0, -1.0, initial_food);
    }

    pub fn time(&self) -> f64 { self.ticks as f64 * DT }

    pub fn nutrient_food_total(&self) -> f64 { self.food.iter().map(|food| food.nutrients).sum() }
    pub fn nutrient_cell_total(&self) -> f64 {
        self.cells.iter().map(|cell| cell.size + cell.nutrient_stored).sum()
    }
    pub fn nutrient_total(&self) -> f64 {
        self.environment.nutrient_total() + self.nutrient_food_total() + self.nutrient_cell_total()
    }
    pub fn nutrient_input_total(&self) -> f64 {
        self.environment.nutrient_input_total() + self.nutrient_imports
    }

    pub fn seed_cells(&mut self, genome_id: u32, count: usize, x: f64, y: f64) -> usize {
        let count = count.min(MAX_CELLS - self.cells.len()).min((u32::MAX - self.next_id) as usize);
        let scatter = x < 0.0 || y < 0.0 || !x.is_finite() || !y.is_finite();
        for _ in 0..count {
            let (cx, cy) = if scatter {
                (self.rng.unit() * WIDTH, self.rng.unit() * HEIGHT)
            } else {
                let angle = self.rng.unit() * TAU;
                let radius = self.rng.unit().sqrt() * 38.0;
                (wrap(x + angle.cos() * radius, WIDTH), wrap(y + angle.sin() * radius, HEIGHT))
            };
            let angle = self.rng.unit() * TAU;
            self.cells.push(Cell {
                id: self.next_id, genome_id, x: cx, y: cy,
                vx: angle.cos() * 8.0, vy: angle.sin() * 8.0, angle,
                energy: 88.0 + self.rng.unit() * 12.0,
                size: 1.0,
                age: 0.0, generation: 0, parent_id: 0, cooldown: DIVISION_COOLDOWN,
                memory: Registers::default(), integrity: 1.0, photosynthesis_rate: 0.0,
                nutrient_stored: 0.5, body: [0.2; 5], uptake_rate: 0.0,
                remodeling_rate: 0.0, engulf_cooldown: 0.0,
            });
            self.nutrient_imports += 1.5;
            self.next_id += 1;
        }
        count
    }

    pub fn add_food(&mut self, x: f64, y: f64, count: usize) -> usize {
        let count = count.min(MAX_FOOD - self.food.len());
        let scatter = x < 0.0 || y < 0.0 || !x.is_finite() || !y.is_finite();
        for _ in 0..count {
            let (fx, fy) = if scatter {
                (self.rng.unit() * WIDTH, self.rng.unit() * HEIGHT)
            } else {
                let a = self.rng.unit() * TAU;
                let r = self.rng.unit().sqrt() * 45.0;
                (wrap(x + a.cos() * r, WIDTH), wrap(y + a.sin() * r, HEIGHT))
            };
            self.food.push(Food { x: fx, y: fy, energy: 18.0 + self.rng.unit() * 6.0,
                nutrients: FOOD_NUTRIENTS });
            self.nutrient_imports += FOOD_NUTRIENTS;
        }
        count
    }

    pub fn set_food_rate(&mut self, rate: f64) {
        self.food_rate = if rate.is_finite() { rate.clamp(0.0, 60.0) } else { 0.0 };
    }

    pub fn set_mutation_rate(&mut self, rate: f64) {
        self.mutation_rate = if rate.is_finite() { rate.clamp(0.0, 1.0) } else { 0.0 };
    }

    pub fn sense_all(&self) -> Vec<Inputs> {
        let mut food_grid = Grid::new();
        let mut cell_grid = Grid::new();
        for (i, f) in self.food.iter().enumerate() { food_grid.insert(f.x, f.y, i); }
        for (i, c) in self.cells.iter().enumerate() { if c.alive() { cell_grid.insert(c.x, c.y, i); } }
        self.cells.iter().map(|cell| {
            let (flow_x, flow_y) = self.environment.flow_at(cell.x, cell.y, self.time());
            let (light_gradient, temperature_gradient) = self.environment.gradients(cell.x, cell.y);
            let directional = |(x, y): (f64, f64)| if x.hypot(y) < 1e-12 { 0.0 } else { relative_angle(y.atan2(x) - cell.angle) };
            let used = cell.memory.populated_count() as f64;
            let range = cell.sense_range();
            let mut input = Inputs {
                energy: cell.energy, size: cell.size, age: cell.age,
                speed: cell.vx.hypot(cell.vy), time: self.time(), id: cell.id as f64,
                energy_fraction: (cell.energy / cell.energy_capacity()).clamp(0.0, 1.0),
                memory_used: used, memory_cost: used * MEMORY_REGISTER_COST,
                light_level: self.environment.light_at(cell.x, cell.y), light_angle: directional(light_gradient),
                temperature: self.environment.temperature_at(cell.x, cell.y), temperature_angle: directional(temperature_gradient),
                flow_x, flow_y, flow_speed: flow_x.hypot(flow_y), flow_angle: directional((flow_x, flow_y)),
                integrity: cell.integrity,
                nutrient_stored: cell.nutrient_stored, nutrient_capacity: cell.nutrient_capacity(),
                nutrient_fraction: (cell.nutrient_stored / cell.nutrient_capacity()).clamp(0.0, 1.0),
                nutrient_level: self.environment.nutrient_at(cell.x, cell.y),
                nutrient_angle: directional(self.environment.nutrient_gradient(cell.x, cell.y)),
                body_propulsion: cell.body[PROPULSION], body_photosynthesis: cell.body[PHOTOSYNTHESIS],
                body_digestion: cell.body[DIGESTION], body_armor: cell.body[ARMOR], body_sensing: cell.body[SENSING],
                body_unassigned: (1.0 - cell.body.iter().sum::<f64>()).max(0.0), sense_range: range,
                food_distance: range, neighbor_distance: range,
                prey_distance: range, threat_distance: range,
                ..Inputs::default()
            };
            let mut nearest_food = range * range;
            for i in food_grid.nearby_radius(cell.x, cell.y, range) {
                let target = &self.food[i];
                let dx = delta(target.x - cell.x, WIDTH);
                let dy = delta(target.y - cell.y, HEIGHT);
                let d2 = dx * dx + dy * dy;
                if d2 <= nearest_food {
                    nearest_food = d2;
                    input.food_visible = true;
                    input.food_distance = d2.sqrt();
                    input.food_angle = relative_angle(dy.atan2(dx) - cell.angle);
                }
            }
            let mut nearest_neighbor = range * range;
            let mut nearest_prey = range * range;
            let mut nearest_threat = range * range;
            let mut threat_id = u32::MAX;
            for i in cell_grid.nearby_radius(cell.x, cell.y, range) {
                let target = &self.cells[i];
                if target.id == cell.id { continue; }
                let dx = delta(target.x - cell.x, WIDTH);
                let dy = delta(target.y - cell.y, HEIGHT);
                let d2 = dx * dx + dy * dy;
                let angle = relative_angle(dy.atan2(dx) - cell.angle);
                if d2 <= nearest_neighbor {
                    nearest_neighbor = d2;
                    input.neighbor_visible = true;
                    input.neighbor_distance = d2.sqrt();
                    input.neighbor_angle = angle;
                }
                if target.size <= cell.size * 1.05 && d2 <= nearest_prey
                    && (!input.prey_visible || d2 < nearest_prey || target.id < input.prey_id as u32) {
                    nearest_prey = d2;
                    input.prey_visible = true; input.prey_id = target.id as f64;
                    input.prey_angle = angle; input.prey_distance = d2.sqrt(); input.prey_size = target.size;
                    input.prey_same_genome = target.genome_id == cell.genome_id;
                }
                if target.size >= cell.size * ENGULF_RATIO && d2 <= nearest_threat
                    && (!input.threat_visible || d2 < nearest_threat || target.id < threat_id) {
                    nearest_threat = d2; threat_id = target.id;
                    input.threat_visible = true; input.threat_angle = angle;
                    input.threat_distance = d2.sqrt(); input.threat_size = target.size;
                }
            }
            input
        }).collect()
    }

    pub fn step(&mut self, genomes: &mut Vec<Genome>) {
        self.environment.step(self.time());
        self.decay_food();
        let inputs = self.sense_all();
        let mut scratch = EvalScratch::default();
        let outputs: Vec<Outputs> = self.cells.iter_mut().zip(inputs.iter()).map(|(cell, input)| {
            genomes[(cell.genome_id - 1) as usize].program.evaluate_cell(input, &mut cell.memory, &mut scratch)
        }).collect();

        // Decisions and register updates all read the same pre-step world.
        // Each cell pays for the register occupancy it had before evaluation.
        let drag = (-1.8 * DT).exp();
        for ((cell, output), input) in self.cells.iter_mut().zip(outputs.iter()).zip(inputs.iter()) {
            cell.photosynthesis_rate = 0.0;
            cell.uptake_rate = 0.0;
            cell.remodeling_rate = 0.0;
            if !cell.alive() { continue; }
            let propulsion = cell.capacity_factor(PROPULSION);
            cell.angle = wrap(cell.angle + output.turn * 3.2 * propulsion.sqrt() * DT, TAU);
            cell.vx = input.flow_x + (cell.vx - input.flow_x + cell.angle.cos() * output.thrust * propulsion * 100.0 * DT) * drag;
            cell.vy = input.flow_y + (cell.vy - input.flow_y + cell.angle.sin() * output.thrust * propulsion * 100.0 * DT) * drag;
            cell.x = wrap(cell.x + cell.vx * DT, WIDTH);
            cell.y = wrap(cell.y + cell.vy * DT, HEIGHT);
            cell.age += DT;
            cell.cooldown = (cell.cooldown - DT).max(0.0);
            cell.engulf_cooldown = (cell.engulf_cooldown - DT).max(0.0);
            let maintenance = (cell.basal_metabolism() * metabolic_factor(input.temperature)
                + output.thrust * THRUST_ENERGY_COST * cell.size * propulsion + input.memory_cost
                + cell.size * (PHOTO_UPKEEP * output.photosynthesize * cell.capacity_factor(PHOTOSYNTHESIS)
                    + BITE_UPKEEP * output.bite * cell.capacity_factor(DIGESTION)
                    + DEFENSE_UPKEEP * output.defend * cell.capacity_factor(ARMOR))) * DT;
            cell.energy = (cell.energy - maintenance).max(0.0);
            let stress = (0.15 - input.temperature).max(input.temperature - 0.85).max(0.0);
            cell.integrity = (cell.integrity - stress * 0.3 * (1.0 - 0.5 * cell.body[ARMOR]) * DT).max(0.0);
            if !cell.alive() { continue; }
            let requested_repair = (REPAIR_RATE * output.repair * DT).min(1.0 - cell.integrity);
            let repair_cost = requested_repair * REPAIR_COST;
            let repair = if repair_cost >= cell.energy {
                let affordable = cell.energy / REPAIR_COST;
                // Spending every remaining unit must not leave a rounding
                // residue that permits eating or photosynthesis to revive it.
                cell.energy = 0.0;
                affordable
            } else {
                cell.energy -= repair_cost;
                requested_repair
            };
            cell.integrity = (cell.integrity + repair).min(1.0);
        }
        self.absorb_nutrients(&outputs);
        for (cell, output) in self.cells.iter_mut().zip(&outputs) { cell.grow(output.grow); }
        let time = self.time();
        for food in &mut self.food {
            let (vx, vy) = self.environment.flow_at(food.x, food.y, time);
            food.x = wrap(food.x + vx * DT, WIDTH); food.y = wrap(food.y + vy * DT, HEIGHT);
        }
        self.resolve_collisions();
        self.eat();
        self.photosynthesize(&outputs);
        let consumed = self.resolve_predation(&inputs, &outputs);
        for (cell, output) in self.cells.iter_mut().zip(&outputs) { cell.remodel(output); }

        // Newborns only join after all original cells have completed this tick.
        let mut babies = Vec::new();
        let existing_count = self.cells.len();
        for (cell, output) in self.cells.iter_mut().zip(outputs.iter()) {
            if !output.divide || !cell.alive() || cell.cooldown > 0.0
                || cell.size < 2.0 * MIN_SIZE || cell.energy < DIVISION_MIN_ENERGY * cell.size
                || existing_count + babies.len() >= MAX_CELLS || self.next_id == u32::MAX || cell.generation == u32::MAX { continue; }
            let split_angle = cell.angle + PI * 0.5 + (self.rng.unit() - 0.5) * 0.6;
            let new_size = cell.size * 0.5;
            let daughter_radius = CELL_RADIUS * new_size.sqrt();
            let dx = split_angle.cos() * (daughter_radius + 0.3);
            let dy = split_angle.sin() * (daughter_radius + 0.3);
            let new_energy = (cell.energy - DIVISION_COST * cell.size) * 0.5;
            let mut child_genome_id = cell.genome_id;
            if self.mutation_rate > 0.0 {
                // Mutation never consumes the physics/food PRNG. Turning it off
                // preserves the original simulation's seeded random sequence.
                let seed = mutation_seed(self.seed, self.ticks, cell.id, self.next_id);
                let chance = (mix64(seed) >> 11) as f64 / ((1u64 << 53) as f64);
                if chance < self.mutation_rate {
                    let mutation = mutate_registered(genomes, cell.genome_id, seed, MutationKind::Any);
                    if mutation.skipped.is_some() {
                        self.mutations_skipped += 1;
                    } else {
                        child_genome_id = mutation.genome_id;
                        self.mutations += 1;
                    }
                }
            }
            let child = Cell {
                id: self.next_id, genome_id: child_genome_id,
                x: wrap(cell.x + dx, WIDTH), y: wrap(cell.y + dy, HEIGHT),
                vx: cell.vx + split_angle.cos() * 3.0,
                vy: cell.vy + split_angle.sin() * 3.0,
                angle: wrap(cell.angle + 0.5, TAU), energy: new_energy,
                size: new_size,
                age: 0.0, generation: cell.generation + 1, parent_id: cell.id,
                cooldown: DIVISION_COOLDOWN,
                memory: Registers::default(), integrity: cell.integrity, photosynthesis_rate: 0.0,
                nutrient_stored: cell.nutrient_stored * 0.5, body: cell.body,
                uptake_rate: 0.0, remodeling_rate: 0.0, engulf_cooldown: cell.engulf_cooldown,
            };
            cell.x = wrap(cell.x - dx, WIDTH);
            cell.y = wrap(cell.y - dy, HEIGHT);
            cell.vx -= split_angle.cos() * 3.0;
            cell.vy -= split_angle.sin() * 3.0;
            cell.energy = new_energy;
            cell.size = new_size;
            cell.nutrient_stored *= 0.5;
            cell.cooldown = DIVISION_COOLDOWN;
            self.next_id += 1;
            self.births += 1;
            babies.push(child);
        }
        let remains: Vec<(f64, f64, f64, f64)> = self.cells.iter().enumerate()
            .filter(|(i, cell)| !cell.alive() && !consumed[*i])
            .map(|(_, cell)| (cell.x, cell.y, REMAINS_RECOVERY * (cell.energy.max(0.0) + GROWTH_ENERGY_COST * cell.size),
                cell.size + cell.nutrient_stored))
            .collect();
        for (x, y, energy, nutrients) in remains { self.emit_remains(x, y, energy, nutrients); }
        let before = self.cells.len();
        self.cells.retain(Cell::alive);
        self.deaths += (before - self.cells.len()) as u64;
        self.cells.extend(babies);
        self.food_credit += self.food_rate * DT;
        let new_food = self.food_credit.floor() as usize;
        self.food_credit -= new_food as f64;
        self.add_food(-1.0, -1.0, new_food);
        self.ticks += 1;
    }

    fn decay_food(&mut self) {
        let retained = (-FOOD_DECAY * DT).exp();
        self.food.retain_mut(|food| {
            let nutrients = food.nutrients * retained;
            self.environment.recycle_nutrients(food.x, food.y, food.nutrients - nutrients);
            food.nutrients = nutrients;
            food.energy *= retained;
            if food.energy <= 1e-6 {
                self.environment.recycle_nutrients(food.x, food.y, food.nutrients);
                false
            } else { true }
        });
    }

    fn absorb_nutrients(&mut self, outputs: &[Outputs]) {
        let mut groups: Vec<Vec<(usize, f64)>> = vec![Vec::new(); PATCHES];
        for (i, (cell, output)) in self.cells.iter().zip(outputs).enumerate() {
            if !cell.alive() || output.absorb <= 0.0 { continue; }
            let request = (output.absorb * UPTAKE_RATE * cell.size * cell.capacity_factor(DIGESTION) * DT)
                .min((cell.nutrient_capacity() - cell.nutrient_stored).max(0.0)).min(cell.energy);
            if request > 0.0 { groups[Environment::patch(cell.x, cell.y)].push((i, request)); }
        }
        for (patch, group) in groups.iter_mut().enumerate() {
            if group.is_empty() { continue; }
            // Stable summation order makes a vector reordering irrelevant, as
            // well as ensuring proportional rather than first-come uptake.
            group.sort_by_key(|&(i, _)| self.cells[i].id);
            let mut demand: f64 = group.iter().map(|&(_, request)| request).sum();
            let mut remaining = self.environment.take_nutrients(patch, demand);
            let fully_funded = remaining >= demand;
            for &(i, request) in group.iter() {
                let gained = if fully_funded { request }
                    else if demand <= request { remaining.min(request) }
                    else { (remaining * request / demand).min(request).min(remaining) };
                let cell = &mut self.cells[i];
                cell.nutrient_stored += gained;
                cell.energy = if gained >= cell.energy { 0.0 } else { cell.energy - gained };
                cell.uptake_rate = gained / DT;
                remaining = (remaining - gained).max(0.0);
                demand = (demand - request).max(0.0);
            }
            // Return only a possible floating-point remainder from sharing.
            let cell = &self.cells[group[0].0];
            self.environment.recycle_nutrients(cell.x, cell.y, remaining);
        }
    }

    fn photosynthesize(&mut self, outputs: &[Outputs]) {
        let mut requests = vec![0.0; self.cells.len()];
        let mut totals = vec![0.0; PATCHES];
        for (i, (cell, output)) in self.cells.iter().zip(outputs).enumerate() {
            if !cell.alive() || output.photosynthesize <= 0.0 { continue; }
            let request = (PHOTO_RATE * output.photosynthesize * cell.size * cell.capacity_factor(PHOTOSYNTHESIS)
                * self.environment.light_at(cell.x, cell.y) * DT)
                .min((cell.energy_capacity() - cell.energy).max(0.0));
            requests[i] = request;
            totals[Environment::patch(cell.x, cell.y)] += request;
        }
        for (i, cell) in self.cells.iter_mut().enumerate() {
            if requests[i] <= 0.0 { continue; }
            let patch = Environment::patch(cell.x, cell.y);
            // Each 20×20 patch receives a finite supply of photons each tick.
            // Competing cells share that supply in proportion to their demand.
            let supply = PHOTO_PATCH_POWER * PATCH_AREA * self.environment.patch_light(patch) * DT;
            let gain = requests[i] * (supply / totals[patch]).min(1.0);
            cell.energy += gain;
            cell.photosynthesis_rate = gain / DT;
            self.photosynthesis_total += gain;
        }
    }

    fn resolve_predation(&mut self, inputs: &[Inputs], outputs: &[Outputs]) -> Vec<bool> {
        let mut consumed = vec![false; self.cells.len()];
        let indices: HashMap<u32, usize> = self.cells.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
        let target = |i: usize| -> Option<usize> {
            if !inputs[i].prey_visible { return None; }
            indices.get(&(inputs[i].prey_id as u32)).copied().filter(|&j| j != i)
        };
        let contact = |a: &Cell, b: &Cell| {
            let dx = delta(a.x - b.x, WIDTH); let dy = delta(a.y - b.y, HEIGHT);
            dx.hypot(dy) <= a.radius() + b.radius() + 0.75
        };

        // Large cells claim first. A stable ID breaks equal-mass ties, so a
        // victim can never be harvested twice or harvest after being consumed.
        let mut order: Vec<usize> = (0..self.cells.len()).collect();
        order.sort_by(|&a, &b| self.cells[b].size.total_cmp(&self.cells[a].size)
            .then_with(|| self.cells[a].id.cmp(&self.cells[b].id)));
        let mut overflow = Vec::new();
        for i in order {
            if !outputs[i].engulf || !self.cells[i].alive() || self.cells[i].engulf_cooldown > 0.0
                || self.cells[i].body[DIGESTION] < 0.05 { continue; }
            let Some(j) = target(i) else { continue; };
            let attacker = &self.cells[i]; let prey = &self.cells[j];
            if !prey.alive() || attacker.size < ENGULF_RATIO * prey.size || !contact(attacker, prey) { continue; }
            let cost = ENGULF_COST * prey.size;
            if attacker.energy <= cost { continue; }
            let recovered = ENGULF_RECOVERY * (prey.energy + GROWTH_ENERGY_COST * prey.size);
            let capacity = attacker.energy_capacity();
            let gained = recovered.min(capacity - (attacker.energy - cost));
            let excess = recovered - gained;
            let (x, y) = (prey.x, prey.y);
            let minerals = prey.size + prey.nutrient_stored;
            self.cells[i].energy = attacker.energy - cost + gained;
            let nutrient_overflow = self.cells[i].receive_nutrients(minerals);
            self.environment.recycle_nutrients(x, y, nutrient_overflow);
            self.cells[i].engulf_cooldown = 2.0 / self.cells[i].capacity_factor(DIGESTION);
            self.cells[j].energy = 0.0;
            self.cells[j].integrity = 0.0;
            // The consumed body is already elsewhere; it cannot remain on the
            // material ledger while waiting for this tick's removal phase.
            self.cells[j].size = 0.0;
            self.cells[j].nutrient_stored = 0.0;
            consumed[j] = true;
            self.predations += 1;
            if excess > 0.0 { overflow.push((x, y, excess)); }
        }

        // Every surviving attack reads the state after engulfing but before any
        // bites. Aggregate first, then apply: reciprocal bites can kill both.
        let mut damage = vec![0.0; self.cells.len()];
        for i in 0..self.cells.len() {
            if outputs[i].bite <= 0.0 || !self.cells[i].alive() || self.cells[i].body[DIGESTION] <= 0.0 { continue; }
            let Some(j) = target(i) else { continue; };
            if !self.cells[j].alive() || !contact(&self.cells[i], &self.cells[j]) { continue; }
            let protection = (0.4 * self.cells[j].body[ARMOR]
                + 0.65 * outputs[j].defend * self.cells[j].capacity_factor(ARMOR)).clamp(0.0, 0.95);
            damage[j] += 0.25 * outputs[i].bite * self.cells[i].capacity_factor(DIGESTION)
                * (self.cells[i].size / self.cells[j].size).min(4.0) * (1.0 - protection) * DT;
            self.attacks += 1;
        }
        for (cell, damage) in self.cells.iter_mut().zip(damage) {
            if damage <= 0.0 || !cell.alive() { continue; }
            cell.integrity = (cell.integrity - damage).max(0.0);
            if cell.integrity == 0.0 { self.predations += 1; }
        }
        for (x, y, energy) in overflow { self.emit_remains(x, y, energy, 0.0); }
        consumed
    }

    fn emit_remains(&mut self, x: f64, y: f64, mut energy: f64, mut nutrients: f64) {
        let mut n = 0usize;
        while energy.is_finite() && energy > 0.0 && self.food.len() < MAX_FOOD {
            let portion = energy.min(24.0);
            let minerals = if portion == energy { nutrients } else { nutrients * (portion / energy) };
            // Deterministic scatter does not perturb the food/physics PRNG.
            let angle = n as f64 * 2.399_963_229_728_653;
            let radius = (n as f64).sqrt() * 2.0;
            self.food.push(Food { x: wrap(x + angle.cos() * radius, WIDTH),
                y: wrap(y + angle.sin() * radius, HEIGHT), energy: portion, nutrients: minerals });
            energy -= portion;
            nutrients = (nutrients - minerals).max(0.0);
            n += 1;
        }
        // Energy beyond the bounded particle budget dissipates; matter does not.
        self.environment.recycle_nutrients(x, y, nutrients);
    }

    fn resolve_collisions(&mut self) {
        // Symmetric soft separation. Predation is a separate contact phase.
        for i in 0..self.cells.len() {
            let (left, right) = self.cells.split_at_mut(i + 1);
            let a = &mut left[i];
            if !a.alive() { continue; }
            for b in right {
                if !b.alive() { continue; }
                let dx = delta(b.x - a.x, WIDTH);
                let dy = delta(b.y - a.y, HEIGHT);
                let distance = dx.hypot(dy);
                let contact_distance = a.radius() + b.radius();
                if distance >= contact_distance { continue; }
                let (nx, ny) = if distance > 0.00001 {
                    (dx / distance, dy / distance)
                } else {
                    let angle = (a.id.wrapping_mul(13) ^ b.id) as f64;
                    (angle.cos(), angle.sin())
                };
                let overlap = contact_distance - distance;
                let total_mass = a.size + b.size;
                let displacement_a = overlap * b.size / total_mass;
                let displacement_b = overlap * a.size / total_mass;
                a.x = wrap(a.x - nx * displacement_a, WIDTH);
                a.y = wrap(a.y - ny * displacement_a, HEIGHT);
                b.x = wrap(b.x + nx * displacement_b, WIDTH);
                b.y = wrap(b.y + ny * displacement_b, HEIGHT);
                let approaching = (b.vx - a.vx) * nx + (b.vy - a.vy) * ny;
                if approaching < 0.0 {
                    let inverse_a = 1.0 / a.size;
                    let inverse_b = 1.0 / b.size;
                    let impulse = -approaching / (inverse_a + inverse_b);
                    a.vx -= nx * impulse * inverse_a;
                    a.vy -= ny * impulse * inverse_a;
                    b.vx += nx * impulse * inverse_b;
                    b.vy += ny * impulse * inverse_b;
                }
            }
        }
    }

    fn eat(&mut self) {
        let mut grid = Grid::new();
        for (i, c) in self.cells.iter().enumerate() { grid.insert(c.x, c.y, i); }
        let mut budgets: Vec<f64> = self.cells.iter().map(|cell|
            DIGESTION_RATE * cell.size * cell.capacity_factor(DIGESTION) * DT).collect();
        self.food.retain_mut(|food| {
            let mut winner: Option<usize> = None;
            let mut best_distance = f64::INFINITY;
            for i in grid.nearby(food.x, food.y) {
                let cell = &self.cells[i];
                if !cell.alive() || budgets[i] <= 0.0
                    || (cell.energy >= cell.energy_capacity()
                        && (cell.nutrient_stored >= cell.nutrient_capacity() || food.nutrients <= 0.0)) { continue; }
                let dx = delta(cell.x - food.x, WIDTH);
                let dy = delta(cell.y - food.y, HEIGHT);
                let d2 = dx * dx + dy * dy;
                if d2 > (cell.radius() + 2.5).powi(2) { continue; }
                if d2 < best_distance || (d2 == best_distance && winner.map_or(true, |w| cell.id < self.cells[w].id)) {
                    winner = Some(i);
                    best_distance = d2;
                }
            }
            if let Some(index) = winner {
                let cell = &mut self.cells[index];
                let energy_room = (cell.energy_capacity() - cell.energy).max(0.0);
                let mineral_room = (cell.nutrient_capacity() - cell.nutrient_stored).max(0.0);
                let energy_fraction = if food.energy > 0.0 { energy_room / food.energy } else { 0.0 };
                let mineral_fraction = if food.nutrients > 0.0 { mineral_room / food.nutrients } else { 0.0 };
                let capacity_fraction = if food.energy > 0.0 { budgets[index] / food.energy } else { 1.0 };
                let fraction = energy_fraction.max(mineral_fraction).min(capacity_fraction).clamp(0.0, 1.0);
                let energy = food.energy * fraction;
                let nutrients = food.nutrients * fraction;
                cell.energy += energy.min(energy_room);
                let overflow = cell.receive_nutrients(nutrients);
                self.environment.recycle_nutrients(food.x, food.y, overflow);
                budgets[index] = (budgets[index] - energy).max(0.0);
                food.energy = (food.energy - energy).max(0.0);
                food.nutrients = (food.nutrients - nutrients).max(0.0);
                if food.energy <= 1e-6 {
                    self.environment.recycle_nutrients(food.x, food.y, food.nutrients);
                    self.eaten += 1;
                    false
                } else { true }
            } else { true }
        });
    }

    pub fn snapshot(&self, target: &mut Vec<f32>) {
        let inputs = self.sense_all();
        target.clear();
        target.extend_from_slice(&[
            self.ticks as f32, self.time() as f32, self.cells.len() as f32,
            self.food.len() as f32, self.births as f32, self.deaths as f32,
            self.eaten as f32, WIDTH as f32, HEIGHT as f32, MAX_CELLS as f32,
            MAX_FOOD as f32, self.food_rate as f32,
        ]);
        for (cell, input) in self.cells.iter().zip(inputs.iter()) {
            target.extend_from_slice(&[
                cell.id as f32, cell.genome_id as f32,
                (cell.x as f32).rem_euclid(WIDTH as f32), (cell.y as f32).rem_euclid(HEIGHT as f32),
                cell.vx as f32, cell.vy as f32, cell.angle as f32, cell.energy as f32,
                cell.age as f32, cell.generation as f32, cell.parent_id as f32,
                cell.radius() as f32, input.food_angle as f32, input.food_distance as f32,
                if input.food_visible { 1.0 } else { 0.0 }, input.neighbor_angle as f32,
                input.neighbor_distance as f32, if input.neighbor_visible { 1.0 } else { 0.0 },
                cell.size as f32, cell.energy_capacity() as f32, (cell.basal_metabolism() * metabolic_factor(input.temperature)) as f32,
                input.memory_used as f32, input.memory_cost as f32,
                input.light_level as f32, input.light_angle as f32, input.temperature as f32, input.temperature_angle as f32,
                input.flow_x as f32, input.flow_y as f32, input.prey_id as f32, input.prey_angle as f32,
                input.prey_distance as f32, input.prey_size as f32, if input.prey_visible { 1.0 } else { 0.0 },
                input.threat_angle as f32, input.threat_distance as f32, input.threat_size as f32,
                if input.threat_visible { 1.0 } else { 0.0 }, cell.integrity as f32, cell.photosynthesis_rate as f32,
                cell.nutrient_stored as f32, cell.nutrient_capacity() as f32,
                input.nutrient_level as f32, input.nutrient_angle as f32, cell.uptake_rate as f32,
                cell.body[PROPULSION] as f32, cell.body[PHOTOSYNTHESIS] as f32, cell.body[DIGESTION] as f32,
                cell.body[ARMOR] as f32, cell.body[SENSING] as f32, cell.sense_range() as f32,
                cell.remodeling_rate as f32, cell.body_upkeep() as f32,
            ]);
        }
        for food in &self.food {
            target.extend_from_slice(&[
                (food.x as f32).rem_euclid(WIDTH as f32),
                (food.y as f32).rem_euclid(HEIGHT as f32), food.energy as f32, food.nutrients as f32,
            ]);
        }
    }
}

fn metabolic_factor(temperature: f64) -> f64 { 0.5 + temperature.clamp(0.0, 1.0) }

fn wrap(value: f64, size: f64) -> f64 { value.rem_euclid(size) }
fn delta(value: f64, size: f64) -> f64 { (value + size * 0.5).rem_euclid(size) - size * 0.5 }
fn relative_angle(angle: f64) -> f64 { (angle + PI).rem_euclid(TAU) - PI }

fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn mutation_seed(world_seed: u32, tick: u64, parent_cell: u32, daughter_cell: u32) -> u64 {
    mix64((world_seed as u64) ^ tick.wrapping_mul(0xD6E8_FEB8_6659_FD93)
        ^ ((parent_cell as u64) << 32) ^ daughter_cell as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::DEFAULT_PROGRAM;

    fn genomes(source: &str) -> Vec<Genome> { vec![Genome::new(source.into(), Program::compile(source).unwrap(), 0, 0)] }
    fn idle() -> Vec<Genome> { genomes("thrust = 0; turn = 0; divide = false; absorb = 0;") }
    fn one_cell() -> World {
        let mut world = World::empty(17);
        // Isolate legacy action costs from dissolved uptake; nutrient tests add their own water.
        world.environment.set_initial_nutrients(0.0);
        world.environment.reset_dynamic();
        world.food_rate = 0.0;
        world.mutation_rate = 0.0;
        world.seed_cells(1, 1, -1.0, -1.0);
        world.cells[0].vx = 0.0;
        world.cells[0].vy = 0.0;
        world
    }

    #[test]
    fn deterministic_seed_and_program() {
        let mut g = genomes(DEFAULT_PROGRAM);
        let mut a = World::empty(0); let mut b = World::empty(0);
        a.reset(42, 1); b.reset(42, 1);
        for _ in 0..300 { a.step(&mut g); b.step(&mut g); }
        let mut sa = Vec::new(); let mut sb = Vec::new();
        a.snapshot(&mut sa); b.snapshot(&mut sb);
        assert_eq!(sa, sb);
        assert!(a.eaten > 0);
    }

    #[test]
    fn division_conserves_energy_minus_explicit_cost_and_metabolism() {
        let mut g = genomes("thrust = 0; turn = 0; divide = true;");
        let mut world = one_cell();
        world.cells[0].energy = 140.0;
        world.cells[0].cooldown = 0.0;
        world.step(&mut g);
        assert_eq!(world.cells.len(), 2);
        let total: f64 = world.cells.iter().map(|c| c.energy).sum();
        assert!((total - (140.0 - 0.65 * DT - DIVISION_COST)).abs() < 1e-8);
        assert_eq!(world.cells[1].genome_id, world.cells[0].genome_id);
        assert_eq!(world.cells[1].parent_id, world.cells[0].id);
        assert_eq!(world.cells[1].generation, 1);
        assert_eq!(world.cells[1].age, 0.0);
        assert_eq!(world.births, 1);
        world.step(&mut g);
        assert_eq!(world.cells.len(), 2, "cooldown blocks repeated division");
    }

    #[test]
    fn starvation_removes_cell_and_counts_death() {
        let mut world = one_cell();
        world.cells[0].energy = 0.001;
        world.step(&mut idle());
        assert!(world.cells.is_empty());
        assert_eq!(world.deaths, 1);
    }

    #[test]
    fn food_and_neighbor_sensing_cross_both_wrap_seams() {
        let mut world = one_cell();
        world.cells[0].x = 2.0; world.cells[0].y = 2.0; world.cells[0].angle = PI;
        world.food.push(Food { x: WIDTH - 3.0, y: 2.0, energy: 20.0, nutrients: FOOD_NUTRIENTS });
        world.seed_cells(1, 1, -1.0, -1.0);
        world.cells[1].x = 2.0; world.cells[1].y = HEIGHT - 6.0;
        let input = world.sense_all();
        assert!(input[0].food_visible && input[0].neighbor_visible);
        assert!((input[0].food_distance - 5.0).abs() < 1e-8);
        assert!(input[0].food_angle.abs() < 1e-8);
        assert!((input[0].neighbor_distance - 8.0).abs() < 1e-8);
    }

    #[test]
    fn grid_covers_full_radius_near_short_final_world_segment() {
        let mut world = one_cell();
        world.cells[0].x = 200.0; world.cells[0].y = 2.0;
        world.food.push(Food { x: 200.0, y: 650.0, energy: 20.0, nutrients: FOOD_NUTRIENTS });
        assert!(world.sense_all()[0].food_visible);
        assert_eq!(world.sense_all()[0].food_distance, 112.0);
    }

    #[test]
    fn food_is_consumed_once_when_two_cells_reach_it() {
        let mut world = one_cell();
        world.seed_cells(1, 1, -1.0, -1.0);
        for c in &mut world.cells { c.x = 500.0; c.y = 400.0; c.energy = 50.0; c.vx = 0.0; c.vy = 0.0; }
        world.food.push(Food { x: 500.0, y: 400.0, energy: 2.0, nutrients: FOOD_NUTRIENTS });
        world.step(&mut idle());
        assert!(world.food.is_empty());
        assert_eq!(world.eaten, 1);
        let total: f64 = world.cells.iter().map(|c| c.energy).sum();
        assert!((total - (100.0 + 2.0 * (-FOOD_DECAY * DT).exp() - 2.0 * 0.65 * DT)).abs() < 1e-8);
    }

    #[test]
    fn caps_and_finite_world_invariants() {
        let mut world = one_cell();
        assert_eq!(world.seed_cells(1, usize::MAX, -1.0, -1.0), MAX_CELLS - 1);
        assert_eq!(world.add_food(-1.0, -1.0, usize::MAX), MAX_FOOD);
        world.set_food_rate(f64::NAN);
        assert_eq!(world.food_rate, 0.0);
        for _ in 0..5 { world.step(&mut genomes(DEFAULT_PROGRAM)); }
        assert!(world.cells.len() <= MAX_CELLS && world.food.len() <= MAX_FOOD);
        for c in &world.cells {
            assert!(c.x >= 0.0 && c.x < WIDTH && c.y >= 0.0 && c.y < HEIGHT);
            assert!(c.size.is_finite() && c.size >= MIN_SIZE && c.size <= MAX_SIZE);
            assert!(c.energy.is_finite() && c.energy > 0.0 && c.energy <= c.energy_capacity());
        }
    }

    #[test]
    fn sensor_values_are_frozen_before_any_cell_moves() {
        let mut g = genomes("thrust = if neighbor.distance > 20 then 1 else 0; turn = 0; divide = false;");
        let mut world = one_cell();
        world.seed_cells(1, 1, -1.0, -1.0);
        world.cells[0].x = 100.0; world.cells[0].y = 100.0;
        world.cells[1].x = 120.01; world.cells[1].y = 100.0;
        for c in &mut world.cells { c.vx = 0.0; c.vy = 0.0; c.angle = 0.0; }
        world.step(&mut g);
        assert!(world.cells[0].vx > 0.0 && world.cells[1].vx > 0.0);
        assert!((world.cells[0].vx - world.cells[1].vx).abs() < 1e-8);
    }

    #[test]
    fn daughter_mutation_changes_only_her_genome_at_birth() {
        let source = "thrust = 0; turn = 0; divide = true;";
        let mut original = genomes(source);
        let mut evolving = original.clone();
        let mut off = one_cell();
        let mut on = one_cell();
        for world in [&mut off, &mut on] {
            world.cells[0].energy = 140.0;
            world.cells[0].cooldown = 0.0;
        }
        on.set_mutation_rate(1.0);
        off.step(&mut original);
        on.step(&mut evolving);

        assert_eq!(off.births, 1);
        assert_eq!(on.births, 1);
        assert_eq!(off.mutations, 0);
        assert_eq!(off.mutations_skipped, 0);
        assert_eq!(original.len(), 1);
        assert_eq!(off.cells[1].genome_id, 1);
        assert_eq!(on.mutations, 1);
        assert_eq!(on.mutations_skipped, 0);
        assert_eq!(on.cells[0].genome_id, 1, "parent retains the original genome");
        assert_eq!(evolving[0].source, source);
        let daughter_id = on.cells[1].genome_id;
        assert_ne!(daughter_id, 1);
        let daughter_genome = &evolving[(daughter_id - 1) as usize];
        assert_eq!(daughter_genome.parent_id, 1);
        assert!((1..=3).contains(&daughter_genome.mutation_kind));
        Program::compile(&daughter_genome.source).expect("mutated source must remain valid");

        let mut sa = Vec::new(); let mut sb = Vec::new();
        off.snapshot(&mut sa); on.snapshot(&mut sb);
        for index in 0..off.cells.len() { sb[12 + index * CELL_STRIDE + 1] = sa[12 + index * CELL_STRIDE + 1]; }
        assert_eq!(sa, sb, "mutation does not change birth energy, physics, or timing");
        assert_eq!(off.rng.0, on.rng.0, "mutation uses no physics/food RNG draws");
    }

    #[test]
    fn replay_after_reset_reuses_mutation_lineage_and_ids() {
        let mut registry = genomes(DEFAULT_PROGRAM);
        let mut world = World::empty(1);
        world.set_mutation_rate(1.0);
        world.reset(42, 1);
        for _ in 0..900 { world.step(&mut registry); }
        assert!(world.mutations > 0);
        let first_count = registry.len();
        let first_mutations = world.mutations;
        let first_skipped = world.mutations_skipped;
        let mut before = Vec::new();
        world.snapshot(&mut before);

        world.reset(42, 1);
        assert_eq!(world.mutation_rate, 1.0);
        assert_eq!(world.mutations, 0);
        for _ in 0..900 { world.step(&mut registry); }
        let mut after = Vec::new();
        world.snapshot(&mut after);
        assert_eq!(before, after);
        assert_eq!(registry.len(), first_count, "replay must not create duplicate genomes");
        assert_eq!(world.mutations, first_mutations);
        assert_eq!(world.mutations_skipped, first_skipped);
    }

    #[test]
    fn registry_capacity_falls_back_to_inheritance_and_counts_skipped_births() {
        let template = genomes("thrust = 0; turn = 0; divide = true;").remove(0);
        let mut registry = vec![template; MAX_GENOMES];
        let mut world = one_cell();
        world.set_mutation_rate(1.0);
        world.cells[0].energy = 140.0;
        world.cells[0].cooldown = 0.0;
        world.step(&mut registry);
        assert_eq!(world.cells.len(), 2);
        assert_eq!(world.cells[1].genome_id, 1);
        assert_eq!(world.mutations, 0);
        assert_eq!(world.mutations_skipped, 1);
        assert_eq!(registry.len(), MAX_GENOMES);
        let total: f64 = world.cells.iter().map(|cell| cell.energy).sum();
        assert!((total - (140.0 - 0.65 * DT - DIVISION_COST)).abs() < 1e-8);
    }

    #[test]
    fn existing_mutation_can_replay_at_registry_capacity() {
        let mut registry = genomes(DEFAULT_PROGRAM);
        let first = mutate_registered(&mut registry, 1, 57, MutationKind::Constant);
        assert!(first.skipped.is_none());
        let fill = registry[0].clone();
        registry.resize(MAX_GENOMES, fill);
        let replay = mutate_registered(&mut registry, 1, 57, MutationKind::Constant);
        assert!(replay.skipped.is_none());
        assert_eq!(replay.genome_id, first.genome_id);
        assert_eq!(registry.len(), MAX_GENOMES);
    }

    #[test]
    fn growth_converts_energy_at_the_requested_rate_and_zero_is_inert() {
        for signal in [0.0, 0.4, 1.0] {
            let mut world = one_cell();
            world.cells[0].energy = 100.0;
            let source = format!("thrust = 0; turn = 0; divide = false; grow = {signal};");
            world.step(&mut genomes(&source));
            let cell = &world.cells[0];
            let growth = signal * MAX_GROWTH_RATE * DT;
            assert!((cell.size - (1.0 + growth)).abs() < 1e-12);
            assert!((cell.energy - (100.0 - BASAL_METABOLISM * DT - growth * GROWTH_ENERGY_COST)).abs() < 1e-10);
            assert!((cell.radius() - CELL_RADIUS * cell.size.sqrt()).abs() < 1e-12);
        }
    }

    #[test]
    fn growth_stops_at_max_size_and_pays_only_for_the_actual_increment() {
        let mut world = one_cell();
        world.cells[0].size = MAX_SIZE - 0.001;
        world.cells[0].energy = 700.0;
        let old_size = world.cells[0].size;
        let mut registry = genomes("thrust = 0; turn = 0; divide = false; grow = 1;");
        world.step(&mut registry);
        assert_eq!(world.cells[0].size, MAX_SIZE);
        let expected = 700.0 - BASAL_METABOLISM * old_size * DT - (MAX_SIZE - old_size) * GROWTH_ENERGY_COST;
        assert!((world.cells[0].energy - expected).abs() < 1e-10);
        world.step(&mut registry);
        assert_eq!(world.cells[0].size, MAX_SIZE);
        assert!((world.cells[0].energy - (expected - BASAL_METABOLISM * MAX_SIZE * DT)).abs() < 1e-10);
    }

    #[test]
    fn unaffordable_growth_spends_only_available_energy_and_dead_cells_cannot_eat_or_divide() {
        let mut probe = one_cell().cells.remove(0);
        probe.energy = 0.05;
        let previous_size = probe.size;
        probe.grow(1.0);
        assert_eq!(probe.energy, 0.0);
        assert!((probe.size - previous_size - 0.05 / GROWTH_ENERGY_COST).abs() < 1e-12);
        let dead_size = probe.size;
        probe.grow(1.0);
        assert_eq!(probe.size, dead_size, "a dead cell cannot grow again");

        for energy in [0.001, BASAL_METABOLISM * DT + 0.05] {
            let mut world = one_cell();
            world.cells[0].energy = energy;
            world.cells[0].cooldown = 0.0;
            world.food.push(Food { x: world.cells[0].x, y: world.cells[0].y, energy: 24.0, nutrients: FOOD_NUTRIENTS });
            world.step(&mut genomes("thrust = 0; turn = 0; grow = 1; divide = true;"));
            assert!(world.cells.is_empty());
            assert!(world.food.iter().any(|food| (food.energy - 24.0 * (-FOOD_DECAY * DT).exp()).abs() < 1e-12), "original food cannot revive a dead cell; remains may be added afterwards");
            assert_eq!(world.births, 0);
            assert_eq!(world.deaths, 1);
            assert_eq!(world.eaten, 0);
        }
    }

    #[test]
    fn size_scales_capacity_basal_and_locomotion_energy_costs() {
        for size in [MIN_SIZE, 1.0, MAX_SIZE] {
            let mut world = one_cell();
            world.cells[0].size = size;
            world.cells[0].energy = MAX_ENERGY * size - 1.0;
            world.food.push(Food { x: world.cells[0].x, y: world.cells[0].y, energy: 1.0, nutrients: FOOD_NUTRIENTS });
            world.eat();
            assert_eq!(world.cells[0].energy, MAX_ENERGY * size);
            assert!((world.cells[0].basal_metabolism() - BASAL_METABOLISM * size).abs() < 1e-12);
            assert_eq!(world.cells[0].radius(), CELL_RADIUS * size.sqrt());
            world.step(&mut genomes("thrust = .5; turn = 0; grow = 0; divide = false;"));
            let cost = (BASAL_METABOLISM + 0.5 * THRUST_ENERGY_COST) * size * DT;
            assert!((world.cells[0].energy - (MAX_ENERGY * size - cost)).abs() < 1e-10);
        }
    }

    #[test]
    fn division_splits_biomass_and_energy_and_uses_daughter_radii() {
        let mut world = one_cell();
        world.cells[0].size = 2.4;
        world.cells[0].energy = 400.0;
        world.cells[0].cooldown = 0.0;
        world.cells[0].x = 1.0;
        world.cells[0].y = 1.0;
        world.step(&mut genomes("thrust = 0; turn = 0; grow = 0; divide = true;"));
        assert_eq!(world.cells.len(), 2);
        let total_size: f64 = world.cells.iter().map(|cell| cell.size).sum();
        let total_energy: f64 = world.cells.iter().map(|cell| cell.energy).sum();
        assert_eq!(total_size, 2.4);
        assert!((total_energy - (400.0 - BASAL_METABOLISM * 2.4 * DT - DIVISION_COST * 2.4)).abs() < 1e-10);
        assert_eq!(world.cells[0].size, 1.2);
        assert_eq!(world.cells[1].size, 1.2);
        let dx = delta(world.cells[0].x - world.cells[1].x, WIDTH);
        let dy = delta(world.cells[0].y - world.cells[1].y, HEIGHT);
        let separation = world.cells[0].radius() + world.cells[1].radius() + 0.6;
        assert!((dx.hypot(dy) - separation).abs() < 1e-10);
        assert_eq!(world.cells[1].age, 0.0);
    }

    #[test]
    fn division_requires_two_minimum_sized_daughters() {
        let mut registry = genomes("thrust = 0; turn = 0; grow = 0; divide = true;");
        let mut world = one_cell();
        world.cells[0].size = 0.999;
        world.cells[0].energy = 180.0;
        world.cells[0].cooldown = 0.0;
        world.step(&mut registry);
        assert_eq!(world.cells.len(), 1);
        world.cells[0].size = 1.0;
        world.step(&mut registry);
        assert_eq!(world.cells.len(), 2);
        assert!(world.cells.iter().all(|cell| cell.size == MIN_SIZE));
        for cell in &mut world.cells { cell.energy = cell.energy_capacity(); cell.cooldown = 0.0; }
        world.step(&mut registry);
        assert_eq!(world.cells.len(), 2, "small daughters must regrow before another division");
        assert_eq!(world.births, 1);
    }

    #[test]
    fn division_uses_post_growth_size_for_eligibility_and_cost() {
        let mut registry = genomes("thrust = 0; turn = 0; grow = 1; divide = true;");
        let mut world = one_cell();
        world.cells[0].size = 2.0;
        world.cells[0].energy = 180.5;
        world.cells[0].cooldown = 0.0;
        world.step(&mut registry);
        assert_eq!(world.cells.len(), 1, "energy must meet the larger post-growth threshold");
        let previous_size = world.cells[0].size;
        world.cells[0].energy = 300.0;
        world.step(&mut registry);
        assert_eq!(world.cells.len(), 2);
        let grown_size = previous_size + MAX_GROWTH_RATE * DT;
        let total_size: f64 = world.cells.iter().map(|cell| cell.size).sum();
        let total_energy: f64 = world.cells.iter().map(|cell| cell.energy).sum();
        assert!((total_size - grown_size).abs() < 1e-12);
        let expected = 300.0 - BASAL_METABOLISM * previous_size * DT
            - MAX_GROWTH_RATE * DT * GROWTH_ENERGY_COST - DIVISION_COST * grown_size;
        assert!((total_energy - expected).abs() < 1e-10);
    }

    #[test]
    fn unequal_radius_collisions_preserve_mass_weighted_center_and_momentum_across_seams() {
        let mut world = one_cell();
        world.seed_cells(1, 1, -1.0, -1.0);
        world.cells[0].size = MAX_SIZE;
        world.cells[0].x = 3.0; world.cells[0].y = 3.0;
        world.cells[0].vx = -2.0; world.cells[0].vy = -1.0;
        world.cells[1].size = MIN_SIZE;
        world.cells[1].x = WIDTH - 4.0; world.cells[1].y = HEIGHT - 4.0;
        world.cells[1].vx = 3.0; world.cells[1].vy = 2.0;
        let before = world.cells.clone();
        let momentum_x: f64 = before.iter().map(|cell| cell.size * cell.vx).sum();
        let momentum_y: f64 = before.iter().map(|cell| cell.size * cell.vy).sum();
        world.resolve_collisions();
        let a = &world.cells[0]; let b = &world.cells[1];
        let distance = delta(b.x - a.x, WIDTH).hypot(delta(b.y - a.y, HEIGHT));
        assert!((distance - a.radius() - b.radius()).abs() < 1e-10);
        let center_shift_x: f64 = world.cells.iter().zip(&before).map(|(now, old)| now.size * delta(now.x - old.x, WIDTH)).sum();
        let center_shift_y: f64 = world.cells.iter().zip(&before).map(|(now, old)| now.size * delta(now.y - old.y, HEIGHT)).sum();
        assert!(center_shift_x.abs() < 1e-10 && center_shift_y.abs() < 1e-10);
        assert!((a.size * a.vx + b.size * b.vx - momentum_x).abs() < 1e-10);
        assert!((a.size * a.vy + b.size * b.vy - momentum_y).abs() < 1e-10);
    }

    #[test]
    fn food_capture_uses_each_cells_radius_and_wraps_at_the_boundary() {
        for (size, expected_meals) in [(MIN_SIZE, 0), (MAX_SIZE, 1)] {
            let mut world = one_cell();
            world.cells[0].size = size;
            world.cells[0].energy = 20.0;
            world.cells[0].x = 2.0; world.cells[0].y = 100.0;
            world.food.push(Food { x: WIDTH - 10.0, y: 100.0, energy: 1.0, nutrients: FOOD_NUTRIENTS });
            world.eat();
            assert_eq!(world.eaten, expected_meals);
            assert_eq!(world.food.len(), 1 - expected_meals as usize);
        }
    }

    #[test]
    fn nearest_eligible_center_wins_food_with_stable_id_ties() {
        let mut world = one_cell();
        world.seed_cells(1, 1, -1.0, -1.0);
        world.cells[0].size = MAX_SIZE;
        world.cells[1].size = MIN_SIZE;
        for cell in &mut world.cells { cell.y = 100.0; cell.energy = 50.0; }
        world.cells[0].x = 100.0; world.cells[1].x = 118.0;
        world.food.push(Food { x: 114.0, y: 100.0, energy: 1.0, nutrients: FOOD_NUTRIENTS });
        world.eat();
        assert_eq!(world.cells[0].energy, 50.0);
        assert_eq!(world.cells[1].energy, 51.0, "nearest center wins despite the other cell's larger radius");
        world.cells[0].x = 110.0;
        world.food.push(Food { x: 114.0, y: 100.0, energy: 1.0, nutrients: FOOD_NUTRIENTS });
        world.eat();
        assert_eq!(world.cells[0].energy, 51.0, "lower stable ID breaks an equal-center-distance tie");
        world.cells[0].x = 100.0;
        world.cells[1].energy = world.cells[1].energy_capacity();
        world.cells[1].nutrient_stored = world.cells[1].nutrient_capacity();
        world.food.push(Food { x: 114.0, y: 100.0, energy: 1.0, nutrients: FOOD_NUTRIENTS });
        world.eat();
        assert_eq!(world.cells[0].energy, 52.0, "a full cell is ineligible even when it is closer");
        assert_eq!(world.eaten, 3);
    }

    #[test]
    fn size_sensor_is_frozen_before_growth_and_reacts_on_the_next_tick() {
        let mut registry = genomes("thrust = if size > 1 then 1 else 0; turn = 0; grow = 1; divide = false;");
        let mut world = one_cell();
        assert_eq!(world.sense_all()[0].size, 1.0);
        world.step(&mut registry);
        assert!(world.cells[0].size > 1.0);
        assert_eq!(world.cells[0].vx, 0.0);
        assert_eq!(world.cells[0].vy, 0.0);
        world.step(&mut registry);
        assert!(world.cells[0].vx.hypot(world.cells[0].vy) > 0.0);
    }

    #[test]
    fn memory_upkeep_uses_previous_occupancy_and_clear_changes_the_next_tick() {
        let mut registry = genomes("ri001 = ri001 + 1; clear ri001 when ri001 == 2; thrust = 0; turn = 0; divide = false;");
        let mut world = one_cell();
        world.cells[0].energy = 100.0;
        for (paid, occupied) in [(0.0, 1), (1.0, 1), (1.0, 0), (0.0, 1)] {
            let before = world.cells[0].energy;
            world.step(&mut registry);
            assert!((before - world.cells[0].energy - (BASAL_METABOLISM + paid * MEMORY_REGISTER_COST) * DT).abs() < 1e-10);
            assert_eq!(world.cells[0].memory.populated_count(), occupied);
            assert_eq!(world.sense_all()[0].memory_cost, occupied as f64 * MEMORY_REGISTER_COST);
        }
    }

    #[test]
    fn division_keeps_parent_registers_and_starts_daughter_empty_without_free_repair() {
        let mut registry = genomes("ri001 = ri001 + 1; thrust = 0; turn = 0; divide = true;");
        let mut world = one_cell();
        world.cells[0].energy = 140.0;
        world.cells[0].cooldown = 0.0;
        world.cells[0].integrity = 0.4;
        world.step(&mut registry);
        assert_eq!(world.cells.len(), 2);
        assert_eq!(world.cells[0].memory.populated_count(), 1);
        assert_eq!(world.cells[1].memory.populated_count(), 0);
        assert!(world.cells.iter().all(|cell| cell.integrity == 0.4));
        world.step(&mut registry);
        assert!(world.cells.iter().all(|cell| cell.memory.populated_count() == 1));
        assert_ne!(world.cells[0].memory.to_json(), world.cells[1].memory.to_json());
    }

    #[test]
    fn light_is_shared_within_a_patch_and_full_or_dead_cells_take_no_photons() {
        let mut world = one_cell();
        world.seed_cells(1, 5, -1.0, -1.0);
        world.environment.configure(0.5, 1.0, 0.0);
        for cell in &mut world.cells { cell.x = 110.0; cell.y = 110.0; cell.energy = 100.0; }
        world.cells[4].energy = MAX_ENERGY;
        world.cells[5].energy = 0.0;
        let output = Program::compile("thrust = 0; turn = 0; divide = false; photosynthesize = 1;").unwrap().evaluate(&Inputs::default());
        world.photosynthesize(&vec![output; world.cells.len()]);
        let available = PHOTO_PATCH_POWER * PATCH_AREA * DT;
        let harvested: f64 = world.cells.iter().take(4).map(|cell| cell.energy - 100.0).sum();
        assert!((harvested - available).abs() < 1e-10);
        assert!((world.photosynthesis_total - available).abs() < 1e-10);
        assert!(world.cells.iter().take(4).all(|cell| (cell.photosynthesis_rate - available / 4.0 / DT).abs() < 1e-10));
        assert_eq!(world.cells[4].energy, MAX_ENERGY);
        assert_eq!(world.cells[5].energy, 0.0);
        assert_eq!(world.cells[4].photosynthesis_rate, 0.0);

        world.cells.truncate(1);
        world.cells[0].energy = MAX_ENERGY - 0.01;
        world.photosynthesize(&[output]);
        assert_eq!(world.cells[0].energy, MAX_ENERGY);
        assert!((world.cells[0].photosynthesis_rate - 0.01 / DT).abs() < 1e-10);
    }

    #[test]
    fn photosynthesis_pays_upkeep_in_darkness_and_cannot_revive_a_starved_cell() {
        let mut registry = genomes("thrust = 0; turn = 0; divide = false; photosynthesize = 1;");
        let mut dark = one_cell();
        dark.cells[0].energy = 100.0;
        dark.step(&mut registry);
        assert!((dark.cells[0].energy - (100.0 - (BASAL_METABOLISM + PHOTO_UPKEEP) * DT)).abs() < 1e-10);
        assert_eq!(dark.photosynthesis_total, 0.0);
        let mut starved = one_cell();
        starved.environment.configure(0.5, 1.0, 0.0);
        starved.cells[0].energy = 0.001;
        starved.step(&mut registry);
        assert!(starved.cells.is_empty());
        assert_eq!(starved.photosynthesis_total, 0.0);
    }

    #[test]
    fn currents_advect_food_and_drag_idle_cells_toward_water_velocity() {
        let mut world = one_cell();
        world.environment.configure(0.5, 0.0, 30.0);
        world.cells[0].x = 10.0; world.cells[0].y = HEIGHT / 4.0;
        world.food.push(Food { x: 400.0, y: HEIGHT / 4.0, energy: 20.0, nutrients: FOOD_NUTRIENTS });
        let (water_x, water_y) = world.environment.flow_at(world.cells[0].x, world.cells[0].y, 0.0);
        let (food_x, food_y) = world.environment.flow_at(400.0, HEIGHT / 4.0, 0.0);
        world.step(&mut idle());
        let drag = (-1.8 * DT).exp();
        assert!((world.cells[0].vx - water_x * (1.0 - drag)).abs() < 1e-12);
        assert!((world.cells[0].vy - water_y * (1.0 - drag)).abs() < 1e-12);
        assert!((world.food[0].x - 400.0 - food_x * DT).abs() < 1e-10);
        assert!((world.food[0].y - HEIGHT / 4.0 - food_y * DT).abs() < 1e-10);
    }

    #[test]
    fn temperature_changes_metabolism_and_repair_pays_for_actual_damage_repaired() {
        let mut registry = genomes("thrust = 0; turn = 0; divide = false; repair = 1;");
        for temperature in [0.0, 0.5, 1.0] {
            let mut world = one_cell();
            world.environment.configure(temperature, 0.0, 0.0);
            world.cells[0].energy = 100.0;
            world.cells[0].integrity = 0.8;
            let stress = (0.15 - temperature).max(temperature - 0.85).max(0.0);
            world.step(&mut registry);
            assert!((world.cells[0].integrity - (0.8 - stress * 0.3 * 0.9 * DT + REPAIR_RATE * DT)).abs() < 1e-12);
            let expected = 100.0 - BASAL_METABOLISM * (0.5 + temperature) * DT - REPAIR_RATE * DT * REPAIR_COST;
            assert!((world.cells[0].energy - expected).abs() < 1e-10);
        }
        let mut healthy = one_cell();
        healthy.cells[0].energy = 100.0;
        healthy.step(&mut registry);
        assert!((healthy.cells[0].energy - (100.0 - BASAL_METABOLISM * DT)).abs() < 1e-10);
    }

    #[test]
    fn unaffordable_repair_exhausts_energy_exactly_and_food_cannot_revive_the_cell() {
        let mut world = one_cell();
        world.cells[0].energy = BASAL_METABOLISM * DT + 0.039_069;
        world.cells[0].integrity = 0.5;
        world.food.push(Food { x: world.cells[0].x, y: world.cells[0].y, energy: 24.0, nutrients: FOOD_NUTRIENTS });
        world.step(&mut genomes("thrust = 0; turn = 0; divide = false; repair = 1;"));
        assert!(world.cells.is_empty());
        assert_eq!(world.deaths, 1);
        assert_eq!(world.eaten, 0);
        assert!(world.food.iter().any(|food| (food.energy - 24.0 * (-FOOD_DECAY * DT).exp()).abs() < 1e-12));
    }

    #[test]
    fn source_configuration_survives_reset_and_field_evolution_replays() {
        let mut registry = genomes("ri001 = ri001 + 1; thrust = 0.2; turn = 0; divide = false; photosynthesize = 1;");
        let mut world = World::empty(1);
        world.environment.configure(0.4, 0.2, 8.0);
        let source = world.environment.add_source(1, 200.0, 300.0, 1.0, 100.0);
        world.environment.add_source(0, 400.0, 200.0, 0.8, 180.0);
        world.environment.add_source(2, 200.0, 300.0, -0.5, 180.0);
        world.reset(91, 1);
        let mut initial = Vec::new(); world.environment.snapshot(world.time(), &mut initial);
        for _ in 0..90 { world.step(&mut registry); }
        let mut first = Vec::new(); let mut first_environment = Vec::new();
        world.snapshot(&mut first); world.environment.snapshot(world.time(), &mut first_environment);
        let harvested = world.photosynthesis_total;
        assert!(harvested > 0.0);
        world.reset(91, 1);
        assert_eq!(world.environment.sources[0].id, source);
        assert_eq!(world.photosynthesis_total, 0.0);
        assert_eq!(world.predations, 0);
        assert_eq!(world.attacks, 0);
        let mut reset_environment = Vec::new(); world.environment.snapshot(world.time(), &mut reset_environment);
        assert_eq!(initial, reset_environment);
        for _ in 0..90 { world.step(&mut registry); }
        let mut second = Vec::new(); let mut second_environment = Vec::new();
        world.snapshot(&mut second); world.environment.snapshot(world.time(), &mut second_environment);
        assert_eq!(first, second);
        assert_eq!(first_environment, second_environment);
        assert_eq!(harvested, world.photosynthesis_total);
    }

    #[test]
    fn engulf_claims_use_stable_ids_and_only_one_attacker_recovers_the_prey() {
        let output = Program::compile("thrust = 0; turn = 0; divide = false; engulf = true;").unwrap().evaluate(&Inputs::default());
        for reversed in [false, true] {
            let mut world = one_cell(); world.seed_cells(1, 2, -1.0, -1.0);
            for (i, cell) in world.cells.iter_mut().enumerate() {
                cell.x = [100.0, 140.0, 120.0][i]; cell.y = 100.0;
                cell.size = if i < 2 { 4.0 } else { 1.0 }; cell.energy = 100.0;
            }
            if reversed { world.cells.reverse(); }
            let inputs = world.sense_all();
            let consumed = world.resolve_predation(&inputs, &[output; 3]);
            assert_eq!(consumed.iter().filter(|&&value| value).count(), 1);
            let winner = world.cells.iter().find(|cell| cell.id == 1).unwrap();
            let loser = world.cells.iter().find(|cell| cell.id == 2).unwrap();
            assert_eq!(winner.energy, 100.0 - ENGULF_COST + ENGULF_RECOVERY * 120.0);
            assert_eq!(loser.energy, 100.0);
            assert_eq!(world.predations, 1);
            assert_eq!(world.attacks, 0);
        }
    }

    #[test]
    fn a_consumed_predator_cannot_harvest_or_bite_later_in_the_same_tick() {
        let output = Program::compile("thrust = 0; turn = 0; divide = false; engulf = true; bite = 1;").unwrap().evaluate(&Inputs::default());
        let mut world = one_cell(); world.seed_cells(1, 2, -1.0, -1.0);
        for (i, cell) in world.cells.iter_mut().enumerate() {
            cell.x = 100.0; cell.y = 100.0;
            cell.size = [4.0, 2.0, 1.0][i]; cell.energy = [300.0, 150.0, 50.0][i];
        }
        let inputs = world.sense_all();
        assert_eq!(inputs[0].prey_id, 2.0); assert_eq!(inputs[1].prey_id, 3.0);
        let consumed = world.resolve_predation(&inputs, &[output; 3]);
        assert_eq!(consumed, [false, true, false]);
        assert_eq!(world.cells[0].energy, 300.0 - 16.0 + 0.8 * 190.0);
        assert_eq!(world.cells[2].energy, 50.0);
        assert_eq!(world.cells[2].integrity, 1.0);
        assert_eq!(world.predations, 1);
        assert_eq!(world.attacks, 0);
    }

    #[test]
    fn frozen_prey_identity_is_not_retargeted_and_contact_wraps_at_seams() {
        let output = Program::compile("thrust = 0; turn = 0; divide = false; engulf = true;").unwrap().evaluate(&Inputs::default());
        let mut world = one_cell(); world.seed_cells(1, 2, -1.0, -1.0);
        for cell in &mut world.cells { cell.y = 100.0; cell.energy = 100.0; }
        world.cells[0].x = 3.0; world.cells[0].size = 4.0;
        world.cells[1].x = WIDTH - 10.0; world.cells[2].x = 60.0;
        let inputs = world.sense_all();
        assert_eq!(inputs[0].prey_id, 2.0);
        world.cells[1].x = 80.0; world.cells[2].x = 5.0;
        assert!(world.resolve_predation(&inputs, &[output; 3]).iter().all(|&value| !value));
        world.cells[1].x = WIDTH - 10.0;
        assert_eq!(world.resolve_predation(&inputs, &[output; 3]), [false, true, false]);
    }

    #[test]
    fn engulfing_pays_before_harvesting_and_overflow_becomes_bounded_food() {
        let output = Program::compile("thrust = 0; turn = 0; divide = false; engulf = true;").unwrap().evaluate(&Inputs::default());
        let mut world = one_cell(); world.seed_cells(1, 1, -1.0, -1.0);
        for cell in &mut world.cells { cell.x = 100.0; cell.y = 100.0; cell.energy = 100.0; }
        world.cells[0].size = 4.0; world.cells[0].energy = ENGULF_COST;
        let inputs = world.sense_all();
        assert_eq!(world.resolve_predation(&inputs, &[output; 2]), [false, false]);
        world.cells[0].energy = 799.0;
        let before = 799.0 + 100.0 + GROWTH_ENERGY_COST * 5.0;
        assert_eq!(world.resolve_predation(&inputs, &[output; 2]), [false, true]);
        assert_eq!(world.cells[0].energy, 800.0);
        let food: f64 = world.food.iter().map(|food| food.energy).sum();
        let after = world.cells[0].energy + GROWTH_ENERGY_COST * world.cells[0].size + food;
        let lost = ENGULF_COST + (1.0 - ENGULF_RECOVERY) * (100.0 + GROWTH_ENERGY_COST);
        assert!((after - (before - lost)).abs() < 1e-10);
        assert!(world.food.iter().all(|food| food.energy > 0.0 && food.energy <= 24.0));
        world.food.resize(MAX_FOOD, Food { x: 0.0, y: 0.0, energy: 1.0, nutrients: FOOD_NUTRIENTS });
        world.emit_remains(1.0, 1.0, 100.0, 1.0);
        assert_eq!(world.food.len(), MAX_FOOD);
    }

    #[test]
    fn reciprocal_bites_are_simultaneous_and_remains_do_not_duplicate_resources() {
        let mut registry = genomes("thrust = 0; turn = 0; divide = false; bite = 1;");
        let mut world = one_cell(); world.seed_cells(1, 1, -1.0, -1.0);
        for (i, cell) in world.cells.iter_mut().enumerate() {
            cell.x = 100.0 + 14.0 * i as f64; cell.y = 100.0;
            cell.energy = 50.0; cell.integrity = 0.001; cell.vx = 0.0; cell.vy = 0.0;
        }
        world.step(&mut registry);
        assert!(world.cells.is_empty());
        assert_eq!(world.attacks, 2); assert_eq!(world.predations, 2); assert_eq!(world.deaths, 2);
        let recovered: f64 = world.food.iter().map(|food| food.energy).sum();
        let expected = 2.0 * REMAINS_RECOVERY * (50.0 - (BASAL_METABOLISM + BITE_UPKEEP) * DT + GROWTH_ENERGY_COST);
        assert!((recovered - expected).abs() < 1e-10);
    }

    #[test]
    fn defense_reduces_each_simultaneous_bite_and_costs_energy() {
        let mut world = one_cell(); world.seed_cells(1, 1, -1.0, -1.0);
        for cell in &mut world.cells { cell.x = 100.0; cell.y = 100.0; cell.energy = 100.0; }
        let mut registry = genomes("thrust = 0; turn = 0; divide = false; bite = 1; defend = 1;");
        world.step(&mut registry);
        assert_eq!(world.attacks, 2);
        for cell in &world.cells {
            assert!((cell.integrity - (1.0 - 0.25 * (1.0 - 0.08 - 0.65) * DT)).abs() < 1e-12);
            assert!((cell.energy - (100.0 - (BASAL_METABOLISM + BITE_UPKEEP + DEFENSE_UPKEEP) * DT)).abs() < 1e-10);
        }
    }

    #[test]
    fn registry_byte_capacity_prevents_new_mutations_but_allows_existing_replay() {
        let mut registry = genomes(DEFAULT_PROGRAM);
        let first = mutate_registered(&mut registry, 1, 57, MutationKind::Constant);
        assert!(first.skipped.is_none());
        registry[0].bytes = MAX_GENOME_BYTES - registry[1].bytes;
        assert_eq!(registry_bytes(&registry), MAX_GENOME_BYTES);
        let replay = mutate_registered(&mut registry, 1, 57, MutationKind::Constant);
        assert_eq!(replay.genome_id, first.genome_id);
        assert!(replay.skipped.is_none());
        let denied = mutate_registered(&mut registry, 1, 58, MutationKind::Constant);
        assert_eq!(denied.genome_id, 1);
        assert!(denied.skipped.unwrap().contains("64 MiB"));
        assert_eq!(registry.len(), 2);
    }

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    #[test]
    fn material_imports_count_only_actual_introductions_and_restart_replaces_the_ledger() {
        let mut world = World::empty(11);
        close(world.nutrient_total(), 0.3 * PATCHES as f64);
        close(world.nutrient_total(), world.nutrient_input_total());
        world.seed_cells(1, 2, -1.0, -1.0);
        world.add_food(-1.0, -1.0, 4);
        close(world.nutrient_input_total(), 0.3 * PATCHES as f64 + 4.0);
        close(world.nutrient_total(), world.nutrient_input_total());
        world.seed_cells(1, usize::MAX, -1.0, -1.0);
        world.add_food(-1.0, -1.0, usize::MAX);
        let full = world.nutrient_input_total();
        assert_eq!(world.seed_cells(1, 5, 100.0, 100.0), 0);
        assert_eq!(world.add_food(100.0, 100.0, 5), 0);
        close(world.nutrient_input_total(), full);
        close(world.nutrient_total(), full);
        world.environment.set_initial_nutrients(0.1);
        close(world.nutrient_total(), full);
        world.reset(11, 1);
        close(world.nutrient_input_total(), 0.1 * PATCHES as f64 + 360.0 * FOOD_NUTRIENTS + 22.0 * 1.5);
        close(world.nutrient_total(), world.nutrient_input_total());
    }

    #[test]
    fn mineral_limited_growth_binds_only_available_material_and_charges_only_actual_growth() {
        let mut world = one_cell();
        let cell = &mut world.cells[0];
        cell.nutrient_stored = 0.001;
        cell.energy = 100.0;
        let material = cell.size + cell.nutrient_stored;
        cell.grow(1.0);
        close(cell.size, 1.001);
        assert_eq!(cell.nutrient_stored, 0.0);
        close(cell.energy, 100.0 - 0.001 * GROWTH_ENERGY_COST);
        close(cell.size + cell.nutrient_stored, material);
        let energy = cell.energy;
        cell.grow(1.0);
        assert_eq!(cell.energy, energy);
        close(cell.size, 1.001);
        cell.nutrient_stored = 1.0;
        cell.energy = 0.039_069;
        let material = cell.size + cell.nutrient_stored;
        cell.grow(1.0);
        assert_eq!(cell.energy, 0.0);
        close(cell.size + cell.nutrient_stored, material);
    }

    #[test]
    fn scarce_dissolved_material_is_shared_proportionally_independent_of_cell_order() {
        let mut results = Vec::new();
        for reversed in [false, true] {
            let mut world = one_cell();
            world.seed_cells(1, 3, -1.0, -1.0);
            for cell in &mut world.cells {
                cell.x = 105.0; cell.y = 105.0; cell.energy = 100.0; cell.nutrient_stored = 0.0;
            }
            world.cells[1].body = [0.15, 0.15, 0.4, 0.15, 0.15];
            world.cells[2].energy = 0.0;
            world.cells[3].nutrient_stored = world.cells[3].nutrient_capacity();
            world.environment.add_nutrients(105.0, 105.0, 0.003);
            let before = world.nutrient_total();
            if reversed { world.cells.reverse(); }
            world.absorb_nutrients(&vec![Outputs::default(); 4]);
            close(world.nutrient_total(), before);
            close(world.environment.nutrient_total(), 0.0);
            world.cells.sort_by_key(|cell| cell.id);
            close(world.cells[0].nutrient_stored, 0.001);
            close(world.cells[1].nutrient_stored, 0.002);
            close(world.cells[0].energy, 99.999);
            close(world.cells[1].energy, 99.998);
            assert_eq!(world.cells[2].nutrient_stored, 0.0);
            assert_eq!(world.cells[3].nutrient_stored, 2.0);
            results.push(world.cells.iter().map(|cell| (cell.nutrient_stored, cell.energy, cell.uptake_rate)).collect::<Vec<_>>());
        }
        assert_eq!(results[0], results[1]);
    }

    #[test]
    fn uptake_can_fund_same_tick_growth_while_programs_read_the_old_reserve() {
        let mut world = one_cell();
        world.cells[0].nutrient_stored = 0.0;
        world.cells[0].energy = 100.0;
        let (x, y) = (world.cells[0].x, world.cells[0].y);
        world.environment.add_nutrients(x, y, 2.0);
        let before = world.nutrient_total();
        let mut registry = genomes("thrust = if nutrient.stored > 0 then 1 else 0; turn = 0; divide = false; grow = 0.5; absorb = 1;");
        world.step(&mut registry);
        let gain = UPTAKE_RATE * DT;
        let growth = 0.5 * MAX_GROWTH_RATE * DT;
        close(world.cells[0].size, 1.0 + growth);
        close(world.cells[0].nutrient_stored, gain - growth);
        close(world.cells[0].energy, 100.0 - BASAL_METABOLISM * DT - gain - growth * GROWTH_ENERGY_COST);
        assert_eq!(world.cells[0].vx.hypot(world.cells[0].vy), 0.0);
        close(world.nutrient_total(), before);
        world.step(&mut registry);
        assert!(world.cells[0].vx.hypot(world.cells[0].vy) > 0.0);
        close(world.nutrient_total(), before);
    }

    #[test]
    fn uptake_exhaustion_is_exact_and_neither_food_nor_light_can_resurrect_the_cell() {
        let mut world = one_cell();
        world.environment.configure(0.5, 1.0, 0.0);
        world.cells[0].energy = (BASAL_METABOLISM + PHOTO_UPKEEP) * DT + 0.002;
        world.cells[0].nutrient_stored = 0.0;
        world.cells[0].cooldown = 0.0;
        let (x, y) = (world.cells[0].x, world.cells[0].y);
        world.environment.add_nutrients(x, y, 2.0);
        world.food.push(Food { x, y, energy: 24.0, nutrients: 0.25 });
        let before = world.nutrient_total();
        world.step(&mut genomes("thrust = 0; turn = 0; divide = true; grow = 1; absorb = 1; photosynthesize = 1;"));
        assert!(world.cells.is_empty());
        assert_eq!(world.births, 0);
        assert_eq!(world.eaten, 0);
        assert_eq!(world.photosynthesis_total, 0.0);
        close(world.nutrient_total(), before);
    }

    #[test]
    fn feeding_is_partial_has_one_total_throughput_budget_and_recycles_excess_minerals() {
        let mut world = one_cell();
        world.cells[0].energy = 50.0;
        world.cells[0].nutrient_stored = 1.99;
        let (x, y) = (world.cells[0].x, world.cells[0].y);
        world.food = vec![Food { x, y, energy: 10.0, nutrients: 1.0 }; 2];
        let before = world.nutrient_total();
        world.eat();
        close(world.cells[0].energy, 53.0);
        close(world.cells[0].nutrient_stored, 2.0);
        close(world.food[0].energy, 7.0);
        close(world.food[0].nutrients, 0.7);
        assert_eq!(world.food[1].energy, 10.0);
        close(world.environment.nutrient_total(), 0.29);
        assert_eq!(world.eaten, 0);
        close(world.nutrient_total(), before);

        world.cells[0].energy = world.cells[0].energy_capacity();
        world.eat();
        close(world.food[0].energy, 7.0);
        assert_eq!(world.food[1].energy, 10.0);
        close(world.nutrient_total(), before);

        world.cells[0].nutrient_stored = 0.0;
        let before = world.nutrient_total();
        world.eat();
        assert_eq!(world.cells[0].energy, world.cells[0].energy_capacity());
        close(world.cells[0].nutrient_stored, 0.3);
        close(world.food[0].energy, 4.0);
        close(world.nutrient_total(), before);
    }

    #[test]
    fn food_decay_and_tiny_particle_cleanup_return_every_mineral_to_water() {
        let mut world = one_cell();
        world.food = vec![Food { x: 1.0, y: 1.0, energy: 1.0, nutrients: 4.0 },
            Food { x: WIDTH - 1.0, y: HEIGHT - 1.0, energy: 1e-8, nutrients: 7.0 }];
        let before = world.nutrient_total();
        let imports = world.nutrient_input_total();
        world.decay_food();
        let retained = (-FOOD_DECAY * DT).exp();
        assert_eq!(world.food.len(), 1);
        close(world.food[0].energy, retained);
        close(world.food[0].nutrients, 4.0 * retained);
        close(world.environment.nutrient_total(), 11.0 - 4.0 * retained);
        close(world.nutrient_total(), before);
        assert_eq!(world.nutrient_input_total(), imports);
    }

    #[test]
    fn death_and_remains_preserve_all_material_even_when_the_food_budget_is_full() {
        let mut world = one_cell();
        world.cells[0].energy = 0.001;
        world.add_food(-1.0, -1.0, MAX_FOOD);
        let before = world.nutrient_total();
        let imports = world.nutrient_input_total();
        world.step(&mut idle());
        assert!(world.cells.is_empty());
        assert_eq!(world.food.len(), MAX_FOOD);
        let decay = MAX_FOOD as f64 * FOOD_NUTRIENTS * (1.0 - (-FOOD_DECAY * DT).exp());
        close(world.environment.nutrient_total(), 1.5 + decay);
        close(world.nutrient_total(), before);
        assert_eq!(world.nutrient_input_total(), imports);

        world.food.pop();
        let before = world.nutrient_total();
        let water = world.environment.nutrient_total();
        world.emit_remains(1.0, 1.0, 100.0, 5.0);
        assert_eq!(world.food.len(), MAX_FOOD);
        close(world.food.last().unwrap().nutrients, 1.2);
        close(world.environment.nutrient_total() - water, 3.8);
        close(world.nutrient_total() - before, 5.0);
        assert_eq!(world.nutrient_input_total(), imports);
    }

    #[test]
    fn engulf_transfers_all_material_once_and_respects_digestive_capacity_and_cooldown() {
        let mut world = one_cell();
        world.seed_cells(1, 1, -1.0, -1.0);
        for cell in &mut world.cells { cell.x = 100.0; cell.y = 100.0; cell.energy = 100.0; }
        world.cells[0].size = 4.0;
        world.cells[0].energy = 799.0;
        world.cells[0].nutrient_stored = 7.8;
        world.cells[1].nutrient_stored = 1.2;
        let output = Outputs { engulf: true, ..Outputs::default() };
        let before = world.nutrient_total();
        let inputs = world.sense_all();
        assert_eq!(world.resolve_predation(&inputs, &[output; 2]), [false, true]);
        close(world.cells[0].nutrient_stored, 8.0);
        close(world.environment.nutrient_total(), 2.0);
        close(world.nutrient_total(), before);
        assert_eq!(world.cells[0].engulf_cooldown, 2.0);
        assert!(world.food.iter().all(|food| food.nutrients == 0.0));

        world.cells.retain(Cell::alive);
        world.seed_cells(1, 1, -1.0, -1.0);
        world.cells[1].x = 100.0; world.cells[1].y = 100.0;
        let inputs = world.sense_all();
        assert_eq!(world.resolve_predation(&inputs, &[output; 2]), [false, false]);
        world.cells[0].engulf_cooldown = 0.0;
        world.cells[0].body[DIGESTION] = 0.049;
        assert_eq!(world.resolve_predation(&inputs, &[output; 2]), [false, false]);
        world.cells[0].body[DIGESTION] = 0.05;
        assert_eq!(world.resolve_predation(&inputs, &[output; 2]), [false, true]);
        close(world.cells[0].engulf_cooldown, 8.0);
    }

    #[test]
    fn division_splits_reserves_and_invested_mass_and_inherits_engulf_cooldown() {
        let mut world = one_cell();
        world.cells[0].size = 2.4;
        world.cells[0].energy = 400.0;
        world.cells[0].nutrient_stored = 2.1;
        world.cells[0].cooldown = 0.0;
        world.cells[0].engulf_cooldown = 3.0;
        world.cells[0].body = [0.1, 0.3, 0.4, 0.1, 0.1];
        let body = world.cells[0].body;
        let before = world.nutrient_total();
        world.step(&mut genomes("thrust = 0; turn = 0; divide = true; absorb = 0; invest.propulsion = .1; invest.photosynthesis = .3; invest.digestion = .4; invest.armor = .1; invest.sensing = .1;"));
        assert_eq!(world.cells.len(), 2);
        for cell in &world.cells {
            close(cell.nutrient_stored, 1.05);
            close(cell.engulf_cooldown, 3.0 - DT);
            for (actual, expected) in cell.body.into_iter().zip(body) { close(actual, expected); }
            close(cell.size, 1.2);
        }
        close(world.nutrient_total(), before);
    }

    #[test]
    fn remodeling_normalizes_targets_pays_for_actual_mass_and_includes_unassigned_core() {
        let mut cell = one_cell().cells.remove(0);
        cell.energy = 100.0;
        let original_material = cell.size + cell.nutrient_stored;
        let equal = Outputs { invest_propulsion: 1.0, invest_photosynthesis: 1.0, invest_digestion: 1.0,
            invest_armor: 1.0, invest_sensing: 1.0, ..Outputs::default() };
        cell.remodel(&equal);
        assert_eq!(cell.body, [0.2; 5]);
        assert_eq!(cell.energy, 100.0);
        let propulsion = Outputs { invest_propulsion: 1.0, invest_photosynthesis: 0.0, invest_digestion: 0.0,
            invest_armor: 0.0, invest_sensing: 0.0, ..Outputs::default() };
        cell.remodel(&propulsion);
        close(cell.remodeling_rate, REMODELING_RATE);
        close(cell.body[PROPULSION], 0.2 + REMODELING_RATE * DT);
        close(cell.energy, 100.0 - REMODELING_RATE * DT * REMODELING_COST);
        for _ in 0..400 { cell.remodel(&propulsion); }
        close(cell.body[PROPULSION], 1.0);
        close(cell.body.iter().sum(), 1.0);
        close(cell.energy, 100.0 - 0.8 * REMODELING_COST);
        let core = Outputs { invest_propulsion: 0.0, ..propulsion };
        for _ in 0..400 { cell.remodel(&core); }
        close(cell.body.iter().sum(), 0.0);
        close(cell.energy, 100.0 - (0.8 + 1.0) * REMODELING_COST);
        close(cell.basal_metabolism(), 0.35);
        close(cell.body_upkeep(), 0.0);
        close(cell.size + cell.nutrient_stored, original_material);

        cell.energy = 0.039_069;
        cell.remodel(&propulsion);
        // One tick is rate-limited; the following tick exhausts the remainder.
        cell.remodel(&propulsion);
        assert_eq!(cell.energy, 0.0);
        assert!(!cell.alive());
        close(cell.size + cell.nutrient_stored, original_material);
    }

    #[test]
    fn remodeled_propulsion_is_paid_now_and_changes_motion_only_on_the_next_tick() {
        let mut world = one_cell();
        world.cells[0].energy = 100.0;
        world.cells[0].angle = 0.0;
        let mut registry = genomes("thrust = 1; turn = 1; divide = false; absorb = 0; invest.propulsion = 1; invest.photosynthesis = 0; invest.digestion = 0; invest.armor = 0; invest.sensing = 0;");
        world.step(&mut registry);
        close(world.cells[0].angle, 3.2 * DT);
        close(world.cells[0].vx.hypot(world.cells[0].vy), 100.0 * DT * (-1.8 * DT).exp());
        close(world.cells[0].energy, 100.0 - (BASAL_METABOLISM + THRUST_ENERGY_COST) * DT
            - REMODELING_RATE * DT * REMODELING_COST);
        let previous = world.cells[0].clone();
        let factor = previous.capacity_factor(PROPULSION);
        assert!(factor > 1.0);
        world.step(&mut registry);
        close(world.cells[0].angle, previous.angle + 3.2 * factor.sqrt() * DT);
        close(world.cells[0].vx, (previous.vx + world.cells[0].angle.cos() * 100.0 * factor * DT) * (-1.8 * DT).exp());
        close(world.cells[0].energy, previous.energy - (BASAL_METABOLISM + THRUST_ENERGY_COST * factor) * DT
            - REMODELING_RATE * DT * REMODELING_COST);
    }

    #[test]
    fn specialized_sensing_crosses_seams_and_large_queries_never_duplicate_buckets() {
        let mut world = one_cell();
        world.cells[0].x = 10.0; world.cells[0].y = 10.0;
        world.cells[0].body = [0.0, 0.0, 0.0, 0.0, 1.0];
        world.food.push(Food { x: WIDTH - 300.0, y: 10.0, energy: 1.0, nutrients: 0.25 });
        world.seed_cells(1, 1, -1.0, -1.0);
        world.cells[1].x = 10.0; world.cells[1].y = HEIGHT - 300.0;
        let inputs = world.sense_all();
        assert_eq!(inputs[0].sense_range, 440.0);
        assert!(inputs[0].food_visible && inputs[0].neighbor_visible);
        close(inputs[0].food_distance, 310.0);
        close(inputs[0].neighbor_distance, 310.0);
        let mut grid = Grid::new();
        for (i, bucket) in grid.buckets.iter_mut().enumerate() { bucket.push(i); }
        let mut found: Vec<_> = grid.nearby_radius(10.0, 10.0, 440.0).collect();
        let count = found.len();
        found.sort_unstable(); found.dedup();
        assert_eq!(found.len(), count);
        world.cells[0].body[SENSING] = 0.0;
        let input = world.sense_all()[0];
        assert!(!input.food_visible && !input.neighbor_visible);
        assert_eq!(input.food_distance, 40.0);
        assert_eq!(input.neighbor_distance, 40.0);
        assert_eq!(input.prey_distance, 40.0);
        assert_eq!(input.threat_distance, 40.0);
    }

    #[test]
    fn digestive_and_photosynthetic_allocations_bound_resource_processing() {
        for fraction in [0.0, 0.2, 0.4] {
            let mut world = one_cell();
            world.cells[0].body = [0.0, 0.2, fraction, 0.0, 0.0];
            world.cells[0].energy = 50.0;
            world.cells[0].nutrient_stored = 0.0;
            let (x, y) = (world.cells[0].x, world.cells[0].y);
            world.food.push(Food { x, y, energy: 100.0, nutrients: 1.0 });
            world.eat();
            close(world.cells[0].energy - 50.0, DIGESTION_RATE * fraction / 0.2 * DT);
            world.environment.add_nutrients(x, y, 1.0);
            let reserve = world.cells[0].nutrient_stored;
            world.absorb_nutrients(&[Outputs::default()]);
            close(world.cells[0].nutrient_stored - reserve, UPTAKE_RATE * fraction / 0.2 * DT);

            world.cells[0].body[PHOTOSYNTHESIS] = fraction;
            world.environment.configure(0.5, 1.0, 0.0);
            let energy = world.cells[0].energy;
            world.photosynthesize(&[Outputs { photosynthesize: 1.0, ..Outputs::default() }]);
            close(world.cells[0].energy - energy, PHOTO_RATE * fraction / 0.2 * DT);
        }
    }

    #[test]
    fn armor_caps_bite_protection_and_digestive_allocation_scales_damage() {
        let mut world = one_cell();
        world.seed_cells(1, 1, -1.0, -1.0);
        for cell in &mut world.cells { cell.x = 100.0; cell.y = 100.0; }
        world.cells[0].body = [0.0, 0.0, 0.4, 0.0, 0.6];
        world.cells[1].body = [0.0, 0.0, 0.0, 0.5, 0.5];
        let inputs = world.sense_all();
        world.resolve_predation(&inputs, &[Outputs { bite: 1.0, ..Outputs::default() },
            Outputs { defend: 1.0, ..Outputs::default() }]);
        close(world.cells[1].integrity, 1.0 - 0.25 * 2.0 * 0.05 * DT);
        assert_eq!(world.attacks, 1);
        assert_eq!(world.cells[0].integrity, 1.0);
    }

    #[test]
    fn mixed_ecosystem_keeps_a_closed_material_ledger_across_sources_actions_and_deaths() {
        let mut registry = genomes("thrust = .4; turn = food.angle; divide = energy > 110 * size; grow = 1; absorb = 1; photosynthesize = 1; bite = .4; engulf = true; repair = .2; invest.propulsion = .1; invest.photosynthesis = .3; invest.digestion = .3; invest.armor = .2; invest.sensing = .1;");
        let mut world = World::empty(73);
        world.environment.configure(0.5, 0.8, 60.0);
        world.environment.add_source(3, 100.0, 100.0, 0.8, 150.0);
        world.environment.add_source(2, 800.0, 600.0, -1.0, 240.0);
        world.reset(73, 1);
        world.food_rate = 0.0;
        world.mutation_rate = 0.0;
        for tick in 0..360 {
            if tick == 60 { world.environment.add_nutrients(300.0, 700.0, 8.0); }
            if tick == 90 { world.add_food(500.0, 200.0, 30); }
            if tick == 120 { world.seed_cells(1, 4, 500.0, 200.0); }
            world.step(&mut registry);
            assert!((world.nutrient_total() - world.nutrient_input_total()).abs() < 1e-7);
            for cell in &world.cells {
                assert!(cell.size.is_finite() && cell.size >= MIN_SIZE && cell.size <= MAX_SIZE);
                assert!(cell.nutrient_stored.is_finite() && cell.nutrient_stored >= 0.0
                    && cell.nutrient_stored <= cell.nutrient_capacity() + 1e-12);
                assert!(cell.body.iter().all(|fraction| fraction.is_finite() && *fraction >= 0.0 && *fraction <= 1.0));
                assert!(cell.body.iter().sum::<f64>() <= 1.0 + 1e-12);
                assert!(cell.energy > 0.0 && cell.energy <= cell.energy_capacity());
            }
            assert!(world.food.iter().all(|food| food.energy > 0.0 && food.nutrients >= 0.0 && food.nutrients.is_finite()));
        }
    }
}
