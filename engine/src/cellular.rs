//! Material-bearing products and locally assembled cellular machinery.
//!
//! Program declarations are blueprints, never inventories. Each built object
//! keeps its own recipe and ingredient definitions, so editing or mutating a
//! genome cannot reinterpret already paid-for physical matter.
use crate::body::{
    Body, OrganelleKind, OrganelleSummary, WorkReport, CORE_MASS_PER_AREA, EMBODIED_FRACTION,
    FAMILY_COUNT, MAX_AREA, MIN_FUNCTIONAL_INTEGRITY, SCAFFOLD_ENERGY_PER_MASS,
    SCAFFOLD_MASS_PER_AREA, INTERNAL_PER_AREA, SURFACE_PER_RADIUS, PROTEIN_INTERNAL_FOOTPRINT,
};
use crate::checkpoint::{ensure, Reader, Result as CheckpointResult, Writer};
use crate::material_allocation::{AllocationScratch, MaterialDemand};
use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

pub const MAX_PRODUCTS: usize = 64;
pub const MAX_ASSEMBLIES: usize = 64;
const MAX_POOLS: usize = 1024;
const MAX_INSTANCES: usize = 32768;
const ADHESIN_FINISH_COST: f64 = 25.0;
const EPS: f64 = 1e-12;
const TRANSLATION_WORK: f64 = 0.6;
const INSTALLATION_WORK: f64 = 0.4;
const RECYCLE_RATE: f64 = 0.8;
const RECYCLE_ENERGY: f64 = 20.0;
const REPLICA_MASS_PER_AREA: f64 = 0.01;
const REPLICA_ENERGY_PER_MASS: f64 = 80.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ProductKind {
    Scaffold,
    Anchor,
    Motor,
    Filament,
    Pigment,
    Enzyme,
    Membrane,
    Transporter,
    Receptor,
    Regulator,
    Replicase,
    Constrictor,
    Adhesin,
}
impl ProductKind {
    pub const ALL: [Self; 13] = [
        Self::Scaffold,
        Self::Anchor,
        Self::Motor,
        Self::Filament,
        Self::Pigment,
        Self::Enzyme,
        Self::Membrane,
        Self::Transporter,
        Self::Receptor,
        Self::Regulator,
        Self::Replicase,
        Self::Constrictor,
        Self::Adhesin,
    ];
    pub fn name(self) -> &'static str {
        [
            "scaffold",
            "anchor",
            "motor",
            "filament",
            "pigment",
            "enzyme",
            "membrane",
            "transporter",
            "receptor",
            "regulator",
            "replicase",
            "constrictor",
            "adhesin",
        ][self as usize]
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
    pub fn synthesis_cost(self) -> f64 {
        [
            40.0, 50.0, 70.0, 45.0, 70.0, 65.0, 40.0, 65.0, 70.0, 35.0, 80.0, 60.0,
            // A finished unit contains one translated scaffold plus the finishing work.
            40.0 + ADHESIN_FINISH_COST,
        ][self as usize]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProductDef {
    pub name: String,
    pub kind: ProductKind,
    /// Zero means stable; positive values are a half-life in simulation seconds.
    pub half_life: f64,
}

/// Each mechanism grants one operation. Storage, structural support, and
/// translation are deliberately independent of food processing and sensing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MechanismKind {
    Support, Translation, EnergyStorage, MineralStorage, FoodStorage,
    LightHarvest, MineralImport, ParticleCapture, Engulf,
    DigestExudate, DigestTissue, DigestWaste, Propulsion, Pierce, Shield,
    Repair, SenseLight, SenseHeat, SenseChemical, SenseContact, SenseOptical,
    Grow, Replicate, Constrict, Recycle, RespireOxygen, Adhere, JunctionEnergy, JunctionMineral, JunctionCarbon, JunctionSignal,
}
impl MechanismKind {
    pub const ALL: [Self; 31] = [
        Self::Support, Self::Translation, Self::EnergyStorage, Self::MineralStorage,
        Self::FoodStorage, Self::LightHarvest, Self::MineralImport, Self::ParticleCapture,
        Self::Engulf, Self::DigestExudate, Self::DigestTissue, Self::DigestWaste,
        Self::Propulsion, Self::Pierce, Self::Shield, Self::Repair, Self::SenseLight,
        Self::SenseHeat, Self::SenseChemical, Self::SenseContact, Self::SenseOptical,
        Self::Grow, Self::Replicate, Self::Constrict, Self::Recycle, Self::RespireOxygen, Self::Adhere,
        Self::JunctionEnergy, Self::JunctionMineral, Self::JunctionCarbon, Self::JunctionSignal,
    ];
    pub fn name(self) -> &'static str {
        ["support()", "translate()", "store(energy)", "store(mineral)", "store(food)",
         "harvest(light)", "import(mineral)", "import(particle)", "engulf()",
         "digest(exudate)", "digest(tissue)", "digest(waste)", "propel()", "pierce()",
         "shield()", "repair()", "sense(light)", "sense(heat)", "sense(chemical)",
         "sense(contact)", "sense(optical)", "grow()", "replicate()", "constrict()", "recycle()", "respire(oxygen)", "adhere()",
         "junction(energy)", "junction(mineral)", "junction(carbon)", "junction(signal)"]
            [self as usize]
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
    pub fn constructor(self) -> &'static str {
        self.name().split_once('(').unwrap().0
    }
    pub fn target(self) -> Option<&'static str> {
        let target = self.name().split_once('(').unwrap().1.trim_end_matches(')');
        if target.is_empty() { None } else { Some(target) }
    }
    pub fn description(self) -> &'static str {
        ["Structural support for cell viability and division; no storage or translation.",
         "Paid product synthesis and installation work; no storage or metabolic capacity.",
         "Capacity to retain usable energy; does not produce energy.",
         "Capacity to retain free minerals; does not import minerals.",
         "A food compartment; does not capture or digest food.",
         "Fixes inorganic carbon into organic reserves and oxygen using shared local light.",
         "Imports dissolved minerals using energy; requires available mineral storage.",
         "Captures nearby organic particles into available food storage; no digestion.",
         "Engulfs sufficiently small, damaged cells into available food storage.",
         "Processes stored exudate; does not process tissue or waste.",
         "Processes stored tissue; does not process exudate or waste.",
         "Processes stored waste; does not process exudate or tissue.",
         "Applies force at the declared attachment angle, with physical torque and energy cost.",
         "Damages contacted cells; released tissue needs separate capture, storage and digestion.",
         "Physical protection from damage; no repair or growth.",
         "Paid repair of existing damaged structure; no growth.",
         "Measures local light; does not harvest energy.",
         "Measures local temperature.",
         "Measures local mineral and organic concentrations.",
         "Measures flow and physical contact.",
         "Detects visible objects in its viewing direction, subject to occlusion.",
         "Pays for additional envelope area; structural support must already exist.",
         "Pays for a material-bearing genome copy over time.",
         "Performs paid constriction after replication and feasible division geometry.",
         "Dismantles selected machinery over time, recovering original ingredients with paid work.",
         "Consumes organic carbon and oxygen to make usable energy and inorganic carbon.",
         "Holds contacting cells together while their surface messages match in both directions.",
         "Exchanges stored ATP with an adhered neighbor carrying an active energy junction.",
         "Exchanges free mineral with an adhered neighbor carrying an active mineral junction.",
         "Exchanges organic carbon reserves with an adhered neighbor carrying an active carbon junction.",
         "Reads the number and mean published signal of directly adhered neighbors with active signal junctions."]
            [self as usize]
    }
    pub fn required_roles(self) -> &'static [ProductKind] {
        use ProductKind::*;
        match self {
            Self::Support | Self::Shield => &[Scaffold],
            Self::Translation => &[Scaffold, Enzyme],
            Self::EnergyStorage | Self::MineralStorage | Self::FoodStorage => &[Membrane],
            Self::LightHarvest => &[Anchor, Pigment, Enzyme],
            Self::RespireOxygen => &[Enzyme, Membrane],
            Self::Adhere => &[Anchor, Membrane, Adhesin],
            Self::JunctionSignal => &[Membrane, Receptor],
            Self::JunctionEnergy | Self::JunctionMineral | Self::JunctionCarbon => &[Membrane, Transporter],
            Self::MineralImport => &[Anchor, Transporter],
            Self::ParticleCapture => &[Anchor, Motor],
            Self::Engulf => &[Anchor, Motor, Membrane],
            Self::DigestExudate | Self::DigestTissue | Self::DigestWaste | Self::Repair | Self::Recycle => &[Enzyme],
            Self::Propulsion => &[Anchor, Motor, Filament],
            Self::Pierce => &[Anchor, Filament, Enzyme],
            Self::SenseLight | Self::SenseOptical => &[Anchor, Receptor, Pigment],
            Self::SenseHeat | Self::SenseChemical => &[Anchor, Receptor],
            Self::SenseContact => &[Anchor, Receptor, Filament],
            Self::Grow => &[Membrane, Enzyme],
            Self::Replicate => &[Scaffold, Replicase],
            Self::Constrict => &[Anchor, Motor, Constrictor],
        }
    }
    pub fn minimum_mass(self) -> f64 {
        [0.04, 0.02, 0.02, 0.02, 0.04, 0.04, 0.015, 0.015, 0.025,
         0.01, 0.01, 0.01, 0.025, 0.025, 0.02, 0.025, 0.01, 0.01,
         0.015, 0.01, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02][self as usize]
    }
    fn internal_footprint(self) -> f64 {
        [1.0, 1.0, 2.4, 2.4, 2.5, 1.35, 0.7, 0.8, 0.8,
         1.0, 1.0, 1.0, 0.65, 0.8, 0.35, 1.0, 0.8, 0.8,
         1.0, 0.7, 1.3, 1.0, 1.0, 1.0, 1.0, 1.2, 1.0, 0.7, 0.7, 0.7, 0.7][self as usize]
    }
    fn surface_footprint(self) -> f64 {
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.55, 1.8, 2.2, 2.2,
         0.0, 0.0, 0.0, 1.5, 2.0, 3.0, 0.0, 1.2, 1.1,
         1.5, 1.1, 2.0, 1.5, 0.0, 1.5, 0.0, 0.0, 1.4, 1.4, 1.4, 1.4, 1.4][self as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssemblyKind {
    Organelle(OrganelleKind),
    Membrane,
    Replicator,
    Constrictor,
    Mechanism(MechanismKind),
}
impl AssemblyKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Organelle(kind) => kind.name(),
            Self::Membrane => "membrane",
            Self::Replicator => "replicator",
            Self::Constrictor => "constrictor",
            Self::Mechanism(kind) => kind.name(),
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "membrane" => Some(Self::Membrane),
            "replicator" => Some(Self::Replicator),
            "constrictor" => Some(Self::Constrictor),
            _ => MechanismKind::from_name(name).map(Self::Mechanism)
                .or_else(|| OrganelleKind::from_name(name).map(Self::Organelle)),
        }
    }
    pub fn required_roles(self) -> &'static [ProductKind] {
        use ProductKind::*;
        match self {
            Self::Organelle(kind) => match kind {
                OrganelleKind::Core | OrganelleKind::Metabolic => &[Scaffold, Enzyme],
                OrganelleKind::Photosynthetic => &[Anchor, Pigment, Enzyme],
                OrganelleKind::Digestive => &[Membrane, Enzyme],
                OrganelleKind::Reserve => &[Membrane],
                OrganelleKind::Transporter => &[Anchor, Transporter],
                OrganelleKind::Phagocyte => &[Anchor, Motor, Membrane],
                OrganelleKind::Flagellum => &[Anchor, Motor, Filament],
                OrganelleKind::Piercer => &[Anchor, Filament, Enzyme],
                OrganelleKind::Cortex => &[Scaffold],
                OrganelleKind::Repair => &[Enzyme],
                OrganelleKind::Photoreceptor | OrganelleKind::Optical => {
                    &[Anchor, Receptor, Pigment]
                }
                OrganelleKind::Thermoreceptor | OrganelleKind::Chemoreceptor => &[Anchor, Receptor],
                OrganelleKind::Mechanoreceptor => &[Anchor, Receptor, Filament],
            },
            Self::Membrane => &[Membrane, Enzyme],
            Self::Replicator => &[Scaffold, Replicase],
            Self::Constrictor => &[Anchor, Motor, Constrictor],
            Self::Mechanism(kind) => kind.required_roles(),
        }
    }
    pub fn allows_role(self, kind: ProductKind) -> bool {
        kind == ProductKind::Scaffold || self.required_roles().contains(&kind)
    }
    pub fn minimum_mass(self) -> f64 {
        match self {
            Self::Organelle(kind) => kind.minimum_mass(),
            Self::Mechanism(kind) => kind.minimum_mass(),
            _ => 0.02,
        }
    }
    pub fn internal_footprint(self) -> f64 {
        match self {
            Self::Organelle(kind) => kind.internal_footprint(),
            Self::Mechanism(kind) => kind.internal_footprint(),
            _ => 1.0,
        }
    }
    pub fn surface_footprint(self) -> f64 {
        match self {
            Self::Organelle(kind) => kind.surface_footprint(),
            Self::Membrane | Self::Constrictor => 1.5,
            Self::Replicator => 0.0,
            Self::Mechanism(kind) => kind.surface_footprint(),
        }
    }
    fn installation_cost(self) -> f64 {
        match self {
            Self::Organelle(kind) => kind.construction_cost() * 0.4,
            _ => 30.0,
        }
    }
    pub fn supports_cell(self) -> bool {
        matches!(self, Self::Organelle(OrganelleKind::Core) | Self::Mechanism(MechanismKind::Support))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssemblyDef {
    pub name: String,
    pub kind: AssemblyKind,
    pub parts: Vec<(usize, f64)>,
    /// Body-relative polar attachment angle, in radians.
    pub at: f64,
    /// Body-relative force (or viewing) direction, in radians.
    pub direction: f64,
}

/// Immutable while shared. Mutation detaches the declaration data before any
/// cached binding or installed recipe can observe a changed definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellularProgram(Rc<CellularDefinition>);

#[derive(Debug, Default)]
pub struct CellularDefinition {
    pub products: Vec<ProductDef>,
    /// Empty for ordinary translation. Otherwise indexed by output product.
    pub precursors: Vec<Option<usize>>,
    pub assemblies: Vec<AssemblyDef>,
    recipes: OnceCell<Vec<Rc<Recipe>>>,
}
impl Clone for CellularDefinition {
    fn clone(&self) -> Self {
        Self { products: self.products.clone(), precursors: self.precursors.clone(), assemblies: self.assemblies.clone(),
            recipes: OnceCell::new() }
    }
}
impl PartialEq for CellularDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.products == other.products && self.precursors == other.precursors && self.assemblies == other.assemblies
    }
}
impl Deref for CellularProgram {
    type Target = CellularDefinition;
    fn deref(&self) -> &Self::Target { &self.0 }
}
impl DerefMut for CellularProgram {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let definition = Rc::make_mut(&mut self.0);
        definition.recipes.take();
        definition
    }
}
impl CellularProgram {
    pub fn new(products: Vec<ProductDef>, assemblies: Vec<AssemblyDef>) -> Self {
        Self(Rc::new(CellularDefinition { products, precursors: Vec::new(), assemblies, recipes: OnceCell::new() }))
    }
    fn same_definition(&self, other: &Self) -> bool { Rc::ptr_eq(&self.0, &other.0) }
    fn recipe(&self, index: usize) -> Rc<Recipe> {
        self.recipes.get_or_init(|| self.assemblies.iter()
            .map(|definition| Rc::new(Recipe::from_def(self, definition))).collect())[index].clone()
    }
    /// Keep the authored allocation estimate independent of runtime sharing.
    pub(crate) fn authored_inline_bytes() -> usize {
        std::mem::size_of::<Vec<ProductDef>>() + std::mem::size_of::<Vec<AssemblyDef>>()
    }
    /// Charge the complete lazy recipe cache up front so creating a machine
    /// never changes a genome's retained allocation budget or save bytes.
    pub(crate) fn derived_bytes(&self) -> usize {
        let rc_header = 2 * std::mem::size_of::<usize>();
        rc_header + std::mem::size_of::<Vec<Option<usize>>>()
            + std::mem::size_of::<OnceCell<Vec<Rc<Recipe>>>>()
            + self.assemblies.len() * std::mem::size_of::<Rc<Recipe>>()
            + self.assemblies.iter().map(|def| rc_header + std::mem::size_of::<Recipe>()
                + def.name.len() + def.parts.len() * std::mem::size_of::<RecipePart>()
                + def.parts.iter().map(|&(product,_)| self.products[product].name.len()).sum::<usize>())
                .sum::<usize>()
    }
    pub fn validate(&self) -> Result<(), String> {
        self.validate_program(false)
    }
    pub(crate) fn validate_stored(&self) -> Result<(), String> {
        self.validate_program(true)
    }
    fn validate_program(&self, allow_inert_surfaces: bool) -> Result<(), String> {
        if !self.precursors.is_empty() && self.precursors.len() != self.products.len() {
            return Err("Precursor table must match the product declarations.".into());
        }
        for (index, product) in self.products.iter().enumerate() {
            let input = self.precursors.get(index).copied().flatten();
            if product.kind == ProductKind::Adhesin {
                if !input.is_some_and(|i| self.products.get(i).is_some_and(|p| p.kind == ProductKind::Scaffold)) {
                    return Err(format!("Product '{}' needs a named scaffold precursor: adhesin(Frame).", product.name));
                }
            } else if input.is_some() {
                return Err(format!("Product '{}' uses ordinary translation.", product.name));
            }
        }
        self.validate_material_recipes(allow_inert_surfaces)
    }
    // Physical stocks/ingredients contain the finished material identity. The
    // reaction that made them belongs to the producing program, possibly long gone.
    fn validate_material_recipes(&self, allow_inert_surfaces: bool) -> Result<(), String> {
        if self.products.len() > MAX_PRODUCTS || self.assemblies.len() > MAX_ASSEMBLIES {
            return Err("At most 64 products and 64 assemblies are supported.".into());
        }
        let mut names = std::collections::HashSet::new();
        for product in &self.products {
            if !valid_name(&product.name) || !names.insert(product.name.as_str()) {
                return Err("Product names must be unique identifiers.".into());
            }
            if !product.half_life.is_finite()
                || product.half_life < 0.0
                || product.half_life > 1e6
                || (product.kind == ProductKind::Regulator && product.half_life < 0.001)
            {
                return Err(format!(
                    "Product '{}' has an invalid half-life.",
                    product.name
                ));
            }
        }
        names.clear();
        for assembly in &self.assemblies {
            if !valid_name(&assembly.name) || !names.insert(assembly.name.as_str()) {
                return Err("Assembly names must be unique identifiers.".into());
            }
            if !assembly.at.is_finite()
                || !assembly.direction.is_finite()
                || assembly.at.abs() > 1e12
                || assembly.direction.abs() > 1e12
            {
                return Err(format!(
                    "Assembly '{}' has an invalid angle.",
                    assembly.name
                ));
            }
            let mut roles = [0.0; ProductKind::ALL.len()];
            let mut used = std::collections::HashSet::new();
            let mut total = 0.0;
            for &(index, amount) in &assembly.parts {
                let Some(product) = self.products.get(index) else {
                    return Err(format!(
                        "Assembly '{}' references an unknown product.",
                        assembly.name
                    ));
                };
                if !amount.is_finite() || amount <= 0.0 || !used.insert(index) {
                    return Err(format!(
                        "Assembly '{}' needs positive, distinct ingredients.",
                        assembly.name
                    ));
                }
                if !assembly.kind.allows_role(product.kind) {
                    return Err(format!(
                        "{} cannot use a {} product.",
                        assembly.kind.name(),
                        product.kind.name()
                    ));
                }
                roles[product.kind as usize] += amount;
                total += amount;
            }
            if total + EPS < assembly.kind.minimum_mass()
                || total > MAX_AREA * 0.9
                || total * assembly.kind.internal_footprint() > MAX_AREA * 0.9
                || total * assembly.kind.surface_footprint() > MAX_AREA.sqrt() * 0.9
            {
                return Err(format!(
                    "Assembly '{}' recipe mass must fit its physical family (minimum {}).",
                    assembly.name,
                    assembly.kind.minimum_mass()
                ));
            }
            for &role in assembly.kind.required_roles() {
                // Historical two-component mounting hardware remains inspectable
                // and recyclable. It exposes zero binding sites.
                if allow_inert_surfaces && role == ProductKind::Adhesin && roles[role as usize] == 0.0 {
                    continue;
                }
                if roles[role as usize] + EPS < total * 0.1 {
                    return Err(format!(
                        "Assembly '{}' needs at least 10% {} material.",
                        assembly.name,
                        role.name()
                    ));
                }
            }
        }
        Ok(())
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellularControls {
    pub expression: Vec<f64>,
    pub activity: Vec<f64>,
    /// Initiation gates never suspend already installed ingredients.
    pub start: Vec<bool>,
    /// Paid disassembly effort, selecting physical recipes by declaration name.
    pub recycle: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProductPool {
    pub product: ProductDef,
    pub mass: f64,
    pub embodied_energy: f64,
    pub expression_requested: f64,
    pub expression_rate: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecipePart {
    pub product: ProductDef,
    pub required_mass: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    pub name: String,
    pub kind: AssemblyKind,
    pub parts: Vec<RecipePart>,
    pub at: f64,
    pub direction: f64,
}
impl Recipe {
    fn from_def(program: &CellularProgram, def: &AssemblyDef) -> Self {
        Self {
            name: def.name.clone(),
            kind: def.kind,
            at: def.at,
            direction: def.direction,
            parts: def
                .parts
                .iter()
                .map(|&(i, amount)| RecipePart {
                    product: program.products[i].clone(),
                    required_mass: amount,
                })
                .collect(),
        }
    }
    pub fn required_mass(&self) -> f64 {
        self.parts.iter().map(|p| p.required_mass).sum()
    }
    #[cfg(test)]
    fn matches(&self, program: &CellularProgram, def: &AssemblyDef) -> bool {
        self.name == def.name
            && self.kind == def.kind
            && self.at == def.at
            && self.direction == def.direction
            && self.parts.len() == def.parts.len()
            && self.parts.iter().zip(&def.parts).all(|(a, &(i, amount))| {
                a.required_mass == amount && program.products.get(i) == Some(&a.product)
            })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ingredient {
    pub mass: f64,
    pub embodied_energy: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssemblyInstance {
    pub id: u32,
    pub recipe: Rc<Recipe>,
    pub ingredients: Vec<Ingredient>,
    pub integrity: f64,
    pub activity: f64,
}
// A tolerance scaled to this particular ingredient permits rounding at full
// installation without treating a small, explicitly declared ingredient as free.
fn counted_quota(mass:f64) -> f64 {
    let n=mass*f64::from(crate::counts::SCALE);
    let whole=if (n-n.round()).abs()<1e-9 { n.round() } else { n.ceil() };
    crate::counts::units((whole as u32).max(1))
}
fn ingredient_complete(mass: f64, required: f64) -> bool {
    mass > 0.0 && required - mass <= required * EPS
}
impl AssemblyInstance {
    pub fn mass(&self) -> f64 {
        self.ingredients.iter().map(|p| p.mass).sum()
    }
    pub fn embodied_energy(&self) -> f64 {
        self.ingredients.iter().map(|p| p.embodied_energy).sum()
    }
    pub fn complete(&self) -> bool {
        self.ingredients
            .iter()
            .zip(&self.recipe.parts)
            .all(|(part, recipe)| ingredient_complete(part.mass, recipe.required_mass))
    }
    fn has_functional_recipe(&self) -> bool {
        self.recipe.kind != AssemblyKind::Mechanism(MechanismKind::Adhere)
            || self.recipe.parts.iter().any(|part| part.product.kind == ProductKind::Adhesin)
    }
    /// Decoder-only interpretation of the completion rule in v5–15. Uses the
    /// actual installed mass and integrity; it never adds a missing ingredient.
    pub(crate) fn pre_v16_functional_mass(&self) -> f64 {
        if self.integrity >= MIN_FUNCTIONAL_INTEGRITY && self.ingredients.iter()
            .zip(&self.recipe.parts).all(|(p, r)| p.mass + EPS >= r.required_mass) {
            self.mass() * self.integrity
        } else { 0.0 }
    }
    pub fn functional_mass(&self) -> f64 {
        if self.has_functional_recipe() && self.complete() && self.integrity >= MIN_FUNCTIONAL_INTEGRITY {
            self.mass() * self.integrity
        } else {
            0.0
        }
    }
    pub fn internal_space(&self) -> f64 {
        self.mass() * self.recipe.kind.internal_footprint()
    }
    pub fn surface_space(&self) -> f64 {
        self.mass() * self.recipe.kind.surface_footprint()
    }
    pub fn building(&self) -> f64 {
        (self.recipe.required_mass() - self.mass()).max(0.0)
    }
}

/// A definition identity compares every physical attribute. Names remain labels;
/// a renamed or changed product cannot commandeer previously synthesized matter.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ProductIdentity(String, u8, u64);
fn identity_number(value: f64) -> u64 {
    // The physical definitions use numeric equality, where +0 and -0 are equal.
    if value == 0.0 { 0 } else { value.to_bits() }
}
impl ProductIdentity {
    fn new(product: &ProductDef) -> Self {
        Self(product.name.clone(), product.kind as u8, identity_number(product.half_life))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RecipeIdentity {
    name: String,
    kind: u8,
    at: u64,
    direction: u64,
    parts: Vec<(ProductIdentity, u64)>,
}
impl RecipeIdentity {
    fn kind(kind: AssemblyKind) -> u8 {
        match kind {
            AssemblyKind::Organelle(kind) => kind as u8,
            AssemblyKind::Membrane => 16,
            AssemblyKind::Replicator => 17,
            AssemblyKind::Constrictor => 18,
            AssemblyKind::Mechanism(kind) => 19 + kind as u8,
        }
    }
    fn from_definition(program: &CellularProgram, def: &AssemblyDef) -> Self {
        Self {
            name: def.name.clone(), kind: Self::kind(def.kind),
            at: identity_number(def.at), direction: identity_number(def.direction),
            parts: def.parts.iter().map(|&(i, mass)|
                (ProductIdentity::new(&program.products[i]), identity_number(mass))).collect(),
        }
    }
    pub(crate) fn from_recipe(recipe: &Recipe) -> Self {
        Self {
            name: recipe.name.clone(), kind: Self::kind(recipe.kind),
            at: identity_number(recipe.at), direction: identity_number(recipe.direction),
            parts: recipe.parts.iter().map(|p|
                (ProductIdentity::new(&p.product), identity_number(p.required_mass))).collect(),
        }
    }
}

/// These tables bind immutable physical identities to the current program.
/// Mass, damage, completion and activity are deliberately not cached here.
#[derive(Debug, Default)]
struct BindingCache {
    program: Option<CellularProgram>,
    pool_count: usize,
    instance_count: usize,
    next_id: u32,
    product_pool: Vec<Option<usize>>,
    pool_current: Vec<bool>,
    instance_definition: Vec<Option<usize>>,
    instance_selector: Vec<Option<usize>>,
    ingredient_pool: Vec<Vec<Option<usize>>>,
    definition_instances: Vec<Vec<usize>>,
}
impl BindingCache {
    fn ensure(&mut self, program: &CellularProgram, pools: &[ProductPool], assemblies: &[AssemblyInstance], next_id: u32) {
        if self.program.as_ref().is_some_and(|cached| cached.same_definition(program))
            && self.pool_count == pools.len()
            && self.instance_count == assemblies.len()
            && self.next_id == next_id
        { return; }
        if let Some(cached) = &mut self.program { cached.clone_from(program); }
        else { self.program = Some(program.clone()); }
        self.pool_count = pools.len();
        self.instance_count = assemblies.len();
        self.next_id = next_id;
        let stocks: HashMap<_, _> = pools.iter().enumerate()
            .map(|(i, p)| (ProductIdentity::new(&p.product), i)).collect();
        let recipes: HashMap<_, _> = program.assemblies.iter().enumerate()
            .map(|(i, d)| (RecipeIdentity::from_definition(program, d), i)).collect();
        let selectors: HashMap<_, _> = program.assemblies.iter().enumerate()
            .map(|(i, d)| (d.name.as_str(), i)).collect();
        self.product_pool.clear();
        self.product_pool.extend(program.products.iter().map(|p| stocks.get(&ProductIdentity::new(p)).copied()));
        self.pool_current.resize(pools.len(), false);
        self.pool_current.fill(false);
        for &pool in self.product_pool.iter().flatten() { self.pool_current[pool] = true; }
        self.instance_definition.clear();
        self.instance_selector.clear();
        self.ingredient_pool.resize_with(assemblies.len(), Vec::new);
        self.definition_instances.resize_with(program.assemblies.len(), Vec::new);
        for instances in &mut self.definition_instances { instances.clear(); }
        for (i, assembly) in assemblies.iter().enumerate() {
            let definition = recipes.get(&RecipeIdentity::from_recipe(&assembly.recipe)).copied();
            self.instance_definition.push(definition);
            self.instance_selector.push(selectors.get(assembly.recipe.name.as_str()).copied());
            if let Some(definition) = definition { self.definition_instances[definition].push(i); }
            let ingredients = &mut self.ingredient_pool[i];
            ingredients.clear();
            ingredients.extend(assembly.recipe.parts.iter().map(|p| stocks.get(&ProductIdentity::new(&p.product)).copied()));
        }
    }
    fn invalidate(&mut self) {
        self.next_id = 0;
    }
}

/// Derived lookup state is never checkpointed, compared or inherited as biology.
#[derive(Debug, Default)]
pub struct DerivedBindings(RefCell<BindingCache>);
impl Clone for DerivedBindings {
    fn clone(&self) -> Self { Self::default() }
}
impl PartialEq for DerivedBindings {
    fn eq(&self, _other: &Self) -> bool { true }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeControlReport {
    pub name: String,
    pub start_allowed: bool,
    pub recycle_requested: f64,
    /// Actual mass dismantled per second during the last work phase.
    pub recycle_rate: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct AssemblyMeasures {
    mass: f64,
    functional: f64,
    complete: bool,
}
impl AssemblyMeasures {
    fn of(assembly: &AssemblyInstance) -> Self {
        let mut mass = -0.0;
        let mut complete = true;
        for (ingredient, part) in assembly.ingredients.iter().zip(&assembly.recipe.parts) {
            mass += ingredient.mass;
            complete &= ingredient_complete(ingredient.mass, part.required_mass);
        }
        Self { mass, complete, functional: if complete && assembly.has_functional_recipe() && assembly.integrity >= MIN_FUNCTIONAL_INTEGRITY {
            mass * assembly.integrity
        } else { 0.0 } }
    }
}

/// Valid only within one work phase, before anything is built or dismantled.
struct WorkSnapshot {
    free_space: f64,
    free_surface: f64,
    support_area: f64,
    legacy_core: f64,
    active: [f64; FAMILY_COUNT],
    membrane: f64,
    replicator: f64,
    constrictor: f64,
    translation: f64,
    recycling: f64,
    damaged: f64,
}
impl Body {
    fn work_snapshot(&self, measures: &mut Vec<AssemblyMeasures>) -> WorkSnapshot {
        let mut organ_space = -0.0;
        let mut organ_surface = -0.0;
        let mut organ_damage = -0.0;
        let mut legacy_core = -0.0;
        for organ in &self.organs {
            let functional = organ.functional_mass();
            if organ.kind == OrganelleKind::Core { legacy_core += functional; }
            organ_space += organ.internal_space();
            organ_surface += organ.surface_space();
            organ_damage += organ.mass * (1.0 - organ.integrity);
        }
        let mut result = WorkSnapshot {
            free_space: 0.0, free_surface: 0.0, support_area: 0.0,
            legacy_core, active: [0.0; FAMILY_COUNT], membrane: 0.0, replicator: 0.0,
            constrictor: 0.0, translation: 0.0, recycling: 0.0, damaged: 0.0,
        };
        measures.clear();
        let mut assembly_space = -0.0;
        let mut assembly_surface = -0.0;
        let mut assembly_damage = -0.0;
        let mut support = legacy_core;
        let mut primitive_support = -0.0;
        let mut cell_space = 0.0;
        if let Some(state) = &self.cellular {
            measures.extend(state.assemblies.iter().map(AssemblyMeasures::of));
            for (assembly, measure) in state.assemblies.iter().zip(measures.iter()) {
                assembly_space += measure.mass * assembly.recipe.kind.internal_footprint();
                assembly_surface += measure.mass * assembly.recipe.kind.surface_footprint();
                assembly_damage += measure.mass * (1.0 - assembly.integrity);
                let active = measure.functional * assembly.activity;
                match assembly.recipe.kind {
                    AssemblyKind::Organelle(kind) => {
                        result.active[kind as usize] += active;
                        if kind == OrganelleKind::Core { support += measure.functional; }
                    }
                    AssemblyKind::Membrane => result.membrane += active,
                    AssemblyKind::Replicator => result.replicator += active,
                    AssemblyKind::Constrictor => result.constrictor += active,
                    AssemblyKind::Mechanism(kind) => match kind {
                        MechanismKind::Support => primitive_support += measure.functional,
                        MechanismKind::Translation => result.translation += active,
                        MechanismKind::Grow => result.membrane += active,
                        MechanismKind::Replicate => result.replicator += active,
                        MechanismKind::Constrict => result.constrictor += active,
                        MechanismKind::Repair => result.active[OrganelleKind::Repair as usize] += active,
                        MechanismKind::Recycle => result.recycling += active,
                        _ => {},
                    },
                }
            }
            cell_space = state.stock_mass() + state.genetic_mass() + assembly_space;
        }
        result.free_space = (INTERNAL_PER_AREA * self.area - (organ_space
            + self.precursor_mass.iter().sum::<f64>() * PROTEIN_INTERNAL_FOOTPRINT + cell_space)).max(0.0);
        result.free_surface = (SURFACE_PER_RADIUS * self.area.sqrt() - (organ_surface + assembly_surface)).max(0.0);
        result.support_area = (support + primitive_support) / CORE_MASS_PER_AREA;
        result.damaged = organ_damage + assembly_damage;
        result
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CellularState {
    pub pools: Vec<ProductPool>,
    pub assemblies: Vec<AssemblyInstance>,
    pub next_assembly_id: u32,
    /// Previously replicated genetic material, partitioned rather than erased.
    pub genome_mass: f64,
    pub genome_energy: f64,
    pub replica_mass: f64,
    pub replica_energy: f64,
    pub constriction: f64,
    pub control_reports: Vec<RecipeControlReport>,
    pub bindings: DerivedBindings,
}
impl Default for CellularState {
    fn default() -> Self {
        Self {
            pools: Vec::new(),
            assemblies: Vec::new(),
            next_assembly_id: 1,
            genome_mass: 0.0,
            genome_energy: 0.0,
            replica_mass: 0.0,
            replica_energy: 0.0,
            constriction: 0.0,
            control_reports: Vec::new(),
            bindings: DerivedBindings::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CellularActions {
    pub force_x: f64,
    pub force_y: f64,
    pub torque: f64,
    pub motor_cost: f64,
    pub photosynthesize: f64,
    pub absorb: f64,
    pub feed: f64,
    pub engulf: bool,
    pub capture_activity: f64,
    pub engulf_activity: f64,
    pub independent_engulf: f64,
    pub bite_extraction_fraction: f64,
    pub light_harvest: f64,
    pub respiration: f64,
    pub substrate_digestion: [f64; 3],
    pub bite: f64,
    pub defend: f64,
    pub repair: f64,
    pub digest: [f64; 3],
    pub divide: bool,
    pub scan_bearing: f64,
    pub scan_width: f64,
}

impl CellularState {
    pub fn add_mechanism_capacities(&self, cap: &mut crate::body::BodyCapacities, area: f64,
        functional: impl Fn(&AssemblyInstance) -> f64) {
        use MechanismKind::*;
        for assembly in &self.assemblies {
            let AssemblyKind::Mechanism(kind) = assembly.recipe.kind else { continue; };
            let mass = functional(assembly);
            let active = mass * assembly.activity;
            match kind {
                // Capacity is physical room. Switching activity off never
                // destroys a compartment or makes its contents disappear.
                EnergyStorage => cap.energy += 2000.0 * mass,
                MineralStorage => cap.minerals += 25.0 * mass,
                FoodStorage => {
                    cap.gut_energy += 800.0 * mass;
                    cap.gut_minerals += 12.0 * mass;
                }
                Translation => cap.assembly += 0.55 * active,
                LightHarvest => cap.light_harvest += 55.0 * mass,
                RespireOxygen => cap.respiration += 60.0 * active,
                MineralImport => cap.uptake += 0.8 * mass,
                ParticleCapture => cap.capture += mass,
                Engulf => { cap.phagocytosis += mass; cap.independent_engulf += mass; }
                DigestExudate => cap.substrate_digestion[0] += 65.0 * mass,
                DigestTissue => cap.substrate_digestion[1] += 65.0 * mass,
                DigestWaste => cap.substrate_digestion[2] += 65.0 * mass,
                Propulsion => { cap.thrust += 100.0 * mass; cap.turn += 25.0 * mass; }
                Pierce => cap.piercing += mass,
                Shield => cap.protection = (cap.protection + 6.0 * mass / area.sqrt()).min(0.8),
                Repair => cap.repair += 0.08 * mass,
                SenseLight => cap.light_sensing += active,
                SenseHeat => cap.heat_sensing += active,
                SenseChemical => cap.chemical_sensing += active,
                SenseContact => cap.mechanical_sensing += active,
                JunctionSignal => cap.signal_junction += active,
                SenseOptical => cap.optical_sensing += active,
                Support | Grow | Replicate | Constrict | Recycle | Adhere | JunctionEnergy | JunctionMineral | JunctionCarbon => {},
            }
        }
        cap.optical_range = if cap.optical_sensing > 0.0 {
            (35.0 + 300.0 * cap.optical_sensing.sqrt()).min(190.0)
        } else { 0.0 };
        cap.energy_storage = cap.energy;
        cap.mineral_storage = cap.minerals;
        cap.gut_storage = cap.gut_energy;
        cap.propulsion = cap.thrust;
        cap.attack = cap.piercing;
        cap.defense = cap.protection;
    }

    pub fn stock_mass(&self) -> f64 {
        self.pools.iter().map(|p| p.mass).sum()
    }
    pub fn genetic_mass(&self) -> f64 {
        self.genome_mass + self.replica_mass
    }
    pub fn structural_mass(&self) -> f64 {
        self.stock_mass()
            + self.genetic_mass()
            + self
                .assemblies
                .iter()
                .map(AssemblyInstance::mass)
                .sum::<f64>()
    }
    pub fn embodied_energy(&self) -> f64 {
        self.genome_energy
            + self.replica_energy
            + self.pools.iter().map(|p| p.embodied_energy).sum::<f64>()
            + self
                .assemblies
                .iter()
                .map(AssemblyInstance::embodied_energy)
                .sum::<f64>()
    }
    pub fn internal_space(&self) -> f64 {
        self.stock_mass()
            + self.genetic_mass()
            + self
                .assemblies
                .iter()
                .map(AssemblyInstance::internal_space)
                .sum::<f64>()
    }
    pub fn surface_space(&self) -> f64 {
        self.assemblies
            .iter()
            .map(AssemblyInstance::surface_space)
            .sum()
    }
    pub fn upkeep(&self) -> f64 {
        0.6 * (self.stock_mass() + self.genetic_mass())
            + self
                .assemblies
                .iter()
                .map(|a| {
                    a.mass()
                        * match a.recipe.kind {
                            AssemblyKind::Organelle(
                                OrganelleKind::Flagellum | OrganelleKind::Piercer,
                            )
                            | AssemblyKind::Constrictor
                            | AssemblyKind::Mechanism(MechanismKind::Propulsion | MechanismKind::Pierce | MechanismKind::Constrict) => 0.9,
                            _ => 0.45,
                        }
                })
                .sum::<f64>()
    }
    pub fn add_summaries(&self, summaries: &mut [OrganelleSummary; FAMILY_COUNT]) {
        for assembly in &self.assemblies {
            if let AssemblyKind::Organelle(kind) = assembly.recipe.kind {
                let entry = &mut summaries[kind as usize];
                entry.count += 1;
                entry.mass += assembly.mass();
                entry.functional_mass += assembly.functional_mass();
                entry.damaged += assembly.mass() * (1.0 - assembly.integrity);
                entry.building += assembly.building();
            }
        }
    }
    pub fn family_mass(&self, kind: OrganelleKind, functional: bool) -> f64 {
        self.assemblies
            .iter()
            .filter(|a| a.recipe.kind == AssemblyKind::Organelle(kind))
            .map(|a| {
                if functional {
                    a.functional_mass()
                } else {
                    a.mass()
                }
            })
            .sum()
    }
    pub fn replication_progress(&self, area: f64) -> f64 {
        (self.replica_mass / (REPLICA_MASS_PER_AREA * area)).clamp(0.0, 1.0)
    }
    pub fn partition_base(&self) -> Self {
        Self {
            pools: self
                .pools
                .iter()
                .map(|p| ProductPool {
                    product: p.product.clone(),
                    mass: p.mass * 0.5,
                    embodied_energy: p.embodied_energy * 0.5,
                    expression_requested: p.expression_requested,
                    expression_rate: 0.0,
                })
                .collect(),
            assemblies: Vec::new(),
            next_assembly_id: self.next_assembly_id,
            genome_mass: self.genetic_mass() * 0.5,
            genome_energy: (self.genome_energy + self.replica_energy) * 0.5,
            replica_mass: 0.0,
            replica_energy: 0.0,
            constriction: 0.0,
            control_reports: self.control_reports.iter().map(|report| RecipeControlReport {
                recycle_rate: 0.0, ..report.clone()
            }).collect(),
            bindings: DerivedBindings::default(),
        }
    }
    pub(crate) fn validate_material(&self, retention: f64, whole_quota: bool) -> CheckpointResult<()> {
        ensure(
            self.pools.len() <= MAX_POOLS && self.assemblies.len() <= MAX_INSTANCES,
            "too many cellular material records",
        )?;
        ensure(
            self.next_assembly_id > 0,
            "invalid next assembly identifier",
        )?;
        for (index, pool) in self.pools.iter().enumerate() {
            validate_product(&pool.product)?;
            ensure(
                !self.pools[..index]
                    .iter()
                    .any(|p| p.product == pool.product),
                "duplicate cellular product pool",
            )?;
            validate_ledger(
                pool.mass,
                pool.embodied_energy,
                pool.product.kind.synthesis_cost(),
                retention,
            )?;
            ensure(
                pool.expression_requested.is_finite()
                    && pool.expression_requested >= 0.0
                    && pool.expression_rate.is_finite()
                    && pool.expression_rate >= 0.0,
                "invalid cellular expression rate",
            )?;
        }
        let mut ids = std::collections::HashSet::new();
        for assembly in &self.assemblies {
            ensure(
                assembly.id > 0 && assembly.id < self.next_assembly_id && ids.insert(assembly.id),
                "invalid or duplicate assembly identifier",
            )?;
            let recipe = &assembly.recipe;
            let program = CellularProgram::new(
                recipe.parts.iter().map(|p| p.product.clone()).collect(),
                vec![AssemblyDef {
                    name: recipe.name.clone(),
                    kind: recipe.kind,
                    at: recipe.at,
                    direction: recipe.direction,
                    parts: recipe
                        .parts
                        .iter()
                        .enumerate()
                        .map(|(i, p)| (i, p.required_mass))
                        .collect(),
                }],
            );
            ensure(
                program.validate_material_recipes(true).is_ok(),
                "invalid installed assembly recipe",
            )?;
            ensure(
                assembly.ingredients.len() == recipe.parts.len() && assembly.mass() > 0.0,
                "invalid assembly ingredient count",
            )?;
            ensure(
                assembly.integrity.is_finite()
                    && (0.0..=1.0).contains(&assembly.integrity)
                    && assembly.activity.is_finite()
                    && (0.0..=1.0).contains(&assembly.activity),
                "invalid assembly integrity or activity",
            )?;
            for (ingredient, part) in assembly.ingredients.iter().zip(&recipe.parts) {
                validate_ledger(
                    ingredient.mass,
                    ingredient.embodied_energy,
                    part.product.kind.synthesis_cost() + recipe.kind.installation_cost(),
                    retention,
                )?;
                ensure(
                    ingredient.mass <= part.required_mass + EPS || whole_quota &&
                        (ingredient.mass - (part.required_mass*f64::from(crate::counts::SCALE)).ceil()/f64::from(crate::counts::SCALE)).abs() <= EPS,
                    "assembly ingredient exceeds recipe",
                )?;
            }
        }
        validate_ledger(
            self.genome_mass,
            self.genome_energy,
            REPLICA_ENERGY_PER_MASS,
            retention,
        )?;
        validate_ledger(
            self.replica_mass,
            self.replica_energy,
            REPLICA_ENERGY_PER_MASS,
            retention,
        )?;
        ensure(
            self.constriction.is_finite() && (0.0..=1.0).contains(&self.constriction),
            "invalid constriction progress",
        )?;
        ensure(self.control_reports.len() <= MAX_ASSEMBLIES, "too many recipe control reports")?;
        let mut names = std::collections::HashSet::new();
        for report in &self.control_reports {
            ensure(valid_name(&report.name) && names.insert(&report.name)
                && report.recycle_requested.is_finite() && (0.0..=1.0).contains(&report.recycle_requested)
                && report.recycle_rate.is_finite() && report.recycle_rate >= 0.0,
                "invalid recipe control report")?;
        }
        Ok(())
    }
    pub(crate) fn write_checkpoint(&self, writer: &mut Writer) {
        writer.u32(self.next_assembly_id);
        for value in [
            self.genome_mass,
            self.genome_energy,
            self.replica_mass,
            self.replica_energy,
            self.constriction,
        ] {
            writer.f64(value);
        }
        writer.count(self.pools.len());
        for pool in &self.pools {
            write_product(writer, &pool.product);
            for value in [
                pool.mass,
                pool.embodied_energy,
                pool.expression_requested,
                pool.expression_rate,
            ] {
                writer.f64(value);
            }
        }
        writer.count(self.assemblies.len());
        for assembly in &self.assemblies {
            writer.u32(assembly.id);
            writer.text(&assembly.recipe.name);
            writer.u8(match assembly.recipe.kind {
                AssemblyKind::Organelle(kind) => kind as u8,
                AssemblyKind::Membrane => 16,
                AssemblyKind::Replicator => 17,
                AssemblyKind::Constrictor => 18,
                AssemblyKind::Mechanism(kind) => 19 + kind as u8,
            });
            for value in [
                assembly.recipe.at,
                assembly.recipe.direction,
                assembly.integrity,
                assembly.activity,
            ] {
                writer.f64(value);
            }
            writer.count(assembly.recipe.parts.len());
            for (part, ingredient) in assembly.recipe.parts.iter().zip(&assembly.ingredients) {
                write_product(writer, &part.product);
                writer.f64(part.required_mass);
                writer.f64(ingredient.mass);
                writer.f64(ingredient.embodied_energy);
            }
        }
        writer.count(self.control_reports.len());
        for report in &self.control_reports {
            writer.text(&report.name);
            writer.u8(u8::from(report.start_allowed));
            writer.f64(report.recycle_requested);
            writer.f64(report.recycle_rate);
        }
    }
    pub(crate) fn read_checkpoint_version(reader: &mut Reader<'_>, version: u32) -> CheckpointResult<Self> {
        let mut state = Self {
            next_assembly_id: reader.u32()?,
            genome_mass: reader.f64()?,
            genome_energy: reader.f64()?,
            replica_mass: reader.f64()?,
            replica_energy: reader.f64()?,
            constriction: reader.f64()?,
            ..Self::default()
        };
        let count = reader.count(MAX_POOLS)?;
        for _ in 0..count {
            state.pools.push(ProductPool {
                product: read_product(reader, version)?,
                mass: reader.f64()?,
                embodied_energy: reader.f64()?,
                expression_requested: reader.f64()?,
                expression_rate: reader.f64()?,
            });
        }
        let count = reader.count(MAX_INSTANCES)?;
        for _ in 0..count {
            let id = reader.u32()?;
            let name = reader.text(128)?;
            let kind = match reader.u8()? {
                16 => AssemblyKind::Membrane,
                17 => AssemblyKind::Replicator,
                18 => AssemblyKind::Constrictor,
                value @ 19..=42 if version >= 6 => AssemblyKind::Mechanism(MechanismKind::ALL[(value - 19) as usize]),
                43 if version >= 7 => AssemblyKind::Mechanism(MechanismKind::Recycle),
                44 if version >= 9 => AssemblyKind::Mechanism(MechanismKind::RespireOxygen),
                45 if version >= 10 => AssemblyKind::Mechanism(MechanismKind::Adhere),
                49 if version >= 12 => AssemblyKind::Mechanism(MechanismKind::JunctionSignal),
                tag @ 46..=48 if version >= 11 => AssemblyKind::Mechanism(MechanismKind::ALL[(tag - 19) as usize]),
                value => AssemblyKind::Organelle(
                    OrganelleKind::from_index(value as usize)
                        .ok_or("Invalid simulation save: unknown assembly family.")?,
                ),
            };
            let at = reader.f64()?;
            let direction = reader.f64()?;
            let integrity = reader.f64()?;
            let activity = reader.f64()?;
            let count = reader.count(MAX_PRODUCTS)?;
            let mut parts = Vec::with_capacity(count);
            let mut ingredients = Vec::with_capacity(count);
            for _ in 0..count {
                parts.push(RecipePart {
                    product: read_product(reader, version)?,
                    required_mass: reader.f64()?,
                });
                ingredients.push(Ingredient {
                    mass: reader.f64()?,
                    embodied_energy: reader.f64()?,
                });
            }
            state.assemblies.push(AssemblyInstance {
                id,
                recipe: Rc::new(Recipe {
                    name,
                    kind,
                    parts,
                    at,
                    direction,
                }),
                ingredients,
                integrity,
                activity,
            });
        }
        if version >= 7 {
            let count = reader.count(MAX_ASSEMBLIES)?;
            for _ in 0..count {
                let name = reader.text(128)?;
                let start_allowed = match reader.u8()? {
                    0 => false, 1 => true,
                    _ => return Err("Invalid simulation save: invalid assembly initiation flag.".into()),
                };
                state.control_reports.push(RecipeControlReport {
                    name, start_allowed,
                    recycle_requested: reader.f64()?,
                    recycle_rate: reader.f64()?,
                });
            }
        }
        state.validate_material(crate::body::max_embodied_fraction(version), version >= 17)?;
        Ok(state)
    }
}

fn validate_product(product: &ProductDef) -> CheckpointResult<()> {
    ensure(
        valid_name(&product.name)
            && product.half_life.is_finite()
            && (0.0..=1e6).contains(&product.half_life)
            && (product.kind != ProductKind::Regulator || product.half_life >= 0.001),
        "invalid cellular product definition",
    )
}
fn validate_ledger(mass: f64, energy: f64, cost: f64, retention: f64) -> CheckpointResult<()> {
    ensure(
        mass.is_finite()
            && (0.0..=MAX_AREA * 0.9).contains(&mass)
            && energy.is_finite()
            && energy >= 0.0
            && energy <= mass * cost * retention + 1e-8,
        "invalid cellular material or energy ledger",
    )
}
fn write_product(writer: &mut Writer, product: &ProductDef) {
    writer.text(&product.name);
    writer.u8(product.kind as u8);
    writer.f64(product.half_life);
}
fn read_product(reader: &mut Reader<'_>, version: u32) -> CheckpointResult<ProductDef> {
    let name = reader.text(128)?;
    let kind = *ProductKind::ALL
        .get(reader.u8()? as usize)
        .ok_or("Invalid simulation save: unknown product role.")?;
    ensure(kind != ProductKind::Adhesin || version >= 16, "adhesin requires checkpoint version 16")?;
    let product = ProductDef {
        name,
        kind,
        half_life: reader.f64()?,
    };
    validate_product(&product)?;
    Ok(product)
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
    /// Gene inputs are a read-only snapshot, including no material from this tick.
    pub fn cellular_inputs(&self, program: &CellularProgram) -> (Vec<f64>, Vec<f64>) {
        let mut products = Vec::new();
        let mut assemblies = Vec::new();
        self.cellular_inputs_into(program, &mut products, &mut assemblies);
        (products, assemblies)
    }

    /// Retain the caller's buffers and resolve immutable physical identities only
    /// when the program or its material records change. Derived tables never enter
    /// checkpoints, so even first-time inspection leaves canonical state intact.
    pub fn cellular_inputs_into(&self, program: &CellularProgram, products: &mut Vec<f64>, assemblies: &mut Vec<f64>) {
        products.resize(program.products.len(), 0.0);
        products.fill(0.0);
        assemblies.resize(program.assemblies.len(), 0.0);
        assemblies.fill(0.0);
        let Some(state) = &self.cellular else { return; };
        let mut bindings = state.bindings.0.borrow_mut();
        bindings.ensure(program, &state.pools, &state.assemblies, state.next_assembly_id);
        for (value, pool) in products.iter_mut().zip(&bindings.product_pool) {
            if let Some(pool) = pool { *value = state.pools[*pool].mass; }
        }
        // Iterator Sum uses negative zero as the identity. Keep that exact old
        // convention, as well as physical-instance order within every recipe.
        assemblies.fill(-0.0);
        for (assembly, definition) in state.assemblies.iter().zip(&bindings.instance_definition) {
            if let Some(definition) = definition {
                assemblies[*definition] += assembly.functional_mass();
            }
        }
    }

    /// Stored activity belongs to the preceding snapshot, making these values
    /// suitable for both tick observation and immutable player inspection.
    pub fn active_recipe_masses_into(&self, program: &CellularProgram, masses: &mut Vec<f64>) {
        masses.resize(program.assemblies.len(), 0.0);
        masses.fill(0.0);
        let Some(state) = &self.cellular else { return; };
        let mut bindings = state.bindings.0.borrow_mut();
        bindings.ensure(program, &state.pools, &state.assemblies, state.next_assembly_id);
        for (assembly, definition) in state.assemblies.iter().zip(&bindings.instance_definition) {
            if let Some(definition) = definition {
                masses[*definition] += assembly.functional_mass() * assembly.activity;
            }
        }
    }

    pub fn active_recipe_mass(&self, program: &CellularProgram, definition: usize) -> f64 {
        let Some(state) = &self.cellular else { return 0.0; };
        let mut bindings = state.bindings.0.borrow_mut();
        bindings.ensure(program, &state.pools, &state.assemblies, state.next_assembly_id);
        bindings.definition_instances.get(definition).map_or(0.0, |indices| indices.iter()
            .map(|&i| state.assemblies[i].functional_mass() * state.assemblies[i].activity).sum())
    }

    /// Freeze activities of already installed machines before any construction.
    /// The vector force and its cross-product torque retain motor geometry.
    pub fn cellular_actions(
        &mut self,
        program: &CellularProgram,
        controls: &CellularControls,
    ) -> CellularActions {
        // One phase-local pass replaces a complete body scan per legacy family.
        // Preserve each family's original summation order, including its Sum identity.
        let mut total = [-0.0; FAMILY_COUNT];
        for organ in &self.organs {
            total[organ.kind as usize] += organ.functional_mass();
        }
        let radius = self.radius();
        let area = self.area;
        let state = self.cellular.get_or_insert_with(CellularState::default);
        let mut active = [0.0; FAMILY_COUNT];
        let mut mechanism_total = [0.0; MechanismKind::ALL.len()];
        let mut mechanism_active = [0.0; MechanismKind::ALL.len()];
        let mut actions = CellularActions {
            scan_width: std::f64::consts::TAU,
            digest: [1.0; 3],
            ..CellularActions::default()
        };
        let mut optical_direction = [0.0; 2];
        let mut bindings = state.bindings.0.borrow_mut();
        bindings.ensure(program, &state.pools, &state.assemblies, state.next_assembly_id);
        for (index, assembly) in state.assemblies.iter_mut().enumerate() {
            assembly.activity = bindings.instance_definition[index].map_or(0.0, |definition| {
                effort(controls.activity.get(definition).copied().unwrap_or(0.0))
            });
            let functional = assembly.functional_mass();
            let mass = functional * assembly.activity;
            match assembly.recipe.kind {
                AssemblyKind::Organelle(kind) => {
                    total[kind as usize] += functional;
                    active[kind as usize] += mass;
                }
                AssemblyKind::Mechanism(kind) => {
                    mechanism_total[kind as usize] += functional;
                    mechanism_active[kind as usize] += mass;
                }
                _ => {},
            }
            if matches!(assembly.recipe.kind, AssemblyKind::Organelle(OrganelleKind::Flagellum)
                | AssemblyKind::Mechanism(MechanismKind::Propulsion)) {
                    let force = 100.0 * mass;
                    let fx = force * assembly.recipe.direction.cos();
                    let fy = force * assembly.recipe.direction.sin();
                    actions.force_x += fx;
                    actions.force_y += fy;
                    actions.torque +=
                        radius * (assembly.recipe.at.cos() * fy - assembly.recipe.at.sin() * fx);
                    actions.motor_cost += force * 0.06;
            } else if matches!(assembly.recipe.kind, AssemblyKind::Organelle(OrganelleKind::Optical)
                | AssemblyKind::Mechanism(MechanismKind::SenseOptical)) {
                    optical_direction[0] += mass * assembly.recipe.direction.cos();
                    optical_direction[1] += mass * assembly.recipe.direction.sin();
            }
        }
        let ratio = |kind: OrganelleKind| {
            if total[kind as usize] > 0.0 {
                (active[kind as usize] / total[kind as usize]).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let combined_ratio = |family: OrganelleKind, mechanism: MechanismKind| {
            let total = total[family as usize] + mechanism_total[mechanism as usize];
            if total > 0.0 {
                ((active[family as usize] + mechanism_active[mechanism as usize]) / total).clamp(0.0, 1.0)
            } else { 0.0 }
        };
        actions.photosynthesize = ratio(OrganelleKind::Photosynthetic);
        actions.light_harvest = 55.0 * mechanism_active[MechanismKind::LightHarvest as usize];
        actions.respiration = 60.0 * mechanism_active[MechanismKind::RespireOxygen as usize];
        actions.substrate_digestion = [MechanismKind::DigestExudate, MechanismKind::DigestTissue, MechanismKind::DigestWaste]
            .map(|kind| 65.0 * mechanism_active[kind as usize]);
        actions.absorb = combined_ratio(OrganelleKind::Transporter, MechanismKind::MineralImport);
        actions.feed = ratio(OrganelleKind::Digestive);
        actions.digest = [actions.feed; 3];
        actions.engulf = active[OrganelleKind::Phagocyte as usize] + mechanism_active[MechanismKind::Engulf as usize] > 0.0;
        actions.capture_activity = combined_ratio(OrganelleKind::Phagocyte, MechanismKind::ParticleCapture);
        actions.engulf_activity = combined_ratio(OrganelleKind::Phagocyte, MechanismKind::Engulf);
        actions.independent_engulf = mechanism_active[MechanismKind::Engulf as usize];
        actions.bite = combined_ratio(OrganelleKind::Piercer, MechanismKind::Pierce);
        let piercing = active[OrganelleKind::Piercer as usize] + mechanism_active[MechanismKind::Pierce as usize];
        actions.bite_extraction_fraction = if piercing > 0.0 {
            active[OrganelleKind::Piercer as usize] / piercing
        } else { 0.0 };
        actions.defend = combined_ratio(OrganelleKind::Cortex, MechanismKind::Shield);
        actions.repair = combined_ratio(OrganelleKind::Repair, MechanismKind::Repair);
        if optical_direction[0] != 0.0 || optical_direction[1] != 0.0 {
            actions.scan_bearing = optical_direction[1].atan2(optical_direction[0]);
            actions.scan_width = std::f64::consts::PI;
        }
        actions.divide =
            state.replication_progress(area) >= 1.0 - EPS && state.constriction >= 1.0 - EPS;
        actions
    }

    /// Translation, installation and the cell cycle compete for the same
    /// ribosome work and free energy. Installation can consume only the product
    /// inventory sampled before translation; recovered resources and new room
    /// likewise become available on the next call.
    pub fn cellular_work(
        &mut self,
        program: &CellularProgram,
        controls: &CellularControls,
        energy: &mut f64,
        minerals: &mut f64,
        dt: f64,
        max_area: f64,
        division_geometry_ready: bool,
    ) -> WorkReport {
        self.cellular_work_in(
            program, controls, energy, minerals, dt, max_area,
            division_geometry_ready, &mut CellularWorkScratch::default(),
        )
    }

    /// World reuses this transient workspace between cells and ticks.
    pub(crate) fn cellular_work_in(
        &mut self,
        program: &CellularProgram,
        controls: &CellularControls,
        energy: &mut f64,
        minerals: &mut f64,
        dt: f64,
        max_area: f64,
        division_geometry_ready: bool,
        scratch: &mut CellularWorkScratch,
    ) -> WorkReport {
        let mut report = WorkReport::default();
        let counted = self.has_count_funding();
        // The body view also serves continuous worlds. In a counted world
        // update its product and ingredient stocks with exact integer arithmetic.
        let change_stock = |stock: &mut f64, amount: f64| {
            if counted {
                let before = crate::counts::nearest(*stock);
                let change = crate::counts::nearest(amount.abs());
                let after = if amount < 0.0 {
                    before.checked_sub(change).expect("funded material transfer")
                } else {
                    before.checked_add(change).expect("available product space")
                };
                *stock = crate::counts::units(after);
            } else {
                *stock = (*stock + amount).max(0.0);
            }
        };
        if !dt.is_finite() || dt <= 0.0 || !energy.is_finite() || !minerals.is_finite() {
            return report;
        }
        let snapshot = self.work_snapshot(&mut scratch.measures);
        let mut state = self.cellular.take().unwrap_or_default();
        let active = snapshot.active;
        let membrane = snapshot.membrane;
        let replicator = snapshot.replicator;
        let constrictor = snapshot.constrictor;
        let recycling = snapshot.recycling;
        let core_work = 0.55 * (snapshot.legacy_core + active[OrganelleKind::Core as usize]
            + snapshot.translation) * dt;
        let CellularWorkScratch { current_pools, stock, density, demands, recycle_requests, measures, allocation } = scratch;
        current_pools.clear();
        stock.clear();
        density.clear();
        demands.clear();
        recycle_requests.clear();
        let mut bindings = state.bindings.0.borrow_mut();
        bindings.ensure(program, &state.pools, &state.assemblies, state.next_assembly_id);
        current_pools.extend_from_slice(&bindings.product_pool);
        for (product, index) in program.products.iter().zip(current_pools.iter_mut()) {
            if index.is_none() && state.pools.len() < MAX_POOLS {
                *index = Some(state.pools.len());
                state.pools.push(ProductPool {
                    product: product.clone(), mass: 0.0, embodied_energy: 0.0,
                    expression_requested: 0.0, expression_rate: 0.0,
                });
            }
        }
        bindings.ensure(program, &state.pools, &state.assemblies, state.next_assembly_id);
        state.control_reports.resize_with(program.assemblies.len(), RecipeControlReport::default);
        for (index, (report, recipe)) in state.control_reports.iter_mut().zip(&program.assemblies).enumerate() {
            report.name.clone_from(&recipe.name);
            report.start_allowed = controls.start.get(index).copied().unwrap_or(true);
            report.recycle_requested = effort(controls.recycle.get(index).copied().unwrap_or(0.0));
            report.recycle_rate = 0.0;
        }
        for (index, pool) in state.pools.iter_mut().enumerate() {
            pool.expression_requested = 0.0;
            pool.expression_rate = 0.0;
            if pool.product.half_life > 0.0 {
                let fraction = -(-std::f64::consts::LN_2 * dt / pool.product.half_life).exp_m1();
                let (mass, embodied) = if let Some(funding) = &mut self.count_funding {
                    let n = crate::counts::nearest(pool.mass);
                    let done = funding.complete_work((4u64 << 56) | index as u64, pool.mass * fraction).min(n);
                    (crate::counts::units(done), if n==0 { 0.0 } else { pool.embodied_energy * (f64::from(done) / f64::from(n)) })
                } else { (pool.mass * fraction, pool.embodied_energy * fraction) };
                change_stock(&mut pool.mass, -mass);
                pool.embodied_energy = (pool.embodied_energy - embodied).max(0.0);
                report.minerals_recovered += mass;
                report.dissipated_energy += embodied;
            }
        }
        // Old-format protein remains physical when a genome is replaced. It
        // turns over normally, without being retyped into the new products.
        let legacy_decay =
            -(-std::f64::consts::LN_2 * dt / crate::body::PROTEIN_HALF_LIFE).exp_m1();
        for k in 0..FAMILY_COUNT {
            let mass = self.precursor_mass[k] * legacy_decay;
            let embodied = self.precursor_energy[k] * legacy_decay;
            self.precursor_mass[k] -= mass;
            self.precursor_energy[k] -= embodied;
            report.minerals_recovered += mass;
            report.dissipated_energy += embodied;
        }
        stock.extend(state.pools.iter().map(|p| p.mass));
        density.extend(state
            .pools
            .iter()
            .map(|p| {
                if p.mass > 0.0 {
                    p.embodied_energy / p.mass
                } else {
                    0.0
                }
            }));
        for (product, pool) in current_pools.iter().enumerate() {
            if let Some(pool) = *pool {
                let expression =
                    nonnegative(controls.expression.get(product).copied().unwrap_or(0.0))
                        .min(256.0);
                state.pools[pool].expression_requested = expression;
                if expression <= 0.0 { continue; }
                let requested = 0.12 * self.area.sqrt() * dt * expression;
                if program.products[product].kind == ProductKind::Adhesin {
                    let Some(input) = program.precursors.get(product).copied().flatten()
                        .and_then(|i| current_pools.get(i).copied().flatten()) else { continue; };
                    // Conversion and installation compete for the same frozen
                    // input stock. Processing occupies the precursor's old space.
                    let mut demand = CellDemand::new(CellOperation::Finish(input, pool), requested,
                        [ADHESIN_FINISH_COST, 0.0, 0.0, 0.0, TRANSLATION_WORK]);
                    demand.stock = Some(input);
                    demands.push(demand);
                } else {
                    demands.push(CellDemand::new(CellOperation::Synthesis(pool), requested, [
                        program.products[product].kind.synthesis_cost(), 1.0, 1.0, 0.0, TRANSLATION_WORK,
                    ]));
                }
            }
        }
        let mut pending_ids = 0_u32;
        for (definition, def) in program.assemblies.iter().enumerate() {
            if !controls.start.get(definition).copied().unwrap_or(true)
                || bindings.definition_instances[definition].iter().any(|&i| !measures[i].complete)
            {
                continue;
            }
            if state.assemblies.len() >= MAX_INSTANCES {
                break;
            }
            if self
                .next_cellular_identifier_available(&state, pending_ids)
                .is_none()
            {
                report.identifier_exhausted = true;
                continue;
            }
            // A declaration alone never allocates a physical object.
            if !def.parts.iter().any(|&(product, _)| {
                current_pools
                    .get(product)
                    .and_then(|p| *p)
                    .is_some_and(|pool| stock[pool] > EPS)
            }) {
                continue;
            }
            pending_ids += 1;
            state.assemblies.push(AssemblyInstance {
                id: 0,
                recipe: program.recipe(definition),
                ingredients: vec![Ingredient::default(); def.parts.len()],
                integrity: 1.0,
                activity: effort(controls.activity.get(definition).copied().unwrap_or(0.0)),
            });
            measures.push(AssemblyMeasures::default());
        }
        bindings.ensure(program, &state.pools, &state.assemblies, state.next_assembly_id);
        recycle_requests.extend(state.assemblies.iter().enumerate().map(|(index, _assembly)| {
            let requested = bindings.instance_selector[index].map_or(0.0, |definition|
                state.control_reports[definition].recycle_requested);
            if measures[index].mass > 0.0 { requested } else { 0.0 }
        }));
        // Effort requests a share of finite enzyme throughput, not a fraction
        // of remaining mass. A partially dismantled object therefore finishes
        // in finite time even when its effort is below one.
        let recycle_work = RECYCLE_RATE * recycling * dt
            / recycle_requests.iter().sum::<f64>().max(1.0);
        for (index, assembly) in state.assemblies.iter().enumerate() {
            // A teardown request never competes to install into the same object.
            // This also prevents freshly recovered ingredients funding this tick.
            if recycle_requests[index] > 0.0 {
                let amount = (recycle_requests[index] * recycle_work).min(measures[index].mass);
                if amount > 0.0 {
                    demands.push(CellDemand::new(CellOperation::RecycleAssembly(index), amount,
                        [RECYCLE_ENERGY, 0.0, (1.0 - assembly.recipe.kind.internal_footprint()).max(0.0), 0.0, INSTALLATION_WORK]));
                }
                continue;
            }
            if measures[index].complete {
                continue;
            }
            for (part, recipe) in assembly.recipe.parts.iter().enumerate() {
                let target = if self.count_funding.is_some() { counted_quota(recipe.required_mass) } else { recipe.required_mass };
                let needed = (target - assembly.ingredients[part].mass).max(0.0);
                if ingredient_complete(assembly.ingredients[part].mass, recipe.required_mass) {
                    continue;
                }
                let Some(pool) = bindings.ingredient_pool[index][part]
                else {
                    continue;
                };
                if stock[pool] <= 0.0 {
                    continue;
                }
                let mut demand = CellDemand::new(
                    CellOperation::Install(index, part, pool),
                    needed,
                    [
                        assembly.recipe.kind.installation_cost(),
                        0.0,
                        (assembly.recipe.kind.internal_footprint() - 1.0).max(0.0),
                        assembly.recipe.kind.surface_footprint(),
                        INSTALLATION_WORK,
                    ],
                );
                demand.stock = Some(pool);
                demands.push(demand);
            }
        }
        let max_area = if max_area.is_finite() {
            max_area.clamp(self.area, MAX_AREA)
        } else {
            self.area
        };
        let growth = (2.0 * membrane * dt).min((max_area.min(snapshot.support_area) - self.area).max(0.0));
        if growth > EPS {
            demands.push(CellDemand::new(
                CellOperation::Grow,
                growth * SCAFFOLD_MASS_PER_AREA,
                [SCAFFOLD_ENERGY_PER_MASS, 1.0, 0.0, 0.0, 1.0],
            ));
        }
        let replica_target = REPLICA_MASS_PER_AREA * self.area;
        let replication =
            (0.25 * replicator * dt).min((replica_target - state.replica_mass).max(0.0));
        if replication > EPS {
            demands.push(CellDemand::new(
                CellOperation::Replicate,
                replication,
                [REPLICA_ENERGY_PER_MASS, 1.0, 1.0, 0.0, 1.0],
            ));
        }
        // A newly completed copy is not usable for constriction in this tick.
        if division_geometry_ready && state.replica_mass + EPS >= replica_target {
            let constriction =
                (20.0 * constrictor / self.area * dt).min((1.0 - state.constriction).max(0.0));
            if constriction > EPS {
                demands.push(CellDemand::new(
                    CellOperation::Constrict,
                    constriction,
                    [8.0 * self.area, 0.0, 0.0, 0.0, 0.1 * self.area],
                ));
            }
        }
        let damaged = snapshot.damaged;
        if damaged > EPS {
            let repair = (0.08 * active[OrganelleKind::Repair as usize] * dt / damaged).min(1.0);
            if repair > 0.0 {
                for (index, organ) in self.organs.iter().enumerate() {
                    let amount = organ.mass * (1.0 - organ.integrity) * repair;
                    if amount > EPS {
                        demands.push(CellDemand::new(
                            CellOperation::RepairOrgan(index),
                            amount,
                            [60.0, 0.0, 0.0, 0.0, 1.0],
                        ));
                    }
                }
                for (index, assembly) in state.assemblies.iter().enumerate() {
                    if recycle_requests[index] > 0.0 { continue; }
                    let amount = measures[index].mass * (1.0 - assembly.integrity) * repair;
                    if amount > EPS {
                        demands.push(CellDemand::new(
                            CellOperation::RepairAssembly(index),
                            amount,
                            [60.0, 0.0, 0.0, 0.0, 1.0],
                        ));
                    }
                }
            }
        }
        let mut remaining = [energy.max(0.0), minerals.max(0.0), snapshot.free_space, snapshot.free_surface, core_work];
        let amounts = allocation.allocate(demands, remaining, stock);
        for (demand, &planned) in demands.iter().zip(amounts) {
            let mut amount=planned;
            if let Some(funding)=&mut self.count_funding {
                // The allocator apportions continuous work. A finished molecule
                // may use work accumulated over earlier ticks, but its inputs,
                // space and ATP must all still be available when it commits.
                let mut limit=f64::INFINITY;
                for (&cost, &budget) in demand.common[..4].iter().zip(&remaining) {
                    if cost>0.0 { limit=limit.min(budget/cost); }
                }
                if let Some(i)=demand.stock { limit=limit.min(stock[i]); }
                let key=match demand.operation {
                    CellOperation::Synthesis(i) => Some(i as u64),
                    CellOperation::Finish(_,i) => Some((1u64<<56) | i as u64),
                    CellOperation::Install(i,p,_) => {
                        let a=&state.assemblies[i];
                        
                        // Old recipes can specify sub-molecule quotas. Installing
                        // a molecule satisfies its physical requirement.
                        let need=a.recipe.parts[p].required_mass;
                        let target=crate::counts::nearest(counted_quota(need));
                        limit=limit.min(crate::counts::units(target.saturating_sub(crate::counts::nearest(a.ingredients[p].mass))));
                        Some((2u64<<56) | ((i as u64)<<16) | p as u64)
                    }
                    CellOperation::RecycleAssembly(i) => {
                        limit=limit.min(measures[i].mass);
                        Some((3u64<<56) | i as u64)
                    }
                    _=>None,
                };
                amount=if let Some(key)=key {
                    crate::counts::units(funding.complete_work(key,planned).min(crate::counts::floor(limit)))
                } else { planned.min(limit) };
                for (budget,&cost) in remaining[..4].iter_mut().zip(&demand.common) {
                    *budget=(*budget-amount*cost).max(0.0);
                }
                if let Some(i)=demand.stock { change_stock(&mut stock[i], -amount); }
            }
            if amount<=0.0 { continue; }
            let spent = amount * demand.common[0];
            report.energy_spent += spent;
            report.minerals_spent += amount * demand.common[1];
            match demand.operation {
                CellOperation::Synthesis(pool) => {
                    change_stock(&mut state.pools[pool].mass, amount);
                    state.pools[pool].embodied_energy += spent * EMBODIED_FRACTION;
                    state.pools[pool].expression_rate += amount / dt;
                    report.synthesized_mass += amount;
                    report.dissipated_energy += spent * (1.0 - EMBODIED_FRACTION);
                }
                CellOperation::Finish(input, output) => {
                    let carried = amount * density[input];
                    let source = &mut state.pools[input];
                    change_stock(&mut source.mass, -amount);
                    source.embodied_energy = (source.embodied_energy - carried).max(0.0);
                    let finished = &mut state.pools[output];
                    change_stock(&mut finished.mass, amount);
                    finished.embodied_energy += carried + spent * EMBODIED_FRACTION;
                    finished.expression_rate += amount / dt;
                    report.synthesized_mass += amount;
                    report.dissipated_energy += spent * (1.0 - EMBODIED_FRACTION);
                }
                CellOperation::Install(index, part, pool) => {
                    let substrate_energy = amount * density[pool];
                    change_stock(&mut state.pools[pool].mass, -amount);
                    state.pools[pool].embodied_energy =
                        (state.pools[pool].embodied_energy - substrate_energy).max(0.0);
                    let assembly = &mut state.assemblies[index];
                    let old_mass = assembly.mass();
                    assembly.integrity =
                        (old_mass * assembly.integrity + amount) / (old_mass + amount);
                    change_stock(&mut assembly.ingredients[part].mass, amount);
                    assembly.ingredients[part].embodied_energy +=
                        substrate_energy + spent * EMBODIED_FRACTION;
                    report.constructed_mass += amount;
                    report.dissipated_energy += spent * (1.0 - EMBODIED_FRACTION);
                }
                CellOperation::Grow => {
                    let area = amount / SCAFFOLD_MASS_PER_AREA;
                    self.area += area;
                    self.scaffold_energy += spent * EMBODIED_FRACTION;
                    report.area_grown += area;
                    report.constructed_mass += amount;
                    report.dissipated_energy += spent * (1.0 - EMBODIED_FRACTION);
                }
                CellOperation::Replicate => {
                    state.replica_mass += amount;
                    state.replica_energy += spent * EMBODIED_FRACTION;
                    report.constructed_mass += amount;
                    report.dissipated_energy += spent * (1.0 - EMBODIED_FRACTION);
                }
                CellOperation::Constrict => {
                    state.constriction = (state.constriction + amount).min(1.0);
                    report.dissipated_energy += spent;
                }
                CellOperation::RepairOrgan(index) => {
                    let organ = &mut self.organs[index];
                    organ.integrity = (organ.integrity + amount / organ.mass).min(1.0);
                    report.repaired_mass += amount;
                    report.dissipated_energy += spent;
                }
                CellOperation::RepairAssembly(index) => {
                    let assembly = &mut state.assemblies[index];
                    assembly.integrity = (assembly.integrity + amount / assembly.mass()).min(1.0);
                    report.repaired_mass += amount;
                    report.dissipated_energy += spent;
                }
                CellOperation::RecycleAssembly(index) => {
                    let assembly = &mut state.assemblies[index];
                    let fraction = (amount / measures[index].mass).min(1.0);
                    let mut recovered = 0.0;
                    let whole = crate::counts::nearest(measures[index].mass);
                    let dismantled = crate::counts::nearest(amount);
                    let mut prefix = 0u32;
                    for (part_index, (ingredient, part)) in assembly.ingredients.iter_mut()
                        .zip(&assembly.recipe.parts).enumerate()
                    {
                        let (mass, embodied) = if self.count_funding.is_some() {
                            let available=crate::counts::nearest(ingredient.mass);
                            let first=crate::counts::proportional(dismantled,prefix,whole);
                            prefix+=available;
                            let part=crate::counts::proportional(dismantled,prefix,whole)-first;
                            (crate::counts::units(part), if available==0 {0.0} else {ingredient.embodied_energy*(f64::from(part)/f64::from(available))})
                        } else { (ingredient.mass*fraction, ingredient.embodied_energy*fraction) };
                        change_stock(&mut ingredient.mass, -mass);
                        ingredient.embodied_energy = (ingredient.embodied_energy - embodied).max(0.0);
                        recovered += mass;
                        if mass <= 0.0 { continue; }
                        let pool = bindings.ingredient_pool[index][part_index].or_else(|| {
                            // Only an inherited ingredient without a current pool
                            // reaches this exceptional path. Other recovery in the
                            // same phase may already have recreated its identity.
                            state.pools.iter().position(|p| p.product == part.product)
                        }).or_else(|| {
                            if state.pools.len() >= MAX_POOLS { return None; }
                            let pool = state.pools.len();
                            state.pools.push(ProductPool {
                                product: part.product.clone(), mass: 0.0, embodied_energy: 0.0,
                                expression_requested: 0.0, expression_rate: 0.0,
                            });
                            Some(pool)
                        });
                        if let Some(pool) = pool {
                            bindings.ingredient_pool[index][part_index] = Some(pool);
                            // Installation energy is dissipated, never refunded as
                            // usable energy or inflated product energy density.
                            let retained = embodied.min(mass * part.product.kind.synthesis_cost() * EMBODIED_FRACTION);
                            change_stock(&mut state.pools[pool].mass, mass);
                            state.pools[pool].embodied_energy += retained;
                            report.dissipated_energy += embodied - retained;
                        } else {
                            // A full inherited-identity table cannot destroy matter.
                            report.minerals_recovered += mass;
                            report.dissipated_energy += embodied;
                        }
                    }
                    report.recycled_mass += recovered;
                    report.dissipated_energy += spent;
                    if let Some(definition) = bindings.instance_selector[index] {
                        state.control_reports[definition].recycle_rate += recovered / dt;
                    }
                }
            }
        }
        let previous_instances = state.assemblies.len();
        state.assemblies.retain(|assembly| assembly.mass() > 0.0);
        for assembly in &mut state.assemblies {
            if assembly.id == 0 {
                assembly.id = state.next_assembly_id;
                state.next_assembly_id += 1;
            }
        }
        // Drop only exactly empty obsolete pools; never discard small residues.
        let previous_pools = state.pools.len();
        let mut pool_index = 0;
        state.pools.retain(|pool| {
            let current = bindings.pool_current.get(pool_index).copied().unwrap_or(false);
            pool_index += 1;
            pool.mass > 0.0
                || pool.embodied_energy > 0.0
                || current
        });
        if previous_instances != state.assemblies.len() || previous_pools != state.pools.len() {
            bindings.invalidate();
        }
        drop(bindings);
        self.cellular = Some(state);
        *energy = (*energy - report.energy_spent + report.energy_recovered).max(0.0);
        *minerals = (*minerals - report.minerals_spent + report.minerals_recovered).max(0.0);
        debug_assert!(self.space().used <= self.space().capacity + 1e-7);
        debug_assert!(self.surface().used <= self.surface().capacity + 1e-7);
        report
    }

    fn next_cellular_identifier_available(
        &self,
        state: &CellularState,
        pending: u32,
    ) -> Option<u32> {
        state
            .next_assembly_id
            .checked_add(pending)
            .filter(|&id| id < u32::MAX)
    }

    /// Inspection describes real pools and objects, including obsolete material
    /// retained after a genome change. Declared recipes are shown separately.
    pub fn cellular_json(&self, program: Option<&CellularProgram>) -> String {
        use std::fmt::Write;
        let mut json = String::from("{\"products\":[");
        let mut first = true;
        let finish_product = |json: &mut String, definition: Option<(&CellularProgram, usize)>| {
            if let Some((program, index)) = definition {
                if let Some(input) = program.precursors.get(index).copied().flatten() {
                    write!(json, ",\"precursor\":{:?}", program.products[input].name).unwrap();
                }
            }
            json.push('}');
        };
        if let Some(state) = &self.cellular {
            for pool in &state.pools {
                if !first {
                    json.push(',');
                }
                first = false;
                let definition = program.and_then(|p|
                    p.products.iter().position(|item| item == &pool.product).map(|i| (p, i)));
                write!(&mut json, "{{\"name\":{:?},\"role\":{:?},\"mass\":{},\"concentration\":{},\"halfLife\":{},\"expressionRequested\":{},\"expressionRate\":{},\"current\":{}",
                    pool.product.name, pool.product.kind.name(), pool.mass, pool.mass / self.area,
                    pool.product.half_life, pool.expression_requested, pool.expression_rate, definition.is_some()).unwrap();
                finish_product(&mut json, definition);
            }
        }
        if let Some(program) = program {
            for (index, product) in program.products.iter().enumerate() {
                if self
                    .cellular
                    .as_ref()
                    .is_some_and(|state| state.pools.iter().any(|p| &p.product == product))
                {
                    continue;
                }
                if !first {
                    json.push(',');
                }
                first = false;
                write!(&mut json, "{{\"name\":{:?},\"role\":{:?},\"mass\":0,\"concentration\":0,\"halfLife\":{},\"expressionRequested\":0,\"expressionRate\":0,\"current\":true",
                    product.name, product.kind.name(), product.half_life).unwrap();
                finish_product(&mut json, Some((program, index)));
            }
        }
        json.push_str("],\"assemblies\":[");
        if let Some(state) = &self.cellular {
            for (index, assembly) in state.assemblies.iter().enumerate() {
                if index > 0 {
                    json.push(',');
                }
                let recycle = state.control_reports.iter().find(|r| r.name == assembly.recipe.name)
                    .map_or(0.0, |r| r.recycle_requested);
                write!(&mut json, "{{\"id\":{},\"name\":{:?},\"kind\":{:?},\"mass\":{},\"requiredMass\":{},\"functionalMass\":{},\"complete\":{},\"integrity\":{},\"activity\":{},\"at\":{},\"direction\":{},\"recycleRequested\":{},\"bottleneck\":",
                    assembly.id, assembly.recipe.name, assembly.recipe.kind.name(), assembly.mass(),
                    assembly.recipe.required_mass(), assembly.functional_mass(), assembly.complete(),
                    assembly.integrity, assembly.activity, assembly.recipe.at, assembly.recipe.direction, recycle).unwrap();
                if assembly.complete() {
                    json.push_str("null");
                } else {
                    let part = assembly
                        .ingredients
                        .iter()
                        .zip(&assembly.recipe.parts)
                        .min_by(|(a, ar), (b, br)| {
                            (a.mass / ar.required_mass).total_cmp(&(b.mass / br.required_mass))
                        })
                        .map(|(_, part)| part.product.name.as_str())
                        .unwrap_or("");
                    write!(&mut json, "{part:?}").unwrap();
                }
                json.push_str(",\"components\":[");
                for (index, (ingredient, part)) in assembly
                    .ingredients
                    .iter()
                    .zip(&assembly.recipe.parts)
                    .enumerate()
                {
                    if index > 0 {
                        json.push(',');
                    }
                    write!(
                        &mut json,
                        "{{\"name\":{:?},\"role\":{:?},\"mass\":{},\"requiredMass\":{}}}",
                        part.product.name,
                        part.product.kind.name(),
                        ingredient.mass,
                        part.required_mass
                    )
                    .unwrap();
                }
                json.push_str("]}");
            }
        }
        json.push_str("],\"recipes\":[");
        if let Some(program) = program {
            for (index, recipe) in program.assemblies.iter().enumerate() {
                if index > 0 {
                    json.push(',');
                }
                let report = self.cellular.as_ref().and_then(|s| s.control_reports.iter().find(|r| r.name == recipe.name));
                write!(&mut json, "{{\"name\":{:?},\"kind\":{:?},\"requiredMass\":{},\"at\":{},\"direction\":{},\"startAllowed\":{},\"recycleRequested\":{},\"recycleRate\":{},\"components\":[",
                    recipe.name, recipe.kind.name(), recipe.parts.iter().map(|p| p.1).sum::<f64>(),
                    recipe.at, recipe.direction,
                    report.is_none_or(|r| r.start_allowed),
                    report.map_or(0.0, |r| r.recycle_requested),
                    report.map_or(0.0, |r| r.recycle_rate)).unwrap();
                for (index, &(product, amount)) in recipe.parts.iter().enumerate() {
                    if index > 0 {
                        json.push(',');
                    }
                    write!(
                        &mut json,
                        "{{\"name\":{:?},\"role\":{:?},\"requiredMass\":{}}}",
                        program.products[product].name,
                        program.products[product].kind.name(),
                        amount
                    )
                    .unwrap();
                }
                json.push_str("]}");
            }
        }
        let (mass, progress, genome, constriction) =
            self.cellular
                .as_ref()
                .map_or((0.0, 0.0, 0.0, 0.0), |state| {
                    (
                        state.replica_mass,
                        state.replication_progress(self.area),
                        state.genome_mass,
                        state.constriction,
                    )
                });
        write!(&mut json, "],\"replication\":{{\"mass\":{},\"requiredMass\":{},\"progress\":{},\"genomeMass\":{}}},\"constriction\":{{\"progress\":{}}}}}",
            mass, self.area * REPLICA_MASS_PER_AREA, progress, genome, constriction).unwrap();
        json
    }
}

#[derive(Clone, Copy)]
enum CellOperation {
    Synthesis(usize),
    Finish(usize, usize),
    Install(usize, usize, usize),
    Grow,
    Replicate,
    Constrict,
    RepairOrgan(usize),
    RepairAssembly(usize),
    RecycleAssembly(usize),
}
type CellDemand = MaterialDemand<CellOperation>;

/// Material planning scratch belongs to the tick workspace, not to a physical
/// cell or a checkpoint. Its buffers are reused even when genomes differ.
#[derive(Default)]
pub(crate) struct CellularWorkScratch {
    current_pools: Vec<Option<usize>>,
    stock: Vec<f64>,
    density: Vec<f64>,
    demands: Vec<CellDemand>,
    recycle_requests: Vec<f64>,
    measures: Vec<AssemblyMeasures>,
    allocation: AllocationScratch,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::BodyFixture;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }
    fn product(name: &str, kind: ProductKind) -> ProductDef {
        ProductDef {
            name: name.into(),
            kind,
            half_life: if kind == ProductKind::Regulator {
                1.0
            } else {
                0.0
            },
        }
    }
    fn flagellum_program() -> CellularProgram {
        CellularProgram::new(
            vec![
                product("Anchor", ProductKind::Anchor),
                product("Motor", ProductKind::Motor),
                product("Tail", ProductKind::Filament),
            ],
            vec![AssemblyDef {
                name: "Swim".into(),
                kind: AssemblyKind::Organelle(OrganelleKind::Flagellum),
                parts: vec![(0, 0.005), (1, 0.01), (2, 0.025)],
                at: 0.0,
                direction: 0.0,
            }],
        )
    }

    #[test]
    fn shared_declarations_detach_before_mutation_and_invalidate_bindings() {
        let mut program = flagellum_program();
        let old = program.clone();
        assert!(program.same_definition(&old));
        let mut body = stocked(&program, &[0.02,0.03,0.04]);
        add_complete(&mut body, &program, 0);
        let before = body.cellular_inputs(&program);
        let shared = body.cellular.as_ref().unwrap().bindings.0.borrow().program.clone().unwrap();
        assert!(shared.same_definition(&program));
        let installed = body.cellular.as_ref().unwrap().assemblies[0].recipe.clone();
        assert!(Rc::ptr_eq(&installed, &program.recipe(0)));
        let budget = program.derived_bytes();
        assert_eq!(program.derived_bytes(), budget);

        // Field-style editing must detach even if the caller never explicitly
        // cloned the handle: the binding cache itself retains the old version.
        drop(old);
        drop(shared);
        program.products[1].half_life = 8.0;
        assert_eq!(installed.parts[1].product.half_life, 0.0);
        let changed = body.cellular_inputs(&program);
        assert_eq!(changed.0[1], 0.0);
        assert_eq!(changed.1[0], 0.0);
        assert!(body.cellular.as_ref().unwrap().bindings.0.borrow().program.as_ref()
            .unwrap().same_definition(&program));
        assert!(!Rc::ptr_eq(&installed, &program.recipe(0)));

        program.products[1].half_life = 0.0;
        let restored = body.cellular_inputs(&program);
        assert_eq!(before, restored);
        let clone = program.clone();
        program.assemblies[0].direction = 0.75;
        assert_eq!(clone.assemblies[0].direction, 0.0);
        assert_eq!(installed.direction, 0.0);
        assert_eq!(body.cellular_inputs(&program).1[0], 0.0);
    }

    #[test]
    fn installed_recipes_share_declarations_without_sharing_material() {
        let program = flagellum_program();
        let budget = program.derived_bytes();
        let mut first = stocked(&program, &[0.02,0.03,0.04]);
        let mut second = first.clone();
        add_complete(&mut first, &program, 0);
        add_complete(&mut second, &program, 0);
        let a = &first.cellular.as_ref().unwrap().assemblies[0];
        let b = &second.cellular.as_ref().unwrap().assemblies[0];
        assert!(Rc::ptr_eq(&a.recipe, &b.recipe));
        assert_eq!(program.derived_bytes(), budget, "lazy cache has a fixed runtime budget");
        let before = b.ingredients[0].mass;
        first.cellular.as_mut().unwrap().assemblies[0].ingredients[0].mass *= 0.5;
        assert_eq!(second.cellular.as_ref().unwrap().assemblies[0].ingredients[0].mass, before);
        let mut writer = Writer::new();
        second.write_checkpoint(&mut writer);
        let saved = writer.0;
        let restored = Body::read_checkpoint(&mut Reader::new(&saved)).unwrap();
        let mut writer = Writer::new();
        restored.write_checkpoint(&mut writer);
        assert_eq!(writer.0, saved, "sharing never enters canonical body bytes");
    }
    fn controls(program: &CellularProgram, expression: f64) -> CellularControls {
        CellularControls {
            expression: vec![expression; program.products.len()],
            activity: vec![1.0; program.assemblies.len()],
            ..CellularControls::default()
        }
    }
    fn stocked(program: &CellularProgram, masses: &[f64]) -> Body {
        let (mut body, _) = Body::founder();
        body.cellular = Some(CellularState {
            pools: program
                .products
                .iter()
                .zip(masses)
                .map(|(product, &mass)| ProductPool {
                    product: product.clone(),
                    mass,
                    embodied_energy: mass * product.kind.synthesis_cost() * EMBODIED_FRACTION,
                    expression_requested: 0.0,
                    expression_rate: 0.0,
                })
                .collect(),
            ..CellularState::default()
        });
        body
    }
    fn complete(program: &CellularProgram, definition: usize, id: u32) -> AssemblyInstance {
        let recipe = program.recipe(definition);
        let ingredients = recipe
            .parts
            .iter()
            .map(|part| Ingredient {
                mass: part.required_mass,
                embodied_energy: part.required_mass
                    * (part.product.kind.synthesis_cost() + recipe.kind.installation_cost())
                    * EMBODIED_FRACTION,
            })
            .collect();
        AssemblyInstance {
            id,
            recipe,
            ingredients,
            integrity: 1.0,
            activity: 1.0,
        }
    }
    fn add_complete(body: &mut Body, program: &CellularProgram, definition: usize) {
        let state = body.cellular.get_or_insert_with(CellularState::default);
        state
            .assemblies
            .push(complete(program, definition, state.next_assembly_id));
        state.next_assembly_id += 1;
    }
    fn work(
        body: &mut Body,
        program: &CellularProgram,
        controls: &CellularControls,
        energy: &mut f64,
        minerals: &mut f64,
        dt: f64,
    ) -> WorkReport {
        body.cellular_actions(program, controls);
        let can_divide = body.area >= 2.0 && body.can_partition();
        let report = body.cellular_work(
            program, controls, energy, minerals, dt, MAX_AREA, can_divide,
        );
        body.validate().unwrap();
        report
    }

    fn adhesive_program() -> CellularProgram {
        let mut program = CellularProgram::new(vec![
            product("Frame", ProductKind::Scaffold), product("Finish", ProductKind::Adhesin),
            product("Mount", ProductKind::Anchor), product("Wall", ProductKind::Membrane),
        ], vec![AssemblyDef { name: "Glue".into(), kind: AssemblyKind::Mechanism(MechanismKind::Adhere),
            parts: vec![(1, 0.006), (2, 0.01), (3, 0.01)], at: 0.0, direction: 0.0 }]);
        program.precursors = vec![None, Some(0), None, None];
        program.validate().unwrap();
        program
    }

    #[test]
    fn finishing_consumes_only_prior_named_stock_and_transfers_its_exact_paid_investment() {
        let program = adhesive_program();
        let (founder, _) = Body::founder();
        assert!(founder.cellular_json(Some(&program)).contains("\"current\":true,\"precursor\":\"Frame\""));
        let mut body = stocked(&program, &[0.0, 0.0, 0.01, 0.01]);
        let mut request = controls(&program, 0.0);
        request.expression = vec![1.0, 1.0, 0.0, 0.0];
        request.start = vec![false];
        let (mut atp, mut minerals) = (100.0, 0.012);
        let initial = (body.structural_mass() + minerals, body.embodied_energy() + atp);
        let a = work(&mut body, &program, &request, &mut atp, &mut minerals, 1.0);
        let pools = &body.cellular.as_ref().unwrap().pools;
        close(pools[0].mass, 0.012);
        close(pools[1].mass, 0.0);
        close(a.energy_spent, 0.012 * 40.0);
        close(minerals, 0.0);
        request.expression[0] = 0.0;
        // Independent prior investment, e.g. a partially paid historical stock.
        body.cellular.as_mut().unwrap().pools[0].embodied_energy *= 0.5;
        let density = 40.0 * 0.9 * 0.5;
        let removed = 0.012 * density;
        let mut halted = body.clone();
        let (mut zero, mut none) = (0.0, 0.0);
        let blocked = work(&mut halted, &program, &request, &mut zero, &mut none, 1.0);
        close(blocked.synthesized_mass, 0.0);
        let mut short = body.clone();
        let (mut limited_atp, mut none) = (0.025, 0.0);
        work(&mut short, &program, &request, &mut limited_atp, &mut none, 1.0);
        close(short.cellular.as_ref().unwrap().pools[1].mass, 0.001);
        let mut no_worker = body.clone();
        no_worker.organs.clear();
        let (mut ready, mut none) = (10.0, 0.0);
        no_worker.cellular_work(&program, &request, &mut ready, &mut none, 1.0, MAX_AREA, false);
        close(no_worker.cellular.as_ref().unwrap().pools[1].mass, 0.0);
        request.start[0] = true;
        let b = work(&mut body, &program, &request, &mut atp, &mut minerals, 0.2);
        let state = body.cellular.as_ref().unwrap();
        let finished = state.pools[1].mass;
        assert!(finished > 0.0 && finished < 0.012);
        close(state.pools[0].mass + finished, 0.012);
        close(state.pools[1].embodied_energy, finished * (density + 25.0 * 0.9));
        close(b.energy_spent, finished * 25.0 + b.constructed_mass * 30.0);
        close(b.minerals_spent, 0.0);
        close(state.assemblies[0].ingredients[0].mass, 0.0);
        close(state.assemblies[0].functional_mass(), 0.0);
        let mut heat = a.dissipated_energy + b.dissipated_energy;
        for _ in 0..10 {
            let r = work(&mut body, &program, &request, &mut atp, &mut minerals, 1.0);
            heat += r.dissipated_energy;
        }
        assert!(body.cellular.as_ref().unwrap().assemblies[0].functional_mass() > 0.0);
        close(body.structural_mass() + minerals, initial.0);
        close(body.embodied_energy() + atp + heat + removed, initial.1);
        assert!(body.cellular_json(Some(&program)).contains("\"precursor\":\"Frame\""));
        let mut writer = Writer::new();
        body.write_checkpoint(&mut writer);
        assert_eq!(body, Body::read_checkpoint(&mut Reader::new(&writer.0)).unwrap());
        assert!(Body::read_checkpoint_version(&mut Reader::new(&writer.0), 15).is_err());
    }

    #[test]
    fn two_finishing_reactions_and_an_installer_share_one_named_stock() {
        let mut program = CellularProgram::new(vec![
            product("Blank", ProductKind::Scaffold), product("Other", ProductKind::Scaffold),
            product("A", ProductKind::Adhesin), product("B", ProductKind::Adhesin),
        ], vec![AssemblyDef { name: "Structure".into(), kind: AssemblyKind::Mechanism(MechanismKind::Support),
            parts: vec![(0, 0.04)], at: 0.0, direction: 0.0 }]);
        program.precursors = vec![None, None, Some(0), Some(0)];
        program.validate().unwrap();
        let mut body = stocked(&program, &[0.006, 0.05, 0.0, 0.0]);
        let (mut atp, mut minerals) = (20.0, 0.0);
        let mut request = controls(&program, 0.0);
        request.expression = vec![0.0, 0.0, 1.0, 1.0];
        let before = (body.structural_mass(), body.embodied_energy() + atp);
        let r = work(&mut body, &program, &request, &mut atp, &mut minerals, 2.0);
        let state = body.cellular.as_ref().unwrap();
        let (a, b, installed) = (state.pools[2].mass, state.pools[3].mass, state.assemblies[0].mass());
        assert!(a > 0.0 && b > 0.0 && installed > 0.0);
        close(a, b);
        close(a + b + installed, 0.006);
        close(r.energy_spent, 25.0 * (a + b) + 30.0 * installed);
        close(state.pools[0].mass, 0.0);
        close(state.pools[1].mass, 0.05);
        close(body.structural_mass(), before.0);
        close(body.embodied_energy() + atp + r.dissipated_energy, before.1);
        let idle = work(&mut body, &program, &request, &mut atp, &mut minerals, 1.0);
        close(idle.energy_spent, 0.0);
    }

    #[test]
    fn tiny_named_ingredient_must_arrive_before_a_paid_machine_can_operate() {
        let mut program = flagellum_program();
        program.products.push(product("Trace", ProductKind::Scaffold));
        program.assemblies[0].parts.push((3, 1e-13));
        program.validate().unwrap();
        let mut body = stocked(&program, &[0.005, 0.01, 0.025, 0.0]);
        let (mut atp, mut minerals) = (10.0, 1.0);
        let mut request = controls(&program, 0.0);
        work(&mut body, &program, &request, &mut atp, &mut minerals, 10.0);
        assert_eq!(body.cellular.as_ref().unwrap().assemblies[0].functional_mass(), 0.0);
        request.expression[3] = 1.0;
        work(&mut body, &program, &request, &mut atp, &mut minerals, 1.0);
        assert_eq!(body.cellular.as_ref().unwrap().assemblies[0].functional_mass(), 0.0);
        request.expression[3] = 0.0;
        work(&mut body, &program, &request, &mut atp, &mut minerals, 1.0);
        let machine = &body.cellular.as_ref().unwrap().assemblies[0];
        assert_eq!(machine.ingredients[3].mass, 1e-13);
        assert!(machine.functional_mass() > 0.0);
    }

    #[test]
    fn synthesis_is_paid_and_new_products_cannot_install_until_the_following_tick() {
        let program = flagellum_program();
        program.validate().unwrap();
        let (mut body, _) = Body::founder();
        let mut energy = 100.0;
        let mut minerals = 2.0;
        let initial_material = body.structural_mass() + minerals;
        let embodied_start = body.embodied_energy();
        let initial_energy = embodied_start + energy;
        let report = work(
            &mut body,
            &program,
            &controls(&program, 1.0),
            &mut energy,
            &mut minerals,
            5.0,
        );
        assert!(report.synthesized_mass > 0.0);
        close(report.constructed_mass, 0.0);
        assert!(body.cellular.as_ref().unwrap().assemblies.is_empty());
        close(body.capacities().thrust, 0.0);
        close(body.structural_mass() + minerals, initial_material);
        close(
            body.embodied_energy() + energy + report.dissipated_energy,
            initial_energy,
        );
        close(body.embodied_energy() - embodied_start, (100.0 - energy) * 0.90);
        let before_install = (body.embodied_energy(), energy);
        let outputs = body.cellular_actions(&program, &controls(&program, 0.0));
        close(outputs.force_x, 0.0);
        let second = work(
            &mut body,
            &program,
            &controls(&program, 0.0),
            &mut energy,
            &mut minerals,
            1.0,
        );
        close(body.embodied_energy() - before_install.0, (before_install.1 - energy) * 0.90);
        close(second.constructed_mass, 0.04);
        close(body.capacities().thrust, 4.0);
        close(body.structural_mass() + minerals, initial_material);
        close(
            body.embodied_energy() + energy + report.dissipated_energy + second.dissipated_energy,
            initial_energy,
        );
        let outputs = body.cellular_actions(&program, &controls(&program, 0.0));
        close(outputs.force_x, 4.0);
        close(outputs.torque, 0.0);
    }

    #[test]
    fn missing_ingredients_leave_paid_spatially_present_nonfunctional_assemblies() {
        let program = flagellum_program();
        let mut body = stocked(&program, &[0.005, 0.0, 0.025]);
        let mut energy = 100.0;
        let mut minerals = 1.0;
        let initial = body.structural_mass() + minerals;
        work(
            &mut body,
            &program,
            &controls(&program, 0.0),
            &mut energy,
            &mut minerals,
            10.0,
        );
        let state = body.cellular.as_ref().unwrap();
        assert_eq!(state.assemblies.len(), 1);
        close(state.assemblies[0].mass(), 0.03);
        assert!(!state.assemblies[0].complete());
        assert!(state.assemblies[0].internal_space() > 0.0);
        assert!(state.assemblies[0].surface_space() > 0.0);
        close(body.capacities().thrust, 0.0);
        close(body.building(OrganelleKind::Flagellum), 0.01);
        close(body.structural_mass() + minerals, initial);
        assert!(body
            .cellular_json(Some(&program))
            .contains("\"bottleneck\":\"Motor\""));
        let state = body.cellular.as_mut().unwrap();
        state.pools[1].mass = 0.01;
        state.pools[1].embodied_energy =
            0.01 * ProductKind::Motor.synthesis_cost() * EMBODIED_FRACTION;
        work(
            &mut body,
            &program,
            &controls(&program, 0.0),
            &mut energy,
            &mut minerals,
            1.0,
        );
        close(body.capacities().thrust, 4.0);
        close(body.building(OrganelleKind::Flagellum), 0.0);
    }

    #[test]
    fn duplicated_genes_change_competition_without_duplicating_translation_capacity() {
        let program = CellularProgram::new(
            vec![
                product("A", ProductKind::Scaffold),
                product("B", ProductKind::Scaffold),
            ],
            vec![],
        );
        let (mut body, _) = Body::founder();
        let mut energy = 100.0;
        let mut minerals = 1.0;
        let request = CellularControls {
            expression: vec![2.0, 1.0],
            activity: vec![],
            ..CellularControls::default()
        };
        let report = work(
            &mut body,
            &program,
            &request,
            &mut energy,
            &mut minerals,
            1.0,
        );
        let (pools, _) = body.cellular_inputs(&program);
        close(pools[0], 2.0 * pools[1]);
        close(report.synthesized_mass * TRANSLATION_WORK, 0.55 * 0.04);
        close(report.minerals_spent, report.synthesized_mass);

        let program = flagellum_program();
        let mut body = stocked(&program, &[0.005, 0.01, 0.025]);
        let report = work(
            &mut body,
            &program,
            &controls(&program, 1.0),
            &mut energy,
            &mut minerals,
            1.0,
        );
        assert!(report.constructed_mass > 0.0 && report.synthesized_mass > 0.0);
        close(
            report.synthesized_mass * TRANSLATION_WORK
                + report.constructed_mass * INSTALLATION_WORK,
            0.55 * 0.04,
        );
    }

    #[test]
    fn regulator_half_life_recovers_material_and_accounts_for_all_embodied_energy() {
        let program = CellularProgram::new(
            vec![product("Run", ProductKind::Regulator)],
            vec![],
        );
        let mut body = stocked(&program, &[0.2]);
        let mut energy = 20.0;
        let mut minerals = 0.0;
        let initial_material = body.structural_mass();
        let initial_energy = body.embodied_energy() + energy;
        let report = work(
            &mut body,
            &program,
            &controls(&program, 0.0),
            &mut energy,
            &mut minerals,
            1.0,
        );
        close(body.cellular_inputs(&program).0[0], 0.1);
        close(minerals, 0.1);
        close(body.structural_mass() + minerals, initial_material);
        close(
            body.embodied_energy() + energy + report.dissipated_energy,
            initial_energy,
        );
        close(report.energy_recovered, 0.0);
    }

    #[test]
    fn installed_motors_generate_vector_forces_and_cross_product_torque() {
        let mut program = flagellum_program();
        program.assemblies[0].at = std::f64::consts::FRAC_PI_2;
        let mut other = program.assemblies[0].clone();
        other.name = "Other".into();
        other.at = -std::f64::consts::FRAC_PI_2;
        program.assemblies.push(other);
        let (mut body, _) = Body::founder();
        add_complete(&mut body, &program, 0);
        add_complete(&mut body, &program, 1);
        body.validate().unwrap();
        let action = body.cellular_actions(&program, &controls(&program, 0.0));
        close(action.force_x, 8.0);
        close(action.force_y, 0.0);
        close(action.torque, 0.0);
        let mut request = controls(&program, 0.0);
        request.activity[1] = 0.0;
        let action = body.cellular_actions(&program, &request);
        close(action.force_x, 4.0);
        close(action.torque, -4.0 * body.radius());

        program.assemblies[1].direction = std::f64::consts::PI;
        body.cellular.as_mut().unwrap().assemblies[1] = complete(&program, 1, 2);
        let action = body.cellular_actions(&program, &controls(&program, 0.0));
        close(action.force_x, 0.0);
        close(action.force_y, 0.0);
        close(action.torque, -8.0 * body.radius());
        close(action.motor_cost, 8.0 * 0.06);
    }

    #[test]
    fn sensors_and_processing_obey_stored_activity_and_damage_never_erases_material() {
        let program = CellularProgram::new(
            vec![
                product("Anchor", ProductKind::Anchor),
                product("Sensor", ProductKind::Receptor),
                product("Pigment", ProductKind::Pigment),
                product("Frame", ProductKind::Scaffold),
                product("Enzyme", ProductKind::Enzyme),
            ],
            vec![
                AssemblyDef {
                    name: "Light".into(),
                    kind: AssemblyKind::Organelle(OrganelleKind::Photoreceptor),
                    parts: vec![(0, 0.002), (1, 0.006), (2, 0.002)],
                    at: 0.0,
                    direction: 0.0,
                },
                AssemblyDef {
                    name: "Process".into(),
                    kind: AssemblyKind::Organelle(OrganelleKind::Metabolic),
                    parts: vec![(3, 0.01), (4, 0.02)],
                    at: 0.0,
                    direction: 0.0,
                },
            ],
        );
        program.validate().unwrap();
        let (mut body, _) = Body::founder();
        add_complete(&mut body, &program, 0);
        add_complete(&mut body, &program, 1);
        let mut request = controls(&program, 0.0);
        request.activity.fill(0.0);
        body.cellular_actions(&program, &request);
        close(body.capacities().light_sensing, 0.0);
        close(body.capacities().metabolism, 0.8);
        request.activity.fill(0.5);
        body.cellular_actions(&program, &request);
        close(body.capacities().light_sensing, 0.005);
        close(body.capacities().metabolism, 1.7);
        let initial = (
            body.structural_mass(),
            body.embodied_energy(),
            body.space().used,
            body.surface().used,
        );
        body.damage(0.9);
        close(body.capacities().light_sensing, 0.0);
        close(body.structural_mass(), initial.0);
        close(body.embodied_energy(), initial.1);
        close(body.space().used, initial.2);
        close(body.surface().used, initial.3);
    }

    #[test]
    fn division_partitions_actual_recipes_stocks_and_copied_genome_material() {
        let program = flagellum_program();
        let (mut body, _) = Body::fixture(BodyFixture::Bare);
        body.cellular = stocked(&program, &[0.02, 0.03, 0.04]).cellular;
        add_complete(&mut body, &program, 0);
        add_complete(&mut body, &program, 0);
        let state = body.cellular.as_mut().unwrap();
        state.genome_mass = 0.01;
        state.genome_energy = 0.2;
        state.replica_mass = 0.01;
        state.replica_energy = 0.2;
        state.constriction = 1.0;
        body.validate().unwrap();
        assert!(body.can_partition());
        let (left, right) = body.partition().unwrap();
        left.validate().unwrap();
        right.validate().unwrap();
        close(
            left.structural_mass() + right.structural_mass(),
            body.structural_mass(),
        );
        close(
            left.embodied_energy() + right.embodied_energy(),
            body.embodied_energy(),
        );
        let mut actual: Vec<_> = left
            .cellular
            .as_ref()
            .unwrap()
            .assemblies
            .iter()
            .chain(&right.cellular.as_ref().unwrap().assemblies)
            .cloned()
            .collect();
        actual.sort_by_key(|a| a.id);
        assert_eq!(actual, body.cellular.as_ref().unwrap().assemblies);
        for daughter in [&left, &right] {
            let state = daughter.cellular.as_ref().unwrap();
            close(state.pools[1].mass, 0.015);
            close(state.genome_mass, 0.01);
            close(state.replica_mass, 0.0);
            close(state.constriction, 0.0);
        }
    }

    #[test]
    fn cellular_cores_support_whole_daughters_and_partial_cores_never_do() {
        let program = CellularProgram::new(
            vec![
                product("Frame", ProductKind::Scaffold),
                product("Enzyme", ProductKind::Enzyme),
            ],
            vec![AssemblyDef {
                name: "Core".into(),
                kind: AssemblyKind::Organelle(OrganelleKind::Core),
                parts: vec![(0, 0.03), (1, 0.03)],
                at: 0.0,
                direction: 0.0,
            }],
        );
        let (mut body, _) = Body::founder();
        body.area = 2.001;
        body.scaffold_energy *= body.area;
        add_complete(&mut body, &program, 0);
        assert!(!body.can_partition()); // one daughter would receive only .04 core
        add_complete(&mut body, &program, 0);
        assert!(body.can_partition());
        let (left, right) = body.partition().unwrap();
        assert!(left.viable() && right.viable());
        assert_eq!(
            left.count(OrganelleKind::Core) + right.count(OrganelleKind::Core),
            3
        );
        body.cellular.as_mut().unwrap().assemblies[1].ingredients[1].mass = 0.0;
        body.cellular.as_mut().unwrap().assemblies[1].ingredients[1].embodied_energy = 0.0;
        assert!(!body.can_partition());
    }

    #[test]
    fn membrane_replication_and_constriction_are_paid_distinct_delayed_steps() {
        let program = CellularProgram::new(
            vec![
                product("Skin", ProductKind::Membrane),
                product("Enzyme", ProductKind::Enzyme),
                product("Frame", ProductKind::Scaffold),
                product("Copy", ProductKind::Replicase),
                product("Anchor", ProductKind::Anchor),
                product("Motor", ProductKind::Motor),
                product("Ring", ProductKind::Constrictor),
            ],
            vec![
                AssemblyDef {
                    name: "Growth".into(),
                    kind: AssemblyKind::Membrane,
                    parts: vec![(0, 0.01), (1, 0.01)],
                    at: 0.0,
                    direction: 0.0,
                },
                AssemblyDef {
                    name: "Replication".into(),
                    kind: AssemblyKind::Replicator,
                    parts: vec![(2, 0.01), (3, 0.01)],
                    at: 0.0,
                    direction: 0.0,
                },
                AssemblyDef {
                    name: "Division".into(),
                    kind: AssemblyKind::Constrictor,
                    parts: vec![(4, 0.005), (5, 0.005), (6, 0.01)],
                    at: 0.0,
                    direction: 0.0,
                },
            ],
        );
        program.validate().unwrap();
        let (mut body, _) = Body::fixture(BodyFixture::Bare);
        for index in 0..3 {
            add_complete(&mut body, &program, index);
        }
        let mut energy = 200.0;
        let mut minerals = 3.0;
        let material = minerals + body.structural_mass();
        let initial_energy = energy + body.embodied_energy();
        let mut heat = 0.0;
        let request = controls(&program, 0.0);
        let first = work(
            &mut body,
            &program,
            &request,
            &mut energy,
            &mut minerals,
            1.0,
        );
        heat += first.dissipated_energy;
        assert!(body.area > 1.0);
        assert!(body.cellular.as_ref().unwrap().replica_mass > 0.0);
        close(body.cellular.as_ref().unwrap().constriction, 0.0);
        assert!(!body.cellular_actions(&program, &request).divide);
        for _ in 0..80 {
            heat += work(
                &mut body,
                &program,
                &request,
                &mut energy,
                &mut minerals,
                1.0,
            )
            .dissipated_energy;
            close(minerals + body.structural_mass(), material);
            close(energy + body.embodied_energy() + heat, initial_energy);
            if body.cellular_actions(&program, &request).divide {
                break;
            }
        }
        close(body.area, 2.0);
        close(
            body.cellular
                .as_ref()
                .unwrap()
                .replication_progress(body.area),
            1.0,
        );
        close(body.cellular.as_ref().unwrap().constriction, 1.0);
        assert!(body.cellular_actions(&program, &request).divide);
        assert!(body.partition().is_some());
    }

    #[test]
    fn genome_changes_do_not_retype_old_products_or_reinterpret_existing_machines() {
        let original = flagellum_program();
        let mut body = stocked(&original, &[0.02, 0.03, 0.04]);
        add_complete(&mut body, &original, 0);
        let old = body.cellular.as_ref().unwrap().assemblies[0].clone();
        let replacement = CellularProgram::new(
            vec![product("Motor", ProductKind::Scaffold)],
            vec![AssemblyDef {
                name: "Swim".into(),
                kind: AssemblyKind::Organelle(OrganelleKind::Cortex),
                parts: vec![(0, 0.02)],
                at: 0.0,
                direction: 0.0,
            }],
        );
        replacement.validate().unwrap();
        let mut energy = 100.0;
        let mut minerals = 2.0;
        let initial_material = minerals + body.structural_mass();
        work(
            &mut body,
            &replacement,
            &controls(&replacement, 1.0),
            &mut energy,
            &mut minerals,
            1.0,
        );
        let state = body.cellular.as_ref().unwrap();
        assert_eq!(state.assemblies[0].recipe, old.recipe);
        assert_eq!(state.assemblies[0].ingredients, old.ingredients);
        assert!(state
            .pools
            .iter()
            .any(|p| p.product.name == "Motor" && p.product.kind == ProductKind::Motor));
        assert!(state
            .pools
            .iter()
            .any(|p| p.product.name == "Motor" && p.product.kind == ProductKind::Scaffold));
        close(
            body.cellular_actions(&replacement, &controls(&replacement, 0.0))
                .force_x,
            0.0,
        );
        close(body.capacities().thrust, 4.0); // retained physical flagellum
        close(minerals + body.structural_mass(), initial_material);
    }

    #[test]
    fn checkpoint_preserves_recipe_identity_partial_stocks_and_exact_continuation() {
        let program = flagellum_program();
        let mut body = stocked(&program, &[0.005, 0.0, 0.025]);
        let mut energy = 100.0;
        let mut minerals = 1.0;
        let request = controls(&program, 1.0);
        work(
            &mut body,
            &program,
            &request,
            &mut energy,
            &mut minerals,
            0.25,
        );
        let mut writer = Writer::new();
        body.write_checkpoint(&mut writer);
        let mut reader = Reader::new(&writer.0);
        let mut restored = Body::read_checkpoint(&mut reader).unwrap();
        assert!(reader.finished());
        assert_eq!(restored, body);
        let mut restored_energy = energy;
        let mut restored_minerals = minerals;
        for _ in 0..40 {
            work(
                &mut body,
                &program,
                &request,
                &mut energy,
                &mut minerals,
                0.1,
            );
            work(
                &mut restored,
                &program,
                &request,
                &mut restored_energy,
                &mut restored_minerals,
                0.1,
            );
        }
        assert_eq!(body, restored);
        assert_eq!(energy, restored_energy);
        assert_eq!(minerals, restored_minerals);
        restored.cellular.as_mut().unwrap().pools[0].embodied_energy = 1000.0;
        let mut writer = Writer::new();
        restored.write_checkpoint(&mut writer);
        assert!(Body::read_checkpoint(&mut Reader::new(&writer.0)).is_err());
    }
    include!("cellular_control_tests.rs");

}
