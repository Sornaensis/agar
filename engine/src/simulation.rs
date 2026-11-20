//! Fixed-step physical cells. Programs read one immutable snapshot; capabilities
//! come exclusively from paid physical organelles carried into that tick.
#[cfg(test)]
use crate::body::BodyFixture;
use crate::body::{
    Body, BodyCapacities, BodyControls, FounderBudget, OrganelleKind as Kind, FAMILY_COUNT,
};
use crate::cellular::{AssemblyKind, CellularWorkScratch, MechanismKind};
use crate::environment::{delta, wrapped as wrap, Environment, PATCHES};
use crate::genome::{
    EvalScratch, Inputs, MutationKind, OrganelleInput, Outputs, Program, ReceptorInput, Registers, Sensor,
};
use std::f64::consts::{PI, TAU};
use std::fmt::Write;
use std::rc::Rc;
mod adhesion;
mod energetics;
mod development;
mod manipulation;
mod checkpoint;
mod chemistry;
mod substrate;
mod isomers;
mod inlets;
use substrate::Composition;
mod counted;
mod ecology;
#[cfg(test)]
mod count_tests;
mod contact;
use contact::CellSignal;
#[cfg(test)]
mod contact_tests;
#[cfg(test)]
mod chemistry_tests;
mod presentation;
mod receptors;
mod surface;
#[cfg(test)]
mod cellular_tests;
#[cfg(test)]
mod mechanism_tests;
#[cfg(test)]
mod sensing_tests;
#[cfg(test)]
mod local_control_tests;
#[cfg(test)]
mod predation_tests;
#[cfg(test)]
mod foraging_tests;
#[cfg(test)]
mod environment_tests;
#[cfg(test)]
mod performance_tests;
#[cfg(test)]
mod test_substrate;

pub const WIDTH: f64 = 1200.0;
pub const HEIGHT: f64 = 760.0;
pub const DT: f64 = 1.0 / 30.0;
pub const CELL_RADIUS: f64 = 7.0;
// Geometric safety bound, not a carrying capacity: at most one quarter of the
// shorter torus dimension across. Broad phases use actual radii up to this bound.
pub const MAX_AREA: f64 = (HEIGHT / 8.0 / CELL_RADIUS) * (HEIGHT / 8.0 / CELL_RADIUS);
pub const CELL_STRIDE: usize = 39 + FAMILY_COUNT * 6 + 6 + FAMILY_COUNT + 1 + 8;
/// Compatibility signal: zero denotes no age-based death limit.
pub const LIFESPAN: f64 = 0.0;
pub const MEMORY_REGISTER_COST: f64 = 0.0025;
pub const MAX_GENOME_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_GENOMES: usize = 2048;
#[cfg(test)]
const FIXTURE_MINERALS: f64 = 0.25;
/// Consistent abstract stoichiometry. Particle/gut energy is chemical potential,
/// never usable cellular energy. Structural products carry one carbon per mass.
pub const CHEMICAL_ENERGY_PER_CARBON: f64 = 20.0;
pub const RESPIRATION_ENERGY_PER_CARBON: f64 = 16.0;
const STRUCTURAL_CARBON_PER_MASS: f64 = 1.0;
const ORGANIC_PACKET_ENERGY: f64 = 24.0;
const PARTICLE_CAPTURE_RATE: f64 = 1200.0;
// Secretions begin beyond the inclusive particle-capture contact surface.
// The small physical gap prevents rounding-dependent immediate reingestion;
// movement can still bring any cell, including the emitter, back into contact.
const ORGANIC_RELEASE_CLEARANCE: f64 = 0.05;
const DIGESTION_SPEED: [f64; 3] = [1.2, 0.7, 0.35];
const ASSIMILATION_EFFICIENCY: [f64; 3] = [0.8, 0.65, 0.32];
const MINERAL_ASSIMILATION: [f64; 3] = [1.0, 0.8, 0.5];
const LIGAND: usize = 3;
const ORGANIC_DECAY: [f64; 4] = [0.04, 0.012, 0.006, 0.012];
const PHOTO_HABITAT_POWER: f64 = 2400.0;
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
        MutationKind::GeneSilence => 4,
        MutationKind::GeneDuplicate => 5,
        MutationKind::Geometry => 6,
        MutationKind::Ingredient => 7,
        MutationKind::Specificity => 8,
        MutationKind::Turnover => 9,
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
    buckets: Vec<Vec<usize>>,
    columns: usize, rows: usize, width: f64, height: f64,
}
impl Default for Grid { fn default() -> Self { Self::new() } }
impl Grid {
    fn new() -> Self {
        Self { buckets: (0..GRID_BUCKETS).map(|_|Vec::new()).collect(),
            columns: GRID_COLUMNS, rows: GRID_ROWS, width: WIDTH, height: HEIGHT }
    }
    fn configure(&mut self, width: f64, height: f64) {
        let columns = (width / 60.0).ceil() as usize;
        let rows = (height / 60.0).ceil() as usize;
        if self.columns != columns || self.rows != rows { self.buckets = (0..columns*rows).map(|_|Vec::new()).collect(); }
        self.columns=columns;self.rows=rows;self.width=width;self.height=height;
    }
    fn index(&self, x: f64, y: f64) -> usize {
        let col=((wrap(x,self.width)/self.width*self.columns as f64) as usize).min(self.columns-1);
        let row=((wrap(y,self.height)/self.height*self.rows as f64) as usize).min(self.rows-1);
        row*self.columns+col
    }
    fn insert(&mut self, x:f64, y:f64, id:usize) { let i=self.index(x,y);self.buckets[i].push(id); }
    fn clear(&mut self) { for bucket in &mut self.buckets {bucket.clear();} }
    fn index_cells(&mut self, geometry:&CellGeometry) -> f64 {
        self.clear();let mut max_radius:f64=0.0;
        for i in 0..geometry.ids.len() {if geometry.alive[i] {let [x,y]=geometry.positions[i];self.insert(x,y,i);max_radius=max_radius.max(geometry.radii[i]);}}
        max_radius
    }
    fn relocate(&mut self, from:(f64,f64), to:(f64,f64), id:usize) {
        let old=self.index(from.0,from.1);let new=self.index(to.0,to.1);if old==new{return;}
        if let Some(position)=self.buckets[old].iter().position(|&item|item==id){self.buckets[old].swap_remove(position);}
        self.buckets[new].push(id);
    }
    fn nearby_radius(&self,x:f64,y:f64,radius:f64)->impl Iterator<Item=usize>+'_ {
        let index=self.index(x,y);let col=(index%self.columns) as isize;let row=(index/self.columns) as isize;
        let dx=(radius/(self.width/self.columns as f64)).ceil() as isize;
        let dy=(radius/(self.height/self.rows as f64)).ceil() as isize;
        let columns=(2*dx+1).min(self.columns as isize);let rows=(2*dy+1).min(self.rows as isize);
        (0..rows).flat_map(move |y| (0..columns).flat_map(move |x| {
            let c=(col-dx+x).rem_euclid(self.columns as isize) as usize;
            let r=(row-dy+y).rem_euclid(self.rows as isize) as usize;
            self.buckets[r*self.columns+c].iter().copied()
        }))
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
    pub counted: Option<crate::counts::CellCounts>,
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
    pub organic_carbon: f64,
    /// Paid chemical potential carried in addition to the baseline carbon energy.
    pub reserve_enrichment: f64,
    pub structural_enrichment: f64,
    pub internal_oxygen: f64,
    pub internal_inorganic_carbon: f64,
    pub respiration_rate: f64,
    pub carbon_fixation_rate: f64,
    /// Bilateral contacts and signed junction receipts during the last adhesion phase.
    pub adhered_neighbors: u32,
    pub junction_net: [f64; 3],
    pub gut_energy: f64,
    pub gut_nutrients: f64,
    pub gut: [OrganicMatter; 4],
    pub birth_area: f64,
    pub photosynthesis_rate: f64,
    pub uptake_rate: f64,
    pub digestion_rate: f64,
    pub growth_rate: f64,
    pub exudation_rate: f64,
    pub construction_rate: f64,
    pub synthesis_rate: f64,
    pub recycling_rate: f64,
    pub repair_rate: f64,
    pub engulf_cooldown: f64,
    mouth_ready: std::collections::BTreeMap<u32, u64>,
    requested: BodyControls,
    scan_bearing: f64,
    scan_width: f64,
    scan_target: Rc<str>,
    tracking: Tracking,
}
impl Cell {
    fn alive(&self) -> bool {
        self.energy > 0.0 && self.integrity > 0.0 && self.body.support_mass() > 0.0
    }
    pub fn radius(&self) -> f64 {
        CELL_RADIUS * self.body.area.sqrt()
    }
    pub fn mass(&self) -> f64 {
        self.chemical_body_mass() + self.nutrient_stored + self.gut_nutrients
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
    pub fn organic_carbon_total(&self) -> f64 {
        self.chemical_body_mass() * STRUCTURAL_CARBON_PER_MASS + self.organic_carbon
            + (self.gut_energy - self.gut.iter().map(|p| p.extra_energy).sum::<f64>()) / CHEMICAL_ENERGY_PER_CARBON
    }
    pub fn organic_carbon_capacity(&self) -> f64 { self.energy_capacity() / CHEMICAL_ENERGY_PER_CARBON }
    pub fn gas_volume(&self) -> f64 { 0.05 * self.body.area }
    pub fn basal_metabolism(&self) -> f64 {
        0.15 * self.body.area + self.body.upkeep()
    }
    pub fn maturity(&self) -> f64 {
        (self.body.area / self.birth_area - 1.0).clamp(0.0, 1.0)
    }
    pub fn division_ready(&self) -> bool {
        // Cellular programs regulate their own replication and constriction.
        // Empty compartments cannot raise the price of the final paid fission.
        let reserve = if self.body.cellular.is_some() { 0.0 }
            else { 0.35 * self.energy_capacity() };
        self.alive()
            && self.cooldown == 0.0
            && self.body.area + 1e-10 >= 2.0 * self.birth_area
            && self.energy > DIVISION_COST * self.body.area + reserve
            && self.body.can_partition()
    }
    fn receive_nutrients(&mut self, amount: f64) -> f64 {
        let kept = amount.min((self.nutrient_capacity() - self.nutrient_stored).max(0.0));
        self.nutrient_stored += kept;
        amount - kept
    }
    fn refresh_gut_totals(&mut self) {
        self.gut_energy = self.gut.iter().map(|pool| pool.energy).sum();
        self.gut_nutrients = self.gut.iter().map(|pool| pool.nutrients).sum();
    }
    #[cfg(test)]
    fn ingest(&mut self, kind: OrganicKind, energy: f64, nutrients: f64) {
        self.ingest_enriched(kind, energy, nutrients, 0.0);
    }
    #[cfg(test)]
    fn ingest_enriched(&mut self, kind: OrganicKind, energy: f64, nutrients: f64, extra_energy: f64) {
        self.gut[kind as usize].extra_energy += extra_energy;
        self.gut[kind as usize].energy += energy;
        self.gut[kind as usize].nutrients += nutrients;
        self.refresh_gut_totals();
    }
    fn scale_gut(&mut self, scale: f64) {
        for pool in &mut self.gut {
            pool.energy *= scale;
            pool.extra_energy *= scale;
            pool.nutrients *= scale;
        }
        self.refresh_gut_totals();
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OrganicKind {
    Exudate = 0,
    Tissue = 1,
    Waste = 2,
}
impl OrganicKind {
    pub const ALL: [Self; 3] = [Self::Exudate, Self::Tissue, Self::Waste];
    fn from_index(index: u8) -> Option<Self> {
        Self::ALL.get(index as usize).copied()
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OrganicMatter {
    pub energy: f64,
    pub nutrients: f64,
    /// Already included in energy. The remainder carries 20 energy per carbon.
    pub extra_energy: f64,
}
#[derive(Clone, Copy)]
struct OrganicEmission {
    merge_radius: f64,
    composition: Composition,
    kind: OrganicKind,
    x: f64,
    y: f64,
    energy: f64,
    nutrients: f64,
    outside: Option<(f64, f64, f64)>,
    extra_energy: f64,
}
impl OrganicEmission {
    fn new(kind: OrganicKind, x: f64, y: f64, energy: f64, nutrients: f64) -> Self {
        Self {
            kind,
            x,
            y,
            energy,
            nutrients,
            extra_energy: 0.0,
            outside: None,
            merge_radius: 24.0,
            composition: Composition::SOLUBLE,
        }
    }
}
impl OrganicEmission {
    fn material(mut self, composition: Composition) -> Self { self.composition = composition; self }
    fn enriched(mut self, extra: f64) -> Self { self.extra_energy = extra; self }
}
#[derive(Clone, Debug)]
pub struct Food {
    composition: Composition,
    pub counted: Option<crate::counts::FoodCounts>,
    pub id: u64,
    pub kind: OrganicKind,
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    pub nutrients: f64,
    /// Already included in energy. The remainder carries 20 energy per carbon.
    pub extra_energy: f64,
}

/// Compact columns used by spatial passes. Canonical cells retain their bodies,
/// memory and resources; these values are rebuilt at explicit phase boundaries.
#[derive(Default)]
struct CellGeometry {
    ids: Vec<u32>,
    positions: Vec<[f64; 2]>,
    velocities: Vec<[f64; 2]>,
    radii: Vec<f64>,
    areas: Vec<f64>,
    angles: Vec<f64>,
    alive: Vec<bool>,
    pigments: Vec<f64>,
    inertia: Vec<f64>,
}
impl CellGeometry {
    fn rebuild(&mut self, cells: &[Cell], appearance: bool, inertia: bool) {
        self.ids.clear();
        self.positions.clear();
        self.velocities.clear();
        self.radii.clear();
        self.areas.clear();
        self.angles.clear();
        self.alive.clear();
        self.pigments.clear();
        self.inertia.clear();
        for cell in cells {
            self.ids.push(cell.id);
            self.positions.push([cell.x, cell.y]);
            self.velocities.push([cell.vx, cell.vy]);
            self.radii.push(cell.radius());
            self.areas.push(cell.body.area);
            self.angles.push(cell.angle);
            self.alive.push(cell.alive());
            self.pigments.push(if appearance {
                ((cell.body.functional_mass(Kind::Photosynthetic)
                    + cell.body.mechanism_mass(crate::cellular::MechanismKind::LightHarvest)) / cell.body.structural_mass()
                    * 2.0)
                    .clamp(0.0, 1.0)
            } else {
                0.0
            });
            self.inertia
                .push(if inertia { cell.inertia() } else { 0.0 });
        }
    }
}

#[derive(Default)]
struct ObservationWorkspace {
    inputs: Vec<Inputs>,
    tracking: Vec<Tracking>,
    geometry: CellGeometry,
    cell_grid: Grid,
    food_grid: Grid,
    max_radius: f64,
}

#[derive(Clone, Copy)]
struct AttackEvent {
    victim: usize,
    attacker: usize,
    damage: f64,
    /// Only legacy piercing machinery can extract living reserves.
    extraction: f64,
    /// Contact direction relative to the victim's body.
    angle: f64,
}

#[derive(Default)]
struct PhaseWorkspace {
    junctions: adhesion::JunctionWorkspace,
    /// Cell-indexed public messages and receiver settings from before this tick's writes.
    signals: Vec<CellSignal>,
    contact_scratch: Vec<crate::genome::Value>,
    geometry: CellGeometry,
    cell_grid: Grid,
    food_grid: Grid,
    candidates: Vec<usize>,
    nearby: Vec<usize>,
    groups: Vec<Vec<(usize, f64)>>,
    group_patches: Vec<u64>,
    photo_requests: Vec<(usize, usize, f64)>,
    photon_patches: std::collections::HashMap<usize, counted::PhotonShare>,
    photon_receipts: Vec<u32>,
    footprint: Vec<(usize, f64)>,
    totals: Vec<f64>,
    gains: Vec<f64>,
    emissions: Vec<OrganicEmission>,
    targets: Vec<Option<usize>>,
    bite_power: Vec<f64>,
    engulf_targets: Vec<Option<(usize, f64)>>,
    mouth_candidates: Vec<Vec<(u32, usize, f64, f64)>>,
    consumed: Vec<bool>,
    order: Vec<usize>,
    events: Vec<AttackEvent>,
    lethal_hits: Vec<(usize, usize)>,
    primitive_hits: Vec<(usize, usize, f64, f64)>,
    damage: Vec<f64>,
    hit_ranges: Vec<(usize, usize)>,
    hits: Vec<(f64, f64)>,
    removed: Vec<f64>,
    meals: Vec<OrganicMatter>,
    requests: Vec<f64>,
    inlets: Vec<inlets::ParticleInlet>,
    inlet_contacts: Vec<(usize, f64)>,
    spill_capacities: Vec<BodyCapacities>,
}

/// Visit only occupied patches, in the same ascending order as the dense pass.
fn occupied_patches(words: &[u64]) -> impl Iterator<Item = usize> + '_ {
    words.iter().enumerate().flat_map(|(word, &bits)| {
        let mut remaining = bits;
        std::iter::from_fn(move || {
            if remaining == 0 { return None; }
            let bit = remaining.trailing_zeros() as usize;
            remaining &= remaining - 1;
            Some(word * 64 + bit)
        })
    })
}

impl PhaseWorkspace {
    fn clear_groups(&mut self, count: usize) {
        for patch in occupied_patches(&self.group_patches) { self.groups[patch].clear(); }
        self.groups.resize_with(count, Vec::new);
        self.group_patches.resize(count.div_ceil(64), 0);
        self.group_patches.fill(0);
    }
    fn group_request(&mut self, patch: usize, request: (usize, f64)) {
        self.group_patches[patch / 64] |= 1_u64 << (patch % 64);
        self.groups[patch].push(request);
    }
}

/// Reusable derived storage is not part of the checkpoint or simulated state.
#[derive(Default)]
struct TickWorkspace {
    capacities: Vec<BodyCapacities>,
    observations: ObservationWorkspace,
    outputs: Vec<Outputs>,
    /// Body-relative force, torque and cost from machinery present at tick start.
    motors: Vec<[f64; 4]>,
    processing: Vec<f64>,
    cellular_work: CellularWorkScratch,
    phases: PhaseWorkspace,
}

enum FounderPlacement {
    Scatter,
    Cluster(f64, f64),
    Exact(f64, f64),
}
impl FounderPlacement {
    fn seeded(x: f64, y: f64) -> Self {
        if x < 0.0 || y < 0.0 || !x.is_finite() || !y.is_finite() { Self::Scatter }
        else { Self::Cluster(x, y) }
    }
}

pub struct World {
    count_flux: counted::Flux,
    census: ecology::Census,
    energy_loss: ecology::EnergyLoss,
    photon_credit: Vec<u32>,
    deposits: Vec<counted::LocalDeposit>,
    pub cells: Vec<Cell>,
    pub food: Vec<Food>,
    pub ticks: u64,
    pub births: u64,
    pub deaths: u64,
    pub eaten: u64,
    pub mutation_rate: f64,
    pub mutations: u64,
    pub mutations_skipped: u64,
    pub environment: Environment,
    pub predations: u64,
    pub attacks: u64,
    pub photosynthesis_total: f64,
    nutrient_imports: f64,
    energy_imports: f64,
    pub energy_withdrawn: f64,
    pub mineral_withdrawn: f64,
    pub carbon_fixed_total: f64,
    pub carbon_respired_total: f64,
    pub decay_carbon_respired_total: f64,
    carbon_imports: f64,
    pub legacy_carbon_imports: f64,
    next_id: u32,
    next_food_id: u64,
    rng: Rng,
    seed: u32,
    // Derived from the previous adhesion phase, rebuilt from cells when restoring.
    had_adhesive_contacts: bool,
    contacts: std::collections::BTreeMap<(u32, u32), u8>,
    // Derived evaluation storage is reusable, never serialized as world state.
    eval_scratch: EvalScratch,
    workspace: TickWorkspace,
}
impl World {
    pub fn empty(seed: u32) -> Self {
        Self {
            count_flux: Default::default(),
            census: Default::default(),
            energy_loss: Default::default(),
            photon_credit: Vec::new(),
            deposits: Vec::new(),
            cells: Vec::new(),
            food: Vec::new(),
            ticks: 0,
            births: 0,
            deaths: 0,
            eaten: 0,
            mutation_rate: 0.1,
            mutations: 0,
            mutations_skipped: 0,
            environment: Environment::default(),
            predations: 0,
            attacks: 0,
            photosynthesis_total: 0.0,
            nutrient_imports: 0.0,
            energy_imports: 0.0,
            energy_withdrawn: 0.0,
            mineral_withdrawn: 0.0,
            carbon_fixed_total: 0.0,
            carbon_respired_total: 0.0,
            decay_carbon_respired_total: 0.0,
            carbon_imports: 0.0,
            legacy_carbon_imports: 0.0,
            next_id: 1,
            next_food_id: 1,
            rng: Rng::new(seed),
            seed,
            had_adhesive_contacts: false,
            contacts: Default::default(),
            eval_scratch: EvalScratch::default(),
            workspace: TickWorkspace::default(),
        }
    }
    pub fn set_world_size(&mut self, width: f64, height: f64) -> bool {
        let old=(self.environment.width,self.environment.height);
        if !self.environment.set_size(width,height) {return false;}
        if old==(width,height) {return true;}
        // Fractional illumination is transient exposure tied to the old patch geometry.
        self.photon_credit.clear();
        for cell in &mut self.cells {
            cell.x*=width/old.0;cell.y*=height/old.1;
            (cell.x,cell.y)=self.environment.move_circle((cell.x,cell.y),(cell.x,cell.y),cell.radius());
        }
        for food in &mut self.food {
            food.x*=width/old.0;food.y*=height/old.1;
            (food.x,food.y)=self.environment.move_circle((food.x,food.y),(food.x,food.y),2.5);
        }
        self.prune_contacts(None);
        for d in &mut self.deposits {d.x*=width/old.0;d.y*=height/old.1;}
        self.workspace=TickWorkspace::default();true
    }
    pub fn settle_barriers(&mut self) {
        if self.environment.barriers.is_empty() { return; }
        for cell in &mut self.cells {
            (cell.x,cell.y)=self.environment.move_circle((cell.x,cell.y),(cell.x,cell.y),cell.radius());
        }
        for food in &mut self.food {
            (food.x,food.y)=self.environment.move_circle((food.x,food.y),(food.x,food.y),2.5);
        }
    }
    /// Create the historical register-empty founders. Use the registry-aware
    /// reset when creating a program with an authored publication initializer.
    pub fn reset(&mut self, seed: u32, genome_id: u32) {
        self.try_reset(seed, genome_id)
            .expect("Unable to allocate simulation")
    }
    pub fn try_reset(&mut self, seed: u32, genome_id: u32) -> Result<(), String> {
        self.reset_population(seed, genome_id, None)
    }
    pub fn try_reset_in_registry(&mut self, seed: u32, genome_id: u32, genomes: &[Genome]) -> Result<(), String> {
        let program = Self::founder_program(genome_id, genomes)?;
        self.reset_population(seed, genome_id, Some(program))
    }
    fn reset_population(&mut self, seed: u32, genome_id: u32, program: Option<&Program>) -> Result<(), String> {
        let mut next = Self::empty(seed);
        next.environment = self.environment.clone();
        next.environment.reset_dynamic();
        next.mutation_rate = self.mutation_rate;
        next.seed_bodies(genome_id, 22, FounderPlacement::Scatter, Body::founder, program)?;
        *self = next;
        Ok(())
    }
    /// Clear organisms and all organic matter. Dissolved pools are initialized
    /// from the environment recipe. Founders can be introduced explicitly.
    pub fn reset_empty(&mut self, seed: u32) {
        let mut next = Self::empty(seed);
        next.environment = self.environment.clone();
        next.environment.reset_dynamic();
        next.mutation_rate = self.mutation_rate;
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
        self.dissolved_minerals() + self.nutrient_food_total() + self.nutrient_cell_total()
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
                .map(|c| c.energy + c.gut_energy + c.chemical_body_investment()
                    + (c.organic_carbon + c.chemical_body_mass() * STRUCTURAL_CARBON_PER_MASS) * CHEMICAL_ENERGY_PER_CARBON
                    + c.reserve_enrichment + c.structural_enrichment)
                .sum::<f64>()
    }
    /// Registry-free seeding retains its register-empty founder contract.
    /// Program-aware native and WASM callers use `try_seed_cells_in_registry`.
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
        self.seed_bodies(id, count, FounderPlacement::seeded(x, y), Body::founder, None)
    }
    fn founder_program(genome_id: u32, genomes: &[Genome]) -> Result<&Program, String> {
        genomes.get(genome_id.wrapping_sub(1) as usize).map(|genome| &genome.program)
            .ok_or_else(|| "Unknown genome: compile a valid program first.".into())
    }
    pub fn try_seed_cells_in_registry(
        &mut self, id: u32, count: usize, x: f64, y: f64, genomes: &[Genome],
    ) -> Result<usize, String> {
        let program = Self::founder_program(id, genomes)?;
        self.seed_bodies(id, count, FounderPlacement::seeded(x, y), Body::founder, Some(program))
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
        self.seed_bodies(genome_id, count, FounderPlacement::seeded(x, y), || Body::fixture(fixture), None)
    }
    fn seed_bodies(
        &mut self,
        genome_id: u32,
        count: usize,
        placement: FounderPlacement,
        mut create_body: impl FnMut() -> (Body, FounderBudget),
        program: Option<&Program>,
    ) -> Result<usize, String> {
        if count > (u32::MAX - self.next_id) as usize {
            return Err("Cannot add organisms: the cell identifier sequence is exhausted.".into());
        }
        self.cells
            .try_reserve_exact(count)
            .map_err(|_| "Cannot add organisms: not enough memory.".to_string())?;
        for _ in 0..count {
            let (cx, cy) = match placement {
                FounderPlacement::Exact(x, y) => (x, y),
                FounderPlacement::Scatter => (self.rng.unit() * self.environment.width,
                    self.rng.unit() * self.environment.height),
                FounderPlacement::Cluster(x, y) => {
                    let a = self.rng.unit() * TAU;
                    let r = self.rng.unit().sqrt() * 38.0;
                    (wrap(x + a.cos() * r, self.environment.width), wrap(y + a.sin() * r, self.environment.height))
                }
            };
            let angle = self.rng.unit() * TAU;
            let (mut body, budget) = create_body();
            if self.uses_count_chemistry() { body.prepare_count_founder(); }
            let birth_area = body.area;
            let mut memory = Registers::default();
            if let Some(program) = program { program.initialize_message(&mut memory); }
            let (cx,cy)=self.environment.move_circle((cx,cy),(cx,cy),CELL_RADIUS*body.area.sqrt());
            self.cells.push(Cell {
                counted: None,
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
                memory,
                integrity: 1.0,
                body,
                nutrient_stored: budget.minerals,
                organic_carbon: budget.energy / CHEMICAL_ENERGY_PER_CARBON,
                reserve_enrichment: 0.0, structural_enrichment: 0.0,
                internal_oxygen: 0.0,
                internal_inorganic_carbon: 0.0,
                respiration_rate: 0.0,
                carbon_fixation_rate: 0.0,
                adhered_neighbors: 0,
                junction_net: [0.0; 3],
                gut_energy: 0.0,
                gut_nutrients: 0.0,
                gut: [OrganicMatter::default(); 4],
                birth_area,
                photosynthesis_rate: 0.0,
                uptake_rate: 0.0,
                digestion_rate: 0.0,
                growth_rate: 0.0,
                exudation_rate: 0.0,
                construction_rate: 0.0,
                synthesis_rate: 0.0,
                recycling_rate: 0.0,
                repair_rate: 0.0,
                engulf_cooldown: 0.0,
                mouth_ready: Default::default(),
                requested: BodyControls::default(),
                scan_bearing: 0.0,
                scan_width: TAU,
                scan_target: Rc::from(""),
                tracking: Tracking::default(),
            });
            if self.uses_count_chemistry() {
                let c = self.cells.last_mut().unwrap();
                c.counted = Some(crate::counts::CellCounts { atp: crate::counts::nearest(c.energy),
                    mineral: crate::counts::nearest(c.nutrient_stored), carbon: crate::counts::nearest(c.organic_carbon),
                    ..Default::default() });
                c.project_counts();
                let q=c.counted.as_ref().unwrap();
                let body=u64::from(c.body.count_material());
                self.count_flux.mineral_in+=body+u64::from(q.mineral);
                self.count_flux.carbon_in+=body+u64::from(q.carbon);
                self.energy_loss.fabrication += i64::from(crate::counts::nearest(budget.dissipated_energy));
                self.count_flux.energy_in+=u64::from(q.atp)+u64::from(c.body.count_investment())
                    +u64::from(crate::counts::nearest(budget.dissipated_energy))+20*(body+u64::from(q.carbon));
            }
            self.nutrient_imports += budget.introduced_minerals;
            let founder_carbon = self.cells.last().unwrap().organic_carbon_total();
            self.carbon_imports += founder_carbon;
            self.energy_imports += budget.introduced_energy + founder_carbon * CHEMICAL_ENERGY_PER_CARBON;
            self.next_id += 1;
        }
        if self.uses_count_chemistry() {self.project_flows();}
        if count > 0 { self.census.events(genome_id).opening += count as u64; }
        Ok(count)
    }
    fn particle(&mut self, kind: OrganicKind, x: f64, y: f64, energy: f64, nutrients: f64) -> bool {
        if self.next_food_id >= u64::MAX / 2 || self.food.try_reserve(1).is_err() {
            return false;
        }
        let (x,y)=self.environment.move_circle((x,y),(x,y),2.5);
        self.food.push(Food {
            composition: Composition::SOLUBLE,
            counted: None,
            id: self.next_food_id,
            kind,
            x: wrap(x, self.environment.width),
            y: wrap(y, self.environment.height),
            energy,
            nutrients,
            extra_energy: 0.0,
        });
        self.next_food_id += 1;
        true
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
        let caps: Vec<_> = self.cells.iter().map(|c| c.body.capacities()).collect();
        let mut workspace = ObservationWorkspace::default();
        self.observations_into(selected, &caps, &mut workspace, None);
        (workspace.inputs, workspace.tracking)
    }
    fn observations_into(
        &self,
        selected: Option<usize>,
        capacities: &[BodyCapacities],
        workspace: &mut ObservationWorkspace,
        genomes: Option<&[Genome]>,
    ) {
        let ObservationWorkspace {
            inputs: result,
            tracking: tracks,
            geometry,
            cell_grid,
            food_grid,
            max_radius: maximum_radius,
            ..
        } = workspace;
        cell_grid.configure(self.environment.width,self.environment.height);
        cell_grid.clear();
        food_grid.configure(self.environment.width,self.environment.height);
        food_grid.clear();
        let mut max_radius: f64 = 0.0;
        let spatial_sensing = capacities.iter().enumerate().any(|(i, caps)| {
            selected.is_none_or(|which| which == i)
                && (caps.optical_sensing > 0.0 || caps.mechanical_sensing > 0.0 || caps.signal_junction > 0.0)
        });
        if spatial_sensing {
            geometry.rebuild(&self.cells, true, false);
            max_radius = cell_grid.index_cells(geometry);
            for (i, f) in self.food.iter().enumerate() {
                food_grid.insert(f.x, f.y, i);
            }
        }
        *maximum_radius = max_radius;
        // The geometry columns stay immutable throughout observation. Worlds
        // without physical sensors skip this spatial snapshot entirely.
        let count = if selected.is_some() {
            1
        } else {
            self.cells.len()
        };
        result.resize_with(count, Inputs::default);
        tracks.resize_with(count, Tracking::default);
        let mut output_index = 0;
        for (index, cell) in self.cells.iter().enumerate() {
            if selected.is_some_and(|which| which != index) {
                continue;
            }
            let caps = &capacities[index];
            // Inspection requests every field. Ticking only needs pure values
            // reachable from this program; tracking still runs in full below.
            let program = genomes.map(|genomes| &genomes[(cell.genome_id - 1) as usize].program);
            let reads = |sensor| program.is_none_or(|program| program.reads_sensor(sensor));
            let space = cell.body.space();
            let surface = cell.body.surface();
            // Preserve nested cellular buffers when resetting the scalar snapshot.
            let products = std::mem::take(&mut result[output_index].products);
            let assemblies = std::mem::take(&mut result[output_index].assemblies);
            let receptors = std::mem::take(&mut result[output_index].receptors);
            let mut input = Inputs {
                products,
                assemblies,
                receptors,
                organic_carbon: cell.organic_carbon,
                internal_inorganic_carbon: cell.internal_inorganic_carbon,
                internal_oxygen: cell.internal_oxygen,
                respiration_rate: cell.respiration_rate,
                carbon_fixation_rate: cell.carbon_fixation_rate,
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
                ligand: cell.ligand_carbon(),
                gut_exudate_energy: cell.gut[0].energy,
                gut_exudate_nutrients: cell.gut[0].nutrients,
                gut_tissue_energy: cell.gut[1].energy,
                gut_tissue_nutrients: cell.gut[1].nutrients,
                gut_waste_energy: cell.gut[2].energy,
                gut_waste_nutrients: cell.gut[2].nutrients,
                protein: cell.body.precursor_mass,
                expression_rate: cell.synthesis_rate,
                assembly_rate: cell.construction_rate,
                assembly_capacity: caps.assembly,
                exudation_rate: cell.exudation_rate,
                ..Inputs::default()
            };
            input.products.clear();
            input.assemblies.clear();
            input.receptors.clear();
            for (i, family) in cell.body.family_summaries().iter().enumerate() {
                input.organelle[i] = OrganelleInput {
                    count: family.count.min(i32::MAX as u32) as i32,
                    mass: family.mass,
                    capacity: family.capacity,
                    damaged: family.damaged,
                    building: family.building,
                };
            }
            let bearing = |(x, y): (f64, f64)| {
                if x.hypot(y) < 1e-12 {
                    0.0
                } else {
                    relative_angle(y.atan2(x) - cell.angle)
                }
            };
            if caps.light_sensing > 0.0 {
                input.light_available = true;
                input.light_valid = true;
                if reads(Sensor::LightLevel) { input.light_level = measure(
                    self.environment.light_at(cell.x, cell.y),
                    caps.light_sensing,
                    0.0005,
                )
                .clamp(0.0, 1.0); }
                if reads(Sensor::LightBearing) { input.light_bearing = measure(
                    bearing(self.environment.light_gradient(cell.x, cell.y)),
                    caps.light_sensing,
                    0.002,
                ); }
            }
            if caps.heat_sensing > 0.0 {
                input.heat_available = true;
                input.heat_valid = true;
                if reads(Sensor::HeatValue) { input.heat_value = measure(
                    self.environment.temperature_at(cell.x, cell.y),
                    caps.heat_sensing,
                    0.0005,
                )
                .clamp(0.0, 1.0); }
                if reads(Sensor::HeatBearing) { input.heat_bearing = measure(
                    bearing(self.environment.temperature_gradient(cell.x, cell.y)),
                    caps.heat_sensing,
                    0.002,
                ); }
            }
            if caps.chemical_sensing > 0.0 {
                input.chemical_available = true;
                input.chemical_valid = true;
                if reads(Sensor::ChemicalMineral) { input.chemical_mineral = measure(
                    self.environment.nutrient_at(cell.x, cell.y),
                    caps.chemical_sensing,
                    0.001,
                )
                .max(0.0); }
                if reads(Sensor::ChemicalOrganic) { input.chemical_organic = measure(
                    self.environment.organic_at(cell.x, cell.y),
                    caps.chemical_sensing,
                    0.001,
                )
                .max(0.0); }
                if reads(Sensor::ChemicalMineralBearing) { input.chemical_mineral_bearing = measure(
                    bearing(self.environment.nutrient_gradient(cell.x, cell.y)),
                    caps.chemical_sensing,
                    0.002,
                ); }
                if reads(Sensor::ChemicalOrganicBearing) { input.chemical_organic_bearing = measure(
                    bearing(self.environment.organic_gradient(cell.x, cell.y)),
                    caps.chemical_sensing,
                    0.002,
                ); }
            }
            if caps.mechanical_sensing > 0.0 {
                input.flow_available = true;
                input.flow_valid = true;
                input.contact_available = true;
                if reads(Sensor::FlowVector) || reads(Sensor::FlowSpeed) || reads(Sensor::FlowBearing) {
                let (vx, vy) = self.environment.flow_at(cell.x, cell.y, self.time());
                input.flow_vector = local_vector(vx - cell.vx, vy - cell.vy, cell.angle)
                    .map(|v| measure(v, caps.mechanical_sensing, 0.03));
                input.flow_speed = input.flow_vector[0].hypot(input.flow_vector[1]);
                input.flow_bearing = if input.flow_speed > 1e-12 {
                    input.flow_vector[1].atan2(input.flow_vector[0])
                } else {
                    0.0
                };
                }
            }
            input.object_available = caps.optical_sensing > 0.0;
            let tracking = &mut tracks[output_index];
            tracking.entries.clone_from(&cell.tracking.entries);
            tracking.next_serial = cell.tracking.next_serial;
            tracking
                .entries
                .retain(|e| self.ticks.saturating_sub(e.seen) <= TRACK_TTL);
            let requested = tracking.entity(&cell.scan_target, cell.id, self.ticks);
            let units = cell
                .body
                .organs
                .iter()
                .filter(|o| o.kind == Kind::Optical && o.functional_mass() > 0.0)
                .count()
                + cell.body.cellular.as_ref().map_or(0, |state| {
                    state.assemblies.iter().filter(|assembly| {
                        matches!(assembly.recipe.kind,
                            AssemblyKind::Organelle(Kind::Optical)
                            | AssemblyKind::Mechanism(MechanismKind::SenseOptical))
                            && assembly.functional_mass() * assembly.activity > 0.0
                    }).count()
                });
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
                            .light_at(wrap(cell.x + o.dx, self.environment.width), wrap(cell.y + o.dy, self.environment.height))
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
                        }) && self.optical_line_clear(index, &o, cell_grid, max_radius, geometry)
                        {
                            object = Some((o, score));
                        }
                    }
                };
                let reach = (caps.optical_range + max_radius).max(cell.radius() + max_radius + 1.0);
                for i in cell_grid.nearby_radius(cell.x, cell.y, reach) {
                    if geometry.ids[i] == cell.id {
                        continue;
                    }
                    let [x, y] = geometry.positions[i];
                    let [vx, vy] = geometry.velocities[i];
                    let pigment = geometry.pigments[i];
                    consider(Observation {
                        entity: (geometry.ids[i] as u64) << 1,
                        dx: delta(x - cell.x, self.environment.width),
                        dy: delta(y - cell.y, self.environment.height),
                        radius: geometry.radii[i],
                        area: geometry.areas[i],
                        contrast: 0.45 + 0.4 * pigment,
                        pigment,
                        motion: ObservedMotion::Relative(vx - cell.vx, vy - cell.vy),
                        resistance: capacities[i].protection + 0.15,
                    });
                }
                for i in food_grid.nearby_radius(
                    cell.x,
                    cell.y,
                    caps.optical_range.max(cell.radius()) + 3.5,
                ) {
                    let f = &self.food[i];
                    consider(Observation {
                        entity: (f.id << 1) | 1,
                        dx: delta(f.x - cell.x, self.environment.width),
                        dy: delta(f.y - cell.y, self.environment.height),
                        radius: 2.5,
                        area: (2.5 / CELL_RADIUS).powi(2),
                        contrast: f.traits().2,
                        pigment: f.traits().1,
                        // Motion does not participate in selection or occlusion.
                        // Sample the current only for the returned observation.
                        motion: ObservedMotion::ParticleAt(f.x, f.y),
                        resistance: f.traits().0,
                    });
                }
            }
            if let Some(o) = contact {
                input.contact_token = tracking.observe(o.entity, cell.id, self.ticks);
                input.contact_valid = !input.contact_token.is_empty();
                if input.contact_valid {
                    let (vx, vy) = o.relative_motion(cell, &self.environment, self.time());
                    input.contact_offset = local_vector(o.dx, o.dy, cell.angle)
                        .map(|v| measure(v, caps.mechanical_sensing, 0.08));
                    input.contact_pressure = measure(
                        (cell.radius() + o.radius - o.dx.hypot(o.dy)).max(0.0)
                            + (-vx * o.dx - vy * o.dy).max(0.0) / (1.0 + o.dx.hypot(o.dy)),
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
                    let (vx, vy) = o.relative_motion(cell, &self.environment, self.time());
                    input.object_offset = local_vector(o.dx, o.dy, cell.angle)
                        .map(|v| measure(v, caps.optical_sensing, 0.08));
                    input.object_area = measure(o.area, caps.optical_sensing, 0.002).max(0.02);
                    input.object_contrast =
                        measure(o.contrast, caps.optical_sensing, 0.001).clamp(0.0, 1.0);
                    input.object_pigment =
                        measure(o.pigment, caps.optical_sensing, 0.001).clamp(0.0, 1.0);
                    input.object_motion = local_vector(vx, vy, cell.angle)
                        .map(|v| measure(v, caps.optical_sensing, 0.03));
                }
            }
            result[output_index] = input;
            output_index += 1;
        }
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
        geometry: &CellGeometry,
    ) -> bool {
        let [x, y] = geometry.positions[observer];
        let length_squared = target.dx * target.dx + target.dy * target.dy;
        if self.environment.segment_blocked((x,y),(x+target.dx,y+target.dy)) { return false; }
        let midpoint_x = wrap(x + target.dx * 0.5, self.environment.width);
        let midpoint_y = wrap(y + target.dy * 0.5, self.environment.height);
        let reach = length_squared.sqrt() * 0.5 + max_radius;
        for index in grid.nearby_radius(midpoint_x, midpoint_y, reach) {
            if index == observer
                || (geometry.ids[index] as u64) << 1 == target.entity
                || !geometry.alive[index]
            {
                continue;
            }
            let dx = delta(geometry.positions[index][0] - x, self.environment.width);
            let dy = delta(geometry.positions[index][1] - y, self.environment.height);
            let t = if length_squared > 1e-12 {
                ((dx * target.dx + dy * target.dy) / length_squared).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let nearest_x = dx - t * target.dx;
            let nearest_y = dy - t * target.dy;
            if nearest_x * nearest_x + nearest_y * nearest_y + 1e-9 < geometry.radii[index].powi(2)
            {
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
        // Keep the workspace even when a preflight returns an error. All fallible
        // paths live inside try_step_in; this wrapper always restores the buffers.
        let mut workspace = std::mem::take(&mut self.workspace);
        let result = self.try_step_in(genomes, &mut workspace);
        self.workspace = workspace;
        result
    }
    fn try_step_in(
        &mut self,
        genomes: &mut Vec<Genome>,
        workspace: &mut TickWorkspace,
    ) -> Result<(), String> {
        let TickWorkspace {
            capacities: caps,
            observations,
            outputs,
            motors,
            processing,
            cellular_work,
            phases,
        } = workspace;
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
                .any(|c| c.tracking.next_serial == u64::MAX
                    || c.body.next_organelle_id == u32::MAX
                    || c.body.cellular.as_ref().is_some_and(|state| state.next_assembly_id == u32::MAX))
        {
            return Err(
                "Simulation paused: an object or organelle identifier sequence is exhausted."
                    .into(),
            );
        }
        // Fields and particle decay do not change bodies. Reuse these frozen
        // capacities for preflight, sensing, and the existing action phases.
        caps.clear();
        caps.extend(self.cells.iter().map(|c| c.body.capacities()));
        let packet_energy: f64 = self
            .cells
            .iter()
            .zip(caps.iter())
            .map(|(c, cap)| {
                // A cell may capture outside food before dying in this tick.
                let captured = cap.gut_energy.min(cap.capture * PARTICLE_CAPTURE_RATE * DT);
                c.food_potential().total() + captured
                    + (cap.photosynthesis + cap.light_harvest) * DT
            })
            .sum();
        // Each installed outlet may release a separate sub-packet at its own
        // location. Reserve this space before any cell pays the export cost.
        let packet_overhead = self.cells.iter().try_fold(0usize, |slots, c| {
            let outlets = c.body.cellular.as_ref().map_or(0, |state|
                state.assemblies.iter().filter(|a| matches!(a.recipe.kind, AssemblyKind::Mechanism(MechanismKind::CarbonExport | MechanismKind::LigandExport))).count());
            slots.checked_add(20)?.checked_add(outlets)
        }).ok_or("Simulation paused: organic material exceeds addressable memory.")?;
        self.reserve_organic_packets(packet_energy, packet_overhead)?;
        self.environment.step(self.time());
        if !self.deposits.is_empty() {
            for d in std::mem::take(&mut self.deposits) {
                counted::deposit(&mut self.environment,&mut self.deposits,d.x,d.y,d.quantities);
            }
        }
        if !self.contacts.is_empty() && self.environment.barriers.iter().any(|b| b.floating) {
            self.prune_contacts(None);
        }
        self.decay_food_in(phases);
        // Phase 1: one immutable observation/capability snapshot. Memory and
        // tracking commit only after every cell has observed that same world.
        self.freeze_contact_signals(genomes, &mut phases.signals);
        self.observations_into(None, caps, observations, Some(genomes));
        for (index, (cell, input)) in self.cells.iter().zip(observations.inputs.iter_mut()).enumerate() {
            let program = &genomes[(cell.genome_id - 1) as usize].program;
            if let Some(definition) = program.cellular() {
                cell.body.cellular_inputs_into(definition, &mut input.products, &mut input.assemblies);
                self.receptor_inputs_into(index, program, input, caps,
                    &observations.geometry, &observations.cell_grid, &observations.food_grid,
                    observations.max_radius, false, &phases.signals);
            }
        }
        let inputs = &observations.inputs;
        let scratch = &mut self.eval_scratch;
        outputs.resize_with(self.cells.len(), Outputs::default);
        for (((cell, input), tracks), output) in self.cells.iter_mut()
            .zip(inputs).zip(observations.tracking.iter_mut()).zip(outputs.iter_mut()) {
            std::mem::swap(&mut cell.tracking, tracks);
            genomes[(cell.genome_id - 1) as usize].program.evaluate_cell_into(
                input, &mut cell.memory, scratch, output);
        }
        // Regulation acts on existing machinery only. Construction is a later
        // phase, so a newly synthesized part cannot propel, sense or divide now.
        motors.clear();
        for ((cell, output), cap) in self.cells.iter_mut().zip(outputs.iter_mut()).zip(caps.iter_mut()) {
            let program = &genomes[(cell.genome_id - 1) as usize].program;
            if let (Some(program), Some(controls)) = (program.cellular(), &output.cellular) {
                let action = cell.body.cellular_actions(program, controls);
                cap.capture *= action.capture_activity;
                cap.phagocytosis *= action.engulf_activity;
                cap.independent_engulf = action.independent_engulf;
                cap.bite_extraction_fraction = action.bite_extraction_fraction;
                cap.light_harvest = action.light_harvest;
                cap.substrate_digestion = action.substrate_digestion;
                cap.respiration = cap.metabolism + action.respiration;
                cap.isomerization = action.isomerization;
                motors.push([action.force_x, action.force_y, action.torque, action.motor_cost]);
                output.photosynthesize = action.photosynthesize;
                output.absorb = action.absorb;
                output.feed = action.feed;
                output.bite = action.bite;
                output.defend = action.defend;
                output.repair = action.repair;
                output.engulf = action.engulf;
                output.digest = action.digest;
                output.divide = action.divide;
                output.scan_bearing = action.scan_bearing;
                output.scan_width = action.scan_width;
            } else {
                motors.push([0.0; 4]);
            }
        }
        self.exchange_gases_in(phases);
        self.respire_in(caps);
        self.isomerize_in(caps);
        let time = self.time();
        for (i, cell) in self.cells.iter_mut().enumerate() {
            let output = &outputs[i];
            let cap = &caps[i];
            cell.photosynthesis_rate = 0.0;
            cell.carbon_fixation_rate = 0.0;
            cell.uptake_rate = 0.0;
            cell.digestion_rate = 0.0;
            cell.growth_rate = 0.0;
            cell.exudation_rate = 0.0;
            cell.construction_rate = 0.0;
            cell.synthesis_rate = 0.0;
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
            let temperature = self.environment.temperature_at(cell.x, cell.y);
            let (flow_x, flow_y) = self.environment.flow_at(cell.x, cell.y, time);
            let inertia = cell.inertia();
            let cellular = output.cellular.is_some();
            cell.angle = if cellular {
                wrap(cell.angle + 12.0 * motors[i][2] / (inertia * cell.radius().powi(2)) * DT, TAU)
            } else {
                wrap(cell.angle + output.turn * cap.turn / inertia.sqrt() * DT, TAU)
            };
            let drag = (-1.8 * (cell.body.area.sqrt() + 0.35 * cell.body.surface().used)
                / inertia.sqrt()
                * DT)
                .exp();
            if cellular {
                let [x, y, _, _] = motors[i];
                let force_x = cell.angle.cos() * x - cell.angle.sin() * y;
                let force_y = cell.angle.sin() * x + cell.angle.cos() * y;
                cell.vx = flow_x + (cell.vx - flow_x + force_x * 12.0 / inertia * DT) * drag;
                cell.vy = flow_y + (cell.vy - flow_y + force_y * 12.0 / inertia * DT) * drag;
            } else {
                let acceleration = output.thrust * cap.thrust * 12.0 / inertia;
                cell.vx = flow_x + (cell.vx - flow_x + cell.angle.cos() * acceleration * DT) * drag;
                cell.vy = flow_y + (cell.vy - flow_y + cell.angle.sin() * acceleration * DT) * drag;
            }
            let old=(cell.x,cell.y);
            let desired=(cell.x+cell.vx*DT,cell.y+cell.vy*DT);
            (cell.x,cell.y)=self.environment.move_circle(old,desired,cell.radius());
            if !self.environment.barriers.is_empty() {
                cell.vx=delta(cell.x-old.0,self.environment.width)/DT;
                cell.vy=delta(cell.y-old.1,self.environment.height)/DT;
            }
            cell.cooldown = (cell.cooldown - DT).max(0.0);
            cell.engulf_cooldown = (cell.engulf_cooldown - DT).max(0.0);
            cell.mouth_ready.retain(|_, until| *until > self.ticks);
            let maintenance = (cell.basal_metabolism() * metabolic_factor(temperature)
                + inputs[i].memory_cost
                + if cellular { motors[i][3] } else { 0.06 * cap.thrust * output.thrust }
                + 0.025 * (cap.photosynthesis * output.photosynthesize + cap.light_harvest)
                + 1.5 * cap.piercing * output.bite
                + 0.3 * cap.protection * output.defend)
                * DT;
            if let Some(q) = cell.counted.as_mut() {
                let cost = crate::counts::rate(&mut q.rate_credit[6], maintenance).min(q.atp);
                q.atp -= cost;
                self.energy_loss.maintenance += u64::from(cost);
                cell.energy = crate::counts::units(q.atp);
            } else { cell.energy = (cell.energy - maintenance).max(0.0); }
            let stress = (0.15 - temperature).max(temperature - 0.85).max(0.0)
                * 0.3
                * (1.0 - cap.protection)
                * DT;
            cell.integrity = (cell.integrity - stress).max(0.0);
            cell.body.damage(stress);
        }
        processing.clear();
        processing.extend(caps.iter().map(|c| c.metabolism * DT));
        self.digest_in(outputs, caps, processing, phases);
        self.absorb_nutrients_in(outputs, caps, phases);
        for food in &mut self.food {
            let (vx, vy) = self.environment.flow_at(food.x, food.y, time);
            (food.x,food.y)=self.environment.move_circle((food.x,food.y),(food.x+vx*DT,food.y+vy*DT),2.5);
        }
        // Phase 2: rebuild geometry after movement, maintenance and digestion.
        // Collisions update these columns in pair order while their grid stays
        // frozen. Capture and target selection then index the settled positions.
        self.resolve_collisions_in(phases);
        self.settle_barriers();
        if !self.environment.barriers.is_empty() { phases.geometry.rebuild(&self.cells,false,false); }
        self.apply_adhesion_in(genomes, phases);
        self.eat_in(outputs, caps, genomes, phases);
        self.photosynthesize_in(outputs, caps, processing, phases);
        self.resolve_predation_in(outputs, caps, genomes, phases);
        // Phase 3: damage, construction and births use canonical cells. Geometry
        // from the interaction phase is invalid beyond this boundary.
        for (i, cell) in self.cells.iter_mut().enumerate() {
            if !cell.alive() {
                continue;
            }
            let output = &outputs[i];
            let report = if cell.counted.is_some() {
                cell.develop_counted(&genomes[(cell.genome_id-1) as usize].program, output, &caps[i],
                    inputs[i].body_division_ready, cellular_work, &mut self.energy_loss.fabrication)
            } else {
                // Repair the envelope explicitly; organelle repair is handled by the
                // same paid physical body worker below. Both preserve material.
                let repair_reserve = if cell.body.cellular.is_some() { 0.0 }
                    else { 0.15 * cell.energy_capacity() };
                let repaired = (caps[i].repair * output.repair * DT)
                    .min(1.0 - cell.integrity)
                    .min(
                        (cell.energy - repair_reserve).max(0.0)
                            / (20.0 * cell.body.area),
                    );
                cell.energy -= repaired * 20.0 * cell.body.area;
                cell.integrity += repaired;
                let structural_before = cell.body.structural_mass();
                let withheld_minerals = (cell.nutrient_stored - cell.organic_carbon / STRUCTURAL_CARBON_PER_MASS).max(0.0);
                cell.nutrient_stored -= withheld_minerals;
                let report = if let (Some(program), Some(controls)) = (
                    genomes[(cell.genome_id - 1) as usize].program.cellular(),
                    &output.cellular,
                ) {
                    cell.body.cellular_work_in(
                        program, controls, &mut cell.energy, &mut cell.nutrient_stored,
                        DT, MAX_AREA, inputs[i].body_division_ready, cellular_work,
                    )
                } else {
                    cell.body.work(
                        output.grow, &output.body, output.repair,
                        &mut cell.energy, &mut cell.nutrient_stored, DT, MAX_AREA,
                    )
                };
                cell.nutrient_stored += withheld_minerals;
                cell.move_construction_carbon(structural_before);
                report
            };
            cell.growth_rate = report.area_grown / DT;
            cell.construction_rate = report.constructed_mass / DT;
            cell.synthesis_rate = report.synthesized_mass / DT;
            cell.recycling_rate = report.recycled_mass / DT;
            cell.repair_rate = report.repaired_mass / DT;
        }
        self.spill_excess_stores_in(phases, true);
        let originals = self.cells.len();
        let mut division_bonds: Option<adhesion::DivisionBonds> = None;
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
            let polarity = cell.body.cleavage_axis();
            let split_angle = cell.angle + polarity.unwrap_or_else(|| PI * 0.5 + (self.rng.unit() - 0.5) * 0.6);
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
            genomes[(genome_id - 1) as usize].program.initialize_message(&mut daughter.memory);
            daughter.tracking = Tracking::default();
            daughter.scan_target = Rc::from("");
            daughter.body = daughter_body;
            daughter.birth_area = daughter.body.area;
            daughter.energy = energy;
            daughter.nutrient_stored *= 0.5;
            daughter.organic_carbon *= 0.5;
            daughter.reserve_enrichment *= 0.5;
            // Replica fuel expended during fission does not change conserved
            // structural carbon. Actual discrete bodies can have unequal masses.
            let daughter_share = daughter.body.structural_mass()
                / (daughter.body.structural_mass() + parent_body.structural_mass());
            daughter.structural_enrichment = cell.structural_enrichment * daughter_share;
            cell.structural_enrichment -= daughter.structural_enrichment;
            daughter.internal_oxygen *= 0.5;
            daughter.internal_inorganic_carbon *= 0.5;
            daughter.respiration_rate = 0.0;
            daughter.carbon_fixation_rate = 0.0;
            daughter.adhered_neighbors = 0;
            daughter.junction_net = [0.0; 3];
            daughter.scale_gut(0.5);
            daughter.x = wrap(cell.x + dx, self.environment.width);
            daughter.y = wrap(cell.y + dy, self.environment.height);
            daughter.vx += split_angle.cos() * 3.0;
            daughter.vy += split_angle.sin() * 3.0;
            daughter.angle = if polarity.is_some() { cell.angle } else { wrap(cell.angle + 0.5, TAU) };
            daughter.cooldown = DIVISION_COOLDOWN;
            daughter.photosynthesis_rate = 0.0;
            daughter.uptake_rate = 0.0;
            daughter.digestion_rate = 0.0;
            daughter.growth_rate = 0.0;
            daughter.exudation_rate = 0.0;
            daughter.construction_rate = 0.0;
            daughter.synthesis_rate = 0.0;
            daughter.recycling_rate = 0.0;
            daughter.repair_rate = 0.0;
            cell.body = parent_body;
            cell.birth_area = cell.body.area;
            cell.energy = energy;
            cell.nutrient_stored *= 0.5;
            cell.organic_carbon *= 0.5;
            cell.reserve_enrichment *= 0.5;
            cell.internal_oxygen *= 0.5;
            cell.internal_inorganic_carbon *= 0.5;
            cell.scale_gut(0.5);
            if cell.counted.is_some() {
                let fee = DIVISION_COST * (cell.body.area + daughter.body.area);
                self.energy_loss.work += u64::from(crate::counts::nearest(fee).min(cell.counted.as_ref().unwrap().atp));
                cell.count_partition(&mut daughter, fee);
            }
            cell.x = wrap(cell.x - dx, self.environment.width);
            cell.y = wrap(cell.y - dy, self.environment.height);
            cell.vx -= split_angle.cos() * 3.0;
            cell.vy -= split_angle.sin() * 3.0;
            cell.cooldown = DIVISION_COOLDOWN;
            self.next_id += 1;
            self.births += 1;
            let events = self.census.events(daughter.genome_id);
            events.births += 1;
            events.last_birth = self.ticks + 1;
            // Only division changes bodies between the two spill passes.
            phases.spill_capacities[i] = cell.body.capacities();
            phases.spill_capacities.push(daughter.body.capacities());
            let had_interfaces = cell.adhered_neighbors > 0;
            self.cells.push(daughter);
            let daughter_row = self.cells.len() - 1;
            if let Some(index) = division_bonds.as_mut() {
                index.append_daughter(daughter_row);
            }
            if had_interfaces {
                let index = division_bonds.get_or_insert_with(|| adhesion::DivisionBonds::new(self));
                self.inherit_bond_interfaces(i, daughter_row, genomes, index);
            }
            // Established physical interfaces get first use of the hardware
            // actually inherited by each half. Retention uses one remaining site.
            let parent = &self.cells[i];
            let daughter = &self.cells[daughter_row];
            if self.can_retain_cleavage(i, daughter_row, genomes, division_bonds.as_ref()) {
                self.contacts.insert((parent.id, daughter.id), adhesion::CLONAL);
                self.cells[i].adhered_neighbors += 1;
                self.cells[daughter_row].adhered_neighbors += 1;
                self.had_adhesive_contacts = true;
                if let Some(index) = division_bonds.as_mut() { index.add(i, daughter_row); }
            }
        }
        // Uneven, discrete compartment inheritance can reduce capacity; transfer
        // overflow to the local environment rather than cloning a compartment.
        self.spill_excess_stores_in(phases, false);
        self.remove_dead_in(phases, None);
        self.settle_barriers();
        self.prune_contacts(Some(genomes));
        if self.uses_count_chemistry(){self.project_flows();}
        self.ticks += 1;
        Ok(())
    }
    fn absorb_nutrients_in(
        &mut self,
        outputs: &[Outputs],
        caps: &[BodyCapacities],
        workspace: &mut PhaseWorkspace,
    ) {
        if self.uses_count_chemistry() { return self.count_uptake(outputs, caps, workspace); }
        workspace.clear_groups(self.environment.patch_count());
        for (i, c) in self.cells.iter().enumerate() {
            if !c.alive() {
                continue;
            }
            let request = (outputs[i].absorb * caps[i].uptake * DT)
                .min((caps[i].minerals - c.nutrient_stored).max(0.0))
                .min(c.energy.max(0.0));
            if request > 0.0 {
                workspace.group_request(self.environment.patch_index(c.x, c.y), (i, request));
            }
        }
        for patch in occupied_patches(&workspace.group_patches) {
            let group = &mut workspace.groups[patch];
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
    fn digest_in(
        &mut self, outputs: &[Outputs], caps: &[BodyCapacities], _processing: &mut [f64],
        workspace: &mut PhaseWorkspace,
    ) {
        let residues = &mut workspace.emissions;
        residues.clear();
        for (i, c) in self.cells.iter_mut().enumerate() {
            if !c.alive() || c.gut_energy <= 0.0
                || (outputs[i].feed <= 0.0 && caps[i].substrate_digestion.iter().all(|rate| *rate <= 0.0)) { continue; }
            let room = (caps[i].energy / CHEMICAL_ENERGY_PER_CARBON - c.organic_carbon).max(0.0) * CHEMICAL_ENERGY_PER_CARBON;
            let mineral_room = (caps[i].minerals - c.nutrient_stored).max(0.0);
            let mut demands = [0.0; 3];
            let mut weights = [0.0; 3];
            for k in 0..3 {
                let pool = c.gut[k];
                if pool.energy <= 0.0 || (outputs[i].digest[k] <= 0.0 && caps[i].substrate_digestion[k] <= 0.0) { continue; }
                let mineral_demand = if pool.nutrients > 0.0 {
                    mineral_room / (pool.nutrients * MINERAL_ASSIMILATION[k]) * pool.energy
                } else { 0.0 };
                let carrier_fraction = (pool.energy - pool.extra_energy) / pool.energy;
                // Hydrolysis capacity and reserve space are measured in carrier
                // material. Enriched tissue can deliver more energy per carbon.
                let raw = pool.energy.min((room / (ASSIMILATION_EFFICIENCY[k] * carrier_fraction)).max(mineral_demand));
                demands[k] = raw * carrier_fraction / DIGESTION_SPEED[k];
                weights[k] = outputs[i].digest[k].clamp(0.0, 1.0);
            }
            let budget = caps[i].digestion * outputs[i].feed * DT;
            let independent = std::array::from_fn::<_, 3, _>(|k| demands[k].min(caps[i].substrate_digestion[k] * DT));
            let remaining = std::array::from_fn(|k| (demands[k] - independent[k]).max(0.0));
            let mut work = shared_digestive_work(budget, remaining, weights);
            for k in 0..3 {
                work[k] += independent[k];
                if c.gut[LIGAND].energy > 0.0 {
                    work[k] *= c.hydrolase_availability(k, caps[i].gut_energy);
                }
            }
            if let Some(q) = &c.counted {
                let before = q.atp;
                c.count_digest(&caps[i], work, residues);
                self.energy_loss.work += u64::from(before - c.counted.as_ref().unwrap().atp);
                continue;
            }
            let mut remaining_room = room;
            let mut useful_base = 0.0;
            let mut useful_extra = 0.0;
            for kind in OrganicKind::ALL {
                let k = kind as usize;
                // Hydrolysis/processing is paid work, separate from respiration.
                let pool = c.gut[k];
                if work[k] <= 0.0 || pool.energy <= 0.0 { continue; }
                let base_stock = pool.energy - pool.extra_energy;
                if base_stock <= 0.0 { continue; }
                let base = (work[k] * DIGESTION_SPEED[k]).min(base_stock).min(c.energy / 0.02);
                if base <= 0.0 { continue; }
                let fraction = base / base_stock;
                let extra = pool.extra_energy * fraction;
                let minerals = pool.nutrients * fraction;
                let useful = (base * ASSIMILATION_EFFICIENCY[k]).min(remaining_room);
                let enriched = extra * (useful / base);
                remaining_room -= useful;
                useful_base += useful;
                useful_extra += enriched;
                c.energy = (c.energy - base * 0.02).max(0.0);
                // Compute the unconsumed carrier first. Subtracting enrichment
                // and total energy independently can erase the last carrier.
                let retained_base = base_stock - base;
                let retained_fraction = retained_base / base_stock;
                let retained_extra = pool.extra_energy * retained_fraction;
                c.gut[k].energy = retained_base + retained_extra;
                c.gut[k].extra_energy = retained_extra;
                c.gut[k].nutrients = pool.nutrients * retained_fraction;
                let assimilated_minerals = minerals * MINERAL_ASSIMILATION[k];
                let overflow = c.receive_nutrients(assimilated_minerals);
                self.environment.recycle_nutrients(c.x, c.y, overflow);
                // Every unassimilated carbon atom remains in physical residue;
                // digestion does not turn substrate carbon into untracked heat.
                let r = c.radius() + 2.5;
                let mut residue = OrganicEmission::new(OrganicKind::Waste,
                    c.x - c.angle.cos() * r, c.y - c.angle.sin() * r,
                    (base - useful) + (extra - enriched), minerals - assimilated_minerals).enriched(extra - enriched).material(Composition::residue(k));
                residue.outside = Some((c.x, c.y, c.radius()));
                residues.push(residue);
            }
            c.refresh_gut_totals();
            c.organic_carbon += useful_base / CHEMICAL_ENERGY_PER_CARBON;
            c.reserve_enrichment += useful_extra;
            c.digestion_rate = (useful_base + useful_extra) / DT;
        }
        self.emit_organic_pending(workspace);
    }
    fn spill_excess_stores_in(&mut self, workspace: &mut PhaseWorkspace, refresh: bool) {
        if refresh {
            workspace.spill_capacities.clear();
            workspace.spill_capacities.extend(self.cells.iter().map(|cell| cell.body.capacities()));
        }
        debug_assert_eq!(workspace.spill_capacities.len(), self.cells.len());
        let spills = &mut workspace.emissions;
        spills.clear();
        for (c, cap) in self.cells.iter_mut().zip(&workspace.spill_capacities) {
            self.energy_loss.overflow += u64::from(Self::collect_store_overflow(c, cap, &mut self.environment, spills));
        }
        self.emit_organic_pending(workspace);
    }
    fn collect_store_overflow(c: &mut Cell, cap: &BodyCapacities, environment: &mut Environment,
        spills: &mut Vec<OrganicEmission>) -> u32 {
        if !c.alive() {
            return 0;
        }
        if c.counted.is_some() {return c.count_overflow(cap, spills);}
        let excess = (c.nutrient_stored - cap.minerals).max(0.0);
        c.nutrient_stored -= excess;
        environment.recycle_nutrients(c.x, c.y, excess);
        let energy = (c.energy - cap.energy).max(0.0);
        c.energy -= energy;
        let carbon_overflow = (c.organic_carbon - cap.energy / CHEMICAL_ENERGY_PER_CARBON).max(0.0);
        let extra = c.reserve_extra_for(carbon_overflow);
        c.reserve_enrichment -= extra;
        c.organic_carbon -= carbon_overflow;
        if carbon_overflow > 0.0 {
            spills.push(OrganicEmission::new(
                OrganicKind::Exudate,
                c.x,
                c.y,
                carbon_overflow * CHEMICAL_ENERGY_PER_CARBON + extra,
                0.0,
            ).enriched(extra));
        }
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
        for (index, pool) in c.gut.iter().enumerate() {
            spills.push(OrganicEmission::new(
                OrganicKind::Waste,
                c.x,
                c.y,
                pool.energy * (1.0 - kept),
                pool.nutrients * (1.0 - kept),
            ).enriched(pool.extra_energy * (1.0 - kept)).material(Composition::pool(index)));
        }
        c.scale_gut(kept);
        0
    }
    fn photosynthesize_in(
        &mut self, outputs: &[Outputs], caps: &[BodyCapacities], _processing: &mut [f64],
        workspace: &mut PhaseWorkspace,
    ) {
        let PhaseWorkspace { photo_requests: requests, footprint, totals, gains, photon_patches, photon_receipts, .. } = workspace;
        requests.clear();
        totals.resize(self.environment.patch_count(), 0.0); totals.fill(0.0);
        for (i, c) in self.cells.iter().enumerate() {
            if !c.alive() { continue; }
            let reserve_room = (caps[i].energy / CHEMICAL_ENERGY_PER_CARBON - c.organic_carbon).max(0.0);
            let capacity = (caps[i].photosynthesis * outputs[i].photosynthesize + caps[i].light_harvest) * DT;
            let capacity = capacity.min(c.internal_inorganic_carbon * CHEMICAL_ENERGY_PER_CARBON)
                .min(reserve_room * CHEMICAL_ENERGY_PER_CARBON);
            if capacity <= 0.0 { continue; }
            photon_footprint_into(&self.environment, c.x, c.y, c.body.area, footprint);
            for &(patch, weight) in footprint.iter() {
                let request = capacity * weight * self.environment.patch_light(patch);
                if request > 0.0 { requests.push((i, patch, request)); totals[patch] += request; }
            }
        }
        gains.resize(self.cells.len(), 0.0); gains.fill(0.0);
        if self.uses_count_chemistry() {
            self.share_photons(requests, photon_patches, photon_receipts);
        } else {
            for &(i, patch, request) in requests.iter() {
                let supply = PHOTO_HABITAT_POWER / PATCHES as f64 * self.environment.patch_light(patch) * DT;
                gains[i] += request * (supply / totals[patch]).min(1.0);
            }
        }
        for (i, c) in self.cells.iter_mut().enumerate() {
            if c.counted.is_some() {
                let fixed = c.count_fix(photon_receipts[i], &caps[i]);
                self.count_flux.fixed+=u64::from(fixed);
                self.carbon_fixed_total += crate::counts::units(fixed);
                self.photosynthesis_total += crate::counts::units(fixed) * 20.0;
                continue;
            }
            let fixed = (gains[i] / CHEMICAL_ENERGY_PER_CARBON).min(c.internal_inorganic_carbon)
                .min((caps[i].energy / CHEMICAL_ENERGY_PER_CARBON - c.organic_carbon).max(0.0));
            if fixed <= 0.0 { continue; }
            c.internal_inorganic_carbon -= fixed;
            c.internal_oxygen += fixed;
            c.organic_carbon += fixed;
            c.carbon_fixation_rate = fixed / DT;
            c.photosynthesis_rate = fixed * CHEMICAL_ENERGY_PER_CARBON / DT;
            self.carbon_fixed_total += fixed;
            self.photosynthesis_total += fixed * CHEMICAL_ENERGY_PER_CARBON;
        }
        self.export_carbon_in(workspace);
    }
    fn physical_targets(
        &self,
        outputs: &[Outputs],
        caps: &[BodyCapacities],
        genomes: &[Genome],
        workspace: &mut PhaseWorkspace,
    ) {
        let PhaseWorkspace {
            geometry,
            cell_grid: grid,
            targets,
            bite_power,
            engulf_targets,
            signals,
            contact_scratch,
            ..
        } = workspace;
        grid.configure(self.environment.width, self.environment.height);
        let max_radius = grid.index_cells(geometry);
        targets.clear();
        targets.resize(geometry.ids.len(), None);
        bite_power.clear();
        bite_power.resize(geometry.ids.len(), 0.0);
        engulf_targets.clear();
        engulf_targets.resize(geometry.ids.len(), None);
        for (i, cell) in self.cells.iter().enumerate() {
            if !geometry.alive[i] || (caps[i].phagocytosis == 0.0 && caps[i].piercing == 0.0) {
                continue;
            }
            let installed = cell.body.cellular.as_ref().map_or(&[][..], |state| state.assemblies.as_slice());
            let mounted =
                installed.iter().any(|a| a.recipe.kind == AssemblyKind::Mechanism(MechanismKind::Pierce));
            let legacy_mass = if mounted { cell.body.functional_mass(Kind::Piercer) } else { caps[i].piercing };
            let legacy_power = if mounted && outputs[i].cellular.is_some() {
                installed
                    .iter()
                    .filter(|a| a.recipe.kind == AssemblyKind::Organelle(Kind::Piercer))
                    .map(|a| a.functional_mass() * a.activity)
                    .sum()
            } else {
                legacy_mass * outputs[i].bite
            };
            let mounted_mouth = installed.iter().any(|a| matches!(a.recipe.kind,
                AssemblyKind::Mechanism(MechanismKind::Engulf) | AssemblyKind::Organelle(Kind::Phagocyte)));
            let legacy_engulf = if mounted_mouth {
                cell.body.organs.iter().filter(|o| o.kind == Kind::Phagocyte).map(|o| o.functional_mass()).sum()
            } else { caps[i].phagocytosis };
            if legacy_engulf <= 0.0 && legacy_power <= 0.0 {
                continue;
            }
            let program = genomes
                .get((cell.genome_id - 1) as usize)
                .map(|g| &g.program)
                .filter(|p| p.has_contact_guards());
            let [x, y] = geometry.positions[i];
            let reach = 0.75 + 12.0 * legacy_mass.sqrt();
            let mut best_bite = f64::INFINITY;
            let mut best_engulf = f64::INFINITY;
            for j in grid.nearby_radius(x, y, geometry.radii[i] + max_radius + reach) {
                if i == j || !geometry.alive[j] {
                    continue;
                }
                let dx = delta(geometry.positions[j][0] - x, self.environment.width);
                let dy = delta(geometry.positions[j][1] - y, self.environment.height);
                let gap = dx.hypot(dy) - geometry.radii[i] - geometry.radii[j];
                if gap > reach
                    || relative_angle(dy.atan2(dx) - geometry.angles[i]).abs() > PI * 0.6
                    || self.environment.segment_blocked((x, y), (x + dx, y + dy))
                {
                    continue;
                }
                let base = contact::ContactCapacity {
                    bite: legacy_power,
                    piercing: legacy_mass,
                };
                let contact::ContactCapacity { bite, piercing } =
                    program.map_or(base, |p| {
                        base.for_peer(
                            cell,
                            &outputs[i],
                            p,
                            &signals[i],
                            self.peer_at(i, j, [x + geometry.radii[i] * dy.atan2(dx).cos(), y + geometry.radii[i] * dy.atan2(dx).sin()], self.division_interface(cell.id, self.cells[j].id), signals),
                            contact_scratch,
                        )
                    });
                let earlier = |best: f64, old: Option<usize>| {
                    gap < best || (gap == best && old.is_none_or(|row| geometry.ids[j] < geometry.ids[row]))
                };
                if bite > 0.0 && gap <= 0.75 + 12.0 * piercing.max(0.0).sqrt() && earlier(best_bite, targets[i])
                {
                    best_bite = gap;
                    targets[i] = Some(j);
                    bite_power[i] = bite;
                }
                if legacy_engulf > 0.0
                    && self.can_swallow(i, j, legacy_engulf, &caps[i])
                    && gap <= 0.75
                    && outputs[i].feed > 0.0
                    && earlier(best_engulf, engulf_targets[i].map(|(row, _)| row))
                {
                    best_engulf = gap;
                    engulf_targets[i] = Some((j, legacy_engulf));
                }
            }
        }
    }

    fn resolve_predation_in(
        &mut self,
        outputs: &[Outputs],
        caps: &[BodyCapacities],
        genomes: &[Genome],
        workspace: &mut PhaseWorkspace,
    ) {
        self.physical_targets(outputs, caps, genomes, workspace);
        self.mounted_engulf_targets(outputs, caps, genomes, workspace);
        self.primitive_piercing_targets(outputs, genomes, workspace);
        let PhaseWorkspace {
            targets, bite_power, mouth_candidates,
            consumed,
            order,
            events,
            damage,
            primitive_hits,
            lethal_hits,
            hit_ranges,
            hits,
            removed,
            meals,
            requests,
            ..
        } = workspace;
        lethal_hits.clear();
        consumed.resize(self.cells.len(), false);
        consumed.fill(false);
        order.clear();
        order.extend(0..self.cells.len());
        order.sort_by(|&a, &b| {
            self.cells[b]
                .body
                .area
                .total_cmp(&self.cells[a].body.area)
                .then_with(|| self.cells[a].id.cmp(&self.cells[b].id))
        });
        for i in order.iter().copied() {
            if !outputs[i].engulf
                || (outputs[i].feed <= 0.0 && caps[i].independent_engulf <= 0.0)
                || !self.cells[i].alive()
                || caps[i].phagocytosis <= 0.0
            {
                continue;
            }
            for &(port, j, available_mass, _) in &mouth_candidates[i] {
                if (port == 0 && self.cells[i].engulf_cooldown > 0.0) || self.cells[i].mouth_ready.contains_key(&port) { continue; }
                let a = &self.cells[i];
                let b = &self.cells[j];
                if !b.alive()
                    || b.integrity > 0.35
                    || a.body.area < 1.6 * b.body.area
                    || b.body.area > available_mass * 40.0
                {
                    continue;
                }
                let cost = 8.0 * b.body.area;
                let potential = b.food_potential();
                let energy = potential.total();
                let minerals = b.mass();
                if a.energy <= cost || a.gut_fraction_for(energy, minerals, &caps[i]) < 1.0 {
                    continue;
                }
                if let Some(q)=b.counted.as_ref(){
                    let oxygen=q.oxygen;let dic=q.dic;
                    let tissue=crate::counts::CountMeal{
                        carbon:b.body.count_material().checked_add(q.carbon).expect("whole prey fits gut"),
                        mineral:b.body.count_material().checked_add(q.mineral).expect("whole prey fits gut"),
                        extra:crate::counts::nearest(b.tissue_extra_energy()),
                    };
                    let contents=q.gut;
                    let soluble_carbon = q.carbon;
                    // Gross contents must fit the actual integer capacity before charging.
                    let (room,minerals)=a.counted.as_ref().unwrap().gut_room(&caps[i]);
                    if tissue.energy()+contents.iter().map(|p|p.energy()).sum::<u64>()>room ||
                        u64::from(tissue.mineral)+contents.iter().map(|p|u64::from(p.mineral)).sum::<u64>()>u64::from(minerals){continue;}
                    counted::deposit(&mut self.environment,&mut self.deposits,b.x,b.y,[0,oxygen,dic]);
                    let a=&mut self.cells[i];let aq=a.counted.as_mut().unwrap();
                    let spent = crate::counts::nearest(cost).min(aq.atp);
                    self.energy_loss.work += u64::from(spent);
                    aq.atp -= spent;
                    let mut structure = tissue;
                    aq.gut[0].join(structure.take(soluble_carbon));
                    aq.gut[1].join(structure);
                    for (store,meal) in aq.gut.iter_mut().zip(contents){store.join(meal);}
                    a.project_counts();
                    if port == 0 { a.engulf_cooldown=2.0; } else { a.mouth_ready.insert(port, self.ticks + 60); }
                    let b=&mut self.cells[j];b.counted=Some(Default::default());b.project_counts();b.integrity=0.0;
                    consumed[j]=true;lethal_hits.push((j,i));self.predations+=1;continue;
                }
                let tissue_energy = potential.body + potential.reserve + potential.atp;
                let tissue_extra = b.tissue_extra_energy();
                let tissue_minerals = b.body.structural_mass() + b.nutrient_stored;
                let contents = b.gut;
                let mixture = Composition::from_body(b.chemical_body_mass() * STRUCTURAL_CARBON_PER_MASS, b.organic_carbon);
                self.environment.release_oxygen(b.x, b.y, b.internal_oxygen);
                self.environment.release_inorganic_carbon(b.x, b.y, b.internal_inorganic_carbon);
                self.cells[i].energy -= cost;
                self.cells[i].ingest_mixture(mixture, tissue_energy, tissue_minerals, tissue_extra);
                for (held, incoming) in self.cells[i].gut.iter_mut().zip(contents) {
                    held.energy += incoming.energy;
                    held.nutrients += incoming.nutrients;
                    held.extra_energy += incoming.extra_energy;
                }
                self.cells[i].refresh_gut_totals();
                if port == 0 { self.cells[i].engulf_cooldown = 2.0; } else { self.cells[i].mouth_ready.insert(port,self.ticks+60); }
                self.cells[j].energy = 0.0;
                self.cells[j].organic_carbon = 0.0;
                self.cells[j].reserve_enrichment = 0.0;
                self.cells[j].structural_enrichment = 0.0;
                self.cells[j].scale_gut(0.0);
                self.cells[j].internal_oxygen = 0.0;
                self.cells[j].internal_inorganic_carbon = 0.0;
                self.cells[j].integrity = 0.0;
                consumed[j] = true;
                lethal_hits.push((j,i));
                self.predations += 1;
            }
        }
        events.clear();
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
            let contact_angle = relative_angle(delta(self.cells[i].y - self.cells[j].y, self.environment.height)
                .atan2(delta(self.cells[i].x - self.cells[j].x, self.environment.width)) - self.cells[j].angle);
            let protection = self.contact_protection(j, contact_angle,
                self.legacy_shield_effort(j, &outputs[j]), caps);
            let has_primitive = self.cells[i].body.cellular.as_ref().is_some_and(|state|
                state.assemblies.iter().any(|assembly| assembly.recipe.kind
                    == AssemblyKind::Mechanism(MechanismKind::Pierce)));
            // Keep the original operation order when every whole-cell piercer
            // accepts. A filtered target receives only its eligible power.
            let coefficient = if !has_primitive && bite_power[i] == caps[i].piercing * outputs[i].bite {
                0.12 * (caps[i].piercing / 0.06) * outputs[i].bite
            } else {
                0.12 * (bite_power[i] / 0.06)
            };
            let inflicted = coefficient * (self.cells[i].mass() / self.cells[j].mass()).min(2.0)
                * (1.0 - protection)
                * DT;
            if inflicted > 0.0 {
                events.push(AttackEvent { victim: j, attacker: i, damage: inflicted,
                    extraction: if has_primitive { 1.0 } else { caps[i].bite_extraction_fraction },
                    angle: contact_angle });
                self.attacks += 1;
            }
        }
        for &(attacker, victim, power, angle) in primitive_hits.iter() {
            if !self.cells[attacker].alive() || !self.cells[victim].alive() { continue; }
            let protection = self.contact_protection(victim, angle,
                self.legacy_shield_effort(victim, &outputs[victim]), caps);
            let inflicted = 0.12 * (power / 0.06)
                * (self.cells[attacker].mass() / self.cells[victim].mass()).min(2.0)
                * (1.0 - protection) * DT;
            if inflicted > 0.0 {
                events.push(AttackEvent { victim, attacker, damage: inflicted, extraction: 0.0, angle });
                self.attacks += 1;
            }
        }
        events.sort_by_key(|event| (self.cells[event.victim].id, self.cells[event.attacker].id));
        damage.resize(self.cells.len(), 0.0);
        damage.fill(0.0);
        hit_ranges.resize(self.cells.len(), (0, 0));
        hit_ranges.fill((0, 0));
        hits.clear();
        removed.resize(self.cells.len(), 0.0);
        removed.fill(0.0);
        meals.resize(self.cells.len(), OrganicMatter::default());
        meals.fill(OrganicMatter::default());
        let mut count_meals=if self.uses_count_chemistry(){vec![crate::counts::CountMeal::default();self.cells.len()]}else{Vec::new()};
        let mut start = 0;
        while start < events.len() {
            let victim = events[start].victim;
            let mut end = start + 1;
            while end < events.len() && events[end].victim == victim {
                end += 1;
            }
            let group = &events[start..end];
            let total: f64 = group.iter().map(|event| event.damage).sum();
            let actual = total.min(self.cells[victim].integrity);
            damage[victim] = actual;
            let fraction = if total > 0.0 { actual / total } else { 0.0 };
            hit_ranges[victim] = (hits.len(), hits.len() + group.len());
            hits.extend(group.iter().map(|event| (event.angle, event.damage * fraction)));
            let victim_cell = &self.cells[victim];
            // A live extractor samples the contents of the reserve. The envelope,
            // machinery, and usable ATP stay in the victim until it is consumed.
            let reserve_energy = victim_cell.organic_carbon * CHEMICAL_ENERGY_PER_CARBON + victim_cell.reserve_enrichment;
            let mineral_ratio = if reserve_energy > 0.0 { victim_cell.nutrient_stored / reserve_energy } else { 0.0 };
            requests.clear();
            requests.extend(group.iter().map(|event| {
                let attacker = event.attacker;
                let raw = 80.0
                    * event.damage
                    * fraction
                    * victim_cell.body.structural_mass()
                    * outputs[attacker].feed
                    * event.extraction;
                raw * self.cells[attacker].gut_fraction_for(raw, raw * mineral_ratio, &caps[attacker])
            }));
            let demand: f64 = requests.iter().sum();
            if demand > 0.0 && reserve_energy > 0.0 {
                let scale = (reserve_energy / demand).min(1.0);
                if let Some(q)=victim_cell.counted.as_ref() {
                    let mut source=crate::counts::CountMeal{carbon:q.carbon,mineral:q.mineral,extra:q.reserve_extra};
                    for (event,&request) in group.iter().zip(requests.iter()) {
                        let i=event.attacker;
                        let attacker = self.cells[i].counted.as_mut().unwrap();
                        let (room, mineral_room) = attacker.gut_room(&caps[i]);
                        let (carriers, _) = source.work_portion(
                            &mut attacker.rate_credit[16], request * scale,
                            room.saturating_sub(count_meals[i].energy()),
                            mineral_room.saturating_sub(count_meals[i].mineral),
                        );
                        count_meals[i].join(source.take(carriers));
                    }
                    let cell=&mut self.cells[victim];let q=cell.counted.as_mut().unwrap();
                    q.carbon=source.carbon;q.mineral=source.mineral;q.reserve_extra=source.extra;cell.project_counts();
                    start=end;continue;
                }
                let mut remaining = reserve_energy;
                for (event, request) in group.iter().zip(requests.iter()) {
                    let energy = (request * scale).min(remaining);
                    let share = energy / reserve_energy;
                    let meal = &mut meals[event.attacker];
                    meal.energy += energy;
                    meal.extra_energy += victim_cell.reserve_enrichment * share;
                    meal.nutrients += victim_cell.nutrient_stored * share;
                    removed[victim] += share;
                    remaining -= energy;
                }
            }
            start = end;
        }
        for (i, c) in self.cells.iter_mut().enumerate() {
            if !c.alive() {
                continue;
            }
            if let Some(q)=c.counted.as_mut(){
                q.gut[0].join(count_meals[i]);c.project_counts();
            }else{
                let kept = (1.0 - removed[i]).max(0.0);
                c.organic_carbon *= kept;
                c.reserve_enrichment *= kept;
                c.nutrient_stored *= kept;
                let meal = meals[i];
                c.ingest_mixture(Composition::SOLUBLE, meal.energy, meal.nutrients, meal.extra_energy);
            }
            c.integrity = (c.integrity - damage[i]).max(0.0);
            let (from, to) = hit_ranges[i];
            c.body.damage_attributed(damage[i], &hits[from..to]);
            if damage[i] > 0.0 {
                self.environment.emit_organic(c.x, c.y, damage[i] * 2.0);
                if !c.alive() {
                    self.predations += 1;
                    // Grouped hits are in stable cell-ID order. Credit the attacker
                    // that dealt most damage in the killing tick, including all its tools.
                    let group = &events[from..to];
                    let mut winner = (group[0].attacker, 0.0);
                    for hits in group.chunk_by(|a,b| a.attacker == b.attacker) {
                        let total = hits.iter().map(|hit| hit.damage).sum::<f64>();
                        if total > winner.1 { winner = (hits[0].attacker, total); }
                    }
                    lethal_hits.push((i, winner.0));
                }
            }
        }
        for &(victim,killer) in lethal_hits.iter() {
            self.census.events(self.cells[victim].genome_id).predation += 1;
            self.census.events(self.cells[killer].genome_id).kills += 1;
        }
    }
    fn reserve_organic_packets(&mut self, packet_energy: f64, packet_overhead: usize) -> Result<(), String> {
        let energetic_packets = (packet_energy / ORGANIC_PACKET_ENERGY).ceil();
        if !energetic_packets.is_finite() || energetic_packets >= usize::MAX as f64 {
            return Err("Simulation paused: organic material exceeds addressable memory.".into());
        }
        let possible_particles = packet_overhead
            .checked_add(energetic_packets as usize)
            .ok_or("Simulation paused: organic material exceeds addressable memory.")?;
        if possible_particles as u128 > (u64::MAX / 2 - self.next_food_id) as u128 {
            return Err(
                "Simulation paused: the organic particle identifier sequence is exhausted.".into(),
            );
        }
        self.food.try_reserve(possible_particles).map_err(|_| {
            "Simulation paused: not enough memory for organic material.".to_string()
        })?;
        Ok(())
    }
    fn remove_dead_in(&mut self, phases: &mut PhaseWorkspace, withdrawn_cell: Option<u32>) {
        phases.emissions.clear();
        // A newborn can immediately lose all usable energy when its inherited
        // storage overflows. Return its tissue too; only pre-birth cells could
        // have been consumed during this tick's predation phase.
        for (i, c) in self.cells.iter().enumerate() {
            if c.alive() { continue; }
            let events = self.census.events(c.genome_id);
            events.deaths += 1;
            // Fatal damage and engulfment take precedence over empty stores.
            if Some(c.id) != withdrawn_cell && c.energy <= 0.0 && c.integrity > 0.0 && c.body.support_mass() > 0.0 {
                events.depleted += 1;
            }
            if phases.consumed.get(i).copied().unwrap_or(false) { continue; }
            if let Some(q)=&c.counted {
                counted::deposit(&mut self.environment,&mut self.deposits,c.x,c.y,[0,q.oxygen,q.dic]);
            } else {
                self.environment.release_oxygen(c.x, c.y, c.internal_oxygen);
                self.environment.release_inorganic_carbon(c.x, c.y, c.internal_inorganic_carbon);
            }
            let extra_energy = c.tissue_extra_energy();
            phases.emissions.push(OrganicEmission::new(
                OrganicKind::Tissue,
                c.x,
                c.y,
                (c.chemical_body_mass() * STRUCTURAL_CARBON_PER_MASS + c.organic_carbon) * CHEMICAL_ENERGY_PER_CARBON + extra_energy,
                c.chemical_body_mass() + c.nutrient_stored,
            ).enriched(extra_energy).material(Composition::from_body(c.chemical_body_mass() * STRUCTURAL_CARBON_PER_MASS, c.organic_carbon)));
            for (index, pool) in c.gut.iter().enumerate() {
                phases.emissions.push(OrganicEmission::new(
                    OrganicKind::Tissue,
                    c.x,
                    c.y,
                    pool.energy,
                    pool.nutrients,
                ).enriched(pool.extra_energy).material(Composition::pool(index)));
            }
        }
        self.emit_organic_pending(phases);
        let before = self.cells.len();
        self.cells.retain(Cell::alive);
        self.deaths += (before - self.cells.len()) as u64;
    }
    /// Merge locally within a substrate. Building the index once per batch
    /// avoids one full particle scan for every photosynthesizing cell.
    fn emit_organic_pending(&mut self, workspace: &mut PhaseWorkspace) {
        if self.uses_count_chemistry() {return self.count_emit(workspace);}
        let PhaseWorkspace {
            emissions,
            food_grid: grid,
            nearby,
            ..
        } = workspace;
        if emissions
            .iter()
            .all(|e| e.energy <= 0.0 && e.nutrients <= 0.0)
        {
            emissions.clear();
            return;
        }
        grid.configure(self.environment.width,self.environment.height);
        grid.clear();
        for (i, food) in self.food.iter().enumerate() {
            grid.insert(food.x, food.y, i);
        }
        for emission in emissions.drain(..) {
            let OrganicEmission {
                merge_radius,
                composition,
                kind,
                x,
                y,
                mut energy,
                mut nutrients,
                mut extra_energy,
                outside,
            } = emission;
            self.environment.emit_organic(x, y, energy * 0.08);
            if energy <= 0.0 {
                self.environment.recycle_nutrients(x, y, nutrients);
                continue;
            }
            nearby.clear();
            nearby.extend(grid.nearby_radius(x, y, merge_radius));
            nearby.sort_unstable();
            for &index in nearby.iter() {
                let food = &mut self.food[index];
                if food.kind != kind
                    || delta(food.x - x, self.environment.width).hypot(delta(food.y - y, self.environment.height)) > merge_radius
                    || self.environment.segment_blocked((x,y),(food.x,food.y))
                {
                    continue;
                }
                let amount = energy.min((ORGANIC_PACKET_ENERGY - food.energy).max(0.0));
                if amount <= 0.0 {
                    continue;
                }
                // A moving or growing envelope may have overtaken the last
                // small packet. Push that existing packet out before adding
                // material, rather than rejecting it and creating one new
                // microscopic particle every tick. This changes no contents.
                if let Some(envelope) = outside {
                    let from = (food.x, food.y);
                    let (cx, cy, radius) = envelope;
                    // Overtaken secretion joins the current release point.
                    // Both points are within the 24-unit local merge radius;
                    // moving it tangentially around the envelope instead would
                    // repeatedly shed tiny drops from a translating cell.
                    let to = if delta(from.0 - cx, self.environment.width).hypot(delta(from.1 - cy, self.environment.height)) + 1e-9
                        < radius + 2.5 + ORGANIC_RELEASE_CLEARANCE
                    {
                        outside_envelope(&self.environment, (x, y), envelope, (x, y))
                    } else {
                        from
                    };
                    food.x = to.0;
                    food.y = to.1;
                    grid.relocate(from, to, index);
                }
                let material = nutrients * (amount / energy);
                let extra = extra_energy * (amount / energy);
                food.composition = food.composition.mix(food.carbon(), composition, (amount - extra) / CHEMICAL_ENERGY_PER_CARBON);
                food.energy += amount;
                food.nutrients += material;
                food.extra_energy += extra;
                extra_energy -= extra;
                energy -= amount;
                nutrients = (nutrients - material).max(0.0);
                if energy <= 0.0 {
                    break;
                }
            }
            let mut n = 0;
            while energy > 0.0 {
                let amount = energy.min(ORGANIC_PACKET_ENERGY);
                let material = nutrients * (amount / energy);
                let angle = n as f64 * 2.399963229728653;
                let radius = ((n as f64).sqrt() * 2.0).min(merge_radius);
                let mut px = wrap(x + angle.cos() * radius, self.environment.width);
                let mut py = wrap(y + angle.sin() * radius, self.environment.height);
                if let Some(envelope) = outside {
                    (px, py) = outside_envelope(&self.environment, (px, py), envelope, (x, y));
                }
                let index = self.food.len();
                // try_step preflights storage and identifier headroom before
                // any resources change. Failure here is an invariant violation.
                assert!(
                    self.particle(kind, px, py, amount, material),
                    "Unable to store accounted organic material"
                );
                let extra = extra_energy * (amount / energy);
                self.food[index].composition = composition;
                self.food[index].extra_energy = extra;
                extra_energy -= extra;
                grid.insert(self.food[index].x, self.food[index].y, index);
                energy -= amount;
                nutrients = (nutrients - material).max(0.0);
                n += 1;
            }
            self.environment.recycle_nutrients(x, y, nutrients);
        }
    }
    fn resolve_collisions_in(&mut self, workspace: &mut PhaseWorkspace) {
        let PhaseWorkspace {
            geometry,
            cell_grid: grid,
            candidates,
            ..
        } = workspace;
        geometry.rebuild(&self.cells, false, true);
        grid.configure(self.environment.width,self.environment.height);
        let max_radius = grid.index_cells(geometry);
        // The broad-phase buckets deliberately stay frozen for the whole pass.
        // Pair resolution updates positions/velocities in both representations.
        for i in 0..self.cells.len() {
            if !geometry.alive[i] {
                continue;
            }
            candidates.clear();
            candidates.extend(
                grid.nearby_radius(
                    geometry.positions[i][0],
                    geometry.positions[i][1],
                    geometry.radii[i] + max_radius,
                )
                .filter(|&j| j > i),
            );
            // The frozen grid inserts each live cell once and visits each bucket once.
            candidates.sort_unstable();
            for &j in candidates.iter() {
                let (a, b) = self.cells.split_at_mut(j);
                let a = &mut a[i];
                let b = &mut b[0];
                if !geometry.alive[j] {
                    continue;
                }
                let dx = delta(geometry.positions[j][0] - geometry.positions[i][0], self.environment.width);
                let dy = delta(geometry.positions[j][1] - geometry.positions[i][1], self.environment.height);
                let contact = geometry.radii[i] + geometry.radii[j];
                if dx.abs() >= contact || dy.abs() >= contact {
                    continue;
                }
                let distance = dx.hypot(dy);
                if distance >= contact {
                    continue;
                }
                if self.environment.segment_blocked((a.x,a.y),(b.x,b.y)) {continue;}
                let (nx, ny) = if distance > 1e-5 {
                    (dx / distance, dy / distance)
                } else {
                    let a = (geometry.ids[i].wrapping_mul(13) ^ geometry.ids[j]) as f64;
                    (a.cos(), a.sin())
                };
                let ma = geometry.inertia[i];
                let mb = geometry.inertia[j];
                let overlap = contact - distance;
                (a.x,a.y)=self.environment.move_circle((a.x,a.y),
                    (a.x-nx*overlap*mb/(ma+mb),a.y-ny*overlap*mb/(ma+mb)),geometry.radii[i]);
                (b.x,b.y)=self.environment.move_circle((b.x,b.y),
                    (b.x+nx*overlap*ma/(ma+mb),b.y+ny*overlap*ma/(ma+mb)),geometry.radii[j]);
                let approaching = (geometry.velocities[j][0] - geometry.velocities[i][0]) * nx
                    + (geometry.velocities[j][1] - geometry.velocities[i][1]) * ny;
                if approaching < 0.0 {
                    let impulse = -approaching / (1.0 / ma + 1.0 / mb);
                    a.vx -= nx * impulse / ma;
                    a.vy -= ny * impulse / ma;
                    b.vx += nx * impulse / mb;
                    b.vy += ny * impulse / mb;
                }
                geometry.positions[i] = [a.x, a.y];
                geometry.positions[j] = [b.x, b.y];
                geometry.velocities[i] = [a.vx, a.vy];
                geometry.velocities[j] = [b.vx, b.vy];
            }
        }
    }
    // Isolated phase entry points used by existing physical regression fixtures.
    // Production ticks pass the persistent workspace through every phase.
    #[cfg(test)]
    fn resolve_collisions(&mut self) {
        self.resolve_collisions_in(&mut PhaseWorkspace::default());
    }
    #[cfg(test)]
    fn eat(&mut self, outputs: &[Outputs], caps: &[BodyCapacities]) {
        let mut workspace = PhaseWorkspace::default();
        workspace.geometry.rebuild(&self.cells, false, false);
        self.eat_in(outputs, caps, &[], &mut workspace);
    }
    #[cfg(test)]
    fn resolve_predation(&mut self, outputs: &[Outputs], caps: &[BodyCapacities]) -> Vec<bool> {
        let mut workspace = PhaseWorkspace::default();
        workspace.geometry.rebuild(&self.cells, false, false);
        self.resolve_predation_in(outputs, caps, &[], &mut workspace);
        workspace.consumed
    }
    #[cfg(test)]
    fn digest(&mut self, outputs: &[Outputs], caps: &[BodyCapacities], processing: &mut [f64]) {
        self.digest_in(outputs, caps, processing, &mut PhaseWorkspace::default());
    }
    #[cfg(test)]
    fn photosynthesize(
        &mut self,
        outputs: &[Outputs],
        caps: &[BodyCapacities],
        processing: &mut [f64],
    ) {
        self.photosynthesize_in(outputs, caps, processing, &mut PhaseWorkspace::default());
    }
    #[cfg(test)]
    fn spill_excess_stores(&mut self) {
        self.spill_excess_stores_in(&mut PhaseWorkspace::default(), true);
    }
    #[cfg(test)]
    fn emit_organic_batch(&mut self, emissions: Vec<OrganicEmission>) {
        let mut workspace = PhaseWorkspace {
            emissions,
            ..PhaseWorkspace::default()
        };
        self.emit_organic_pending(&mut workspace);
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
            self.environment.width as f64,
            self.environment.height as f64,
            0.0,
            0.0,
            0.0, // Reserved former provision-rate slot.
        ]);
        for c in &self.cells {
            self.snapshot_cell_into(c, target);
        }
        for f in &self.food {
            target.extend([f.x, f.y, f.energy, f.nutrients, f.kind as u8 as f64, f.composition.ligand]);
        }
    }
    pub fn body_json(&self, id: u32, genomes: &[Genome]) -> Option<String> {
        let c = self.cells.iter().find(|c| c.id == id && c.alive())?;
        let space = c.body.space();
        let surface = c.body.surface();
        let capacity = c.body.capacities().assembly;
        let mut json=format!("{{\"cellId\":{},\"area\":{},\"mass\":{},\"spaceUsed\":{},\"spaceCapacity\":{},\"surfaceUsed\":{},\"surfaceCapacity\":{},\"constructionRate\":{},\"recyclingRate\":{},\"repairRate\":{},\"synthesisRate\":{},\"assemblyCapacity\":{},\"organs\":[",
            c.id,c.body.area,c.mass(),space.used,space.capacity,surface.used,surface.capacity,c.construction_rate,c.recycling_rate,c.repair_rate,c.synthesis_rate,capacity);
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
                "{{\"kind\":\"{}\",\"count\":{},\"size\":{},\"build\":{},\"recycle\":{},\"proteinMass\":{}}}",
                kind.name(),
                c.requested.target_counts[i],
                c.requested.target_sizes[i],
                c.requested.build[i],
                c.requested.recycle[i],
                c.body.precursor_mass[i]
            );
        }
        json.push(']');
        let potential = c.food_potential();
        write!(json, ",\"foodPotential\":{{\"body\":{},\"reserve\":{},\"gut\":{},\"atp\":{},\"total\":{}}}",
            potential.body, potential.reserve, potential.gut, potential.atp, potential.total()).unwrap();
        let [energy_net, mineral_net, carbon_net] = c.junction_net;
        let mut neighbor_ids = Vec::new();
        let mut colony = Vec::new();
        let mut feeding = Vec::new();
        let mut contact = Vec::new();
        for (a, b, kind) in self.presented_bonds(Some(id)) {
            let peer = if a == id { b } else { a };
            neighbor_ids.push(peer);
            match kind {
                adhesion::BondKind::Colony => colony.push(peer),
                adhesion::BondKind::Feeding => feeding.push(peer),
                adhesion::BondKind::Contact => contact.push(peer),
            }
        }
        write!(json, r#","colony":{{"neighbors":{},"neighborIds":{neighbor_ids:?},"colonyNeighborIds":{colony:?},"feedingNeighborIds":{feeding:?},"contactNeighborIds":{contact:?},"energyNet":{energy_net},"mineralNet":{mineral_net},"carbonNet":{carbon_net}}}"#, neighbor_ids.len()).unwrap();
        let cellular_program = genomes.get((c.genome_id - 1) as usize)
            .and_then(|genome| genome.program.cellular());
        if cellular_program.is_some() || c.body.cellular.is_some() {
            json.push_str(",\"cellular\":");
            json.push_str(&c.body.cellular_json(cellular_program));
        }
        json.push('}');
        Some(json)
    }
}
#[derive(Clone)]
enum ObservedMotion {
    Relative(f64, f64),
    ParticleAt(f64, f64),
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
    motion: ObservedMotion,
    resistance: f64,
}
impl Observation {
    fn relative_motion(&self, cell: &Cell, environment: &Environment, time: f64) -> (f64, f64) {
        match self.motion {
            ObservedMotion::Relative(vx, vy) => (vx, vy),
            ObservedMotion::ParticleAt(x, y) => {
                let (vx, vy) = environment.flow_at(x, y, time);
                (vx - cell.vx, vy - cell.vy)
            }
        }
    }
}
/// Place a whole organic particle beyond its emitter's capture reach on wrapped geometry.
/// The emission point gives the outward direction when a particle is exactly
/// at the cell center. The same rule handles new and coalescing packets.
fn outside_envelope(
    environment: &Environment,
    point: (f64, f64),
    envelope: (f64, f64, f64),
    emission: (f64, f64),
) -> (f64, f64) {
    let (cx, cy, radius) = envelope;
    let release_radius = radius + 2.5 + ORGANIC_RELEASE_CLEARANCE;
    let dx = delta(point.0 - cx, environment.width);
    let dy = delta(point.1 - cy, environment.height);
    if dx.hypot(dy) + 1e-9 >= release_radius {
        return point;
    }
    let outward = if dx.hypot(dy) > 1e-12 {
        dy.atan2(dx)
    } else {
        delta(emission.1 - cy, environment.height).atan2(delta(emission.0 - cx, environment.width))
    };
    (
        wrap(cx + outward.cos() * release_radius, environment.width),
        wrap(cy + outward.sin() * release_radius, environment.height),
    )
}
fn shared_digestive_work(budget: f64, demands: [f64; 3], weights: [f64; 3]) -> [f64; 3] {
    let mut result = [0.0; 3];
    let mut remaining = budget.max(0.0);
    for _ in 0..3 {
        let total: f64 = (0..3)
            .filter(|&k| demands[k] - result[k] > 1e-12)
            .map(|k| weights[k])
            .sum();
        if total <= 0.0 || remaining <= 0.0 {
            break;
        }
        let before = remaining;
        for k in 0..3 {
            let work = (before * weights[k] / total).min((demands[k] - result[k]).max(0.0));
            result[k] += work;
            remaining = (remaining - work).max(0.0);
        }
    }
    result
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
#[cfg(test)]
fn photon_footprint(x: f64, y: f64, area: f64) -> Vec<(usize, f64)> {
    let mut result = Vec::new();
    photon_footprint_into(&Environment::default(), x, y, area, &mut result);
    result
}
fn photon_footprint_into(environment: &Environment, x: f64, y: f64, area: f64, result: &mut Vec<(usize, f64)>) {
    let radius = (60.0 * area.sqrt()).max(20.0);
    let span = (radius / 20.0).ceil() as isize;
    let center = environment.patch_index(x, y);
    let col = (center % environment.columns) as isize;
    let row = (center / environment.columns) as isize;
    let cols = (2 * span + 1).min(environment.columns as isize);
    let rows = (2 * span + 1).min(environment.rows as isize);
    result.clear();
    let mut total = 0.0;
    for dy in 0..rows {
        for dx in 0..cols {
            let c = (col - span + dx).rem_euclid(environment.columns as isize) as usize;
            let r = (row - span + dy).rem_euclid(environment.rows as isize) as usize;
            let patch = r * environment.columns + c;
            let (px, py) = environment.patch_position(patch);
            let scaled =
                (delta(px - x, environment.width).powi(2) + delta(py - y, environment.height).powi(2)) / radius.powi(2);
            if scaled < 1.0 {
                let weight = (1.0 - scaled * scaled).powi(2);
                result.push((patch, weight));
                total += weight;
            }
        }
    }
    for (_, weight) in result.iter_mut() {
        *weight /= total;
    }
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

#[cfg(test)]
mod workspace_tests {
    use super::*;
    use crate::checkpoint::{Reader, Writer};

    fn state(world: &World) -> Vec<u8> {
        let mut writer = Writer::new();
        world.write_checkpoint(&mut writer);
        assert!(!writer.failed());
        writer.0
    }

    fn genomes() -> Vec<Genome> {
        let source = "thrust=0.4;turn=0.1;divide=false;photosynthesize=1;feed=1;absorb=1;\
            bite=0.3;engulf=true;rs001=object.token when object.valid;scan.target=rs001;";
        vec![Genome::new(
            source.to_owned(),
            Program::compile(source).unwrap(),
            0,
            0,
        )]
    }

    #[test]
    fn reused_tick_workspace_matches_fresh_restores_across_population_changes_and_reset() {
        let mut reused = World::empty(117);
        let mut programs = genomes();
        reused.environment.configure(0.5, 0.8, 12.0);
        reused.environment.add_source(1, 10.0, 755.0, 0.3, 80.0);
        reused.environment.add_source(2, 1195.0, 5.0, -0.7, 90.0);

        reused.set_mutation_rate(0.0);
        for fixture in [
            BodyFixture::Producer,
            BodyFixture::Scavenger,
            BodyFixture::Hunter,
        ] {
            reused
                .try_seed_fixture(1, 2, 1190.0, 750.0, fixture)
                .unwrap();
        }
        reused.fixture_exudate(1190.0, 750.0, 20);

        for tick in 0..32 {
            if tick == 5 {
                reused
                    .try_seed_fixture(1, 3, 8.0, 8.0, BodyFixture::Scavenger)
                    .unwrap();
            } else if tick == 10 {
                // Still valid at checkpoint time; ordinary maintenance kills it.
                reused.cells[0].energy = 1e-12;
            } else if tick == 18 {
                reused.reset_empty(92);
                reused.fixture_exudate(-1.0, -1.0, 20);
                reused.try_seed_cells(1, 2, 20.0, 20.0).unwrap();
            } else if tick == 22 {
                reused
                    .try_seed_fixture(1, 2, 20.0, 20.0, BodyFixture::Hunter)
                    .unwrap();
            }
            let bytes = state(&reused);
            let mut reader = Reader::new(&bytes);
            let mut fresh = World::read_checkpoint(&mut reader, programs.len(), crate::checkpoint::VERSION).unwrap();
            assert!(reader.finished());
            assert!(fresh.workspace.capacities.is_empty());
            let mut fresh_programs = programs.clone();
            reused.try_step(&mut programs).unwrap();
            fresh.try_step(&mut fresh_programs).unwrap();
            assert_eq!(state(&reused), state(&fresh), "tick {tick}");
        }
    }

    #[test]
    fn successful_ticks_retain_buffers_and_failed_preflight_preserves_them_and_state() {
        let mut world = World::empty(43);
        let mut programs = genomes();

        world.set_mutation_rate(0.0);
        world.try_seed_cells(1, 2, 200.0, 200.0).unwrap();
        world.try_step(&mut programs).unwrap();
        let inputs = world.workspace.observations.inputs.as_ptr();
        let outputs = world.workspace.outputs.as_ptr();
        let capacities = world.workspace.capacities.as_ptr();
        let processing = world.workspace.processing.as_ptr();
        world.try_step(&mut programs).unwrap();
        assert_eq!(world.workspace.observations.inputs.as_ptr(), inputs);
        assert_eq!(world.workspace.outputs.as_ptr(), outputs);
        assert_eq!(world.workspace.capacities.as_ptr(), capacities);
        assert_eq!(world.workspace.processing.as_ptr(), processing);

        let next_food_id = world.next_food_id;
        world.next_food_id = u64::MAX / 2;
        let before = state(&world);
        assert!(world.try_step(&mut programs).is_err());
        assert_eq!(state(&world), before);
        assert_eq!(world.workspace.outputs.as_ptr(), outputs);
        assert_eq!(world.workspace.capacities.as_ptr(), capacities);
        world.next_food_id = next_food_id;
        world.try_step(&mut programs).unwrap();

        world.reset_empty(123);
        assert!(world.workspace.capacities.is_empty());
        assert!(world.workspace.observations.inputs.is_empty());
        world.try_step(&mut programs).unwrap();
    }
}
