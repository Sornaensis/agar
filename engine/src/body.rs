//! Paid, individually identifiable machinery inside a separately grown envelope.
//!
//! Minerals are the conserved structural material proxy. Energy spent on growth
//! is split into recoverable embodied energy and heat; neither targets nor damage
//! create or erase material. Storage overflow is deliberately handled by World.
use crate::cellular::{AssemblyKind, CellularState, MechanismKind};
use crate::checkpoint::{ensure, Reader, Result as CheckpointResult, Writer};

pub const FAMILY_COUNT: usize = 16;
pub const BASE_RADIUS: f64 = 7.0;
pub const MIN_AREA: f64 = 0.25;
/// Radius <= 760 / 8 in the 1200 x 760 torus. This is a geometric bound, not a
/// biological size/population quota. World may supply a tighter geometry limit.
pub const MAX_AREA: f64 = (760.0 / (8.0 * BASE_RADIUS)) * (760.0 / (8.0 * BASE_RADIUS));
pub const SCAFFOLD_MASS_PER_AREA: f64 = 0.35;
pub const SCAFFOLD_ENERGY_PER_MASS: f64 = 80.0;
pub const EMBODIED_FRACTION: f64 = 0.25;
pub const RECOVERY_FRACTION: f64 = 0.5;
pub const MIN_FUNCTIONAL_INTEGRITY: f64 = 0.2;
pub const CORE_MASS_PER_AREA: f64 = 0.04;
pub const FOUNDER_AREA: f64 = 1.0;
pub const FOUNDER_ENERGY_FRACTION: f64 = 0.85;
pub const FOUNDER_MINERAL_FRACTION: f64 = 0.6;
const INTERNAL_PER_AREA: f64 = 0.9;
const SURFACE_PER_RADIUS: f64 = 0.9;
const RECYCLE_ENERGY_PER_MASS: f64 = 8.0;
const REPAIR_ENERGY_PER_MASS: f64 = 60.0;
const MAINTENANCE_RESERVE_FRACTION: f64 = 0.15;
/// Translation and assembly together retain the original construction budget.
/// The split represents the work to synthesize a family-specific protein and
/// then fold/install it in the organ. Both stages share the same core machinery.
pub const PROTEIN_SYNTHESIS_FRACTION: f64 = 0.6;
pub const PROTEIN_HALF_LIFE: f64 = 45.0;
pub const PROTEIN_INTERNAL_FOOTPRINT: f64 = 1.0;
pub const PROTEIN_UPKEEP: f64 = 0.6;
const EPS: f64 = 1e-12;
const RESOURCE_COUNT: usize = 5 + FAMILY_COUNT;
// A finished organ occupies at least .007 area. There can additionally be one
// incomplete construction per family; counts are bounded by actual geometry.
const MAX_ORGANS: usize = (MAX_AREA * INTERNAL_PER_AREA / 0.007) as usize + FAMILY_COUNT + 1;
pub const MIN_ORGANELLE_MASS: [f64; FAMILY_COUNT] = [
    0.04, 0.04, 0.03, 0.04, 0.04, 0.015, 0.025, 0.025, 0.025, 0.02, 0.025, 0.01, 0.01, 0.015, 0.01,
    0.02,
];
const INTERNAL: [f64; FAMILY_COUNT] = [
    1.0, 1.35, 1.0, 2.5, 2.4, 0.7, 0.8, 0.65, 0.8, 0.35, 1.0, 0.8, 0.8, 1.0, 0.7, 1.3,
];
const SURFACE: [f64; FAMILY_COUNT] = [
    0.0, 0.55, 0.0, 0.0, 0.0, 1.8, 2.2, 1.5, 2.0, 3.0, 0.0, 1.2, 1.1, 1.5, 1.1, 2.0,
];
const CONSTRUCTION_ENERGY: [f64; FAMILY_COUNT] = [
    80.0, 90.0, 80.0, 65.0, 55.0, 70.0, 85.0, 80.0, 100.0, 65.0, 100.0, 80.0, 80.0, 90.0, 80.0,
    120.0,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum OrganelleKind {
    Core,
    Photosynthetic,
    Metabolic,
    Digestive,
    Reserve,
    Transporter,
    Phagocyte,
    Flagellum,
    Piercer,
    Cortex,
    Repair,
    Photoreceptor,
    Thermoreceptor,
    Chemoreceptor,
    Mechanoreceptor,
    Optical,
}
impl OrganelleKind {
    pub const ALL: [Self; FAMILY_COUNT] = [
        Self::Core,
        Self::Photosynthetic,
        Self::Metabolic,
        Self::Digestive,
        Self::Reserve,
        Self::Transporter,
        Self::Phagocyte,
        Self::Flagellum,
        Self::Piercer,
        Self::Cortex,
        Self::Repair,
        Self::Photoreceptor,
        Self::Thermoreceptor,
        Self::Chemoreceptor,
        Self::Mechanoreceptor,
        Self::Optical,
    ];
    pub fn name(self) -> &'static str {
        [
            "core",
            "photosynthetic",
            "metabolic",
            "digestive",
            "reserve",
            "transporter",
            "phagocyte",
            "flagellum",
            "piercer",
            "cortex",
            "repair",
            "photoreceptor",
            "thermoreceptor",
            "chemoreceptor",
            "mechanoreceptor",
            "optical",
        ][self as usize]
    }
    pub fn label(self) -> &'static str {
        [
            "Core",
            "Photosynthetic unit",
            "Metabolic complex",
            "Digestive vacuole",
            "Reserve vacuole",
            "Mineral transporter",
            "Phagocytic cup",
            "Flagellum",
            "Piercing apparatus",
            "Protective cortex",
            "Repair complex",
            "Photoreceptor",
            "Thermoreceptor",
            "Chemoreceptor",
            "Mechanoreceptor",
            "Optical detector",
        ][self as usize]
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
    pub fn min_mass(self) -> f64 {
        MIN_ORGANELLE_MASS[self as usize]
    }
    pub fn minimum_mass(self) -> f64 {
        self.min_mass()
    }
    pub fn internal_footprint(self) -> f64 {
        INTERNAL[self as usize]
    }
    pub fn surface_footprint(self) -> f64 {
        SURFACE[self as usize]
    }
    pub fn construction_cost(self) -> f64 {
        CONSTRUCTION_ENERGY[self as usize]
    }
    pub fn maximum_mass(self) -> f64 {
        let internal = INTERNAL_PER_AREA * MAX_AREA / self.internal_footprint();
        if self.surface_footprint() == 0.0 {
            internal
        } else {
            internal.min(SURFACE_PER_RADIUS * MAX_AREA.sqrt() / self.surface_footprint())
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BodyControls {
    pub target_counts: [u32; FAMILY_COUNT],
    pub target_sizes: [f64; FAMILY_COUNT],
    /// Gene expression / translation effort. Existing callers retain the field
    /// name, but protein must now be paid for before it can assemble an organ.
    pub build: [f64; FAMILY_COUNT],
    pub recycle: [f64; FAMILY_COUNT],
}
impl Default for BodyControls {
    fn default() -> Self {
        Self {
            target_counts: [0; FAMILY_COUNT],
            target_sizes: MIN_ORGANELLE_MASS,
            build: [0.0; FAMILY_COUNT],
            recycle: [0.0; FAMILY_COUNT],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Organelle {
    pub id: u32,
    pub kind: OrganelleKind,
    /// Conserved material, including incomplete or damaged structure.
    pub mass: f64,
    pub integrity: f64,
    pub embodied_energy: f64,
}
impl Organelle {
    pub fn functional_mass(&self) -> f64 {
        if self.mass + EPS < self.kind.min_mass() || self.integrity < MIN_FUNCTIONAL_INTEGRITY {
            0.0
        } else {
            self.mass * self.integrity
        }
    }
    pub fn internal_space(&self) -> f64 {
        self.mass * self.kind.internal_footprint()
    }
    pub fn surface_space(&self) -> f64 {
        self.mass * self.kind.surface_footprint()
    }
    pub fn building(&self) -> f64 {
        if self.mass + EPS >= self.kind.min_mass() {
            0.0
        } else {
            self.kind.min_mass() - self.mass
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpaceBudget {
    pub capacity: f64,
    pub used: f64,
    pub free: f64,
}

/// Derived values for one organelle family in an immutable body snapshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrganelleSummary {
    pub count: u32,
    pub mass: f64,
    pub functional_mass: f64,
    pub capacity: f64,
    pub damaged: f64,
    pub building: f64,
}

impl Default for OrganelleSummary {
    fn default() -> Self {
        Self {
            count: 0,
            // Iterator::sum::<f64>() starts at negative zero. Keep empty
            // families bit-identical to the individual query methods.
            mass: -0.0,
            functional_mass: -0.0,
            capacity: -0.0,
            damaged: -0.0,
            building: -0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BodyCapacities {
    pub energy: f64,
    pub minerals: f64,
    pub gut_energy: f64,
    pub gut_minerals: f64,
    pub photosynthesis: f64,
    pub metabolism: f64,
    pub digestion: f64,
    pub uptake: f64,
    pub thrust: f64,
    pub turn: f64,
    pub capture: f64,
    pub piercing: f64,
    pub protection: f64,
    pub repair: f64,
    pub assembly: f64,
    pub light_sensing: f64,
    pub heat_sensing: f64,
    pub chemical_sensing: f64,
    pub mechanical_sensing: f64,
    pub optical_sensing: f64,
    pub optical_range: f64,
    // Descriptive storage/action aliases for callers that don't use short names.
    pub energy_storage: f64,
    pub mineral_storage: f64,
    pub gut_storage: f64,
    pub propulsion: f64,
    pub phagocytosis: f64,
    pub attack: f64,
    pub defense: f64,
    /// Independent reaction capacity. These operations need no bundled
    /// metabolic organelle and cannot process a different substrate.
    pub light_harvest: f64,
    pub substrate_digestion: [f64; 3],
    pub independent_engulf: f64,
    /// Only legacy piercing machinery includes extraction from a living cell.
    pub bite_extraction_fraction: f64,
}
pub type Capacities = BodyCapacities;

/// Preassembled specimens used only to isolate hardware in native regression
/// tests. Production founders never select or receive one of these bodies.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BodyFixture {
    Bare,
    Producer,
    Scavenger,
    Hunter,
    Mixed,
}
#[cfg(test)]
impl BodyFixture {
    pub const ALL: [Self; 5] = [
        Self::Bare,
        Self::Producer,
        Self::Scavenger,
        Self::Hunter,
        Self::Mixed,
    ];
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FounderBudget {
    /// Initially available reserves, separate from structural inventories.
    pub energy: f64,
    pub minerals: f64,
    /// All resources charged for introduction, including fabrication heat.
    pub introduced_energy: f64,
    pub introduced_minerals: f64,
    pub construction_energy: f64,
    pub dissipated_energy: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorkReport {
    pub energy_spent: f64,
    pub minerals_spent: f64,
    pub energy_recovered: f64,
    pub minerals_recovered: f64,
    pub dissipated_energy: f64,
    /// Includes scaffold construction, but repair only replaces existing mass.
    pub constructed_mass: f64,
    /// Translated family-specific protein, still awaiting final assembly.
    pub synthesized_mass: f64,
    pub recycled_mass: f64,
    pub repaired_mass: f64,
    pub area_grown: f64,
    pub identifier_exhausted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    pub area: f64,
    pub organs: Vec<Organelle>,
    pub next_organelle_id: u32,
    pub scaffold_energy: f64,
    /// Conserved, family-specific assembly protein. It occupies room and is
    /// inherited, but is not functional organelle mass.
    pub precursor_mass: [f64; FAMILY_COUNT],
    pub precursor_energy: [f64; FAMILY_COUNT],
    /// Named product stocks and discrete recipe machinery for cellular programs.
    pub cellular: Option<CellularState>,
}

fn nonnegative(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}
fn effort(value: f64) -> f64 {
    nonnegative(value).min(1.0)
}

impl Body {
    pub fn geometry_limit(width: f64, height: f64) -> f64 {
        (width.min(height) / (8.0 * BASE_RADIUS)).powi(2)
    }
    /// Every introduced cell begins with the same minimal physical body. The
    /// genome builds all optional machinery and the additional core needed for
    /// division, on ordinary paid simulation ticks.
    pub fn founder() -> (Self, FounderBudget) {
        let kind = OrganelleKind::Core;
        let mass = kind.min_mass();
        let body = Self {
            area: FOUNDER_AREA,
            organs: vec![Organelle {
                id: 1,
                kind,
                mass,
                integrity: 1.0,
                embodied_energy: mass * kind.construction_cost() * EMBODIED_FRACTION,
            }],
            next_organelle_id: 2,
            scaffold_energy: FOUNDER_AREA
                * SCAFFOLD_MASS_PER_AREA
                * SCAFFOLD_ENERGY_PER_MASS
                * EMBODIED_FRACTION,
            precursor_mass: [0.0; FAMILY_COUNT],
            precursor_energy: [0.0; FAMILY_COUNT],
            cellular: None,
        };
        let budget = body.introduction_budget();
        debug_assert!(body.validate().is_ok());
        (body, budget)
    }

    #[cfg(test)]
    pub(crate) fn fixture(recipe: BodyFixture) -> (Self, FounderBudget) {
        use OrganelleKind::*;
        let mut body = Self {
            area: 1.0,
            organs: Vec::new(),
            next_organelle_id: 1,
            scaffold_energy: SCAFFOLD_MASS_PER_AREA * SCAFFOLD_ENERGY_PER_MASS * EMBODIED_FRACTION,
            precursor_mass: [0.0; FAMILY_COUNT],
            precursor_energy: [0.0; FAMILY_COUNT],
            cellular: None,
        };
        // Test specimens isolate feeding, sensing, storage and inheritance.
        // Their full fabrication cost is recorded just like production input.
        let mut add = |kind: OrganelleKind, count: usize, mass: f64| {
            for _ in 0..count {
                body.organs.push(Organelle {
                    id: body.next_organelle_id,
                    kind,
                    mass,
                    integrity: 1.0,
                    embodied_energy: mass * kind.construction_cost() * EMBODIED_FRACTION,
                });
                body.next_organelle_id += 1;
            }
        };
        add(Core, 2, 0.04);
        add(Reserve, 2, 0.04);
        match recipe {
            BodyFixture::Bare => {}
            BodyFixture::Producer => {
                add(Photosynthetic, 2, 0.10);
                add(Metabolic, 2, 0.03);
                add(Transporter, 2, 0.015);
                add(Photoreceptor, 2, 0.01);
            }
            BodyFixture::Scavenger | BodyFixture::Hunter => {
                let hunter = recipe == BodyFixture::Hunter;
                add(Digestive, 2, if hunter { 0.05 } else { 0.06 });
                add(Metabolic, 2, 0.03);
                add(Transporter, 1, 0.015);
                add(Phagocyte, 2, if hunter { 0.025 } else { 0.035 });
                add(Flagellum, 2, if hunter { 0.045 } else { 0.035 });
                add(Chemoreceptor, 2, 0.015);
                add(Mechanoreceptor, 2, 0.01);
                add(Optical, if hunter { 2 } else { 1 }, 0.02);
                if hunter {
                    add(Piercer, 2, 0.04);
                    add(Cortex, 1, 0.02);
                }
            }
            BodyFixture::Mixed => {
                add(Photosynthetic, 2, 0.06);
                add(Digestive, 2, 0.04);
                add(Metabolic, 2, 0.03);
                add(Transporter, 2, 0.015);
                add(Phagocyte, 1, 0.025);
                add(Flagellum, 1, 0.035);
                add(Photoreceptor, 1, 0.01);
                add(Chemoreceptor, 1, 0.015);
                add(Cortex, 1, 0.02);
            }
        }
        let budget = body.introduction_budget();
        debug_assert!(body.validate().is_ok());
        (body, budget)
    }

    fn introduction_budget(&self) -> FounderBudget {
        let construction_energy = self.scaffold_mass() * SCAFFOLD_ENERGY_PER_MASS
            + self
                .organs
                .iter()
                .map(|o| o.mass * o.kind.construction_cost())
                .sum::<f64>();
        let energy = self.energy_capacity() * FOUNDER_ENERGY_FRACTION;
        let minerals = self.mineral_capacity() * FOUNDER_MINERAL_FRACTION;
        FounderBudget {
            energy,
            minerals,
            introduced_energy: energy + construction_energy,
            introduced_minerals: minerals + self.structural_mass(),
            construction_energy,
            dissipated_energy: construction_energy - self.embodied_energy(),
        }
    }

    pub fn radius(&self) -> f64 {
        BASE_RADIUS * self.area.sqrt()
    }
    pub fn scaffold_mass(&self) -> f64 {
        self.area * SCAFFOLD_MASS_PER_AREA
    }
    pub fn structural_mass(&self) -> f64 {
        self.scaffold_mass()
            + self.organs.iter().map(|o| o.mass).sum::<f64>()
            + self.precursor_mass.iter().sum::<f64>()
            + self
                .cellular
                .as_ref()
                .map_or(0.0, CellularState::structural_mass)
    }
    pub fn embodied_energy(&self) -> f64 {
        self.scaffold_energy
            + self.organs.iter().map(|o| o.embodied_energy).sum::<f64>()
            + self.precursor_energy.iter().sum::<f64>()
            + self
                .cellular
                .as_ref()
                .map_or(0.0, CellularState::embodied_energy)
    }
    pub fn precursor(&self, kind: OrganelleKind) -> f64 {
        self.precursor_mass[kind as usize]
    }
    pub fn count(&self, kind: OrganelleKind) -> u32 {
        self.organs.iter().filter(|o| o.kind == kind).count() as u32
            + self.cellular.as_ref().map_or(0, |state| {
                state
                    .assemblies
                    .iter()
                    .filter(|a| a.recipe.kind == AssemblyKind::Organelle(kind))
                    .count() as u32
            })
    }
    pub fn mass(&self, kind: OrganelleKind) -> f64 {
        let mut mass = self
            .organs
            .iter()
            .filter(|o| o.kind == kind)
            .map(|o| o.mass)
            .sum();
        if let Some(state) = &self.cellular {
            for assembly in state
                .assemblies
                .iter()
                .filter(|a| a.recipe.kind == AssemblyKind::Organelle(kind))
            {
                mass += assembly.mass();
            }
        }
        mass
    }
    pub fn functional_mass(&self, kind: OrganelleKind) -> f64 {
        let mut mass = self
            .organs
            .iter()
            .filter(|o| o.kind == kind)
            .map(Organelle::functional_mass)
            .sum();
        if let Some(state) = &self.cellular {
            for assembly in state
                .assemblies
                .iter()
                .filter(|a| a.recipe.kind == AssemblyKind::Organelle(kind))
            {
                mass += assembly.functional_mass();
            }
        }
        mass
    }
    /// Equivalent minimum-sized functional units, not requested capacity.
    pub fn capacity(&self, kind: OrganelleKind) -> f64 {
        self.functional_mass(kind) / kind.min_mass()
    }
    pub fn damaged(&self, kind: OrganelleKind) -> f64 {
        let mut mass = self
            .organs
            .iter()
            .filter(|o| o.kind == kind)
            .map(|o| o.mass * (1.0 - o.integrity))
            .sum();
        if let Some(state) = &self.cellular {
            for assembly in state
                .assemblies
                .iter()
                .filter(|a| a.recipe.kind == AssemblyKind::Organelle(kind))
            {
                mass += assembly.mass() * (1.0 - assembly.integrity);
            }
        }
        mass
    }
    /// Material missing before incomplete organs first become functional.
    pub fn building(&self, kind: OrganelleKind) -> f64 {
        let mut mass = self
            .organs
            .iter()
            .filter(|o| o.kind == kind)
            .map(Organelle::building)
            .sum();
        if let Some(state) = &self.cellular {
            for assembly in state
                .assemblies
                .iter()
                .filter(|a| a.recipe.kind == AssemblyKind::Organelle(kind))
            {
                mass += assembly.building();
            }
        }
        mass
    }
    /// Collect all family signals in one pass, retaining each family's original
    /// summation order. The returned values are valid until the body changes.
    pub fn family_summaries(&self) -> [OrganelleSummary; FAMILY_COUNT] {
        let mut summaries = [OrganelleSummary::default(); FAMILY_COUNT];
        for organ in &self.organs {
            let summary = &mut summaries[organ.kind as usize];
            summary.count += 1;
            summary.mass += organ.mass;
            summary.functional_mass += organ.functional_mass();
            summary.damaged += organ.mass * (1.0 - organ.integrity);
            summary.building += organ.building();
        }
        if let Some(state) = &self.cellular {
            state.add_summaries(&mut summaries);
        }
        for kind in OrganelleKind::ALL {
            let summary = &mut summaries[kind as usize];
            summary.capacity = summary.functional_mass / kind.min_mass();
        }
        summaries
    }
    pub fn space(&self) -> SpaceBudget {
        let capacity = INTERNAL_PER_AREA * self.area;
        let used = self
            .organs
            .iter()
            .map(Organelle::internal_space)
            .sum::<f64>()
            + self.precursor_mass.iter().sum::<f64>() * PROTEIN_INTERNAL_FOOTPRINT
            + self
                .cellular
                .as_ref()
                .map_or(0.0, CellularState::internal_space);
        SpaceBudget {
            capacity,
            used,
            free: (capacity - used).max(0.0),
        }
    }
    pub fn surface(&self) -> SpaceBudget {
        let capacity = SURFACE_PER_RADIUS * self.area.sqrt();
        let used = self
            .organs
            .iter()
            .map(Organelle::surface_space)
            .sum::<f64>()
            + self
                .cellular
                .as_ref()
                .map_or(0.0, CellularState::surface_space);
        SpaceBudget {
            capacity,
            used,
            free: (capacity - used).max(0.0),
        }
    }
    pub fn minimum_core_mass(&self) -> f64 {
        CORE_MASS_PER_AREA * self.area
    }
    pub fn support_mass(&self) -> f64 {
        self.functional_mass(OrganelleKind::Core)
            + self.mechanism_mass(MechanismKind::Support)
    }
    pub fn mechanism_mass(&self, kind: MechanismKind) -> f64 {
        self.cellular.as_ref().map_or(0.0, |state| state.assemblies.iter()
            .filter(|a| a.recipe.kind == AssemblyKind::Mechanism(kind))
            .map(|a| a.functional_mass()).sum())
    }
    /// Compatibility render slots describe visible hardware, not capabilities.
    /// Keep these out of sensing and capacity calculations to avoid granting
    /// legacy family side effects to a primitive mechanism.
    pub fn add_visual_mechanisms(&self, summaries: &mut [OrganelleSummary; FAMILY_COUNT]) {
        let Some(state) = &self.cellular else { return; };
        for a in &state.assemblies {
            let kind = match a.recipe.kind {
                AssemblyKind::Mechanism(MechanismKind::Propulsion) => OrganelleKind::Flagellum,
                AssemblyKind::Mechanism(MechanismKind::LightHarvest) => OrganelleKind::Photosynthetic,
                AssemblyKind::Mechanism(MechanismKind::DigestExudate | MechanismKind::DigestTissue | MechanismKind::DigestWaste) => OrganelleKind::Digestive,
                AssemblyKind::Mechanism(MechanismKind::Shield) => OrganelleKind::Cortex,
                AssemblyKind::Mechanism(MechanismKind::Pierce) => OrganelleKind::Piercer,
                _ => continue,
            };
            let s = &mut summaries[kind as usize];
            s.count += 1;
            s.mass += a.mass();
            s.functional_mass += a.functional_mass();
            s.capacity = s.functional_mass / kind.min_mass();
            s.damaged += a.mass() * (1.0 - a.integrity);
            s.building += a.building();
        }
    }
    pub fn viable(&self) -> bool {
        self.support_mass() + EPS >= self.minimum_core_mass()
    }
    pub fn integrity(&self) -> f64 {
        let mass = self.organs.iter().map(|o| o.mass).sum::<f64>()
            + self
                .cellular
                .as_ref()
                .map_or(0.0, |state| state.assemblies.iter().map(|a| a.mass()).sum());
        if mass == 0.0 {
            0.0
        } else {
            (self
                .organs
                .iter()
                .map(|o| o.mass * o.integrity)
                .sum::<f64>()
                + self.cellular.as_ref().map_or(0.0, |state| {
                    state
                        .assemblies
                        .iter()
                        .map(|a| a.mass() * a.integrity)
                        .sum::<f64>()
                }))
                / mass
        }
    }
    pub fn upkeep(&self) -> f64 {
        // Damaged material remains costly to support; empty envelope is not free.
        0.16 * self.area
            + PROTEIN_UPKEEP * self.precursor_mass.iter().sum::<f64>()
            + self.cellular.as_ref().map_or(0.0, CellularState::upkeep)
            + self
                .organs
                .iter()
                .map(|o| {
                    o.mass
                        * (0.45
                            + if matches!(o.kind, OrganelleKind::Piercer | OrganelleKind::Flagellum)
                            {
                                0.45
                            } else {
                                0.0
                            })
                })
                .sum::<f64>()
    }
    pub fn capacities(&self) -> BodyCapacities {
        let mut mass = [0.0; FAMILY_COUNT];
        for organ in &self.organs {
            mass[organ.kind as usize] += organ.functional_mass();
        }
        if let Some(state) = &self.cellular {
            for assembly in &state.assemblies {
                if let AssemblyKind::Organelle(kind) = assembly.recipe.kind {
                    mass[kind as usize] += assembly.functional_mass();
                }
            }
        }
        self.capacities_from_functional_mass(mass)
    }
    /// Reuse an already collected family snapshot without another organ scan.
    pub fn capacities_from_summaries(
        &self,
        summaries: &[OrganelleSummary; FAMILY_COUNT],
    ) -> BodyCapacities {
        // capacities() starts its accumulation at positive zero, unlike the
        // independent family query sums used by family_summaries().
        self.capacities_from_functional_mass(summaries.map(|s| 0.0 + s.functional_mass))
    }
    fn capacities_from_functional_mass(&self, mut mass: [f64; FAMILY_COUNT]) -> BodyCapacities {
        use OrganelleKind::*;
        // Sensing precedes gene evaluation. Receptors and processing enzymes
        // therefore use their stored activity from the preceding tick, just as
        // optical direction does. Passive storage and structure remain present.
        if let Some(state) = &self.cellular {
            for kind in [
                Metabolic,
                Photoreceptor,
                Thermoreceptor,
                Chemoreceptor,
                Mechanoreceptor,
                Optical,
            ] {
                mass[kind as usize] = self
                    .organs
                    .iter()
                    .filter(|o| o.kind == kind)
                    .map(Organelle::functional_mass)
                    .fold(0.0, |sum, value| sum + value)
                    + state
                        .assemblies
                        .iter()
                        .filter(|a| a.recipe.kind == AssemblyKind::Organelle(kind))
                        .map(|a| a.functional_mass() * a.activity)
                        .sum::<f64>();
            }
        }
        let core = mass[Core as usize];
        let reserve = mass[Reserve as usize];
        let digestive = mass[Digestive as usize];
        let flagellum = mass[Flagellum as usize];
        let optical = mass[Optical as usize];
        let energy = 2000.0 * core + 600.0 * reserve;
        let minerals = 25.0 * core + 12.0 * reserve;
        let gut_energy = 800.0 * digestive;
        let protection = (6.0 * mass[Cortex as usize] / self.area.sqrt()).clamp(0.0, 0.8);
        let mut capacities = BodyCapacities {
            energy,
            minerals,
            gut_energy,
            gut_minerals: 12.0 * digestive,
            photosynthesis: 55.0 * mass[Photosynthetic as usize],
            metabolism: 20.0 * core + 60.0 * mass[Metabolic as usize],
            digestion: 65.0 * digestive,
            uptake: 0.8 * mass[Transporter as usize],
            thrust: 100.0 * flagellum,
            turn: 25.0 * flagellum,
            capture: mass[Phagocyte as usize],
            piercing: mass[Piercer as usize],
            protection,
            repair: 0.0175 * core + 0.08 * mass[Repair as usize],
            assembly: 0.55 * core,
            light_sensing: mass[Photoreceptor as usize],
            heat_sensing: mass[Thermoreceptor as usize],
            chemical_sensing: mass[Chemoreceptor as usize],
            mechanical_sensing: mass[Mechanoreceptor as usize],
            optical_sensing: optical,
            optical_range: if optical > 0.0 {
                (35.0 + 300.0 * optical.sqrt()).min(190.0)
            } else {
                0.0
            },
            energy_storage: energy,
            mineral_storage: minerals,
            gut_storage: gut_energy,
            propulsion: 100.0 * flagellum,
            phagocytosis: mass[Phagocyte as usize],
            attack: mass[Piercer as usize],
            defense: protection,
            light_harvest: 0.0,
            substrate_digestion: [0.0; 3],
            independent_engulf: 0.0,
            bite_extraction_fraction: 1.0,
        };
        if let Some(state) = &self.cellular {
            state.add_mechanism_capacities(&mut capacities, self.area);
        }
        capacities
    }
    pub fn energy_capacity(&self) -> f64 {
        self.capacities().energy
    }
    pub fn mineral_capacity(&self) -> f64 {
        self.capacities().minerals
    }
    pub fn gut_capacity(&self) -> f64 {
        self.capacities().gut_energy
    }

    /// Damage retains every gram, occupied slot and joule of embodied substrate.
    /// World handles the resulting compartment overflow and any final death.
    pub fn damage(&mut self, fraction: f64) {
        let retained = 1.0 - effort(fraction);
        for organ in &mut self.organs {
            organ.integrity *= retained;
        }
        if let Some(state) = &mut self.cellular {
            for assembly in &mut state.assemblies {
                assembly.integrity *= retained;
            }
        }
    }

    /// Translate family-specific proteins, assemble organs, recycle, grow and
    /// repair from one starting snapshot. These demands share finite core work,
    /// energy, material and room. Assembly additionally requires its family's
    /// protein from before this tick: a newly expressed gene grants no instant
    /// capability. Expression continues until the genome switches it off, even
    /// when a target is complete, so surplus protein has real costs.
    ///
    /// Newly created room, recovered resources and new machinery are available
    /// on the following call, never reused for additional work this tick.
    /// Reserve references are debited/credited here; World must spill any excess
    /// after capacity losses. No reserve content is silently deleted in Body.
    pub fn work(
        &mut self,
        grow: f64,
        controls: &BodyControls,
        repair: f64,
        energy: &mut f64,
        minerals: &mut f64,
        dt: f64,
        max_area: f64,
    ) -> WorkReport {
        let mut report = WorkReport::default();
        if !dt.is_finite() || dt <= 0.0 || !energy.is_finite() || !minerals.is_finite() {
            return report;
        }
        let capacities = self.capacities();
        let starting_space = self.space().free;
        let starting_surface = self.surface().free;
        let mut protein_energy_density = [0.0; FAMILY_COUNT];
        // Turnover recovers material but not energy. Neither the released room
        // nor the recovered minerals can fund new work in this same snapshot.
        let decay_fraction = -(-std::f64::consts::LN_2 * dt / PROTEIN_HALF_LIFE).exp_m1();
        for k in 0..FAMILY_COUNT {
            if self.precursor_mass[k] > 0.0 {
                protein_energy_density[k] = self.precursor_energy[k] / self.precursor_mass[k];
            }
            let mass = self.precursor_mass[k] * decay_fraction;
            let energy = self.precursor_energy[k] * decay_fraction;
            self.precursor_mass[k] = (self.precursor_mass[k] - mass).max(0.0);
            self.precursor_energy[k] = (self.precursor_energy[k] - energy).max(0.0);
            report.minerals_recovered += mass;
            report.dissipated_energy += energy;
        }
        let mut demands = Vec::new();
        // Reuse the temporary storage across families; selected demands are
        // drained in their original order while the backing buffers stay here.
        let mut indices: Vec<usize> = Vec::new();
        let mut build = Vec::new();
        let mut recycle = Vec::new();
        // Finite output effort controls rates, not an allocation of arbitrarily
        // many target records. A family can start at most one incomplete unit.
        for kind in OrganelleKind::ALL {
            indices.clear();
            build.clear();
            recycle.clear();
            let k = kind as usize;
            let expression = effort(controls.build[k]);
            if expression > 0.0 {
                demands.push(Demand::synthesis(
                    kind,
                    0.12 * self.area.sqrt() * dt * expression,
                ));
            }
            let count = controls.target_counts[k] as usize;
            let target = if controls.target_sizes[k].is_finite() {
                controls.target_sizes[k].clamp(kind.min_mass(), kind.maximum_mass())
            } else {
                kind.min_mass()
            };
            indices.extend(
                self.organs
                    .iter()
                    .enumerate()
                    .filter(|(_, organ)| organ.kind == kind)
                    .map(|(index, _)| index),
            );
            let keep = count.min(indices.len());
            for &index in indices.iter().take(keep) {
                let organ = &self.organs[index];
                if target > organ.mass + EPS {
                    build.push(Demand::build(kind, Some(index), target - organ.mass));
                }
                if organ.mass > target + EPS {
                    recycle.push(Demand::recycle(kind, index, organ.mass - target));
                }
            }
            // Dismantle extras one at a time, newest first. Partially dismantled
            // organs still occupy space and hold embodied energy until removed.
            if count < indices.len() {
                let index = *indices.last().unwrap();
                recycle.push(Demand::recycle(kind, index, self.organs[index].mass));
            }
            let incomplete = indices
                .iter()
                .any(|&i| self.organs[i].mass + EPS < kind.min_mass());
            if count > indices.len() && !incomplete {
                if self.next_organelle_id == u32::MAX {
                    report.identifier_exhausted = true;
                } else {
                    build.push(Demand::build(kind, None, target));
                }
            }
            // Choose a single structural direction per family. A same-mass
            // count/size rearrangement dismantles excess first, then rebuilds.
            let wanted = target * controls.target_counts[k] as f64;
            let current = indices.iter().map(|&i| self.organs[i].mass).sum::<f64>();
            let choose_build = wanted > current + EPS && !build.is_empty();
            let (selected, request_effort) = if choose_build {
                (&mut build, 1.0)
            } else if !recycle.is_empty() {
                (&mut recycle, effort(controls.recycle[k]))
            } else {
                (&mut build, 1.0)
            };
            if request_effort == 0.0 || selected.is_empty() {
                continue;
            }
            if kind == OrganelleKind::Core && selected[0].operation == Operation::Recycle {
                // Full requested end state must remain viable. Intermediate
                // partial removals then cannot cross an essential threshold.
                let mut final_core = self.functional_mass(OrganelleKind::Core);
                for demand in selected.iter() {
                    let original = &self.organs[demand.index.unwrap()];
                    let mut after = original.clone();
                    after.mass = (after.mass - demand.amount).max(0.0);
                    final_core += after.functional_mass() - original.functional_mass();
                }
                if final_core + EPS < self.minimum_core_mass() {
                    continue;
                }
            }
            let requested = selected.iter().map(|d| d.amount).sum::<f64>();
            let family_rate = 0.12 * self.area.sqrt() * dt * request_effort;
            let scale = (family_rate / requested).min(1.0);
            for demand in selected.iter_mut() {
                demand.amount *= scale;
            }
            demands.extend(selected.drain(..));
        }
        let max_area = if max_area.is_finite() {
            max_area.clamp(MIN_AREA, MAX_AREA)
        } else {
            self.area
        };
        // Concurrent core dismantling cannot leave newly grown frame without
        // support. Conversely, a core being constructed this tick does not yet
        // authorize additional frame, matching the delayed capability rule.
        let mut supporting_core = self.functional_mass(OrganelleKind::Core);
        for demand in &demands {
            if demand.kind == OrganelleKind::Core && demand.operation == Operation::Recycle {
                let original = &self.organs[demand.index.unwrap()];
                let mut after = original.clone();
                after.mass = (after.mass - demand.amount).max(0.0);
                supporting_core += after.functional_mass() - original.functional_mass();
            }
        }
        let core_area = supporting_core / CORE_MASS_PER_AREA;
        let growth = (0.08 * self.area.sqrt() * dt * effort(grow))
            .min((max_area.min(core_area) - self.area).max(0.0));
        if growth > EPS {
            demands.push(Demand::growth(growth * SCAFFOLD_MASS_PER_AREA));
        }
        let damaged = self
            .organs
            .iter()
            .map(|o| o.mass * (1.0 - o.integrity))
            .sum::<f64>();
        if damaged > EPS && effort(repair) > 0.0 {
            let scale = (capacities.repair * dt * effort(repair) / damaged).min(1.0);
            for (index, organ) in self.organs.iter().enumerate() {
                // Repairing a concurrently dismantled unit could charge for
                // already removed material. Its remaining damage is eligible
                // again next tick, after the structural request has resolved.
                if demands
                    .iter()
                    .any(|d| d.operation == Operation::Recycle && d.index == Some(index))
                {
                    continue;
                }
                let amount = organ.mass * (1.0 - organ.integrity) * scale;
                if amount > EPS {
                    demands.push(Demand::repair(organ.kind, index, amount));
                }
            }
        }
        let mut resources = [0.0; RESOURCE_COUNT];
        resources[..5].copy_from_slice(&[
            (*energy - MAINTENANCE_RESERVE_FRACTION * capacities.energy).max(0.0),
            minerals.max(0.0),
            starting_space,
            starting_surface,
            capacities.assembly * dt,
        ]);
        resources[5..].copy_from_slice(&self.precursor_mass);
        let allocated = allocate(&demands, resources);
        // No indices change until all old-index demands have been applied.
        let mut new_organs = Vec::new();
        for (demand, amount) in demands.into_iter().zip(allocated) {
            if amount <= 0.0 {
                continue;
            }
            // Rounding-sized free-space residues must not allocate a record;
            // existing fragments, however, are recycled down to exact zero.
            if demand.operation == Operation::Build && demand.index.is_none() && amount <= EPS {
                continue;
            }
            let cost = amount * demand.costs[0];
            let mineral_cost = amount * demand.costs[1];
            report.energy_spent += cost;
            report.minerals_spent += mineral_cost;
            match demand.operation {
                Operation::Synthesis => {
                    let k = demand.kind as usize;
                    let embodied = cost * EMBODIED_FRACTION;
                    self.precursor_mass[k] += amount;
                    self.precursor_energy[k] += embodied;
                    report.synthesized_mass += amount;
                    report.dissipated_energy += cost - embodied;
                }
                Operation::Build => {
                    if demand.index.is_none() && self.next_organelle_id == u32::MAX {
                        report.identifier_exhausted = true;
                        report.energy_spent -= cost;
                        report.minerals_spent -= mineral_cost;
                        continue;
                    }
                    let k = demand.kind as usize;
                    let protein_energy = amount * protein_energy_density[k];
                    self.precursor_mass[k] = (self.precursor_mass[k] - amount).max(0.0);
                    self.precursor_energy[k] = (self.precursor_energy[k] - protein_energy).max(0.0);
                    let new_embodied = cost * EMBODIED_FRACTION;
                    let embodied = protein_energy + new_embodied;
                    if let Some(index) = demand.index {
                        let organ = &mut self.organs[index];
                        // Newly grown material is intact; existing damage is not
                        // erased when a large unit gains a small new piece.
                        organ.integrity =
                            (organ.mass * organ.integrity + amount) / (organ.mass + amount);
                        organ.mass += amount;
                        organ.embodied_energy += embodied;
                    } else {
                        new_organs.push(Organelle {
                            id: self.next_organelle_id,
                            kind: demand.kind,
                            mass: amount,
                            integrity: 1.0,
                            embodied_energy: embodied,
                        });
                        self.next_organelle_id += 1;
                    }
                    report.constructed_mass += amount;
                    report.dissipated_energy += cost - new_embodied;
                }
                Operation::Grow => {
                    let area = amount / SCAFFOLD_MASS_PER_AREA;
                    self.area += area;
                    self.scaffold_energy += cost * EMBODIED_FRACTION;
                    report.area_grown += area;
                    report.constructed_mass += amount;
                    report.dissipated_energy += cost * (1.0 - EMBODIED_FRACTION);
                }
                Operation::Recycle => {
                    let organ = &mut self.organs[demand.index.unwrap()];
                    let removed_energy = organ.embodied_energy * (amount / organ.mass).min(1.0);
                    organ.mass = (organ.mass - amount).max(0.0);
                    organ.embodied_energy = (organ.embodied_energy - removed_energy).max(0.0);
                    report.recycled_mass += amount;
                    report.minerals_recovered += amount;
                    report.energy_recovered += removed_energy * RECOVERY_FRACTION;
                    report.dissipated_energy += cost + removed_energy * (1.0 - RECOVERY_FRACTION);
                }
                Operation::Repair => {
                    let organ = &mut self.organs[demand.index.unwrap()];
                    // Replace damaged material with reserve material, returning
                    // the equal removed mineral quantity. Frame mass is stable.
                    let fraction = (amount / organ.mass).min(1.0 - organ.integrity);
                    let replaced = fraction * organ.mass;
                    let removed_energy = organ.embodied_energy * fraction;
                    organ.integrity = (organ.integrity + fraction).min(1.0);
                    let embodied = replaced
                        * REPAIR_ENERGY_PER_MASS.min(organ.kind.construction_cost())
                        * EMBODIED_FRACTION;
                    organ.embodied_energy += embodied - removed_energy;
                    report.repaired_mass += replaced;
                    report.minerals_recovered += replaced;
                    report.energy_recovered += removed_energy * RECOVERY_FRACTION;
                    report.dissipated_energy +=
                        cost - embodied + removed_energy * (1.0 - RECOVERY_FRACTION);
                }
            }
        }
        self.organs.retain(|o| o.mass > 0.0);
        self.organs.extend(new_organs);
        *energy = (*energy - report.energy_spent + report.energy_recovered).max(0.0);
        *minerals = (*minerals - report.minerals_spent + report.minerals_recovered).max(0.0);
        debug_assert!(self.space().used <= self.space().capacity + 1e-7);
        debug_assert!(self.surface().used <= self.surface().capacity + 1e-7);
        report
    }

    pub fn can_partition(&self) -> bool {
        if self.cellular.is_some() {
            self.cellular_partition_assignment().is_some()
        } else {
            self.assign_partition(|_, _| {})
        }
    }
    pub fn division_ready(&self) -> bool {
        self.can_partition()
    }
    /// Preview a conservative division. No organelle is cloned or automatically
    /// split: the genome must first build enough discrete machinery. World pays
    /// the division action cost and divides reserve/gut contents separately.
    /// Linear scaffold accounting includes membrane support material throughout
    /// the envelope, so redistributing it preserves its paid material/energy.
    pub fn partition(&self) -> Option<(Self, Self)> {
        let blank = Self {
            area: self.area * 0.5,
            organs: Vec::new(),
            next_organelle_id: self.next_organelle_id,
            scaffold_energy: self.scaffold_energy * 0.5,
            precursor_mass: self.precursor_mass.map(|mass| mass * 0.5),
            precursor_energy: self.precursor_energy.map(|energy| energy * 0.5),
            cellular: self.cellular.as_ref().map(CellularState::partition_base),
        };
        let mut daughters = [blank.clone(), blank];
        if let Some(state) = &self.cellular {
            for (cellular, index, side) in self.cellular_partition_assignment()? {
                if cellular {
                    daughters[side]
                        .cellular
                        .as_mut()
                        .unwrap()
                        .assemblies
                        .push(state.assemblies[index].clone());
                } else {
                    daughters[side].organs.push(self.organs[index].clone());
                }
            }
        } else if !self.assign_partition(|side, organ| daughters[side].organs.push(organ.clone())) {
            return None;
        }
        for daughter in &mut daughters {
            daughter.organs.sort_by_key(|o| o.id);
            if let Some(state) = &mut daughter.cellular {
                state.assemblies.sort_by_key(|a| a.id);
            }
        }
        let [a, b] = daughters;
        Some((a, b))
    }

    /// Pack the actual old-format organs and recipe instances together. A core
    /// assembled from products is as essential as the founder's physical core;
    /// incomplete cores cannot support either daughter. No recipe is cloned.
    fn cellular_partition_assignment(&self) -> Option<Vec<(bool, usize, usize)>> {
        let state = self.cellular.as_ref()?;
        let area = self.area * 0.5;
        if area < MIN_AREA || !self.viable() {
            return None;
        }
        struct Item {
            cellular: bool,
            index: usize,
            id: u64,
            core: f64,
            inside: f64,
            surface: f64,
        }
        let mut items = Vec::with_capacity(self.organs.len() + state.assemblies.len());
        for (index, organ) in self.organs.iter().enumerate() {
            items.push(Item {
                cellular: false,
                index,
                id: u64::from(organ.id) * 2,
                core: if organ.kind == OrganelleKind::Core {
                    organ.functional_mass()
                } else {
                    0.0
                },
                inside: organ.internal_space(),
                surface: organ.surface_space(),
            });
        }
        for (index, assembly) in state.assemblies.iter().enumerate() {
            items.push(Item {
                cellular: true,
                index,
                id: u64::from(assembly.id) * 2 + 1,
                core: if assembly.recipe.kind.supports_cell() {
                    assembly.functional_mass()
                } else {
                    0.0
                },
                inside: assembly.internal_space(),
                surface: assembly.surface_space(),
            });
        }
        if items.iter().filter(|item| item.core > 0.0).count() < 2 {
            return None;
        }
        let required = CORE_MASS_PER_AREA * area;
        let internal = INTERNAL_PER_AREA * area;
        let surface = SURFACE_PER_RADIUS * area.sqrt();
        let shared = (self.precursor_mass.iter().sum::<f64>() * PROTEIN_INTERNAL_FOOTPRINT
            + state.stock_mass()
            + state.genetic_mass())
            * 0.5;
        let mut used = [[shared, 0.0]; 2];
        let mut core = [0.0; 2];
        items.sort_by(|x, y| {
            let xc = x.core > 0.0;
            let yc = y.core > 0.0;
            let score = |item: &Item| {
                if item.core > 0.0 {
                    item.core / required
                } else {
                    (item.inside / internal).max(item.surface / surface)
                }
            };
            yc.cmp(&xc)
                .then_with(|| score(y).total_cmp(&score(x)))
                .then(x.id.cmp(&y.id))
        });
        let mut assignment = Vec::with_capacity(items.len());
        for item in items {
            let load = |side: usize| (used[side][0] / internal).max(used[side][1] / surface);
            let first = if item.core > 0.0 {
                usize::from(core[1] < core[0])
            } else {
                usize::from(load(1) < load(0))
            };
            let fits = |side: usize| {
                used[side][0] + item.inside <= internal + EPS
                    && used[side][1] + item.surface <= surface + EPS
            };
            let side = if fits(first) {
                first
            } else if fits(1 - first) {
                1 - first
            } else {
                return None;
            };
            used[side][0] += item.inside;
            used[side][1] += item.surface;
            core[side] += item.core;
            assignment.push((item.cellular, item.index, side));
        }
        if core.iter().any(|&mass| mass + EPS < required) {
            None
        } else {
            Some(assignment)
        }
    }

    /// Share the exact greedy placement between feasibility and materialization.
    /// Checking readiness needs the placements, but no daughter bodies or cloned
    /// organ records. Visit order and arithmetic match actual division.
    fn assign_partition(&self, mut place: impl FnMut(usize, &Organelle)) -> bool {
        let area = self.area * 0.5;
        if area < MIN_AREA || !self.viable() {
            return false;
        }
        if self
            .organs
            .iter()
            .filter(|organ| organ.kind == OrganelleKind::Core && organ.functional_mass() > 0.0)
            .take(2)
            .count()
            < 2
        {
            // A single discrete core can never make both daughters viable.
            return false;
        }
        let precursor_mass = self.precursor_mass.map(|mass| mass * 0.5);
        let protein_space = precursor_mass.iter().sum::<f64>() * PROTEIN_INTERNAL_FOOTPRINT;
        let mut used = [[protein_space, 0.0]; 2];
        let mut core = [0.0; 2];
        let required = CORE_MASS_PER_AREA * area;
        let internal = INTERNAL_PER_AREA * area;
        let surface = SURFACE_PER_RADIUS * area.sqrt();
        let fits = |side: usize, organ: &Organelle, used: &[[f64; 2]; 2]| {
            used[side][0] + organ.internal_space() <= internal + EPS
                && used[side][1] + organ.surface_space() <= surface + EPS
        };
        let mut indices: Vec<usize> = (0..self.organs.len()).collect();
        // Allocate functional cores first; then largest relative footprints.
        indices.sort_by(|&a, &b| {
            let x = &self.organs[a];
            let y = &self.organs[b];
            let xc = x.kind == OrganelleKind::Core && x.functional_mass() > 0.0;
            let yc = y.kind == OrganelleKind::Core && y.functional_mass() > 0.0;
            yc.cmp(&xc)
                .then_with(|| {
                    let score = |o: &Organelle| {
                        if o.kind == OrganelleKind::Core && o.functional_mass() > 0.0 {
                            o.functional_mass() / required
                        } else {
                            (o.internal_space() / internal).max(o.surface_space() / surface)
                        }
                    };
                    score(y).total_cmp(&score(x))
                })
                .then(x.id.cmp(&y.id))
        });
        for index in indices {
            let organ = &self.organs[index];
            let is_core = organ.kind == OrganelleKind::Core && organ.functional_mass() > 0.0;
            let load = |side: usize| (used[side][0] / internal).max(used[side][1] / surface);
            let first = if is_core {
                usize::from(core[1] < core[0])
            } else {
                usize::from(load(1) < load(0))
            };
            let side = if fits(first, organ, &used) {
                first
            } else if fits(1 - first, organ, &used) {
                1 - first
            } else {
                return false;
            };
            used[side][0] += organ.internal_space();
            used[side][1] += organ.surface_space();
            if is_core {
                core[side] += organ.functional_mass();
            }
            place(side, organ);
        }
        !core.iter().any(|&mass| mass + EPS < required)
    }

    pub fn validate(&self) -> CheckpointResult<()> {
        ensure(
            self.area.is_finite() && self.area >= MIN_AREA && self.area <= MAX_AREA + EPS,
            "body area outside geometric bounds",
        )?;
        ensure(
            self.next_organelle_id > 0,
            "invalid next organelle identifier",
        )?;
        ensure(
            self.scaffold_energy.is_finite()
                && self.scaffold_energy >= 0.0
                && self.scaffold_energy
                    <= self.scaffold_mass() * SCAFFOLD_ENERGY_PER_MASS * EMBODIED_FRACTION + EPS,
            "invalid scaffold energy ledger",
        )?;
        ensure(
            self.organs.len() <= MAX_ORGANS,
            "too many organs for supported geometry",
        )?;
        for kind in OrganelleKind::ALL {
            let k = kind as usize;
            ensure(
                self.precursor_mass[k].is_finite()
                    && self.precursor_mass[k] >= 0.0
                    && self.precursor_mass[k] <= INTERNAL_PER_AREA * MAX_AREA,
                "invalid assembly protein mass",
            )?;
            ensure(
                self.precursor_energy[k].is_finite()
                    && self.precursor_energy[k] >= 0.0
                    && self.precursor_energy[k]
                        <= self.precursor_mass[k]
                            * kind.construction_cost()
                            * PROTEIN_SYNTHESIS_FRACTION
                            * EMBODIED_FRACTION
                            + EPS,
                "invalid assembly protein energy ledger",
            )?;
        }
        if let Some(state) = &self.cellular {
            state.validate()?;
        }
        let mut ids = std::collections::HashSet::with_capacity(self.organs.len());
        let mut incomplete = [false; FAMILY_COUNT];
        for organ in &self.organs {
            ensure(
                organ.id > 0 && organ.id < self.next_organelle_id && ids.insert(organ.id),
                "invalid or duplicate organelle identifier",
            )?;
            ensure(
                organ.mass.is_finite()
                    && organ.mass > 0.0
                    && organ.mass <= organ.kind.maximum_mass() + EPS,
                "invalid organelle mass",
            )?;
            ensure(
                organ.integrity.is_finite() && (0.0..=1.0).contains(&organ.integrity),
                "invalid organelle integrity",
            )?;
            ensure(
                organ.embodied_energy.is_finite()
                    && organ.embodied_energy >= 0.0
                    && organ.embodied_energy
                        <= organ.mass * organ.kind.construction_cost() * EMBODIED_FRACTION + EPS,
                "invalid organelle energy ledger",
            )?;
            if organ.mass + EPS < organ.kind.min_mass() {
                // A family starts no new unit until its current partial is
                // complete. Kept units never shrink below the minimum; extras
                // are dismantled one at a time. Whole-record inheritance also
                // preserves this bound, including during interrupted recycling.
                ensure(
                    !incomplete[organ.kind as usize],
                    "multiple incomplete organs in one family",
                )?;
                incomplete[organ.kind as usize] = true;
            }
        }
        ensure(
            self.space().used <= self.space().capacity + 1e-8,
            "organs exceed internal space",
        )?;
        ensure(
            self.surface().used <= self.surface().capacity + 1e-8,
            "organs exceed membrane space",
        )?;
        ensure(
            self.count(OrganelleKind::Core) > 0 || self.cellular.as_ref().is_some_and(|state|
                state.assemblies.iter().any(|a| a.recipe.kind.supports_cell())),
            "body has no essential core",
        )?;
        Ok(())
    }
    pub(crate) fn write_checkpoint(&self, writer: &mut Writer) {
        writer.f64(self.area);
        writer.f64(self.scaffold_energy);
        writer.u32(self.next_organelle_id);
        writer.count(self.organs.len());
        for organ in &self.organs {
            writer.u32(organ.id);
            writer.u8(organ.kind as u8);
            writer.f64(organ.mass);
            writer.f64(organ.integrity);
            writer.f64(organ.embodied_energy);
        }
        for k in 0..FAMILY_COUNT {
            writer.f64(self.precursor_mass[k]);
            writer.f64(self.precursor_energy[k]);
        }
        writer.u8(u8::from(self.cellular.is_some()));
        if let Some(state) = &self.cellular {
            state.write_checkpoint(writer);
        }
    }
    #[cfg(test)]
    pub(crate) fn read_checkpoint(reader: &mut Reader<'_>) -> CheckpointResult<Self> {
        Self::read_checkpoint_version(reader, 6)
    }
    pub(crate) fn read_checkpoint_version(
        reader: &mut Reader<'_>,
        version: u32,
    ) -> CheckpointResult<Self> {
        ensure(
            matches!(version, 3 | 4 | 5 | 6),
            "unsupported body checkpoint version",
        )?;
        let area = reader.range(MIN_AREA, MAX_AREA)?;
        let scaffold_energy = reader.f64()?;
        let next_organelle_id = reader.u32()?;
        let count = reader.count(MAX_ORGANS)?;
        ensure(
            count <= reader.remaining() / 29,
            "truncated organelle records",
        )?;
        let mut organs = Vec::with_capacity(count);
        for _ in 0..count {
            let id = reader.u32()?;
            let kind = OrganelleKind::from_index(reader.u8()? as usize)
                .ok_or("Invalid simulation save: unknown organelle family.")?;
            organs.push(Organelle {
                id,
                kind,
                mass: reader.f64()?,
                integrity: reader.f64()?,
                embodied_energy: reader.f64()?,
            });
        }
        let mut precursor_mass = [0.0; FAMILY_COUNT];
        let mut precursor_energy = [0.0; FAMILY_COUNT];
        if version >= 4 {
            for k in 0..FAMILY_COUNT {
                precursor_mass[k] = reader.f64()?;
                precursor_energy[k] = reader.f64()?;
            }
        }
        let cellular = if version >= 5 {
            match reader.u8()? {
                0 => None,
                1 => Some(CellularState::read_checkpoint_version(reader, version)?),
                _ => return Err("Invalid simulation save: invalid cellular state marker.".into()),
            }
        } else {
            None
        };
        let body = Self {
            area,
            organs,
            next_organelle_id,
            scaffold_energy,
            precursor_mass,
            precursor_energy,
            cellular,
        };
        body.validate()?;
        Ok(body)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Synthesis,
    Build,
    Recycle,
    Grow,
    Repair,
}
struct Demand {
    operation: Operation,
    kind: OrganelleKind,
    index: Option<usize>,
    amount: f64,
    /// Energy, minerals, room, membrane, shared core work; then one protein
    /// inventory constraint per family.
    costs: [f64; RESOURCE_COUNT],
}
fn resource_costs(common: [f64; 5]) -> [f64; RESOURCE_COUNT] {
    let mut costs = [0.0; RESOURCE_COUNT];
    costs[..5].copy_from_slice(&common);
    costs
}
impl Demand {
    fn synthesis(kind: OrganelleKind, amount: f64) -> Self {
        Self {
            operation: Operation::Synthesis,
            kind,
            index: None,
            amount,
            costs: resource_costs([
                kind.construction_cost() * PROTEIN_SYNTHESIS_FRACTION,
                1.0,
                PROTEIN_INTERNAL_FOOTPRINT,
                0.0,
                PROTEIN_SYNTHESIS_FRACTION,
            ]),
        }
    }
    fn build(kind: OrganelleKind, index: Option<usize>, amount: f64) -> Self {
        let mut costs = resource_costs([
            kind.construction_cost() * (1.0 - PROTEIN_SYNTHESIS_FRACTION),
            0.0,
            (kind.internal_footprint() - PROTEIN_INTERNAL_FOOTPRINT).max(0.0),
            kind.surface_footprint(),
            1.0 - PROTEIN_SYNTHESIS_FRACTION,
        ]);
        costs[5 + kind as usize] = 1.0;
        Self {
            operation: Operation::Build,
            kind,
            index,
            amount,
            costs,
        }
    }
    fn recycle(kind: OrganelleKind, index: usize, amount: f64) -> Self {
        Self {
            operation: Operation::Recycle,
            kind,
            index: Some(index),
            amount,
            costs: resource_costs([RECYCLE_ENERGY_PER_MASS, 0.0, 0.0, 0.0, 0.7]),
        }
    }
    fn growth(amount: f64) -> Self {
        Self {
            operation: Operation::Grow,
            kind: OrganelleKind::Core,
            index: None,
            amount,
            costs: resource_costs([SCAFFOLD_ENERGY_PER_MASS, 1.0, 0.0, 0.0, 1.0]),
        }
    }
    fn repair(kind: OrganelleKind, index: usize, amount: f64) -> Self {
        Self {
            operation: Operation::Repair,
            kind,
            index: Some(index),
            amount,
            costs: resource_costs([REPAIR_ENERGY_PER_MASS, 1.0, 0.0, 0.0, 1.0]),
        }
    }
}

/// Weighted max-min allocation of requested completion fractions. Once a
/// resource binds, only its consumers stop; other demands continue fairly.
fn allocate(demands: &[Demand], mut resources: [f64; RESOURCE_COUNT]) -> Vec<f64> {
    let mut allocation = vec![0.0; demands.len()];
    let mut active = vec![true; demands.len()];
    for _ in 0..=RESOURCE_COUNT {
        let mut needed = [0.0; RESOURCE_COUNT];
        for (index, demand) in demands.iter().enumerate() {
            if active[index] {
                for (r, need) in needed.iter_mut().enumerate() {
                    *need += (demand.amount - allocation[index]) * demand.costs[r];
                }
            }
        }
        if needed.iter().all(|n| *n <= 0.0) {
            break;
        }
        let scale = needed
            .iter()
            .enumerate()
            .filter(|(_, n)| **n > 0.0)
            .map(|(r, &n)| resources[r] / n)
            .fold(1.0, f64::min)
            .clamp(0.0, 1.0);
        for (index, demand) in demands.iter().enumerate() {
            if active[index] {
                let added = (demand.amount - allocation[index]) * scale;
                allocation[index] += added;
                for (r, resource) in resources.iter_mut().enumerate() {
                    *resource = (*resource - added * demand.costs[r]).max(0.0);
                }
            }
        }
        if scale >= 1.0 - f64::EPSILON {
            break;
        }
        let mut froze = false;
        for (index, demand) in demands.iter().enumerate() {
            if active[index]
                && (0..RESOURCE_COUNT)
                    .any(|r| needed[r] > 0.0 && resources[r] <= EPS && demand.costs[r] > 0.0)
            {
                active[index] = false;
                froze = true;
            }
        }
        if !froze {
            break;
        }
    }
    allocation
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-7, "{a} != {b}");
    }
    fn controls(kind: OrganelleKind, count: u32, size: f64) -> BodyControls {
        let mut controls = BodyControls::default();
        let k = kind as usize;
        controls.target_counts[k] = count;
        controls.target_sizes[k] = size;
        controls.build[k] = 1.0;
        controls
    }
    fn run(
        body: &mut Body,
        controls: &BodyControls,
        grow: f64,
        repair: f64,
        energy: &mut f64,
        minerals: &mut f64,
        steps: usize,
    ) -> WorkReport {
        let mut result = WorkReport::default();
        for _ in 0..steps {
            let report = body.work(
                grow,
                controls,
                repair,
                energy,
                minerals,
                1.0 / 30.0,
                MAX_AREA,
            );
            result.dissipated_energy += report.dissipated_energy;
            result.constructed_mass += report.constructed_mass;
            result.synthesized_mass += report.synthesized_mass;
            result.recycled_mass += report.recycled_mass;
            result.repaired_mass += report.repaired_mass;
            assert!(body.validate().is_ok());
        }
        result
    }

    #[test]
    fn family_snapshot_matches_individual_queries_bit_for_bit() {
        fn check(body: &Body) {
            let summaries = body.family_summaries();
            for kind in OrganelleKind::ALL {
                let summary = summaries[kind as usize];
                assert_eq!(summary.count, body.count(kind));
                for (actual, expected) in [
                    (summary.mass, body.mass(kind)),
                    (summary.functional_mass, body.functional_mass(kind)),
                    (summary.capacity, body.capacity(kind)),
                    (summary.damaged, body.damaged(kind)),
                    (summary.building, body.building(kind)),
                ] {
                    assert_eq!(actual.to_bits(), expected.to_bits(), "{kind:?}");
                }
            }
            let actual = body.capacities_from_summaries(&summaries);
            let expected = body.capacities();
            macro_rules! check_capacities {
                ($($field:ident),+ $(,)?) => {
                    $(assert_eq!(actual.$field.to_bits(), expected.$field.to_bits(), stringify!($field));)+
                };
            }
            check_capacities!(
                energy,
                minerals,
                gut_energy,
                gut_minerals,
                photosynthesis,
                metabolism,
                digestion,
                uptake,
                thrust,
                turn,
                capture,
                piercing,
                protection,
                repair,
                assembly,
                light_sensing,
                heat_sensing,
                chemical_sensing,
                mechanical_sensing,
                optical_sensing,
                optical_range,
                energy_storage,
                mineral_storage,
                gut_storage,
                propulsion,
                phagocytosis,
                attack,
                defense,
            );
        }

        let (mut body, _) = Body::founder();
        check(&body); // Most families are absent, including signed-zero sums.
        for round in 0..8 {
            for kind in OrganelleKind::ALL {
                body.organs.push(Organelle {
                    id: body.next_organelle_id,
                    kind,
                    mass: kind.min_mass() * [0.3, 0.999, 1.0, 1.7][round % 4],
                    integrity: [0.0, 0.199, 0.2, 0.63, 1.0][round % 5],
                    embodied_energy: 0.0,
                });
                body.next_organelle_id += 1;
            }
            check(&body); // Interleaved families, incomplete and damaged organs.
        }
        body.organs.reverse();
        check(&body); // Preserve the order of each sum, not a reordered total.
    }

    #[test]
    fn all_test_specimens_are_paid_packed_and_have_explicit_physical_specialization() {
        for recipe in BodyFixture::ALL {
            let (body, budget) = Body::fixture(recipe);
            body.validate().unwrap();
            assert!(body.viable());
            close(
                budget.introduced_minerals,
                body.structural_mass() + budget.minerals,
            );
            close(
                budget.introduced_energy,
                body.embodied_energy() + budget.energy + budget.dissipated_energy,
            );
            assert!(budget.dissipated_energy > 0.0);
        }
        let (bare, _) = Body::fixture(BodyFixture::Bare);
        for kind in OrganelleKind::ALL
            .into_iter()
            .filter(|k| !matches!(k, OrganelleKind::Core | OrganelleKind::Reserve))
        {
            assert_eq!(bare.functional_mass(kind), 0.0);
        }
        assert_eq!(bare.capacities().optical_range, 0.0);
    }
    #[test]
    fn universal_founder_has_only_one_minimum_core_and_bounded_paid_reserves() {
        let (body, budget) = Body::founder();
        body.validate().unwrap();
        assert_eq!(body.area, 1.0);
        assert_eq!(body.organs.len(), 1);
        assert_eq!(body.organs[0].kind, OrganelleKind::Core);
        assert_eq!(body.organs[0].mass, OrganelleKind::Core.min_mass());
        assert_eq!(body.organs[0].id, 1);
        assert_eq!(body.next_organelle_id, 2);
        assert_eq!(body.precursor_mass, [0.0; FAMILY_COUNT]);
        assert_eq!(body.precursor_energy, [0.0; FAMILY_COUNT]);
        close(body.energy_capacity(), 80.0);
        close(body.mineral_capacity(), 1.0);
        close(budget.energy, 68.0);
        close(budget.minerals, 0.6);
        close(budget.introduced_energy, 99.2);
        close(budget.introduced_minerals, 0.99);
        close(
            budget.introduced_energy,
            budget.energy + body.embodied_energy() + budget.dissipated_energy,
        );
        assert!(!body.can_partition());
        for kind in OrganelleKind::ALL
            .into_iter()
            .filter(|kind| *kind != OrganelleKind::Core)
        {
            assert_eq!(body.count(kind), 0);
            assert_eq!(body.functional_mass(kind), 0.0);
        }
    }
    #[test]
    fn expression_is_paid_family_specific_and_cannot_assemble_in_the_same_tick() {
        let (mut body, _) = Body::founder();
        let (mut energy, mut minerals) = (1000.0, 100.0);
        let initial_energy = energy + body.embodied_energy();
        let initial_material = minerals + body.structural_mass();
        let mut expression = BodyControls::default();
        expression.build[OrganelleKind::Flagellum as usize] = 1.0;
        // With no target, protein is still translated and held at a cost.
        let first = body.work(
            0.0,
            &expression,
            0.0,
            &mut energy,
            &mut minerals,
            1.0,
            MAX_AREA,
        );
        assert!(first.synthesized_mass > 0.0);
        assert_eq!(first.constructed_mass, 0.0);
        assert_eq!(body.organs.len(), 1);
        close(
            body.precursor(OrganelleKind::Flagellum),
            first.synthesized_mass,
        );
        assert!(body.upkeep() > 0.178);
        let mut wrong_family = controls(OrganelleKind::Photosynthetic, 1, 0.04);
        wrong_family.build.fill(0.0);
        let second = body.work(
            0.0,
            &wrong_family,
            0.0,
            &mut energy,
            &mut minerals,
            1.0,
            MAX_AREA,
        );
        assert_eq!(second.constructed_mass, 0.0);
        assert_eq!(body.count(OrganelleKind::Photosynthetic), 0);
        // Switching expression off does not erase the protein or prevent its
        // matching target from assembling on a later tick.
        let mut correct_family = controls(OrganelleKind::Flagellum, 1, 0.03);
        correct_family.build.fill(0.0);
        let third = body.work(
            0.0,
            &correct_family,
            0.0,
            &mut energy,
            &mut minerals,
            1.0,
            MAX_AREA,
        );
        assert_eq!(third.synthesized_mass, 0.0);
        assert!(body.capacities().thrust > 0.0);
        close(initial_material, minerals + body.structural_mass());
        close(
            initial_energy,
            energy
                + body.embodied_energy()
                + first.dissipated_energy
                + second.dissipated_energy
                + third.dissipated_energy,
        );
        body.validate().unwrap();
    }
    #[test]
    fn all_expressed_genes_and_assembly_compete_for_one_core_work_budget() {
        let (mut body, _) = Body::founder();
        let (mut energy, mut minerals) = (10000.0, 100.0);
        let mut request = BodyControls::default();
        request.build.fill(1.0);
        request.target_counts.fill(2);
        let capacity = body.capacities().assembly;
        let first = body.work(
            0.0,
            &request,
            0.0,
            &mut energy,
            &mut minerals,
            1.0,
            MAX_AREA,
        );
        close(
            first.synthesized_mass * PROTEIN_SYNTHESIS_FRACTION,
            capacity,
        );
        assert_eq!(first.constructed_mass, 0.0);
        for kind in OrganelleKind::ALL {
            close(
                body.precursor(kind),
                first.synthesized_mass / FAMILY_COUNT as f64,
            );
        }
        let capacity = body.capacities().assembly;
        let second = body.work(
            0.0,
            &request,
            0.0,
            &mut energy,
            &mut minerals,
            1.0,
            MAX_AREA,
        );
        assert!(second.synthesized_mass > 0.0 && second.constructed_mass > 0.0);
        let work = second.synthesized_mass * PROTEIN_SYNTHESIS_FRACTION
            + second.constructed_mass * (1.0 - PROTEIN_SYNTHESIS_FRACTION);
        assert!(work <= capacity + EPS);
        assert_eq!(
            body.capacities().thrust,
            0.0,
            "small protein allocations are not a motor"
        );
        body.validate().unwrap();
    }
    #[test]
    fn unused_protein_turnover_recovers_material_and_dissipates_embodied_energy() {
        let (mut body, _) = Body::founder();
        let k = OrganelleKind::Photosynthetic as usize;
        body.precursor_mass[k] = 0.1;
        body.precursor_energy[k] = 0.1
            * OrganelleKind::Photosynthetic.construction_cost()
            * PROTEIN_SYNTHESIS_FRACTION
            * EMBODIED_FRACTION;
        let (mut energy, mut minerals) = (68.0, 0.5);
        let initial_material = minerals + body.structural_mass();
        let initial_energy = energy + body.embodied_energy();
        let space = body.space().used;
        let report = body.work(
            0.0,
            &BodyControls::default(),
            0.0,
            &mut energy,
            &mut minerals,
            PROTEIN_HALF_LIFE,
            MAX_AREA,
        );
        close(body.precursor_mass[k], 0.05);
        close(report.minerals_recovered, 0.05);
        close(space - body.space().used, 0.05 * PROTEIN_INTERNAL_FOOTPRINT);
        assert_eq!(report.energy_spent, 0.0);
        assert_eq!(report.energy_recovered, 0.0);
        close(initial_material, minerals + body.structural_mass());
        close(
            initial_energy,
            energy + body.embodied_energy() + report.dissipated_energy,
        );
        body.validate().unwrap();
    }
    #[test]
    fn precursor_pools_partition_conservatively_and_old_bodies_start_without_them() {
        let (mut body, _) = Body::fixture(BodyFixture::Bare);
        body.area = 2.0;
        body.scaffold_energy *= 2.0;
        let k = OrganelleKind::Flagellum as usize;
        body.precursor_mass[k] = 0.1;
        body.precursor_energy[k] = 0.1
            * OrganelleKind::Flagellum.construction_cost()
            * PROTEIN_SYNTHESIS_FRACTION
            * EMBODIED_FRACTION;
        let (a, b) = body.partition().unwrap();
        close(a.precursor_mass[k], 0.05);
        close(b.precursor_mass[k], 0.05);
        close(
            a.structural_mass() + b.structural_mass(),
            body.structural_mass(),
        );
        close(
            a.embodied_energy() + b.embodied_energy(),
            body.embodied_energy(),
        );
        a.validate().unwrap();
        b.validate().unwrap();

        let (old, _) = Body::fixture(BodyFixture::Producer);
        let mut writer = Writer::new();
        old.write_checkpoint(&mut writer);
        // Version 3 ended after the individual organelle records.
        writer.0.truncate(writer.0.len() - FAMILY_COUNT * 16 - 1);
        let mut reader = Reader::new(&writer.0);
        assert_eq!(Body::read_checkpoint_version(&mut reader, 3).unwrap(), old);
        assert!(reader.finished());
        assert!(Body::read_checkpoint(&mut Reader::new(&writer.0)).is_err());

        let mut bad = a;
        bad.precursor_energy[k] = 100.0;
        assert!(bad.validate().is_err());
        bad.precursor_energy[k] = 0.0;
        bad.precursor_mass[k] = f64::NAN;
        assert!(bad.validate().is_err());
    }
    #[test]
    fn minimal_core_must_build_a_second_core_before_growing_divisible_frame() {
        let (mut body, budget) = Body::founder();
        let (mut energy, mut minerals) = (budget.energy, budget.minerals);
        let before = body.clone();
        let unsupported = body.work(
            1.0,
            &BodyControls::default(),
            0.0,
            &mut energy,
            &mut minerals,
            1.0,
            MAX_AREA,
        );
        assert_eq!(body, before);
        assert_eq!(unsupported.energy_spent, 0.0);
        let mut request = controls(OrganelleKind::Core, 2, 0.04);
        let mut heat = 0.0;
        for _ in 0..1500 {
            if body.count(OrganelleKind::Core) >= 2 && body.building(OrganelleKind::Core) == 0.0 {
                request.build[OrganelleKind::Core as usize] = 0.0;
            }
            let report = body.work(
                1.0,
                &request,
                0.0,
                &mut energy,
                &mut minerals,
                1.0 / 30.0,
                2.0,
            );
            heat += report.dissipated_energy;
            if body.area + EPS >= 2.0 {
                break;
            }
        }
        close(body.area, 2.0);
        assert_eq!(body.count(OrganelleKind::Core), 2);
        close(
            minerals + body.structural_mass(),
            budget.introduced_minerals,
        );
        close(
            energy + body.embodied_energy() + heat,
            budget.energy + before.embodied_energy(),
        );
        let (a, b) = body
            .partition()
            .expect("the second core and frame were physically constructed");
        assert_eq!(a.organs.len(), 1);
        assert_eq!(b.organs.len(), 1);
        assert_ne!(a.organs[0].id, b.organs[0].id);
        close(
            a.structural_mass() + b.structural_mass(),
            body.structural_mass(),
        );
        assert!(a.viable() && b.viable());
    }
    #[test]
    fn targets_and_unpaid_work_never_grant_capabilities() {
        let (mut body, _) = Body::fixture(BodyFixture::Bare);
        let mut requests = controls(OrganelleKind::Flagellum, u32::MAX, 0.1);
        let (mut energy, mut minerals) = (0.0, 100.0);
        let initial = body.clone();
        run(
            &mut body,
            &requests,
            1.0,
            0.0,
            &mut energy,
            &mut minerals,
            10,
        );
        assert_eq!(initial, body);
        energy = 1000.0;
        requests.build[OrganelleKind::Flagellum as usize] = 0.0;
        run(
            &mut body,
            &requests,
            0.0,
            0.0,
            &mut energy,
            &mut minerals,
            10,
        );
        assert_eq!(initial, body);
        requests.build[OrganelleKind::Flagellum as usize] = 1.0;
        run(
            &mut body,
            &requests,
            0.0,
            0.0,
            &mut energy,
            &mut minerals,
            1,
        );
        assert_eq!(body.count(OrganelleKind::Flagellum), 0);
        assert!(body.precursor(OrganelleKind::Flagellum) > 0.0);
        run(
            &mut body,
            &requests,
            0.0,
            0.0,
            &mut energy,
            &mut minerals,
            1,
        );
        assert_eq!(body.count(OrganelleKind::Flagellum), 1);
        assert!(body.mass(OrganelleKind::Flagellum) > 0.0);
        assert_eq!(body.capacities().thrust, 0.0);
        assert!(body.building(OrganelleKind::Flagellum) > 0.0);
    }
    #[test]
    fn growth_is_paid_does_not_create_machinery_and_cannot_use_new_space_this_tick() {
        let (mut body, _) = Body::fixture(BodyFixture::Bare);
        let initial = body.clone();
        let (mut energy, mut minerals) = (1000.0, 100.0);
        let resources = energy + body.embodied_energy();
        let matter = minerals + body.structural_mass();
        let report = run(
            &mut body,
            &BodyControls::default(),
            1.0,
            0.0,
            &mut energy,
            &mut minerals,
            60,
        );
        assert!(body.area > initial.area);
        assert_eq!(body.organs, initial.organs);
        close(body.structural_mass() + minerals, matter);
        close(
            body.embodied_energy() + energy + report.dissipated_energy,
            resources,
        );
        let area = body.area;
        run(
            &mut body,
            &BodyControls::default(),
            1.0,
            0.0,
            &mut energy,
            &mut minerals,
            6000,
        );
        assert!(body.area >= area);
        assert!(body.area <= 2.0 + EPS, "core limits paid frame expansion");
    }
    #[test]
    fn build_recycle_repair_cycle_conserves_material_and_only_loses_energy() {
        let (mut body, _) = Body::fixture(BodyFixture::Bare);
        let (mut energy, mut minerals) = (1000.0, 100.0);
        let initial_energy = energy + body.embodied_energy();
        let initial_material = minerals + body.structural_mass();
        let mut request = controls(OrganelleKind::Flagellum, 2, 0.08);
        let built = run(
            &mut body,
            &request,
            0.0,
            0.0,
            &mut energy,
            &mut minerals,
            180,
        );
        assert!(body.capacities().thrust > 0.0);
        body.damage(0.3);
        let repaired = run(
            &mut body,
            &BodyControls::default(),
            0.0,
            1.0,
            &mut energy,
            &mut minerals,
            180,
        );
        request.target_counts[OrganelleKind::Flagellum as usize] = 0;
        request.build[OrganelleKind::Flagellum as usize] = 0.0;
        request.recycle[OrganelleKind::Flagellum as usize] = 1.0;
        let recycled = run(
            &mut body,
            &request,
            0.0,
            0.0,
            &mut energy,
            &mut minerals,
            300,
        );
        assert_eq!(body.count(OrganelleKind::Flagellum), 0);
        close(initial_material, minerals + body.structural_mass());
        close(
            initial_energy,
            energy
                + body.embodied_energy()
                + built.dissipated_energy
                + repaired.dissipated_energy
                + recycled.dissipated_energy,
        );
        assert!(repaired.repaired_mass > 0.0);
    }
    #[test]
    fn damage_retains_occupancy_material_and_embodied_energy() {
        let (mut body, _) = Body::fixture(BodyFixture::Hunter);
        let space = body.space();
        let surface = body.surface();
        let material = body.structural_mass();
        let embodied = body.embodied_energy();
        body.damage(0.9);
        assert_eq!(space, body.space());
        assert_eq!(surface, body.surface());
        close(material, body.structural_mass());
        close(embodied, body.embodied_energy());
        assert_eq!(body.capacities().thrust, 0.0);
        assert_eq!(body.capacities().optical_range, 0.0);
        assert!(!body.can_partition());
    }
    #[test]
    fn simultaneous_requests_share_resources_without_family_priority() {
        let (mut body, _) = Body::fixture(BodyFixture::Bare);
        let mut request = controls(OrganelleKind::Photoreceptor, 1, 0.02);
        request.target_counts[OrganelleKind::Thermoreceptor as usize] = 1;
        request.target_sizes[OrganelleKind::Thermoreceptor as usize] = 0.02;
        request.build[OrganelleKind::Thermoreceptor as usize] = 1.0;
        let (mut energy, mut minerals) = (1000.0, 0.0006);
        body.work(
            0.0,
            &request,
            0.0,
            &mut energy,
            &mut minerals,
            1.0 / 30.0,
            MAX_AREA,
        );
        close(body.precursor(OrganelleKind::Photoreceptor), 0.0003);
        close(body.precursor(OrganelleKind::Thermoreceptor), 0.0003);
        close(body.mass(OrganelleKind::Photoreceptor), 0.0);
        close(body.mass(OrganelleKind::Thermoreceptor), 0.0);
        let a = Demand::build(OrganelleKind::Cortex, None, 1.0);
        let b = Demand::growth(1.0);
        let allocation = allocate(&[a, b], resource_costs([100.0, 2.0, 0.0, 0.0, 1.0]));
        close(allocation[0], 0.0);
        close(allocation[1], 1.0);
    }
    #[test]
    fn newly_grown_room_is_unavailable_until_next_tick_and_surface_can_bind_first() {
        let (mut body, _) = Body::fixture(BodyFixture::Bare);
        let mass = body.space().free;
        body.organs.push(Organelle {
            id: body.next_organelle_id,
            kind: OrganelleKind::Metabolic,
            mass,
            integrity: 1.0,
            embodied_energy: mass * 80.0 * EMBODIED_FRACTION,
        });
        body.next_organelle_id += 1;
        let (mut energy, mut minerals) = (1000.0, 100.0);
        let request = controls(OrganelleKind::Flagellum, 1, 0.03);
        body.work(
            1.0,
            &request,
            0.0,
            &mut energy,
            &mut minerals,
            1.0,
            MAX_AREA,
        );
        assert!(body.area > 1.0);
        assert_eq!(body.count(OrganelleKind::Flagellum), 0);
        body.work(
            0.0,
            &request,
            0.0,
            &mut energy,
            &mut minerals,
            1.0,
            MAX_AREA,
        );
        assert!(body.precursor(OrganelleKind::Flagellum) > 0.0);
        assert_eq!(body.functional_mass(OrganelleKind::Flagellum), 0.0);
        body.work(
            0.0,
            &request,
            0.0,
            &mut energy,
            &mut minerals,
            1.0,
            MAX_AREA,
        );
        assert!(body.functional_mass(OrganelleKind::Flagellum) > 0.0);
        let (mut surface_full, _) = Body::fixture(BodyFixture::Bare);
        let mass = surface_full.surface().free / OrganelleKind::Cortex.surface_footprint();
        surface_full.organs.push(Organelle {
            id: surface_full.next_organelle_id,
            kind: OrganelleKind::Cortex,
            mass,
            integrity: 0.0,
            embodied_energy: mass * 65.0 * EMBODIED_FRACTION,
        });
        surface_full.next_organelle_id += 1;
        let request = controls(OrganelleKind::Transporter, 1, 0.02);
        surface_full.work(
            0.0,
            &request,
            0.0,
            &mut energy,
            &mut minerals,
            1.0,
            MAX_AREA,
        );
        assert!(surface_full.space().free > 0.4);
        assert_eq!(
            surface_full.count(OrganelleKind::Transporter),
            0,
            "damaged cortex still occupies its anchors"
        );
    }
    #[test]
    fn lowered_targets_need_explicit_recycling_and_core_cannot_be_removed() {
        let (mut body, _) = Body::fixture(BodyFixture::Producer);
        let (mut energy, mut minerals) = (1000.0, 100.0);
        let before = body.clone();
        run(
            &mut body,
            &BodyControls::default(),
            0.0,
            0.0,
            &mut energy,
            &mut minerals,
            100,
        );
        assert_eq!(before, body);
        let mut request = BodyControls::default();
        request.recycle[OrganelleKind::Core as usize] = 1.0;
        run(
            &mut body,
            &request,
            0.0,
            0.0,
            &mut energy,
            &mut minerals,
            300,
        );
        assert_eq!(body.count(OrganelleKind::Core), 1);
        assert!(body.viable());
    }
    #[test]
    fn concurrent_core_recycling_cannot_authorize_unsupported_growth() {
        let (mut body, _) = Body::fixture(BodyFixture::Bare);
        let mut request = controls(OrganelleKind::Core, 1, 0.04);
        request.build[OrganelleKind::Core as usize] = 0.0;
        request.recycle[OrganelleKind::Core as usize] = 1.0;
        let (mut energy, mut minerals) = (1000.0, 100.0);
        for _ in 0..300 {
            body.work(
                1.0,
                &request,
                0.0,
                &mut energy,
                &mut minerals,
                1.0 / 30.0,
                MAX_AREA,
            );
            assert!(body.viable());
        }
        close(body.area, 1.0);
        assert_eq!(body.count(OrganelleKind::Core), 1);
    }
    #[test]
    fn division_preserves_each_individual_record_and_all_ledgers() {
        for recipe in BodyFixture::ALL {
            let (mut body, _) = Body::fixture(recipe);
            body.area = 2.0;
            body.scaffold_energy *= 2.0;
            assert!(body.can_partition());
            let (a, b) = body
                .partition()
                .expect("paid full-grown recipe can partition");
            a.validate().unwrap();
            b.validate().unwrap();
            assert!(a.viable() && b.viable());
            close(a.area + b.area, body.area);
            close(
                a.structural_mass() + b.structural_mass(),
                body.structural_mass(),
            );
            close(
                a.embodied_energy() + b.embodied_energy(),
                body.embodied_energy(),
            );
            let mut organs: Vec<_> = a.organs.into_iter().chain(b.organs).collect();
            organs.sort_by_key(|o| o.id);
            assert_eq!(organs, body.organs);
        }
        let (mut body, _) = Body::fixture(BodyFixture::Bare);
        body.organs
            .retain(|o| o.kind != OrganelleKind::Core || o.id == 1);
        body.organs[0].mass = 0.16;
        body.organs[0].embodied_energy = 0.16 * 80.0 * EMBODIED_FRACTION;
        body.area = 2.0;
        body.scaffold_energy *= 2.0;
        assert!(
            !body.can_partition(),
            "one large core is not implicitly cloned"
        );
        let (mut oversized, _) = Body::fixture(BodyFixture::Bare);
        oversized.area = 2.0;
        oversized.scaffold_energy *= 2.0;
        oversized.organs.push(Organelle {
            id: oversized.next_organelle_id,
            kind: OrganelleKind::Digestive,
            mass: 0.5,
            integrity: 1.0,
            embodied_energy: 0.5 * 65.0 * EMBODIED_FRACTION,
        });
        oversized.next_organelle_id += 1;
        oversized.validate().unwrap();
        assert!(
            !oversized.can_partition(),
            "oversized indivisible organ cannot be resized to fit a daughter"
        );
    }
    #[test]
    fn hard_targets_cannot_allocate_requested_counts_or_exceed_packing() {
        let (mut body, _) = Body::fixture(BodyFixture::Bare);
        let mut request = BodyControls::default();
        request.target_counts = [u32::MAX; FAMILY_COUNT];
        request.target_sizes = [1e200; FAMILY_COUNT];
        request.build = [1.0; FAMILY_COUNT];
        let (mut energy, mut minerals) = (1e8, 1e6);
        run(
            &mut body,
            &request,
            0.0,
            0.0,
            &mut energy,
            &mut minerals,
            3000,
        );
        assert!(body.organs.len() < 100);
        assert!(body.validate().is_ok());
        assert!(body.space().used <= body.space().capacity + EPS);
        assert!(body.surface().used <= body.surface().capacity + EPS);
        close(Body::geometry_limit(1200.0, 760.0), MAX_AREA);
        body.next_organelle_id = u32::MAX;
        let request = controls(OrganelleKind::Flagellum, u32::MAX, 0.03);
        let report = body.work(
            0.0,
            &request,
            0.0,
            &mut energy,
            &mut minerals,
            1.0 / 30.0,
            MAX_AREA,
        );
        assert!(report.identifier_exhausted || body.building(OrganelleKind::Flagellum) > 0.0);
    }
    #[test]
    fn body_checkpoint_roundtrip_continues_exactly_and_rejects_bad_ledgers() {
        let (mut body, _) = Body::fixture(BodyFixture::Producer);
        let request = controls(OrganelleKind::Flagellum, 2, 0.06);
        let (mut energy, mut minerals) = (1000.0, 100.0);
        run(
            &mut body,
            &request,
            0.2,
            0.0,
            &mut energy,
            &mut minerals,
            17,
        );
        body.damage(0.1);
        let mut writer = Writer::new();
        body.write_checkpoint(&mut writer);
        let mut reader = Reader::new(&writer.0);
        let mut restored = Body::read_checkpoint(&mut reader).unwrap();
        assert!(reader.finished());
        assert_eq!(body, restored);
        let (mut restored_energy, mut restored_minerals) = (energy, minerals);
        run(
            &mut body,
            &request,
            0.2,
            1.0,
            &mut energy,
            &mut minerals,
            60,
        );
        run(
            &mut restored,
            &request,
            0.2,
            1.0,
            &mut restored_energy,
            &mut restored_minerals,
            60,
        );
        assert_eq!(body, restored);
        assert_eq!(energy, restored_energy);
        assert_eq!(minerals, restored_minerals);
        restored.organs[0].embodied_energy = 1000.0;
        let mut writer = Writer::new();
        restored.write_checkpoint(&mut writer);
        assert!(Body::read_checkpoint(&mut Reader::new(&writer.0)).is_err());
        let mut malformed = Writer::new();
        malformed.f64(1.0);
        malformed.f64(7.0);
        malformed.u32(1);
        malformed.u32(u32::MAX);
        assert!(Body::read_checkpoint(&mut Reader::new(&malformed.0)).is_err());
    }
    #[test]
    fn building_uses_the_same_completion_tolerance_as_functional_mass() {
        for kind in OrganelleKind::ALL {
            let mut organ = Organelle {
                id: 1,
                kind,
                mass: kind.min_mass() - EPS * 0.5,
                integrity: 1.0,
                embodied_energy: 0.0,
            };
            assert!(organ.functional_mass() > 0.0);
            assert_eq!(organ.building(), 0.0, "{} is already complete", kind.name());
            organ.integrity = 0.0;
            assert_eq!(
                organ.building(),
                0.0,
                "damage is separate from incomplete construction"
            );
            organ.integrity = 1.0;
            organ.mass = kind.min_mass() - EPS * 2.0;
            assert_eq!(organ.functional_mass(), 0.0);
            assert!(organ.building() > EPS);
        }
    }
    #[test]
    fn checkpoint_rejects_multiple_negligible_partial_units_in_each_family() {
        for kind in OrganelleKind::ALL {
            let (mut body, _) = Body::fixture(BodyFixture::Bare);
            for number in 1..=2 {
                let mass = 1e-9;
                body.organs.push(Organelle {
                    id: body.next_organelle_id,
                    kind,
                    mass,
                    integrity: 1.0,
                    embodied_energy: mass * kind.construction_cost() * EMBODIED_FRACTION,
                });
                body.next_organelle_id += 1;
                let mut writer = Writer::new();
                body.write_checkpoint(&mut writer);
                let restored = Body::read_checkpoint(&mut Reader::new(&writer.0));
                if number == 1 {
                    assert_eq!(
                        restored.unwrap(),
                        body,
                        "one legitimate partial unit is preserved"
                    );
                } else {
                    assert!(
                        restored.unwrap_err().contains("multiple incomplete organs"),
                        "{}",
                        kind.name()
                    );
                }
            }
        }
    }
    #[test]
    fn changing_targets_and_partitioning_keep_at_most_one_partial_per_family() {
        let (mut body, _) = Body::fixture(BodyFixture::Bare);
        let (mut energy, mut minerals) = (1e6, 1e6);
        for _ in 0..60 {
            body.work(
                1.0,
                &BodyControls::default(),
                0.0,
                &mut energy,
                &mut minerals,
                1.0,
                2.0,
            );
        }
        close(body.area, 2.0);
        let mut request = controls(OrganelleKind::Flagellum, 3, 0.03);
        for _ in 0..2 {
            body.work(
                0.0,
                &request,
                0.0,
                &mut energy,
                &mut minerals,
                1.0 / 30.0,
                MAX_AREA,
            );
        }
        assert!(body.building(OrganelleKind::Flagellum) > 0.0);
        let (a, b) = body.partition().unwrap();
        for mut daughter in [a, b] {
            for step in 0..1600 {
                for kind in OrganelleKind::ALL
                    .into_iter()
                    .filter(|kind| *kind != OrganelleKind::Core)
                {
                    let index = kind as usize;
                    request.target_counts[index] = ((step / 23 + index) % 4) as u32;
                    request.target_sizes[index] =
                        kind.min_mass() * (1.0 + (step / 31 % 3) as f64 * 0.5);
                    request.build[index] = 1.0;
                    request.recycle[index] = 1.0;
                }
                daughter.work(
                    0.0,
                    &request,
                    0.0,
                    &mut energy,
                    &mut minerals,
                    0.25,
                    MAX_AREA,
                );
                daughter.validate().unwrap();
                for kind in OrganelleKind::ALL {
                    assert!(
                        daughter
                            .organs
                            .iter()
                            .filter(|o| o.kind == kind && o.mass + EPS < kind.min_mass())
                            .count()
                            <= 1
                    );
                }
            }
        }
    }
}
