//! Fixed-step physical cells. Programs read one immutable snapshot; capabilities
//! come exclusively from paid physical organelles carried into that tick.
#[cfg(test)]
use crate::body::BodyFixture;
use crate::body::{
    Body, BodyCapacities, BodyControls, FounderBudget, OrganelleKind as Kind, FAMILY_COUNT,
};
use crate::environment::{Environment, COLUMNS as FIELD_COLUMNS, PATCHES, ROWS as FIELD_ROWS};
use crate::genome::{
    EvalScratch, Inputs, MutationKind, OrganelleInput, Outputs, Program, Registers,
};
use std::f64::consts::{PI, TAU};
use std::fmt::Write;
use std::rc::Rc;
mod checkpoint;
#[cfg(test)]
mod sensing_tests;

pub const WIDTH: f64 = 1200.0;
pub const HEIGHT: f64 = 760.0;
pub const DT: f64 = 1.0 / 30.0;
pub const CELL_RADIUS: f64 = 7.0;
// Geometric safety bound, not a carrying capacity: at most one quarter of the
// shorter torus dimension across. Broad phases use actual radii up to this bound.
pub const MAX_AREA: f64 = (HEIGHT / 8.0 / CELL_RADIUS) * (HEIGHT / 8.0 / CELL_RADIUS);
pub const MAX_FOOD: usize = 1200;
pub const CELL_STRIDE: usize = 39 + FAMILY_COUNT * 6;
pub const LIFESPAN: f64 = 300.0;
pub const MEMORY_REGISTER_COST: f64 = 0.0025;
pub const MAX_GENOME_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_GENOMES: usize = 2048;
const FOOD_NUTRIENTS: f64 = 0.25;
const FOOD_DECAY: f64 = 0.02;
const ASSIMILATION_EFFICIENCY: f64 = 0.65;
const PHOTO_HABITAT_POWER: f64 = 2400.0;
const PHOTO_EXUDATION: f64 = 0.12;
const DIVISION_COST: f64 = 12.0;
const DIVISION_COOLDOWN: f64 = 12.0;
const GRID_COLUMNS: usize = 20;
const GRID_ROWS: usize = 13;
const GRID_BUCKETS: usize = GRID_COLUMNS * GRID_ROWS;
const TRACK_LIMIT: usize = 8;
const TRACK_TTL: u64 = 30;

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
        Self {
            source,
            program,
            parent_id,
            mutation_kind,
            bytes,
        }
    }
}

pub fn registry_bytes(genomes: &[Genome]) -> usize {
    genomes
        .iter()
        .map(|genome| genome.bytes)
        .fold(0usize, usize::saturating_add)
}

pub struct MutationResult {
    pub genome_id: u32,
    pub skipped: Option<&'static str>,
}

/// Provenance is part of a genome's identity. Replaying a mutation reuses the
/// same entry, while identical source descended from a different parent does not.
pub fn mutate_registered(
    genomes: &mut Vec<Genome>,
    parent_id: u32,
    seed: u64,
    kind: MutationKind,
) -> MutationResult {
    let Some(parent) = genomes.get(parent_id.wrapping_sub(1) as usize) else {
        return MutationResult {
            genome_id: 0,
            skipped: Some("Unknown parent genome."),
        };
    };
    let Some(mutation) = Program::mutate_source(&parent.source, seed, kind) else {
        return MutationResult {
            genome_id: parent_id,
            skipped: Some("No applicable mutation fits this program's limits."),
        };
    };
    let mutation_kind = match mutation.kind {
        MutationKind::Any => 0,
        MutationKind::Constant => 1,
        MutationKind::Expression => 2,
        MutationKind::Reference => 3,
    };
    if mutation_kind == 0 || mutation.source == parent.source {
        return MutationResult {
            genome_id: parent_id,
            skipped: Some("No applicable mutation changes this program."),
        };
    }
    // Look for a replay before applying the capacity limit: reset must still be
    // able to reproduce already-registered lineages when the registry is full.
    if let Some(index) = genomes.iter().position(|genome| {
        genome.parent_id == parent_id
            && genome.mutation_kind == mutation_kind
            && genome.source == mutation.source
    }) {
        return MutationResult {
            genome_id: index as u32 + 1,
            skipped: None,
        };
    }
    if genomes.len() >= MAX_GENOMES {
        return MutationResult {
            genome_id: parent_id,
            skipped: Some(
                "The 2048-genome limit was reached; the original genome is inherited unchanged.",
            ),
        };
    }
    let genome = Genome::new(mutation.source, mutation.program, parent_id, mutation_kind);
    if registry_bytes(genomes).saturating_add(genome.bytes) > MAX_GENOME_BYTES {
        return MutationResult { genome_id: parent_id, skipped: Some("The 64 MiB genome storage limit was reached; the original genome is inherited unchanged.") };
    }
    genomes.push(genome);
    MutationResult {
        genome_id: genomes.len() as u32,
        skipped: None,
    }
}

#[derive(Clone)]
struct Rng(u32);
impl Rng {
    fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x9E37_79B9 } else { seed })
    }
    fn next(&mut self) -> u32 {
        let mut n = self.0;
        n ^= n << 13;
        n ^= n >> 17;
        n ^= n << 5;
        self.0 = n;
        n
    }
    fn unit(&mut self) -> f64 {
        self.next() as f64 / (u32::MAX as f64 + 1.0)
    }
}

/// Wrapped broad-phase queries visit each bucket at most once, even when a
/// specialized cell senses farther than half the world's shorter dimension.
struct Grid {
    buckets: [Vec<usize>; GRID_BUCKETS],
}
impl Grid {
    fn new() -> Self {
        Self {
            buckets: std::array::from_fn(|_| Vec::new()),
        }
    }
    fn index(x: f64, y: f64) -> usize {
        let col = ((x / WIDTH * GRID_COLUMNS as f64) as usize).min(GRID_COLUMNS - 1);
        let row = ((y / HEIGHT * GRID_ROWS as f64) as usize).min(GRID_ROWS - 1);
        row * GRID_COLUMNS + col
    }
    fn insert(&mut self, x: f64, y: f64, id: usize) {
        self.buckets[Self::index(x, y)].push(id);
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

#[derive(Clone, Debug, Default)]
struct Tracking {
    entries: Vec<Track>,
    next_serial: u64,
}
#[derive(Clone, Debug)]
struct Track {
    entity: u64,
    serial: u64,
    seen: u64,
}
impl Tracking {
    fn observe(&mut self, entity: u64, cell_id: u32, tick: u64) -> Rc<str> {
        self.entries
            .retain(|entry| tick.saturating_sub(entry.seen) <= TRACK_TTL);
        let serial =
            if let Some(entry) = self.entries.iter_mut().find(|entry| entry.entity == entity) {
                entry.seen = tick;
                entry.serial
            } else {
                if self.next_serial == u64::MAX {
                    return Rc::from("");
                }
                self.next_serial += 1;
                if self.entries.len() == TRACK_LIMIT {
                    let oldest = self
                        .entries
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, e)| (e.seen, e.serial))
                        .unwrap()
                        .0;
                    self.entries.remove(oldest);
                }
                let serial = self.next_serial;
                self.entries.push(Track {
                    entity,
                    serial,
                    seen: tick,
                });
                serial
            };
        token(cell_id, serial)
    }
    fn entity(&self, requested: &str, cell_id: u32, tick: u64) -> Option<u64> {
        if requested.is_empty() {
            return None;
        }
        self.entries
            .iter()
            .find(|e| {
                tick.saturating_sub(e.seen) <= TRACK_TTL
                    && token(cell_id, e.serial).as_ref() == requested
            })
            .map(|e| e.entity)
    }
}
fn token(cell_id: u32, serial: u64) -> Rc<str> {
    // Per-observer monotonic identities, permuted before presentation. No world
    // ID, object kind, or energy is encoded in a readable signal.
    Rc::from(format!(
        "t{:08x}{:016x}",
        cell_id.wrapping_mul(0x9e3779b1),
        mix64(serial)
    ))
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
    pub age: f64,
    pub generation: u32,
    pub parent_id: u32,
    pub cooldown: f64,
    pub memory: Registers,
    pub integrity: f64,
    pub body: Body,
    pub nutrient_stored: f64,
    pub gut_energy: f64,
    pub gut_nutrients: f64,
    pub birth_area: f64,
    pub photosynthesis_rate: f64,
    pub uptake_rate: f64,
    pub digestion_rate: f64,
    pub growth_rate: f64,
    pub exudation_rate: f64,
    pub construction_rate: f64,
    pub recycling_rate: f64,
    pub repair_rate: f64,
    pub engulf_cooldown: f64,
    requested: BodyControls,
    scan_bearing: f64,
    scan_width: f64,
    scan_target: Rc<str>,
    tracking: Tracking,
}
impl Cell {
    fn alive(&self) -> bool {
        self.energy > 0.0 && self.integrity > 0.0 && self.body.functional_mass(Kind::Core) > 0.0
    }
    pub fn radius(&self) -> f64 {
        CELL_RADIUS * self.body.area.sqrt()
    }
    pub fn mass(&self) -> f64 {
        self.body.structural_mass() + self.nutrient_stored + self.gut_nutrients
    }
    fn inertia(&self) -> f64 {
        self.mass() + 0.35 * self.body.area
    }
    pub fn energy_capacity(&self) -> f64 {
        self.body.energy_capacity()
    }
    pub fn nutrient_capacity(&self) -> f64 {
        self.body.mineral_capacity()
    }
    pub fn gut_capacity(&self) -> f64 {
        self.body.gut_capacity()
    }
    pub fn basal_metabolism(&self) -> f64 {
        0.15 * self.body.area + self.body.upkeep()
    }
    pub fn maturity(&self) -> f64 {
        (self.body.area / self.birth_area - 1.0).clamp(0.0, 1.0)
    }
    pub fn division_ready(&self) -> bool {
        self.alive()
            && self.cooldown == 0.0
            && self.body.area + 1e-10 >= 2.0 * self.birth_area
            && self.energy > DIVISION_COST * self.body.area + 0.35 * self.energy_capacity()
            && self.body.can_partition()
    }
    fn receive_nutrients(&mut self, amount: f64) -> f64 {
        let kept = amount.min((self.nutrient_capacity() - self.nutrient_stored).max(0.0));
        self.nutrient_stored += kept;
        amount - kept
    }
    fn gut_fraction_for(&self, energy: f64, minerals: f64, caps: &BodyCapacities) -> f64 {
        let energy_limit = if energy > 0.0 {
            (caps.gut_energy - self.gut_energy).max(0.0) / energy
        } else {
            1.0
        };
        let mineral_limit = if minerals > 0.0 {
            (caps.gut_minerals - self.gut_nutrients).max(0.0) / minerals
        } else {
            1.0
        };
        energy_limit.min(mineral_limit).clamp(0.0, 1.0)
    }
}
#[derive(Clone, Debug)]
pub struct Food {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    pub nutrients: f64,
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
    energy_imports: f64,
    food_credit: f64,
    next_id: u32,
    next_food_id: u64,
    rng: Rng,
    seed: u32,
}
impl World {
    pub fn empty(seed: u32) -> Self {
        Self {
            cells: Vec::new(),
            food: Vec::new(),
            ticks: 0,
            births: 0,
            deaths: 0,
            eaten: 0,
            food_rate: 6.0,
            mutation_rate: 0.1,
            mutations: 0,
            mutations_skipped: 0,
            environment: Environment::default(),
            predations: 0,
            attacks: 0,
            photosynthesis_total: 0.0,
            nutrient_imports: 0.0,
            energy_imports: 0.0,
            food_credit: 0.0,
            next_id: 1,
            next_food_id: 1,
            rng: Rng::new(seed),
            seed,
        }
    }
    pub fn reset(&mut self, seed: u32, genome_id: u32) {
        self.try_reset(seed, genome_id)
            .expect("Unable to allocate simulation")
    }
    pub fn try_reset(&mut self, seed: u32, genome_id: u32) -> Result<(), String> {
        let mut next = Self::empty(seed);
        next.environment = self.environment.clone();
        next.environment.reset_dynamic();
        next.mutation_rate = self.mutation_rate;
        next.food_rate = self.food_rate;
        next.add_food(-1.0, -1.0, 360);
        next.try_seed_cells(genome_id, 22, -1.0, -1.0)?;
        *self = next;
        Ok(())
    }
    pub fn reset_empty(&mut self, seed: u32, initial_food: usize) {
        let mut next = Self::empty(seed);
        next.environment = self.environment.clone();
        next.environment.reset_dynamic();
        next.mutation_rate = self.mutation_rate;
        next.food_rate = self.food_rate;
        next.add_food(-1.0, -1.0, initial_food);
        *self = next;
    }
    pub fn time(&self) -> f64 {
        self.ticks as f64 * DT
    }
    pub fn nutrient_food_total(&self) -> f64 {
        self.food.iter().map(|f| f.nutrients).sum()
    }
    pub fn nutrient_cell_total(&self) -> f64 {
        self.cells.iter().map(Cell::mass).sum()
    }
    pub fn nutrient_total(&self) -> f64 {
        self.environment.nutrient_total() + self.nutrient_food_total() + self.nutrient_cell_total()
    }
    pub fn nutrient_input_total(&self) -> f64 {
        self.environment.nutrient_input_total() + self.nutrient_imports
    }
    pub fn energy_input_total(&self) -> f64 {
        self.energy_imports
    }
    pub fn energy_total(&self) -> f64 {
        self.food.iter().map(|f| f.energy).sum::<f64>()
            + self
                .cells
                .iter()
                .map(|c| c.energy + c.gut_energy + c.body.embodied_energy())
                .sum::<f64>()
    }
    pub fn seed_cells(&mut self, id: u32, count: usize, x: f64, y: f64) -> usize {
        self.try_seed_cells(id, count, x, y)
            .expect("Unable to allocate seeded cells")
    }
    pub fn try_seed_cells(
        &mut self,
        id: u32,
        count: usize,
        x: f64,
        y: f64,
    ) -> Result<usize, String> {
        self.seed_bodies(id, count, x, y, Body::founder)
    }
    /// Preassembled specimens are available only to native regression tests.
    #[cfg(test)]
    pub(crate) fn try_seed_fixture(
        &mut self,
        genome_id: u32,
        count: usize,
        x: f64,
        y: f64,
        fixture: BodyFixture,
    ) -> Result<usize, String> {
        self.seed_bodies(genome_id, count, x, y, || Body::fixture(fixture))
    }
    fn seed_bodies(
        &mut self,
        genome_id: u32,
        count: usize,
        x: f64,
        y: f64,
        mut create_body: impl FnMut() -> (Body, FounderBudget),
    ) -> Result<usize, String> {
        if count > (u32::MAX - self.next_id) as usize {
            return Err("Cannot add organisms: the cell identifier sequence is exhausted.".into());
        }
        self.cells
            .try_reserve_exact(count)
            .map_err(|_| "Cannot add organisms: not enough memory.".to_string())?;
        let scatter = x < 0.0 || y < 0.0 || !x.is_finite() || !y.is_finite();
        for _ in 0..count {
            let (cx, cy) = if scatter {
                (self.rng.unit() * WIDTH, self.rng.unit() * HEIGHT)
            } else {
                let a = self.rng.unit() * TAU;
                let r = self.rng.unit().sqrt() * 38.0;
                (wrap(x + a.cos() * r, WIDTH), wrap(y + a.sin() * r, HEIGHT))
            };
            let angle = self.rng.unit() * TAU;
            let (body, budget) = create_body();
            let birth_area = body.area;
            self.cells.push(Cell {
                id: self.next_id,
                genome_id,
                x: cx,
                y: cy,
                vx: angle.cos() * 8.0,
                vy: angle.sin() * 8.0,
                angle,
                energy: budget.energy,
                age: 0.0,
                generation: 0,
                parent_id: 0,
                cooldown: DIVISION_COOLDOWN,
                memory: Registers::default(),
                integrity: 1.0,
                body,
                nutrient_stored: budget.minerals,
                gut_energy: 0.0,
                gut_nutrients: 0.0,
                birth_area,
                photosynthesis_rate: 0.0,
                uptake_rate: 0.0,
                digestion_rate: 0.0,
                growth_rate: 0.0,
                exudation_rate: 0.0,
                construction_rate: 0.0,
                recycling_rate: 0.0,
                repair_rate: 0.0,
                engulf_cooldown: 0.0,
                requested: BodyControls::default(),
                scan_bearing: 0.0,
                scan_width: TAU,
                scan_target: Rc::from(""),
                tracking: Tracking::default(),
            });
            self.nutrient_imports += budget.introduced_minerals;
            self.energy_imports += budget.introduced_energy;
            self.next_id += 1;
        }
        Ok(count)
    }
    fn particle(&mut self, x: f64, y: f64, energy: f64, nutrients: f64) -> bool {
        if self.food.len() >= MAX_FOOD || self.next_food_id >= u64::MAX / 2 {
            return false;
        }
        self.food.push(Food {
            id: self.next_food_id,
            x: wrap(x, WIDTH),
            y: wrap(y, HEIGHT),
            energy,
            nutrients,
        });
        self.next_food_id += 1;
        true
    }
    pub fn add_food(&mut self, x: f64, y: f64, count: usize) -> usize {
        let count = count.min(MAX_FOOD.saturating_sub(self.food.len()));
        let mut added = 0;
        for _ in 0..count {
            let (fx, fy) = if x < 0.0 || y < 0.0 || !x.is_finite() || !y.is_finite() {
                (self.rng.unit() * WIDTH, self.rng.unit() * HEIGHT)
            } else {
                let a = self.rng.unit() * TAU;
                let r = self.rng.unit().sqrt() * 45.0;
                (wrap(x + a.cos() * r, WIDTH), wrap(y + a.sin() * r, HEIGHT))
            };
            let energy = 18.0 + self.rng.unit() * 6.0;
            if !self.particle(fx, fy, energy, FOOD_NUTRIENTS) {
                break;
            }
            self.nutrient_imports += FOOD_NUTRIENTS;
            self.energy_imports += energy;
            added += 1;
        }
        added
    }
    pub fn set_food_rate(&mut self, rate: f64) {
        self.food_rate = if rate.is_finite() {
            rate.clamp(0.0, 60.0)
        } else {
            0.0
        };
    }
    pub fn set_mutation_rate(&mut self, rate: f64) {
        self.mutation_rate = if rate.is_finite() {
            rate.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
    pub fn sense_all(&self) -> Vec<Inputs> {
        self.observations(None).0
    }
    pub fn sense_cell(&self, id: u32) -> Option<Inputs> {
        let i = self.cells.iter().position(|c| c.id == id && c.alive())?;
        self.observations(Some(i)).0.into_iter().next()
    }
    fn observations(&self, selected: Option<usize>) -> (Vec<Inputs>, Vec<Tracking>) {
        let mut cell_grid = Grid::new();
        let mut food_grid = Grid::new();
        let mut max_radius: f64 = 0.0;
        for (i, c) in self.cells.iter().enumerate() {
            if c.alive() {
                cell_grid.insert(c.x, c.y, i);
                max_radius = max_radius.max(c.radius());
            }
        }
        for (i, f) in self.food.iter().enumerate() {
            food_grid.insert(f.x, f.y, i);
        }
        let mut result = Vec::new();
        let mut tracks = Vec::new();
        for (index, cell) in self.cells.iter().enumerate() {
            if selected.is_some_and(|which| which != index) {
                continue;
            }
            let caps = cell.body.capacities();
            let space = cell.body.space();
            let surface = cell.body.surface();
            let mut input = Inputs {
                energy: cell.energy,
                energy_fraction: fraction(cell.energy, caps.energy),
                energy_capacity: caps.energy,
                age: cell.age,
                time: self.time(),
                speed: cell.vx.hypot(cell.vy),
                id: cell.id as f64,
                integrity: cell.integrity,
                memory_used: cell.memory.populated_count() as f64,
                memory_cost: cell.memory.populated_count() as f64 * MEMORY_REGISTER_COST,
                body_area: cell.body.area,
                body_mass: cell.mass(),
                body_birth_area: cell.birth_area,
                body_maturity: cell.maturity(),
                body_division_ready: cell.division_ready(),
                body_space_free: space.free,
                body_space_used: space.used,
                body_space_capacity: space.capacity,
                body_surface_free: surface.free,
                body_surface_used: surface.used,
                body_surface_capacity: surface.capacity,
                nutrient_stored: cell.nutrient_stored,
                nutrient_capacity: caps.minerals,
                nutrient_fraction: fraction(cell.nutrient_stored, caps.minerals),
                gut_energy: cell.gut_energy,
                gut_capacity: caps.gut_energy,
                gut_fraction: fraction(cell.gut_energy, caps.gut_energy),
                digestion_rate: cell.digestion_rate,
                growth_rate: cell.growth_rate,
                lifespan: LIFESPAN,
                exudation_rate: cell.exudation_rate,
                ..Inputs::default()
            };
            for (i, kind) in Kind::ALL.iter().copied().enumerate() {
                input.organelle[i] = OrganelleInput {
                    count: cell.body.count(kind).min(i32::MAX as u32) as i32,
                    mass: cell.body.mass(kind),
                    capacity: cell.body.capacity(kind),
                    damaged: cell.body.damaged(kind),
                    building: cell.body.building(kind),
                };
            }
            let bearing = |(x, y): (f64, f64)| {
                if x.hypot(y) < 1e-12 {
                    0.0
                } else {
                    relative_angle(y.atan2(x) - cell.angle)
                }
            };
            let (lg, hg) = self.environment.gradients(cell.x, cell.y);
            if caps.light_sensing > 0.0 {
                input.light_available = true;
                input.light_valid = true;
                input.light_level = measure(
                    self.environment.light_at(cell.x, cell.y),
                    caps.light_sensing,
                    0.0005,
                )
                .clamp(0.0, 1.0);
                input.light_bearing = measure(bearing(lg), caps.light_sensing, 0.002);
            }
            if caps.heat_sensing > 0.0 {
                input.heat_available = true;
                input.heat_valid = true;
                input.heat_value = measure(
                    self.environment.temperature_at(cell.x, cell.y),
                    caps.heat_sensing,
                    0.0005,
                )
                .clamp(0.0, 1.0);
                input.heat_bearing = measure(bearing(hg), caps.heat_sensing, 0.002);
            }
            if caps.chemical_sensing > 0.0 {
                input.chemical_available = true;
                input.chemical_valid = true;
                input.chemical_mineral = measure(
                    self.environment.nutrient_at(cell.x, cell.y),
                    caps.chemical_sensing,
                    0.001,
                )
                .max(0.0);
                input.chemical_organic = measure(
                    self.environment.organic_at(cell.x, cell.y),
                    caps.chemical_sensing,
                    0.001,
                )
                .max(0.0);
                input.chemical_mineral_bearing = measure(
                    bearing(self.environment.nutrient_gradient(cell.x, cell.y)),
                    caps.chemical_sensing,
                    0.002,
                );
                input.chemical_organic_bearing = measure(
                    bearing(self.environment.organic_gradient(cell.x, cell.y)),
                    caps.chemical_sensing,
                    0.002,
                );
            }
            if caps.mechanical_sensing > 0.0 {
                let (vx, vy) = self.environment.flow_at(cell.x, cell.y, self.time());
                input.flow_available = true;
                input.flow_valid = true;
                input.contact_available = true;
                input.flow_vector = local_vector(vx - cell.vx, vy - cell.vy, cell.angle)
                    .map(|v| measure(v, caps.mechanical_sensing, 0.03));
                input.flow_speed = input.flow_vector[0].hypot(input.flow_vector[1]);
                input.flow_bearing = if input.flow_speed > 1e-12 {
                    input.flow_vector[1].atan2(input.flow_vector[0])
                } else {
                    0.0
                };
            }
            input.object_available = caps.optical_sensing > 0.0;
            let mut tracking = cell.tracking.clone();
            tracking
                .entries
                .retain(|e| self.ticks.saturating_sub(e.seen) <= TRACK_TTL);
            let requested = tracking.entity(&cell.scan_target, cell.id, self.ticks);
            let units = cell
                .body
                .organs
                .iter()
                .filter(|o| o.kind == Kind::Optical && o.functional_mass() > 0.0)
                .count();
            let width = cell.scan_width.min(units as f64 * PI * 0.65).min(TAU);
            let mut object: Option<(Observation, f64)> = None;
            let mut contact: Option<Observation> = None;
            if input.object_available || input.contact_available {
                let mut consider = |mut o: Observation| {
                    let distance = o.dx.hypot(o.dy);
                    let gap = distance - cell.radius() - o.radius;
                    if input.contact_available
                        && gap <= 0.9
                        && contact.as_ref().is_none_or(|old| {
                            gap < old.dx.hypot(old.dy) - cell.radius() - old.radius
                                || (gap == old.dx.hypot(old.dy) - cell.radius() - old.radius
                                    && o.entity < old.entity)
                        })
                    {
                        contact = Some(o.clone());
                    }
                    let angle = relative_angle(o.dy.atan2(o.dx) - cell.angle - cell.scan_bearing);
                    if input.object_available
                        && width > 0.0
                        && distance <= caps.optical_range + o.radius
                        && angle.abs() <= width * 0.5
                    {
                        // Visibility depends on light arriving from the observed
                        // surface, not whether the observer itself is in shadow.
                        let illumination = self
                            .environment
                            .light_at(wrap(cell.x + o.dx, WIDTH), wrap(cell.y + o.dy, HEIGHT))
                            .clamp(0.0, 1.0);
                        o.contrast *= illumination;
                        let threshold = 0.005 / (1.0 + 10.0 * caps.optical_sensing.sqrt());
                        if o.contrast < threshold
                            || distance > caps.optical_range * illumination.sqrt() + o.radius
                        {
                            return;
                        }
                        // Ranking is wholly optical: angle, contrast, distance.
                        // A remembered token only wins when currently observable.
                        let score = if requested == Some(o.entity) {
                            1e9
                        } else {
                            o.contrast / (10.0 + distance) / (1.0 + angle.abs())
                        };
                        if object.as_ref().is_none_or(|(old, best)| {
                            score > *best || (score == *best && o.entity < old.entity)
                        }) && self.optical_line_clear(index, &o, &cell_grid, max_radius)
                        {
                            object = Some((o, score));
                        }
                    }
                };
                let reach = (caps.optical_range + max_radius).max(cell.radius() + max_radius + 1.0);
                for i in cell_grid.nearby_radius(cell.x, cell.y, reach) {
                    let c = &self.cells[i];
                    if c.id == cell.id || !c.alive() {
                        continue;
                    }
                    let pigment = (c.body.functional_mass(Kind::Photosynthetic)
                        / c.body.structural_mass()
                        * 2.0)
                        .clamp(0.0, 1.0);
                    consider(Observation {
                        entity: (c.id as u64) << 1,
                        dx: delta(c.x - cell.x, WIDTH),
                        dy: delta(c.y - cell.y, HEIGHT),
                        radius: c.radius(),
                        area: c.body.area,
                        contrast: 0.45 + 0.4 * pigment,
                        pigment,
                        vx: c.vx - cell.vx,
                        vy: c.vy - cell.vy,
                        resistance: c.body.capacities().protection + 0.15,
                    });
                }
                for i in food_grid.nearby_radius(
                    cell.x,
                    cell.y,
                    caps.optical_range.max(cell.radius()) + 3.5,
                ) {
                    let f = &self.food[i];
                    let (vx, vy) = self.environment.flow_at(f.x, f.y, self.time());
                    consider(Observation {
                        entity: (f.id << 1) | 1,
                        dx: delta(f.x - cell.x, WIDTH),
                        dy: delta(f.y - cell.y, HEIGHT),
                        radius: 2.5,
                        area: (2.5 / CELL_RADIUS).powi(2),
                        contrast: 0.35,
                        pigment: 0.1,
                        vx: vx - cell.vx,
                        vy: vy - cell.vy,
                        resistance: 0.05,
                    });
                }
            }
            if let Some(o) = contact {
                input.contact_token = tracking.observe(o.entity, cell.id, self.ticks);
                input.contact_valid = !input.contact_token.is_empty();
                if input.contact_valid {
                    input.contact_offset = local_vector(o.dx, o.dy, cell.angle)
                        .map(|v| measure(v, caps.mechanical_sensing, 0.08));
                    input.contact_pressure = measure(
                        (cell.radius() + o.radius - o.dx.hypot(o.dy)).max(0.0)
                            + (-o.vx * o.dx - o.vy * o.dy).max(0.0) / (1.0 + o.dx.hypot(o.dy)),
                        caps.mechanical_sensing,
                        0.01,
                    );
                    input.contact_resistance =
                        measure(o.resistance, caps.mechanical_sensing, 0.002);
                }
            }
            if let Some((o, _)) = object {
                input.object_token = tracking.observe(o.entity, cell.id, self.ticks);
                input.object_valid = !input.object_token.is_empty();
                if input.object_valid {
                    input.object_offset = local_vector(o.dx, o.dy, cell.angle)
                        .map(|v| measure(v, caps.optical_sensing, 0.08));
                    input.object_area = measure(o.area, caps.optical_sensing, 0.002).max(0.02);
                    input.object_contrast =
                        measure(o.contrast, caps.optical_sensing, 0.001).clamp(0.0, 1.0);
                    input.object_pigment =
                        measure(o.pigment, caps.optical_sensing, 0.001).clamp(0.0, 1.0);
                    input.object_motion = local_vector(o.vx, o.vy, cell.angle)
                        .map(|v| measure(v, caps.optical_sensing, 0.03));
                }
            }
            result.push(input);
            tracks.push(tracking);
        }
        (result, tracks)
    }
    /// Center-ray occlusion on the same wrapped geometry as sensing/collision.
    /// Query the ray's local bounding disk, then test each actual cell circle.
    /// The observed cell and observer do not occlude themselves; a particle
    /// inside another cell is hidden even if its center is at the ray endpoint.
    fn optical_line_clear(
        &self,
        observer: usize,
        target: &Observation,
        grid: &Grid,
        max_radius: f64,
    ) -> bool {
        let cell = &self.cells[observer];
        let length_squared = target.dx * target.dx + target.dy * target.dy;
        let midpoint_x = wrap(cell.x + target.dx * 0.5, WIDTH);
        let midpoint_y = wrap(cell.y + target.dy * 0.5, HEIGHT);
        let reach = length_squared.sqrt() * 0.5 + max_radius;
        for index in grid.nearby_radius(midpoint_x, midpoint_y, reach) {
            let occluder = &self.cells[index];
            if index == observer || (occluder.id as u64) << 1 == target.entity || !occluder.alive()
            {
                continue;
            }
            let dx = delta(occluder.x - cell.x, WIDTH);
            let dy = delta(occluder.y - cell.y, HEIGHT);
            let t = if length_squared > 1e-12 {
                ((dx * target.dx + dy * target.dy) / length_squared).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let nearest_x = dx - t * target.dx;
            let nearest_y = dy - t * target.dy;
            if nearest_x * nearest_x + nearest_y * nearest_y + 1e-9 < occluder.radius().powi(2) {
                return false;
            }
        }
        true
    }
    pub fn step(&mut self, genomes: &mut Vec<Genome>) {
        self.try_step(genomes)
            .expect("Unable to advance simulation")
    }
    pub fn try_step(&mut self, genomes: &mut Vec<Genome>) -> Result<(), String> {
        let remaining = (u32::MAX - self.next_id) as usize;
        // Births require readiness in the frozen starting snapshot. Reserve the
        // maximum eligible set before fields, memory, RNG or resources advance.
        // This rare check keeps identifier exhaustion an atomic paused tick.
        if remaining < self.cells.len() || self.cells.iter().any(|c| c.generation == u32::MAX) {
            let eligible: Vec<_> = self.cells.iter().filter(|c| c.division_ready()).collect();
            if eligible.len() > remaining || eligible.iter().any(|c| c.generation == u32::MAX) {
                return Err(
                    "Simulation paused: the cell ancestry identifier sequence is exhausted.".into(),
                );
            }
        }
        let possible = self.cells.len().min(remaining);
        self.cells.try_reserve(possible).map_err(|_| {
            "Simulation paused: not enough memory for population growth.".to_string()
        })?;
        if self.next_food_id >= u64::MAX / 2
            || self
                .cells
                .iter()
                .any(|c| c.tracking.next_serial == u64::MAX || c.body.next_organelle_id == u32::MAX)
        {
            return Err(
                "Simulation paused: an object or organelle identifier sequence is exhausted."
                    .into(),
            );
        }
        self.environment.step(self.time());
        self.decay_food();
        let (inputs, tracking) = self.observations(None);
        let mut scratch = EvalScratch::default();
        let caps: Vec<_> = self.cells.iter().map(|c| c.body.capacities()).collect();
        let outputs: Vec<_> = self
            .cells
            .iter_mut()
            .zip(&inputs)
            .zip(tracking)
            .map(|((c, input), tracks)| {
                c.tracking = tracks;
                genomes[(c.genome_id - 1) as usize].program.evaluate_cell(
                    input,
                    &mut c.memory,
                    &mut scratch,
                )
            })
            .collect();
        let time = self.time();
        for (i, cell) in self.cells.iter_mut().enumerate() {
            let output = &outputs[i];
            let cap = &caps[i];
            cell.photosynthesis_rate = 0.0;
            cell.uptake_rate = 0.0;
            cell.digestion_rate = 0.0;
            cell.growth_rate = 0.0;
            cell.exudation_rate = 0.0;
            cell.construction_rate = 0.0;
            cell.recycling_rate = 0.0;
            cell.repair_rate = 0.0;
            cell.requested = output.body.clone();
            cell.scan_bearing = output.scan_bearing;
            cell.scan_width = output.scan_width;
            cell.scan_target = output.scan_target.clone();
            if !cell.alive() {
                continue;
            }
            cell.age += DT;
            if cell.age >= LIFESPAN {
                cell.integrity = 0.0;
                continue;
            }
            let temperature = self.environment.temperature_at(cell.x, cell.y);
            let (flow_x, flow_y) = self.environment.flow_at(cell.x, cell.y, time);
            let inertia = cell.inertia();
            cell.angle = wrap(
                cell.angle + output.turn * cap.turn / inertia.sqrt() * DT,
                TAU,
            );
            let drag = (-1.8 * (cell.body.area.sqrt() + 0.35 * cell.body.surface().used)
                / inertia.sqrt()
                * DT)
                .exp();
            let acceleration = output.thrust * cap.thrust * 12.0 / inertia;
            cell.vx = flow_x + (cell.vx - flow_x + cell.angle.cos() * acceleration * DT) * drag;
            cell.vy = flow_y + (cell.vy - flow_y + cell.angle.sin() * acceleration * DT) * drag;
            cell.x = wrap(cell.x + cell.vx * DT, WIDTH);
            cell.y = wrap(cell.y + cell.vy * DT, HEIGHT);
            cell.cooldown = (cell.cooldown - DT).max(0.0);
            cell.engulf_cooldown = (cell.engulf_cooldown - DT).max(0.0);
            let maintenance = (cell.basal_metabolism() * metabolic_factor(temperature)
                + inputs[i].memory_cost
                + 0.06 * cap.thrust * output.thrust
                + 0.025 * cap.photosynthesis * output.photosynthesize
                + 1.5 * cap.piercing * output.bite
                + 0.3 * cap.protection * output.defend)
                * DT;
            cell.energy = (cell.energy - maintenance).max(0.0);
            let stress = (0.15 - temperature).max(temperature - 0.85).max(0.0)
                * 0.3
                * (1.0 - cap.protection)
                * DT;
            cell.integrity = (cell.integrity - stress).max(0.0);
            cell.body.damage(stress);
        }
        let mut processing: Vec<_> = caps.iter().map(|c| c.metabolism * DT).collect();
        self.digest(&outputs, &caps, &mut processing);
        self.absorb_nutrients(&outputs, &caps);
        for food in &mut self.food {
            let (vx, vy) = self.environment.flow_at(food.x, food.y, time);
            food.x = wrap(food.x + vx * DT, WIDTH);
            food.y = wrap(food.y + vy * DT, HEIGHT);
        }
        self.resolve_collisions();
        self.eat(&outputs, &caps);
        self.photosynthesize(&outputs, &caps, &mut processing);
        let consumed = self.resolve_predation(&outputs, &caps);
        for (i, cell) in self.cells.iter_mut().enumerate() {
            if !cell.alive() {
                continue;
            }
            let output = &outputs[i];
            // Repair the envelope explicitly; organelle repair is handled by the
            // same paid physical body worker below. Both preserve material.
            let repaired = (caps[i].repair * output.repair * DT)
                .min(1.0 - cell.integrity)
                .min(
                    (cell.energy - 0.15 * cell.energy_capacity()).max(0.0)
                        / (20.0 * cell.body.area),
                );
            cell.energy -= repaired * 20.0 * cell.body.area;
            cell.integrity += repaired;
            let report = cell.body.work(
                output.grow,
                &output.body,
                output.repair,
                &mut cell.energy,
                &mut cell.nutrient_stored,
                DT,
                MAX_AREA,
            );
            cell.growth_rate = report.area_grown / DT;
            cell.construction_rate = report.constructed_mass / DT;
            cell.recycling_rate = report.recycled_mass / DT;
            cell.repair_rate = report.repaired_mass / DT;
        }
        self.spill_excess_stores();
        let originals = self.cells.len();
        for i in 0..originals {
            if !outputs[i].divide
                || !inputs[i].body_division_ready
                || !self.cells[i].division_ready()
            {
                continue;
            }
            debug_assert!(self.next_id < u32::MAX && self.cells[i].generation < u32::MAX);
            let Some((parent_body, daughter_body)) = self.cells[i].body.partition() else {
                continue;
            };
            let cell = &mut self.cells[i];
            let split_angle = cell.angle + PI * 0.5 + (self.rng.unit() - 0.5) * 0.6;
            let radius = CELL_RADIUS * (cell.body.area * 0.5).sqrt();
            let dx = split_angle.cos() * (radius + 0.3);
            let dy = split_angle.sin() * (radius + 0.3);
            let energy = (cell.energy - DIVISION_COST * cell.body.area) * 0.5;
            let mut genome_id = cell.genome_id;
            if self.mutation_rate > 0.0 {
                let seed = mutation_seed(self.seed, self.ticks, cell.id, self.next_id);
                if (mix64(seed) >> 11) as f64 / ((1u64 << 53) as f64) < self.mutation_rate {
                    let mutation =
                        mutate_registered(genomes, cell.genome_id, seed, MutationKind::Any);
                    if mutation.skipped.is_some() {
                        self.mutations_skipped += 1;
                    } else {
                        genome_id = mutation.genome_id;
                        self.mutations += 1;
                    }
                }
            }
            let mut daughter = cell.clone();
            daughter.id = self.next_id;
            daughter.genome_id = genome_id;
            daughter.parent_id = cell.id;
            daughter.generation += 1;
            daughter.age = 0.0;
            daughter.memory = Registers::default();
            daughter.tracking = Tracking::default();
            daughter.scan_target = Rc::from("");
            daughter.body = daughter_body;
            daughter.birth_area = daughter.body.area;
            daughter.energy = energy;
            daughter.nutrient_stored *= 0.5;
            daughter.gut_energy *= 0.5;
            daughter.gut_nutrients *= 0.5;
            daughter.x = wrap(cell.x + dx, WIDTH);
            daughter.y = wrap(cell.y + dy, HEIGHT);
            daughter.vx += split_angle.cos() * 3.0;
            daughter.vy += split_angle.sin() * 3.0;
            daughter.angle = wrap(cell.angle + 0.5, TAU);
            daughter.cooldown = DIVISION_COOLDOWN;
            daughter.photosynthesis_rate = 0.0;
            daughter.uptake_rate = 0.0;
            daughter.digestion_rate = 0.0;
            daughter.growth_rate = 0.0;
            daughter.exudation_rate = 0.0;
            daughter.construction_rate = 0.0;
            daughter.recycling_rate = 0.0;
            daughter.repair_rate = 0.0;
            cell.body = parent_body;
            cell.birth_area = cell.body.area;
            cell.energy = energy;
            cell.nutrient_stored *= 0.5;
            cell.gut_energy *= 0.5;
            cell.gut_nutrients *= 0.5;
            cell.x = wrap(cell.x - dx, WIDTH);
            cell.y = wrap(cell.y - dy, HEIGHT);
            cell.vx -= split_angle.cos() * 3.0;
            cell.vy -= split_angle.sin() * 3.0;
            cell.cooldown = DIVISION_COOLDOWN;
            self.next_id += 1;
            self.births += 1;
            self.cells.push(daughter);
        }
        // Uneven, discrete compartment inheritance can reduce capacity; transfer
        // overflow to the local environment rather than cloning a compartment.
        self.spill_excess_stores();
        let remains: Vec<_> = self
            .cells
            .iter()
            .enumerate()
            .take(originals)
            .filter(|(i, c)| !c.alive() && !consumed[*i])
            .map(|(_, c)| {
                (
                    c.x,
                    c.y,
                    c.energy.max(0.0) + c.gut_energy + c.body.embodied_energy(),
                    c.mass(),
                )
            })
            .collect();
        for (x, y, energy, minerals) in remains {
            self.emit_remains(x, y, energy, minerals);
        }
        let before = self.cells.len();
        self.cells.retain(Cell::alive);
        self.deaths += (before - self.cells.len()) as u64;
        self.food_credit += self.food_rate * DT;
        let food = self.food_credit.floor() as usize;
        self.food_credit -= food as f64;
        self.add_food(-1.0, -1.0, food);
        self.ticks += 1;
        Ok(())
    }
    fn decay_food(&mut self) {
        let retained = (-FOOD_DECAY * DT).exp();
        self.food.retain_mut(|food| {
            let lost_energy = food.energy * (1.0 - retained);
            let lost_minerals = food.nutrients * (1.0 - retained);
            self.environment
                .emit_organic(food.x, food.y, lost_energy * 0.08);
            self.environment
                .recycle_nutrients(food.x, food.y, lost_minerals);
            food.energy *= retained;
            food.nutrients *= retained;
            if food.energy <= 1e-6 {
                self.environment
                    .recycle_nutrients(food.x, food.y, food.nutrients);
                false
            } else {
                true
            }
        });
    }
    fn absorb_nutrients(&mut self, outputs: &[Outputs], caps: &[BodyCapacities]) {
        let mut groups: Vec<Vec<(usize, f64)>> = vec![Vec::new(); PATCHES];
        for (i, c) in self.cells.iter().enumerate() {
            if !c.alive() {
                continue;
            }
            let request = (outputs[i].absorb * caps[i].uptake * DT)
                .min((caps[i].minerals - c.nutrient_stored).max(0.0))
                .min((c.energy - 0.05 * caps[i].energy).max(0.0));
            if request > 0.0 {
                groups[Environment::patch(c.x, c.y)].push((i, request));
            }
        }
        for (patch, group) in groups.iter_mut().enumerate() {
            if group.is_empty() {
                continue;
            }
            group.sort_by_key(|&(i, _)| self.cells[i].id);
            let demand: f64 = group.iter().map(|(_, v)| v).sum();
            let available = self.environment.take_nutrients(patch, demand);
            let scale = (available / demand).min(1.0);
            let mut remaining = available;
            for &(i, requested) in group.iter() {
                let amount = (requested * scale).min(remaining);
                remaining -= amount;
                self.cells[i].nutrient_stored += amount;
                self.cells[i].energy -= amount;
                self.cells[i].uptake_rate = amount / DT;
            }
            let c = &self.cells[group[0].0];
            self.environment.recycle_nutrients(c.x, c.y, remaining);
        }
    }
    fn digest(&mut self, outputs: &[Outputs], caps: &[BodyCapacities], processing: &mut [f64]) {
        for (i, c) in self.cells.iter_mut().enumerate() {
            if !c.alive() || outputs[i].feed <= 0.0 || c.gut_energy <= 0.0 {
                continue;
            }
            let room = (caps[i].energy - c.energy).max(0.0);
            let mineral_room = (caps[i].minerals - c.nutrient_stored).max(0.0);
            let mineral_demand = if c.gut_nutrients > 0.0 {
                mineral_room / c.gut_nutrients * c.gut_energy
            } else {
                0.0
            };
            let raw = (caps[i].digestion * outputs[i].feed * DT)
                .min(processing[i])
                .min(c.gut_energy)
                .min((room / ASSIMILATION_EFFICIENCY).max(mineral_demand));
            if raw <= 0.0 {
                continue;
            }
            let minerals = c.gut_nutrients * (raw / c.gut_energy);
            let useful = (raw * ASSIMILATION_EFFICIENCY).min(room);
            c.gut_energy = (c.gut_energy - raw).max(0.0);
            c.gut_nutrients = (c.gut_nutrients - minerals).max(0.0);
            c.energy += useful;
            c.digestion_rate = useful / DT;
            processing[i] = (processing[i] - raw).max(0.0);
            let overflow = c.receive_nutrients(minerals);
            self.environment.recycle_nutrients(c.x, c.y, overflow);
            self.environment
                .emit_organic(c.x, c.y, (raw - useful) * 0.01);
        }
    }
    fn spill_excess_stores(&mut self) {
        let mut spills = Vec::new();
        for c in &mut self.cells {
            if !c.alive() {
                continue;
            }
            let cap = c.body.capacities();
            let excess = (c.nutrient_stored - cap.minerals).max(0.0);
            c.nutrient_stored -= excess;
            self.environment.recycle_nutrients(c.x, c.y, excess);
            let energy = (c.energy - cap.energy).max(0.0);
            c.energy -= energy;
            let kept = if c.gut_energy > 0.0 {
                (cap.gut_energy / c.gut_energy).min(1.0)
            } else {
                1.0
            }
            .min(if c.gut_nutrients > 0.0 {
                (cap.gut_minerals / c.gut_nutrients).min(1.0)
            } else {
                1.0
            });
            let gut_energy = c.gut_energy * (1.0 - kept);
            let minerals = c.gut_nutrients * (1.0 - kept);
            c.gut_energy *= kept;
            c.gut_nutrients *= kept;
            if energy + gut_energy > 0.0 || minerals > 0.0 {
                spills.push((c.x, c.y, energy + gut_energy, minerals));
            }
        }
        for (x, y, energy, minerals) in spills {
            self.emit_remains(x, y, energy, minerals);
        }
    }
    fn photosynthesize(
        &mut self,
        outputs: &[Outputs],
        caps: &[BodyCapacities],
        processing: &mut [f64],
    ) {
        let mut requests = Vec::new();
        let mut totals = vec![0.0; PATCHES];
        for (i, c) in self.cells.iter().enumerate() {
            if !c.alive() {
                continue;
            }
            let capacity =
                (caps[i].photosynthesis * outputs[i].photosynthesize * DT).min(processing[i]);
            if capacity <= 0.0 {
                continue;
            }
            for (patch, weight) in photon_footprint(c.x, c.y, c.body.area) {
                let request = capacity * weight * self.environment.patch_light(patch);
                if request > 0.0 {
                    requests.push((i, patch, request));
                    totals[patch] += request;
                }
            }
        }
        let mut gains = vec![0.0; self.cells.len()];
        for (i, patch, request) in requests {
            let supply =
                PHOTO_HABITAT_POWER / PATCHES as f64 * self.environment.patch_light(patch) * DT;
            gains[i] += request * (supply / totals[patch]).min(1.0);
        }
        let mut exudates = Vec::new();
        for (i, c) in self.cells.iter_mut().enumerate() {
            let gain = gains[i];
            if gain <= 0.0 {
                continue;
            }
            let retained =
                (gain * (1.0 - PHOTO_EXUDATION)).min((caps[i].energy - c.energy).max(0.0));
            c.energy += retained;
            c.photosynthesis_rate = gain / DT;
            processing[i] = (processing[i] - gain).max(0.0);
            self.photosynthesis_total += gain;
            exudates.push((i, c.x, c.y, c.radius(), c.angle, gain - retained));
        }
        for (i, x, y, r, a, energy) in exudates {
            self.cells[i].exudation_rate = self.emit_exudate(x, y, r, a, energy) / DT;
        }
    }
    fn physical_targets(&self, caps: &[BodyCapacities]) -> Vec<Option<usize>> {
        let mut grid = Grid::new();
        let mut max_radius: f64 = 0.0;
        for (i, c) in self.cells.iter().enumerate() {
            if c.alive() {
                grid.insert(c.x, c.y, i);
                max_radius = max_radius.max(c.radius());
            }
        }
        self.cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if !c.alive() || (caps[i].capture == 0.0 && caps[i].piercing == 0.0) {
                    return None;
                }
                let reach = 0.75 + 12.0 * caps[i].piercing.sqrt();
                let mut target = None;
                let mut best = f64::INFINITY;
                for j in grid.nearby_radius(c.x, c.y, c.radius() + max_radius + reach) {
                    let b = &self.cells[j];
                    if i == j || !b.alive() {
                        continue;
                    }
                    let dx = delta(b.x - c.x, WIDTH);
                    let dy = delta(b.y - c.y, HEIGHT);
                    let gap = dx.hypot(dy) - c.radius() - b.radius();
                    if gap > reach || relative_angle(dy.atan2(dx) - c.angle).abs() > PI * 0.6 {
                        continue;
                    }
                    if gap < best
                        || (gap == best
                            && target.is_none_or(|old: usize| b.id < self.cells[old].id))
                    {
                        best = gap;
                        target = Some(j);
                    }
                }
                target
            })
            .collect()
    }
    fn resolve_predation(&mut self, outputs: &[Outputs], caps: &[BodyCapacities]) -> Vec<bool> {
        let targets = self.physical_targets(caps);
        let mut consumed = vec![false; self.cells.len()];
        let mut order: Vec<_> = (0..self.cells.len()).collect();
        order.sort_by(|&a, &b| {
            self.cells[b]
                .body
                .area
                .total_cmp(&self.cells[a].body.area)
                .then_with(|| self.cells[a].id.cmp(&self.cells[b].id))
        });
        for i in order {
            if !outputs[i].engulf
                || outputs[i].feed <= 0.0
                || !self.cells[i].alive()
                || caps[i].capture <= 0.0
                || self.cells[i].engulf_cooldown > 0.0
            {
                continue;
            }
            let Some(j) = targets[i] else {
                continue;
            };
            let a = &self.cells[i];
            let b = &self.cells[j];
            if !b.alive()
                || b.integrity > 0.35
                || a.body.area < 1.6 * b.body.area
                || b.body.area > caps[i].capture * 40.0
            {
                continue;
            }
            let distance = delta(b.x - a.x, WIDTH).hypot(delta(b.y - a.y, HEIGHT));
            if distance > a.radius() + b.radius() + 0.75 {
                continue;
            }
            let cost = 8.0 * b.body.area;
            let energy = b.energy + b.gut_energy + b.body.embodied_energy();
            let minerals = b.mass();
            if a.energy <= cost || a.gut_fraction_for(energy, minerals, &caps[i]) < 1.0 {
                continue;
            }
            self.cells[i].energy -= cost;
            self.cells[i].gut_energy += energy;
            self.cells[i].gut_nutrients += minerals;
            self.cells[i].engulf_cooldown = 2.0;
            self.cells[j].energy = 0.0;
            self.cells[j].integrity = 0.0;
            consumed[j] = true;
            self.predations += 1;
        }
        let mut events = Vec::new();
        for i in 0..self.cells.len() {
            if !self.cells[i].alive() || outputs[i].bite <= 0.0 || caps[i].piercing <= 0.0 {
                continue;
            }
            let Some(j) = targets[i] else {
                continue;
            };
            if !self.cells[j].alive() {
                continue;
            }
            let protection = (caps[j].protection * (0.5 + 0.5 * outputs[j].defend)).min(0.9);
            let inflicted = 0.12
                * (caps[i].piercing / 0.06)
                * outputs[i].bite
                * (self.cells[i].mass() / self.cells[j].mass()).min(2.0)
                * (1.0 - protection)
                * DT;
            if inflicted > 0.0 {
                events.push((j, i, inflicted));
                self.attacks += 1;
            }
        }
        events.sort_by_key(|&(j, i, _)| (self.cells[j].id, self.cells[i].id));
        let mut damage = vec![0.0; self.cells.len()];
        let mut removed = vec![(0.0, 0.0); self.cells.len()];
        let mut meals = vec![(0.0, 0.0); self.cells.len()];
        let mut start = 0;
        while start < events.len() {
            let victim = events[start].0;
            let mut end = start + 1;
            while end < events.len() && events[end].0 == victim {
                end += 1;
            }
            let group = &events[start..end];
            let total: f64 = group.iter().map(|e| e.2).sum();
            let actual = total.min(self.cells[victim].integrity);
            damage[victim] = actual;
            let fraction = if total > 0.0 { actual / total } else { 0.0 };
            let victim_cell = &self.cells[victim];
            let requests: Vec<_> = group
                .iter()
                .map(|&(_, attacker, inflicted)| {
                    let raw = 80.0
                        * inflicted
                        * fraction
                        * victim_cell.body.structural_mass()
                        * outputs[attacker].feed;
                    let minerals = if victim_cell.energy > 0.0 {
                        victim_cell.nutrient_stored * raw / victim_cell.energy
                    } else {
                        0.0
                    };
                    raw * self.cells[attacker].gut_fraction_for(raw, minerals, &caps[attacker])
                })
                .collect();
            let demand: f64 = requests.iter().sum();
            let pool = demand.min(victim_cell.energy);
            let scale = if demand > 0.0 { pool / demand } else { 0.0 };
            let mineral_ratio = if victim_cell.energy > 0.0 {
                victim_cell.nutrient_stored / victim_cell.energy
            } else {
                0.0
            };
            for ((_, attacker, _), request) in group.iter().zip(requests) {
                let energy = request * scale;
                let minerals = energy * mineral_ratio;
                meals[*attacker].0 += energy;
                meals[*attacker].1 += minerals;
                removed[victim].0 += energy;
                removed[victim].1 += minerals;
            }
            start = end;
        }
        for (i, c) in self.cells.iter_mut().enumerate() {
            if !c.alive() {
                continue;
            }
            c.energy = (c.energy - removed[i].0).max(0.0);
            c.nutrient_stored = (c.nutrient_stored - removed[i].1).max(0.0);
            c.gut_energy += meals[i].0;
            c.gut_nutrients += meals[i].1;
            c.integrity = (c.integrity - damage[i]).max(0.0);
            c.body.damage(damage[i]);
            if damage[i] > 0.0 {
                self.environment.emit_organic(c.x, c.y, damage[i] * 2.0);
                if !c.alive() {
                    self.predations += 1;
                }
            }
        }
        consumed
    }
    fn emit_exudate(&mut self, x: f64, y: f64, radius: f64, angle: f64, mut energy: f64) -> f64 {
        let px = wrap(x + (radius + 2.5) * angle.cos(), WIDTH);
        let py = wrap(y + (radius + 2.5) * angle.sin(), HEIGHT);
        let requested = energy;
        self.environment.emit_organic(px, py, energy * 0.08);
        for food in &mut self.food {
            if delta(food.x - px, WIDTH).hypot(delta(food.y - py, HEIGHT)) > 24.0
                || delta(food.x - x, WIDTH).hypot(delta(food.y - y, HEIGHT)) < radius
            {
                continue;
            }
            let deposited = energy.min((24.0 - food.energy).max(0.0));
            food.energy += deposited;
            energy -= deposited;
            if energy <= 0.0 {
                return requested;
            }
        }
        while energy > 0.0 {
            let amount = energy.min(24.0);
            if !self.particle(px, py, amount, 0.0) {
                break;
            }
            energy -= amount;
        }
        requested - energy
    }
    fn emit_remains(&mut self, x: f64, y: f64, mut energy: f64, mut minerals: f64) {
        self.environment.emit_organic(x, y, energy * 0.08);
        let mut n = 0;
        while energy > 0.0 {
            let amount = energy.min(24.0);
            let material = if amount == energy {
                minerals
            } else {
                minerals * amount / energy
            };
            let angle = n as f64 * 2.399963229728653;
            let radius = (n as f64).sqrt() * 2.0;
            if !self.particle(
                x + angle.cos() * radius,
                y + angle.sin() * radius,
                amount,
                material,
            ) {
                break;
            }
            energy -= amount;
            minerals = (minerals - material).max(0.0);
            n += 1;
        }
        self.environment.recycle_nutrients(x, y, minerals);
    }
    fn resolve_collisions(&mut self) {
        let mut grid = Grid::new();
        let mut max_radius: f64 = 0.0;
        for (i, c) in self.cells.iter().enumerate() {
            if c.alive() {
                grid.insert(c.x, c.y, i);
                max_radius = max_radius.max(c.radius());
            }
        }
        for i in 0..self.cells.len() {
            if !self.cells[i].alive() {
                continue;
            }
            let mut candidates: Vec<_> = grid
                .nearby_radius(
                    self.cells[i].x,
                    self.cells[i].y,
                    self.cells[i].radius() + max_radius,
                )
                .filter(|&j| j > i)
                .collect();
            candidates.sort_unstable();
            candidates.dedup();
            for j in candidates {
                let (a, b) = self.cells.split_at_mut(j);
                let a = &mut a[i];
                let b = &mut b[0];
                if !b.alive() {
                    continue;
                }
                let dx = delta(b.x - a.x, WIDTH);
                let dy = delta(b.y - a.y, HEIGHT);
                let distance = dx.hypot(dy);
                let contact = a.radius() + b.radius();
                if distance >= contact {
                    continue;
                }
                let (nx, ny) = if distance > 1e-5 {
                    (dx / distance, dy / distance)
                } else {
                    let a = (a.id.wrapping_mul(13) ^ b.id) as f64;
                    (a.cos(), a.sin())
                };
                let ma = a.inertia();
                let mb = b.inertia();
                let overlap = contact - distance;
                a.x = wrap(a.x - nx * overlap * mb / (ma + mb), WIDTH);
                a.y = wrap(a.y - ny * overlap * mb / (ma + mb), HEIGHT);
                b.x = wrap(b.x + nx * overlap * ma / (ma + mb), WIDTH);
                b.y = wrap(b.y + ny * overlap * ma / (ma + mb), HEIGHT);
                let approaching = (b.vx - a.vx) * nx + (b.vy - a.vy) * ny;
                if approaching < 0.0 {
                    let impulse = -approaching / (1.0 / ma + 1.0 / mb);
                    a.vx -= nx * impulse / ma;
                    a.vy -= ny * impulse / ma;
                    b.vx += nx * impulse / mb;
                    b.vy += ny * impulse / mb;
                }
            }
        }
    }
    fn eat(&mut self, outputs: &[Outputs], caps: &[BodyCapacities]) {
        let mut grid = Grid::new();
        let mut max_radius: f64 = 0.0;
        for (i, c) in self.cells.iter().enumerate() {
            if c.alive() {
                grid.insert(c.x, c.y, i);
                max_radius = max_radius.max(c.radius());
            }
        }
        let mut budgets: Vec<_> = caps
            .iter()
            .zip(outputs)
            .map(|(c, o)| c.capture * 1200.0 * o.feed * DT)
            .collect();
        self.food.retain_mut(|food| {
            let mut winner = None;
            let mut best = f64::INFINITY;
            for i in grid.nearby_radius(food.x, food.y, max_radius + 2.5) {
                let c = &self.cells[i];
                if !c.alive()
                    || budgets[i] <= 0.0
                    || c.gut_fraction_for(food.energy, food.nutrients, &caps[i]) <= 0.0
                {
                    continue;
                }
                let distance = delta(c.x - food.x, WIDTH).hypot(delta(c.y - food.y, HEIGHT));
                if distance <= c.radius() + 2.5
                    && (distance < best
                        || (distance == best
                            && winner.is_none_or(|old: usize| c.id < self.cells[old].id)))
                {
                    best = distance;
                    winner = Some(i);
                }
            }
            if let Some(i) = winner {
                let c = &mut self.cells[i];
                let fraction = c
                    .gut_fraction_for(food.energy, food.nutrients, &caps[i])
                    .min(budgets[i] / food.energy)
                    .clamp(0.0, 1.0);
                let energy = food.energy * fraction;
                let minerals = food.nutrients * fraction;
                c.gut_energy += energy;
                c.gut_nutrients += minerals;
                food.energy = (food.energy - energy).max(0.0);
                food.nutrients = (food.nutrients - minerals).max(0.0);
                budgets[i] = (budgets[i] - energy).max(0.0);
                if food.energy <= 1e-6 {
                    self.environment
                        .recycle_nutrients(food.x, food.y, food.nutrients);
                    self.eaten += 1;
                    return false;
                }
            }
            true
        });
    }
    pub fn snapshot(&self, target: &mut Vec<f64>) {
        target.clear();
        target.extend_from_slice(&[
            self.ticks as f64,
            self.time() as f64,
            self.cells.len() as f64,
            self.food.len() as f64,
            self.births as f64,
            self.deaths as f64,
            self.eaten as f64,
            WIDTH as f64,
            HEIGHT as f64,
            0.0,
            MAX_FOOD as f64,
            self.food_rate as f64,
        ]);
        for c in &self.cells {
            let cap = c.body.capacities();
            let space = c.body.space();
            let surface = c.body.surface();
            let used = c.memory.populated_count() as f64;
            let values = [
                c.id as f64,
                c.genome_id as f64,
                c.x,
                c.y,
                c.vx,
                c.vy,
                c.angle,
                c.energy,
                c.age,
                c.generation as f64,
                c.parent_id as f64,
                c.radius(),
                c.body.area,
                c.mass(),
                c.birth_area,
                cap.energy,
                c.basal_metabolism() * metabolic_factor(self.environment.temperature_at(c.x, c.y)),
                used,
                used * MEMORY_REGISTER_COST,
                c.integrity,
                c.photosynthesis_rate,
                c.nutrient_stored,
                cap.minerals,
                c.uptake_rate,
                c.gut_energy,
                cap.gut_energy,
                c.gut_nutrients,
                c.digestion_rate,
                c.growth_rate,
                c.maturity(),
                LIFESPAN,
                c.exudation_rate,
                c.body.upkeep(),
                space.used,
                space.capacity,
                surface.used,
                surface.capacity,
                if c.division_ready() { 1.0 } else { 0.0 },
                cap.optical_range,
            ];
            debug_assert_eq!(values.len(), 39);
            target.extend(values);
            for kind in Kind::ALL {
                target.extend(
                    [
                        c.body.count(kind) as f64,
                        c.body.mass(kind),
                        c.body.capacity(kind),
                        c.body.damaged(kind),
                        c.body.building(kind),
                        c.body.functional_mass(kind),
                    ]
                    .map(|v| v as f64),
                );
            }
        }
        for f in &self.food {
            target.extend([f.x as f64, f.y as f64, f.energy as f64, f.nutrients as f64]);
        }
    }
    pub fn body_json(&self, id: u32) -> Option<String> {
        let c = self.cells.iter().find(|c| c.id == id && c.alive())?;
        let space = c.body.space();
        let surface = c.body.surface();
        let mut json=format!("{{\"cellId\":{},\"area\":{},\"mass\":{},\"spaceUsed\":{},\"spaceCapacity\":{},\"surfaceUsed\":{},\"surfaceCapacity\":{},\"constructionRate\":{},\"recyclingRate\":{},\"repairRate\":{},\"organs\":[",
            c.id,c.body.area,c.mass(),space.used,space.capacity,surface.used,surface.capacity,c.construction_rate,c.recycling_rate,c.repair_rate);
        for (i, o) in c.body.organs.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            let _=write!(json,"{{\"id\":{},\"kind\":\"{}\",\"mass\":{},\"integrity\":{},\"functional\":{},\"minimumMass\":{},\"embodiedEnergy\":{}}}",
                o.id,o.kind.name(),o.mass,o.integrity,o.mass+1e-12>=o.kind.min_mass()&&o.integrity>=0.2,o.kind.min_mass(),o.embodied_energy);
        }
        json.push_str("],\"requested\":[");
        for (i, kind) in Kind::ALL.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            let _ = write!(
                json,
                "{{\"kind\":\"{}\",\"count\":{},\"size\":{},\"build\":{},\"recycle\":{}}}",
                kind.name(),
                c.requested.target_counts[i],
                c.requested.target_sizes[i],
                c.requested.build[i],
                c.requested.recycle[i]
            );
        }
        json.push_str("]}");
        Some(json)
    }
}
#[derive(Clone)]
struct Observation {
    entity: u64,
    dx: f64,
    dy: f64,
    radius: f64,
    area: f64,
    contrast: f64,
    pigment: f64,
    vx: f64,
    vy: f64,
    resistance: f64,
}
fn measure(value: f64, mass: f64, scale: f64) -> f64 {
    let step = scale / mass.max(0.001).sqrt();
    (value / step).round() * step
}
fn local_vector(x: f64, y: f64, angle: f64) -> [f64; 2] {
    [
        x * angle.cos() + y * angle.sin(),
        -x * angle.sin() + y * angle.cos(),
    ]
}
fn fraction(value: f64, capacity: f64) -> f64 {
    if capacity > 0.0 {
        (value / capacity).clamp(0.0, 1.0)
    } else {
        0.0
    }
}
fn metabolic_factor(temperature: f64) -> f64 {
    0.5 + temperature.clamp(0.0, 1.0)
}
fn photon_footprint(x: f64, y: f64, area: f64) -> Vec<(usize, f64)> {
    let radius = (60.0 * area.sqrt()).max(20.0);
    let span = (radius / 20.0).ceil() as isize;
    let center = Environment::patch(x, y);
    let col = (center % FIELD_COLUMNS) as isize;
    let row = (center / FIELD_COLUMNS) as isize;
    let cols = (2 * span + 1).min(FIELD_COLUMNS as isize);
    let rows = (2 * span + 1).min(FIELD_ROWS as isize);
    let mut result = Vec::new();
    let mut total = 0.0;
    for dy in 0..rows {
        for dx in 0..cols {
            let c = (col - span + dx).rem_euclid(FIELD_COLUMNS as isize) as usize;
            let r = (row - span + dy).rem_euclid(FIELD_ROWS as isize) as usize;
            let patch = r * FIELD_COLUMNS + c;
            let (px, py) = Environment::patch_center(patch);
            let scaled =
                (delta(px - x, WIDTH).powi(2) + delta(py - y, HEIGHT).powi(2)) / radius.powi(2);
            if scaled < 1.0 {
                let weight = (1.0 - scaled * scaled).powi(2);
                result.push((patch, weight));
                total += weight;
            }
        }
    }
    for (_, weight) in &mut result {
        *weight /= total;
    }
    result
}
fn wrap(value: f64, size: f64) -> f64 {
    value.rem_euclid(size)
}
fn delta(value: f64, size: f64) -> f64 {
    (value + size * 0.5).rem_euclid(size) - size * 0.5
}
fn relative_angle(angle: f64) -> f64 {
    (angle + PI).rem_euclid(TAU) - PI
}
fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}
fn mutation_seed(world_seed: u32, tick: u64, parent_cell: u32, daughter_cell: u32) -> u64 {
    mix64(
        world_seed as u64
            ^ tick.wrapping_mul(0xD6E8_FEB8_6659_FD93)
            ^ ((parent_cell as u64) << 32)
            ^ daughter_cell as u64,
    )
}

#[cfg(test)]
mod biology_tests;
