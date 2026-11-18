//! A bounded typed signal graph with explicit synchronous, per-cell registers.
//! Expressions are pure: every read sees old memory; all writes commit together.
use crate::body::{BodyControls, FAMILY_COUNT, MIN_ORGANELLE_MASS};
use crate::cellular::{
    AssemblyDef, AssemblyKind, CellularControls, CellularProgram, ProductDef, ProductKind,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

mod cellular;
mod checkpoint;
#[cfg(test)]
mod control_tests;
mod execution;
mod functions;
mod source;
use execution::ExecutionPlan;

pub const MAX_SOURCE_BYTES: usize = 256 * 1024;
const MAX_TOKENS: usize = 65_536;
const MAX_NODES: usize = 8192;
const MAX_DEPTH: usize = 64;
const MAX_SIGNAL: f64 = 1.0e12;
pub const REGISTER_COUNT: usize = 999;
pub const REGISTER_ENERGY_COST: f64 = 0.0025;
const MAX_STRING_BYTES: usize = 128;

pub const DEFAULT_PROGRAM: &str = r#"# Every newborn has one core. This program must express its own energy machinery.
# Protein stocks are bounded by feedback, not by a free body recipe.
let light_path = organelle.photosynthetic.capacity >= 1;
let processing = organelle.metabolic.capacity >= 1;
let supplied = organelle.transporter.capacity >= 1;
let powered = light_path && processing && supplied;
let funded = energy > 18 && nutrient.stored > 0.002;
let core_ready = organelle.core.count >= 2 && organelle.core.building < 0.000001;
thrust = 0;
turn = 0;
photosynthesize = 1;
absorb = 1;
feed = 0;
repair = if integrity < 0.98 && energy > 40 then 0.2 else 0;
grow = if core_ready && body.maturity < 1 && energy > 90 then 0.8 else 0;
divide = body.division_ready && energy > 120
         && organelle.photosynthetic.count >= 2 && organelle.photosynthetic.building < 0.000001
         && organelle.metabolic.count >= 2 && organelle.metabolic.building < 0.000001
         && organelle.transporter.count >= 2 && organelle.transporter.building < 0.000001;
target.photosynthetic.count = 2;
target.photosynthetic.size = 0.10;
express.photosynthetic = if funded && (!light_path || powered && core_ready)
    && protein.photosynthetic < min(0.012, max(0, 0.2 - organelle.photosynthetic.mass)) then 1 else 0;
target.metabolic.count = 2;
target.metabolic.size = 0.03;
express.metabolic = if funded && light_path && (!processing || powered && core_ready)
    && protein.metabolic < min(0.012, max(0, 0.06 - organelle.metabolic.mass)) then 1 else 0;
target.transporter.count = 2;
target.transporter.size = 0.02;
express.transporter = if funded && light_path && processing && (!supplied || core_ready)
    && protein.transporter < min(0.012, max(0, 0.04 - organelle.transporter.mass)) then 1 else 0;
target.core.count = 2;
target.core.size = max(0.04, body.birth_area * 0.04);
express.core = if powered && energy > 50 && nutrient.stored > 0.01
    && protein.core < min(0.012, max(0, max(0.08, body.birth_area * 0.08) - organelle.core.mass)) then 1 else 0;
"#;

/// Owned, physically built machinery, sampled before this tick's construction.
#[derive(Debug, Clone, Copy, Default)]
pub struct OrganelleInput {
    pub count: i32,
    pub mass: f64,
    pub capacity: f64,
    pub damaged: f64,
    pub building: f64,
}

/// One named receptor's frozen reading. Multiple physical instances of the
/// recipe are reduced by the observation phase; programs never receive IDs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ReceptorInput {
    pub available: bool,
    pub valid: bool,
    pub level: f64,
    pub value: f64,
    pub mineral: f64,
    pub organic: f64,
    pub offset: [f64; 2],
    pub pressure: f64,
    pub resistance: f64,
    pub flow: [f64; 2],
    pub speed: f64,
    pub bearing: f64,
    pub area: f64,
    pub contrast: f64,
    pub pigment: f64,
    pub motion: [f64; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ReceptorField {
    Available,
    Valid,
    Level,
    Value,
    Mineral,
    Organic,
    Offset,
    Pressure,
    Resistance,
    Flow,
    Speed,
    Bearing,
    Area,
    Contrast,
    Pigment,
    Motion,
}
impl ReceptorField {
    pub const ALL: [Self; 16] = [
        Self::Available,
        Self::Valid,
        Self::Level,
        Self::Value,
        Self::Mineral,
        Self::Organic,
        Self::Offset,
        Self::Pressure,
        Self::Resistance,
        Self::Flow,
        Self::Speed,
        Self::Bearing,
        Self::Area,
        Self::Contrast,
        Self::Pigment,
        Self::Motion,
    ];
    pub fn name(self) -> &'static str {
        [
            "available",
            "valid",
            "level",
            "value",
            "mineral",
            "organic",
            "offset",
            "pressure",
            "resistance",
            "flow",
            "speed",
            "bearing",
            "area",
            "contrast",
            "pigment",
            "motion",
        ][self as usize]
    }
    pub fn for_kind(kind: AssemblyKind) -> &'static [Self] {
        use crate::body::OrganelleKind as O;
        use crate::cellular::MechanismKind as M;
        use ReceptorField::*;
        match kind {
            AssemblyKind::Mechanism(M::SenseLight) | AssemblyKind::Organelle(O::Photoreceptor) => {
                &[Available, Valid, Level]
            }
            AssemblyKind::Mechanism(M::SenseHeat) | AssemblyKind::Organelle(O::Thermoreceptor) => {
                &[Available, Valid, Value]
            }
            AssemblyKind::Mechanism(M::SenseChemical)
            | AssemblyKind::Organelle(O::Chemoreceptor) => &[Available, Valid, Mineral, Organic],
            AssemblyKind::Mechanism(M::SenseContact)
            | AssemblyKind::Organelle(O::Mechanoreceptor) => &[
                Available, Valid, Offset, Pressure, Resistance, Flow, Speed, Bearing,
            ],
            AssemblyKind::Mechanism(M::SenseOptical) | AssemblyKind::Organelle(O::Optical) => {
                &[Available, Valid, Offset, Area, Contrast, Pigment, Motion]
            }
            _ => &[],
        }
    }
    fn lookup(name: &str) -> Option<Self> {
        if name == "light" {
            return Some(Self::Level);
        }
        Self::ALL.into_iter().find(|field| field.name() == name)
    }
    fn ty(self) -> Type {
        match self {
            Self::Available | Self::Valid => Type::Bool,
            Self::Offset | Self::Flow | Self::Motion => Type::Vector,
            _ => Type::Number,
        }
    }
    fn read(self, reading: Option<&ReceptorInput>) -> Value {
        let Some(input) = reading else {
            return Value::zero(self.ty());
        };
        if self == Self::Available {
            return Value::Boolean(input.available);
        }
        if self == Self::Valid {
            return Value::Boolean(input.available && input.valid);
        }
        if !input.available
            || (!input.valid && !matches!(self, Self::Flow | Self::Speed | Self::Bearing))
        {
            return Value::zero(self.ty());
        }
        match self {
            Self::Offset => Value::Vector(input.offset.map(finite)),
            Self::Flow => Value::Vector(input.flow.map(finite)),
            Self::Motion => Value::Vector(input.motion.map(finite)),
            _ => Value::Scalar(finite(match self {
                Self::Level => input.level,
                Self::Value => input.value,
                Self::Mineral => input.mineral,
                Self::Organic => input.organic,
                Self::Pressure => input.pressure,
                Self::Resistance => input.resistance,
                Self::Speed => input.speed,
                Self::Bearing => input.bearing,
                Self::Area => input.area,
                Self::Contrast => input.contrast,
                Self::Pigment => input.pigment,
                _ => unreachable!(),
            })),
        }
    }
}

/// External measurements are populated by the hardware-gated sensor pipeline.
/// Absent or invalid measurements carry zero values and empty opaque tokens.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub energy: f64,
    pub energy_fraction: f64,
    pub energy_capacity: f64,
    pub age: f64,
    pub time: f64,
    pub speed: f64,
    pub id: f64,
    pub integrity: f64,
    pub memory_used: f64,
    pub memory_cost: f64,
    pub body_area: f64,
    pub body_mass: f64,
    pub body_birth_area: f64,
    pub body_maturity: f64,
    pub body_division_ready: bool,
    pub body_space_free: f64,
    pub body_space_used: f64,
    pub body_space_capacity: f64,
    pub body_surface_free: f64,
    pub body_surface_used: f64,
    pub body_surface_capacity: f64,
    pub nutrient_stored: f64,
    pub nutrient_capacity: f64,
    pub nutrient_fraction: f64,
    pub gut_energy: f64,
    pub gut_capacity: f64,
    pub gut_fraction: f64,
    pub gut_exudate_energy: f64,
    pub gut_exudate_nutrients: f64,
    pub gut_tissue_energy: f64,
    pub gut_tissue_nutrients: f64,
    pub gut_waste_energy: f64,
    pub gut_waste_nutrients: f64,
    pub expression_rate: f64,
    pub assembly_rate: f64,
    pub assembly_capacity: f64,
    pub protein: [f64; FAMILY_COUNT],
    pub digestion_rate: f64,
    pub growth_rate: f64,
    pub lifespan: f64,
    pub exudation_rate: f64,
    pub light_available: bool,
    pub light_valid: bool,
    pub light_level: f64,
    pub light_bearing: f64,
    pub heat_available: bool,
    pub heat_valid: bool,
    pub heat_value: f64,
    pub heat_bearing: f64,
    pub chemical_available: bool,
    pub chemical_valid: bool,
    pub chemical_mineral: f64,
    pub chemical_organic: f64,
    pub chemical_mineral_bearing: f64,
    pub chemical_organic_bearing: f64,
    pub flow_available: bool,
    pub flow_valid: bool,
    pub flow_vector: [f64; 2],
    pub flow_speed: f64,
    pub flow_bearing: f64,
    pub contact_available: bool,
    pub contact_valid: bool,
    pub contact_token: Rc<str>,
    pub contact_offset: [f64; 2],
    pub contact_pressure: f64,
    pub contact_resistance: f64,
    pub object_available: bool,
    pub object_valid: bool,
    pub object_token: Rc<str>,
    pub object_offset: [f64; 2],
    pub object_area: f64,
    pub object_contrast: f64,
    pub object_pigment: f64,
    pub object_motion: [f64; 2],
    pub organelle: [OrganelleInput; FAMILY_COUNT],
    /// Free product stocks and functional installed recipe masses, sampled before the tick.
    pub products: Vec<f64>,
    pub assemblies: Vec<f64>,
    pub receptors: Vec<ReceptorInput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outputs {
    pub thrust: f64,
    pub turn: f64,
    pub grow: f64,
    pub divide: bool,
    pub photosynthesize: f64,
    pub engulf: bool,
    pub bite: f64,
    pub defend: f64,
    pub repair: f64,
    pub absorb: f64,
    pub feed: f64,
    /// Relative digestive enzyme efforts for exudate, tissue, and waste.
    pub digest: [f64; 3],
    pub body: BodyControls,
    /// Body-relative radians. Applied to the next tick's optical sampling.
    pub scan_bearing: f64,
    /// Full angular width, clamped to 0..TAU. Default is an unrestricted scan.
    pub scan_width: f64,
    /// Opaque observation token; it never grants remote visibility or action.
    pub scan_target: Rc<str>,
    pub cellular: Option<CellularControls>,
}

impl Default for Outputs {
    fn default() -> Self {
        Self {
            thrust: 0.0,
            turn: 0.0,
            grow: 0.0,
            divide: false,
            photosynthesize: 0.0,
            engulf: false,
            bite: 0.0,
            defend: 0.0,
            repair: 0.0,
            absorb: 1.0,
            feed: 1.0,
            digest: [1.0, 1.0, 0.35],
            body: BodyControls::default(),
            scan_bearing: 0.0,
            scan_width: std::f64::consts::TAU,
            scan_target: Rc::from(""),
            cellular: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Type {
    Integer,
    Number,
    Bool,
    String,
    Vector,
}
impl Type {
    fn numeric(self) -> bool {
        matches!(self, Self::Integer | Self::Number)
    }
    fn description(self) -> &'static str {
        match self {
            Self::Integer => "an integer",
            Self::Number => "a scalar",
            Self::Bool => "a boolean",
            Self::String => "a string",
            Self::Vector => "a vector",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::Integer => "ri",
            Self::Number => "rf",
            Self::Bool => "rb",
            Self::String => "rs",
            Self::Vector => "rv",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Number => "scalar",
            Self::Bool => "boolean",
            Self::String => "string",
            Self::Vector => "vec2",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i32),
    Scalar(f64),
    Boolean(bool),
    Text(Rc<str>),
    Vector([f64; 2]),
}
impl Value {
    fn ty(&self) -> Type {
        match self {
            Self::Integer(_) => Type::Integer,
            Self::Scalar(_) => Type::Number,
            Self::Boolean(_) => Type::Bool,
            Self::Text(_) => Type::String,
            Self::Vector(_) => Type::Vector,
        }
    }
    fn zero(ty: Type) -> Self {
        match ty {
            Type::Integer => Self::Integer(0),
            Type::Number => Self::Scalar(0.0),
            Type::Bool => Self::Boolean(false),
            Type::String => Self::Text(Rc::from("")),
            Type::Vector => Self::Vector([0.0; 2]),
        }
    }
    fn number(&self) -> f64 {
        match self {
            Self::Integer(x) => *x as f64,
            Self::Scalar(x) => *x,
            _ => 0.0,
        }
    }
    fn boolean(&self) -> bool {
        matches!(self, Self::Boolean(true))
    }
    fn vector(&self) -> [f64; 2] {
        match self {
            Self::Vector(v) => *v,
            _ => [0.0; 2],
        }
    }
    fn convert(self, ty: Type) -> Self {
        match (ty, self) {
            // Every i32 is exactly representable as f64 and is within MAX_SIGNAL.
            // Keep typed integer nodes on the integer path instead of round-tripping
            // through floating-point clamping and truncation on every evaluation.
            (Type::Integer, value @ Self::Integer(_)) => value,
            (Type::Number, Self::Integer(value)) => Self::Scalar(value as f64),
            (Type::Number, Self::Scalar(value)) => Self::Scalar(finite(value)),
            (Type::Integer, Self::Scalar(value)) => Self::Integer(
                finite(value)
                    .clamp(i32::MIN as f64, i32::MAX as f64)
                    .trunc() as i32,
            ),
            (Type::Vector, Self::Vector(v)) => Self::Vector([finite(v[0]), finite(v[1])]),
            (Type::Vector, _) => Self::Vector([0.0; 2]),
            (_, value) => value,
        }
    }
    fn source(&self) -> String {
        match self {
            Self::Integer(n) => n.to_string(),
            Self::Scalar(n) => scalar_literal(*n),
            Self::Boolean(b) => b.to_string(),
            Self::Text(s) => quote(s),
            Self::Vector(v) => format!("vec({}, {})", scalar_literal(v[0]), scalar_literal(v[1])),
        }
    }
    fn json(&self) -> String {
        match self {
            Self::Text(s) => quote(s),
            Self::Vector(v) => format!("[{},{}]", v[0], v[1]),
            _ => match self {
                Self::Scalar(n) => n.to_string(),
                _ => self.source(),
            },
        }
    }
}
fn finite(value: f64) -> f64 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(-MAX_SIGNAL, MAX_SIGNAL)
    }
}
fn scalar_literal(value: f64) -> String {
    let regular = value.to_string();
    let scientific = format!("{value:e}");
    let mut result = if scientific.len() < regular.len() {
        scientific
    } else {
        regular
    };
    if !result.contains('.') && !result.contains('e') {
        result.push_str(".0");
    }
    result
}
fn quote(text: &str) -> String {
    let mut result = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}
fn bounded_text(mut text: String) -> Rc<str> {
    let mut end = text.len().min(MAX_STRING_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    Rc::from(text)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Register {
    ty: Type,
    index: u16,
}
impl Register {
    fn parse(name: &str) -> Option<Self> {
        if name.len() != 5 {
            return None;
        }
        let ty = match name.get(..2)? {
            "ri" => Type::Integer,
            "rf" => Type::Number,
            "rb" => Type::Bool,
            "rs" => Type::String,
            "rv" => Type::Vector,
            _ => return None,
        };
        if !name.as_bytes()[2..].iter().all(u8::is_ascii_digit) {
            return None;
        }
        let index = name[2..].parse::<u16>().ok()?;
        (index > 0 && index <= REGISTER_COUNT as u16).then_some(Self { ty, index })
    }
    fn reserved(name: &str) -> bool {
        ["ri", "rf", "rb", "rs", "rv"]
            .iter()
            .any(|p| name.starts_with(p) && name.as_bytes().get(2).is_some_and(u8::is_ascii_digit))
    }
    fn name(self) -> String {
        format!("{}{:03}", self.ty.prefix(), self.index)
    }
}

/// Sparse occupancy: storing zero, false, or an empty string is still populated.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Registers {
    entries: BTreeMap<Register, Value>,
}
impl Registers {
    pub fn populated_count(&self) -> usize {
        self.entries.len()
    }
    pub fn to_json(&self) -> String {
        format!(
            "[{}]",
            self.entries
                .iter()
                .map(|(key, value)| format!(
                    "{{\"name\":{},\"type\":{},\"value\":{}}}",
                    quote(&key.name()),
                    quote(key.ty.label()),
                    value.json()
                ))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
    pub fn value(&self, name: &str) -> Option<&Value> {
        self.entries.get(&Register::parse(name)?)
    }
    fn read(&self, key: Register) -> Value {
        self.entries
            .get(&key)
            .cloned()
            .unwrap_or_else(|| Value::zero(key.ty))
    }
}
#[derive(Debug, Default)]
pub struct EvalScratch {
    values: Vec<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sensor {
    Energy,
    EnergyFraction,
    EnergyCapacity,
    Age,
    Time,
    Speed,
    Id,
    Integrity,
    MemoryUsed,
    MemoryCost,
    BodyArea,
    BodyMass,
    BodyBirthArea,
    BodyMaturity,
    BodyDivisionReady,
    BodySpaceFree,
    BodySpaceUsed,
    BodySpaceCapacity,
    BodySurfaceFree,
    BodySurfaceUsed,
    BodySurfaceCapacity,
    NutrientStored,
    NutrientCapacity,
    NutrientFraction,
    GutEnergy,
    GutCapacity,
    GutFraction,
    DigestionRate,
    GrowthRate,
    Lifespan,
    ExudationRate,
    OrganelleCoreCount,
    OrganelleCoreMass,
    OrganelleCoreCapacity,
    OrganelleCoreDamaged,
    OrganelleCoreBuilding,
    OrganellePhotosyntheticCount,
    OrganellePhotosyntheticMass,
    OrganellePhotosyntheticCapacity,
    OrganellePhotosyntheticDamaged,
    OrganellePhotosyntheticBuilding,
    OrganelleMetabolicCount,
    OrganelleMetabolicMass,
    OrganelleMetabolicCapacity,
    OrganelleMetabolicDamaged,
    OrganelleMetabolicBuilding,
    OrganelleDigestiveCount,
    OrganelleDigestiveMass,
    OrganelleDigestiveCapacity,
    OrganelleDigestiveDamaged,
    OrganelleDigestiveBuilding,
    OrganelleReserveCount,
    OrganelleReserveMass,
    OrganelleReserveCapacity,
    OrganelleReserveDamaged,
    OrganelleReserveBuilding,
    OrganelleTransporterCount,
    OrganelleTransporterMass,
    OrganelleTransporterCapacity,
    OrganelleTransporterDamaged,
    OrganelleTransporterBuilding,
    OrganellePhagocyteCount,
    OrganellePhagocyteMass,
    OrganellePhagocyteCapacity,
    OrganellePhagocyteDamaged,
    OrganellePhagocyteBuilding,
    OrganelleFlagellumCount,
    OrganelleFlagellumMass,
    OrganelleFlagellumCapacity,
    OrganelleFlagellumDamaged,
    OrganelleFlagellumBuilding,
    OrganellePiercerCount,
    OrganellePiercerMass,
    OrganellePiercerCapacity,
    OrganellePiercerDamaged,
    OrganellePiercerBuilding,
    OrganelleCortexCount,
    OrganelleCortexMass,
    OrganelleCortexCapacity,
    OrganelleCortexDamaged,
    OrganelleCortexBuilding,
    OrganelleRepairCount,
    OrganelleRepairMass,
    OrganelleRepairCapacity,
    OrganelleRepairDamaged,
    OrganelleRepairBuilding,
    OrganellePhotoreceptorCount,
    OrganellePhotoreceptorMass,
    OrganellePhotoreceptorCapacity,
    OrganellePhotoreceptorDamaged,
    OrganellePhotoreceptorBuilding,
    OrganelleThermoreceptorCount,
    OrganelleThermoreceptorMass,
    OrganelleThermoreceptorCapacity,
    OrganelleThermoreceptorDamaged,
    OrganelleThermoreceptorBuilding,
    OrganelleChemoreceptorCount,
    OrganelleChemoreceptorMass,
    OrganelleChemoreceptorCapacity,
    OrganelleChemoreceptorDamaged,
    OrganelleChemoreceptorBuilding,
    OrganelleMechanoreceptorCount,
    OrganelleMechanoreceptorMass,
    OrganelleMechanoreceptorCapacity,
    OrganelleMechanoreceptorDamaged,
    OrganelleMechanoreceptorBuilding,
    OrganelleOpticalCount,
    OrganelleOpticalMass,
    OrganelleOpticalCapacity,
    OrganelleOpticalDamaged,
    OrganelleOpticalBuilding,
    LightAvailable,
    LightValid,
    LightLevel,
    LightBearing,
    HeatAvailable,
    HeatValid,
    HeatValue,
    HeatBearing,
    ChemicalAvailable,
    ChemicalValid,
    ChemicalMineral,
    ChemicalOrganic,
    ChemicalMineralBearing,
    ChemicalOrganicBearing,
    FlowAvailable,
    FlowValid,
    FlowVector,
    FlowSpeed,
    FlowBearing,
    ContactAvailable,
    ContactValid,
    ContactToken,
    ContactOffset,
    ContactPressure,
    ContactResistance,
    ObjectAvailable,
    ObjectValid,
    ObjectToken,
    ObjectOffset,
    ObjectArea,
    ObjectContrast,
    ObjectPigment,
    ObjectMotion,
    GutExudateEnergy,
    GutExudateNutrients,
    GutTissueEnergy,
    GutTissueNutrients,
    GutWasteEnergy,
    GutWasteNutrients,
    ExpressionRate,
    AssemblyRate,
    AssemblyCapacity,
    ProteinCore,
    ProteinPhotosynthetic,
    ProteinMetabolic,
    ProteinDigestive,
    ProteinReserve,
    ProteinTransporter,
    ProteinPhagocyte,
    ProteinFlagellum,
    ProteinPiercer,
    ProteinCortex,
    ProteinRepair,
    ProteinPhotoreceptor,
    ProteinThermoreceptor,
    ProteinChemoreceptor,
    ProteinMechanoreceptor,
    ProteinOptical,
}
impl Sensor {
    const ALL: [Self; 169] = [
        Self::Energy,
        Self::EnergyFraction,
        Self::EnergyCapacity,
        Self::Age,
        Self::Time,
        Self::Speed,
        Self::Id,
        Self::Integrity,
        Self::MemoryUsed,
        Self::MemoryCost,
        Self::BodyArea,
        Self::BodyMass,
        Self::BodyBirthArea,
        Self::BodyMaturity,
        Self::BodyDivisionReady,
        Self::BodySpaceFree,
        Self::BodySpaceUsed,
        Self::BodySpaceCapacity,
        Self::BodySurfaceFree,
        Self::BodySurfaceUsed,
        Self::BodySurfaceCapacity,
        Self::NutrientStored,
        Self::NutrientCapacity,
        Self::NutrientFraction,
        Self::GutEnergy,
        Self::GutCapacity,
        Self::GutFraction,
        Self::DigestionRate,
        Self::GrowthRate,
        Self::Lifespan,
        Self::ExudationRate,
        Self::OrganelleCoreCount,
        Self::OrganelleCoreMass,
        Self::OrganelleCoreCapacity,
        Self::OrganelleCoreDamaged,
        Self::OrganelleCoreBuilding,
        Self::OrganellePhotosyntheticCount,
        Self::OrganellePhotosyntheticMass,
        Self::OrganellePhotosyntheticCapacity,
        Self::OrganellePhotosyntheticDamaged,
        Self::OrganellePhotosyntheticBuilding,
        Self::OrganelleMetabolicCount,
        Self::OrganelleMetabolicMass,
        Self::OrganelleMetabolicCapacity,
        Self::OrganelleMetabolicDamaged,
        Self::OrganelleMetabolicBuilding,
        Self::OrganelleDigestiveCount,
        Self::OrganelleDigestiveMass,
        Self::OrganelleDigestiveCapacity,
        Self::OrganelleDigestiveDamaged,
        Self::OrganelleDigestiveBuilding,
        Self::OrganelleReserveCount,
        Self::OrganelleReserveMass,
        Self::OrganelleReserveCapacity,
        Self::OrganelleReserveDamaged,
        Self::OrganelleReserveBuilding,
        Self::OrganelleTransporterCount,
        Self::OrganelleTransporterMass,
        Self::OrganelleTransporterCapacity,
        Self::OrganelleTransporterDamaged,
        Self::OrganelleTransporterBuilding,
        Self::OrganellePhagocyteCount,
        Self::OrganellePhagocyteMass,
        Self::OrganellePhagocyteCapacity,
        Self::OrganellePhagocyteDamaged,
        Self::OrganellePhagocyteBuilding,
        Self::OrganelleFlagellumCount,
        Self::OrganelleFlagellumMass,
        Self::OrganelleFlagellumCapacity,
        Self::OrganelleFlagellumDamaged,
        Self::OrganelleFlagellumBuilding,
        Self::OrganellePiercerCount,
        Self::OrganellePiercerMass,
        Self::OrganellePiercerCapacity,
        Self::OrganellePiercerDamaged,
        Self::OrganellePiercerBuilding,
        Self::OrganelleCortexCount,
        Self::OrganelleCortexMass,
        Self::OrganelleCortexCapacity,
        Self::OrganelleCortexDamaged,
        Self::OrganelleCortexBuilding,
        Self::OrganelleRepairCount,
        Self::OrganelleRepairMass,
        Self::OrganelleRepairCapacity,
        Self::OrganelleRepairDamaged,
        Self::OrganelleRepairBuilding,
        Self::OrganellePhotoreceptorCount,
        Self::OrganellePhotoreceptorMass,
        Self::OrganellePhotoreceptorCapacity,
        Self::OrganellePhotoreceptorDamaged,
        Self::OrganellePhotoreceptorBuilding,
        Self::OrganelleThermoreceptorCount,
        Self::OrganelleThermoreceptorMass,
        Self::OrganelleThermoreceptorCapacity,
        Self::OrganelleThermoreceptorDamaged,
        Self::OrganelleThermoreceptorBuilding,
        Self::OrganelleChemoreceptorCount,
        Self::OrganelleChemoreceptorMass,
        Self::OrganelleChemoreceptorCapacity,
        Self::OrganelleChemoreceptorDamaged,
        Self::OrganelleChemoreceptorBuilding,
        Self::OrganelleMechanoreceptorCount,
        Self::OrganelleMechanoreceptorMass,
        Self::OrganelleMechanoreceptorCapacity,
        Self::OrganelleMechanoreceptorDamaged,
        Self::OrganelleMechanoreceptorBuilding,
        Self::OrganelleOpticalCount,
        Self::OrganelleOpticalMass,
        Self::OrganelleOpticalCapacity,
        Self::OrganelleOpticalDamaged,
        Self::OrganelleOpticalBuilding,
        Self::LightAvailable,
        Self::LightValid,
        Self::LightLevel,
        Self::LightBearing,
        Self::HeatAvailable,
        Self::HeatValid,
        Self::HeatValue,
        Self::HeatBearing,
        Self::ChemicalAvailable,
        Self::ChemicalValid,
        Self::ChemicalMineral,
        Self::ChemicalOrganic,
        Self::ChemicalMineralBearing,
        Self::ChemicalOrganicBearing,
        Self::FlowAvailable,
        Self::FlowValid,
        Self::FlowVector,
        Self::FlowSpeed,
        Self::FlowBearing,
        Self::ContactAvailable,
        Self::ContactValid,
        Self::ContactToken,
        Self::ContactOffset,
        Self::ContactPressure,
        Self::ContactResistance,
        Self::ObjectAvailable,
        Self::ObjectValid,
        Self::ObjectToken,
        Self::ObjectOffset,
        Self::ObjectArea,
        Self::ObjectContrast,
        Self::ObjectPigment,
        Self::ObjectMotion,
        Self::GutExudateEnergy,
        Self::GutExudateNutrients,
        Self::GutTissueEnergy,
        Self::GutTissueNutrients,
        Self::GutWasteEnergy,
        Self::GutWasteNutrients,
        Self::ExpressionRate,
        Self::AssemblyRate,
        Self::AssemblyCapacity,
        Self::ProteinCore,
        Self::ProteinPhotosynthetic,
        Self::ProteinMetabolic,
        Self::ProteinDigestive,
        Self::ProteinReserve,
        Self::ProteinTransporter,
        Self::ProteinPhagocyte,
        Self::ProteinFlagellum,
        Self::ProteinPiercer,
        Self::ProteinCortex,
        Self::ProteinRepair,
        Self::ProteinPhotoreceptor,
        Self::ProteinThermoreceptor,
        Self::ProteinChemoreceptor,
        Self::ProteinMechanoreceptor,
        Self::ProteinOptical,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Energy => "energy",
            Self::EnergyFraction => "energy.fraction",
            Self::EnergyCapacity => "energy.capacity",
            Self::Age => "age",
            Self::Time => "time",
            Self::Speed => "speed",
            Self::Id => "id",
            Self::Integrity => "integrity",
            Self::MemoryUsed => "memory.used",
            Self::MemoryCost => "memory.cost",
            Self::BodyArea => "body.area",
            Self::BodyMass => "body.mass",
            Self::BodyBirthArea => "body.birth_area",
            Self::BodyMaturity => "body.maturity",
            Self::BodyDivisionReady => "body.division_ready",
            Self::BodySpaceFree => "body.space.free",
            Self::BodySpaceUsed => "body.space.used",
            Self::BodySpaceCapacity => "body.space.capacity",
            Self::BodySurfaceFree => "body.surface.free",
            Self::BodySurfaceUsed => "body.surface.used",
            Self::BodySurfaceCapacity => "body.surface.capacity",
            Self::NutrientStored => "nutrient.stored",
            Self::NutrientCapacity => "nutrient.capacity",
            Self::NutrientFraction => "nutrient.fraction",
            Self::GutEnergy => "gut.energy",
            Self::GutCapacity => "gut.capacity",
            Self::GutFraction => "gut.fraction",
            Self::DigestionRate => "digestion.rate",
            Self::GrowthRate => "growth.rate",
            Self::Lifespan => "lifespan",
            Self::ExudationRate => "exudation.rate",
            Self::OrganelleCoreCount => "organelle.core.count",
            Self::OrganelleCoreMass => "organelle.core.mass",
            Self::OrganelleCoreCapacity => "organelle.core.capacity",
            Self::OrganelleCoreDamaged => "organelle.core.damaged",
            Self::OrganelleCoreBuilding => "organelle.core.building",
            Self::OrganellePhotosyntheticCount => "organelle.photosynthetic.count",
            Self::OrganellePhotosyntheticMass => "organelle.photosynthetic.mass",
            Self::OrganellePhotosyntheticCapacity => "organelle.photosynthetic.capacity",
            Self::OrganellePhotosyntheticDamaged => "organelle.photosynthetic.damaged",
            Self::OrganellePhotosyntheticBuilding => "organelle.photosynthetic.building",
            Self::OrganelleMetabolicCount => "organelle.metabolic.count",
            Self::OrganelleMetabolicMass => "organelle.metabolic.mass",
            Self::OrganelleMetabolicCapacity => "organelle.metabolic.capacity",
            Self::OrganelleMetabolicDamaged => "organelle.metabolic.damaged",
            Self::OrganelleMetabolicBuilding => "organelle.metabolic.building",
            Self::OrganelleDigestiveCount => "organelle.digestive.count",
            Self::OrganelleDigestiveMass => "organelle.digestive.mass",
            Self::OrganelleDigestiveCapacity => "organelle.digestive.capacity",
            Self::OrganelleDigestiveDamaged => "organelle.digestive.damaged",
            Self::OrganelleDigestiveBuilding => "organelle.digestive.building",
            Self::OrganelleReserveCount => "organelle.reserve.count",
            Self::OrganelleReserveMass => "organelle.reserve.mass",
            Self::OrganelleReserveCapacity => "organelle.reserve.capacity",
            Self::OrganelleReserveDamaged => "organelle.reserve.damaged",
            Self::OrganelleReserveBuilding => "organelle.reserve.building",
            Self::OrganelleTransporterCount => "organelle.transporter.count",
            Self::OrganelleTransporterMass => "organelle.transporter.mass",
            Self::OrganelleTransporterCapacity => "organelle.transporter.capacity",
            Self::OrganelleTransporterDamaged => "organelle.transporter.damaged",
            Self::OrganelleTransporterBuilding => "organelle.transporter.building",
            Self::OrganellePhagocyteCount => "organelle.phagocyte.count",
            Self::OrganellePhagocyteMass => "organelle.phagocyte.mass",
            Self::OrganellePhagocyteCapacity => "organelle.phagocyte.capacity",
            Self::OrganellePhagocyteDamaged => "organelle.phagocyte.damaged",
            Self::OrganellePhagocyteBuilding => "organelle.phagocyte.building",
            Self::OrganelleFlagellumCount => "organelle.flagellum.count",
            Self::OrganelleFlagellumMass => "organelle.flagellum.mass",
            Self::OrganelleFlagellumCapacity => "organelle.flagellum.capacity",
            Self::OrganelleFlagellumDamaged => "organelle.flagellum.damaged",
            Self::OrganelleFlagellumBuilding => "organelle.flagellum.building",
            Self::OrganellePiercerCount => "organelle.piercer.count",
            Self::OrganellePiercerMass => "organelle.piercer.mass",
            Self::OrganellePiercerCapacity => "organelle.piercer.capacity",
            Self::OrganellePiercerDamaged => "organelle.piercer.damaged",
            Self::OrganellePiercerBuilding => "organelle.piercer.building",
            Self::OrganelleCortexCount => "organelle.cortex.count",
            Self::OrganelleCortexMass => "organelle.cortex.mass",
            Self::OrganelleCortexCapacity => "organelle.cortex.capacity",
            Self::OrganelleCortexDamaged => "organelle.cortex.damaged",
            Self::OrganelleCortexBuilding => "organelle.cortex.building",
            Self::OrganelleRepairCount => "organelle.repair.count",
            Self::OrganelleRepairMass => "organelle.repair.mass",
            Self::OrganelleRepairCapacity => "organelle.repair.capacity",
            Self::OrganelleRepairDamaged => "organelle.repair.damaged",
            Self::OrganelleRepairBuilding => "organelle.repair.building",
            Self::OrganellePhotoreceptorCount => "organelle.photoreceptor.count",
            Self::OrganellePhotoreceptorMass => "organelle.photoreceptor.mass",
            Self::OrganellePhotoreceptorCapacity => "organelle.photoreceptor.capacity",
            Self::OrganellePhotoreceptorDamaged => "organelle.photoreceptor.damaged",
            Self::OrganellePhotoreceptorBuilding => "organelle.photoreceptor.building",
            Self::OrganelleThermoreceptorCount => "organelle.thermoreceptor.count",
            Self::OrganelleThermoreceptorMass => "organelle.thermoreceptor.mass",
            Self::OrganelleThermoreceptorCapacity => "organelle.thermoreceptor.capacity",
            Self::OrganelleThermoreceptorDamaged => "organelle.thermoreceptor.damaged",
            Self::OrganelleThermoreceptorBuilding => "organelle.thermoreceptor.building",
            Self::OrganelleChemoreceptorCount => "organelle.chemoreceptor.count",
            Self::OrganelleChemoreceptorMass => "organelle.chemoreceptor.mass",
            Self::OrganelleChemoreceptorCapacity => "organelle.chemoreceptor.capacity",
            Self::OrganelleChemoreceptorDamaged => "organelle.chemoreceptor.damaged",
            Self::OrganelleChemoreceptorBuilding => "organelle.chemoreceptor.building",
            Self::OrganelleMechanoreceptorCount => "organelle.mechanoreceptor.count",
            Self::OrganelleMechanoreceptorMass => "organelle.mechanoreceptor.mass",
            Self::OrganelleMechanoreceptorCapacity => "organelle.mechanoreceptor.capacity",
            Self::OrganelleMechanoreceptorDamaged => "organelle.mechanoreceptor.damaged",
            Self::OrganelleMechanoreceptorBuilding => "organelle.mechanoreceptor.building",
            Self::OrganelleOpticalCount => "organelle.optical.count",
            Self::OrganelleOpticalMass => "organelle.optical.mass",
            Self::OrganelleOpticalCapacity => "organelle.optical.capacity",
            Self::OrganelleOpticalDamaged => "organelle.optical.damaged",
            Self::OrganelleOpticalBuilding => "organelle.optical.building",
            Self::LightAvailable => "light.available",
            Self::LightValid => "light.valid",
            Self::LightLevel => "light.level",
            Self::LightBearing => "light.bearing",
            Self::HeatAvailable => "heat.available",
            Self::HeatValid => "heat.valid",
            Self::HeatValue => "heat.value",
            Self::HeatBearing => "heat.bearing",
            Self::ChemicalAvailable => "chemical.available",
            Self::ChemicalValid => "chemical.valid",
            Self::ChemicalMineral => "chemical.mineral",
            Self::ChemicalOrganic => "chemical.organic",
            Self::ChemicalMineralBearing => "chemical.mineral_bearing",
            Self::ChemicalOrganicBearing => "chemical.organic_bearing",
            Self::FlowAvailable => "flow.available",
            Self::FlowValid => "flow.valid",
            Self::FlowVector => "flow.vector",
            Self::FlowSpeed => "flow.speed",
            Self::FlowBearing => "flow.bearing",
            Self::ContactAvailable => "contact.available",
            Self::ContactValid => "contact.valid",
            Self::ContactToken => "contact.token",
            Self::ContactOffset => "contact.offset",
            Self::ContactPressure => "contact.pressure",
            Self::ContactResistance => "contact.resistance",
            Self::ObjectAvailable => "object.available",
            Self::ObjectValid => "object.valid",
            Self::ObjectToken => "object.token",
            Self::ObjectOffset => "object.offset",
            Self::ObjectArea => "object.area",
            Self::ObjectContrast => "object.contrast",
            Self::ObjectPigment => "object.pigment",
            Self::ObjectMotion => "object.motion",
            Self::GutExudateEnergy => "gut.exudate.energy",
            Self::GutExudateNutrients => "gut.exudate.nutrients",
            Self::GutTissueEnergy => "gut.tissue.energy",
            Self::GutTissueNutrients => "gut.tissue.nutrients",
            Self::GutWasteEnergy => "gut.waste.energy",
            Self::GutWasteNutrients => "gut.waste.nutrients",
            Self::ExpressionRate => "expression.rate",
            Self::AssemblyRate => "assembly.rate",
            Self::AssemblyCapacity => "assembly.capacity",
            Self::ProteinCore => "protein.core",
            Self::ProteinPhotosynthetic => "protein.photosynthetic",
            Self::ProteinMetabolic => "protein.metabolic",
            Self::ProteinDigestive => "protein.digestive",
            Self::ProteinReserve => "protein.reserve",
            Self::ProteinTransporter => "protein.transporter",
            Self::ProteinPhagocyte => "protein.phagocyte",
            Self::ProteinFlagellum => "protein.flagellum",
            Self::ProteinPiercer => "protein.piercer",
            Self::ProteinCortex => "protein.cortex",
            Self::ProteinRepair => "protein.repair",
            Self::ProteinPhotoreceptor => "protein.photoreceptor",
            Self::ProteinThermoreceptor => "protein.thermoreceptor",
            Self::ProteinChemoreceptor => "protein.chemoreceptor",
            Self::ProteinMechanoreceptor => "protein.mechanoreceptor",
            Self::ProteinOptical => "protein.optical",
        }
    }
    fn ty(self) -> Type {
        match self {
            Self::OrganelleCoreCount
            | Self::OrganellePhotosyntheticCount
            | Self::OrganelleMetabolicCount
            | Self::OrganelleDigestiveCount
            | Self::OrganelleReserveCount
            | Self::OrganelleTransporterCount
            | Self::OrganellePhagocyteCount
            | Self::OrganelleFlagellumCount
            | Self::OrganellePiercerCount
            | Self::OrganelleCortexCount
            | Self::OrganelleRepairCount
            | Self::OrganellePhotoreceptorCount
            | Self::OrganelleThermoreceptorCount
            | Self::OrganelleChemoreceptorCount
            | Self::OrganelleMechanoreceptorCount
            | Self::OrganelleOpticalCount => Type::Integer,
            Self::BodyDivisionReady
            | Self::LightAvailable
            | Self::LightValid
            | Self::HeatAvailable
            | Self::HeatValid
            | Self::ChemicalAvailable
            | Self::ChemicalValid
            | Self::FlowAvailable
            | Self::FlowValid
            | Self::ContactAvailable
            | Self::ContactValid
            | Self::ObjectAvailable
            | Self::ObjectValid => Type::Bool,
            Self::ContactToken | Self::ObjectToken => Type::String,
            Self::FlowVector | Self::ContactOffset | Self::ObjectOffset | Self::ObjectMotion => {
                Type::Vector
            }
            _ => Type::Number,
        }
    }
    fn lookup(name: &str) -> Option<(Self, Type)> {
        Self::ALL
            .into_iter()
            .find(|s| s.name() == name)
            .map(|s| (s, s.ty()))
    }
    fn read(self, i: &Inputs) -> Value {
        match self {
            Self::GutExudateEnergy => Value::Scalar(finite(i.gut_exudate_energy)),
            Self::GutExudateNutrients => Value::Scalar(finite(i.gut_exudate_nutrients)),
            Self::GutTissueEnergy => Value::Scalar(finite(i.gut_tissue_energy)),
            Self::GutTissueNutrients => Value::Scalar(finite(i.gut_tissue_nutrients)),
            Self::GutWasteEnergy => Value::Scalar(finite(i.gut_waste_energy)),
            Self::GutWasteNutrients => Value::Scalar(finite(i.gut_waste_nutrients)),
            Self::ExpressionRate => Value::Scalar(finite(i.expression_rate)),
            Self::AssemblyRate => Value::Scalar(finite(i.assembly_rate)),
            Self::AssemblyCapacity => Value::Scalar(finite(i.assembly_capacity)),
            Self::ProteinCore => Value::Scalar(finite(i.protein[0])),
            Self::ProteinPhotosynthetic => Value::Scalar(finite(i.protein[1])),
            Self::ProteinMetabolic => Value::Scalar(finite(i.protein[2])),
            Self::ProteinDigestive => Value::Scalar(finite(i.protein[3])),
            Self::ProteinReserve => Value::Scalar(finite(i.protein[4])),
            Self::ProteinTransporter => Value::Scalar(finite(i.protein[5])),
            Self::ProteinPhagocyte => Value::Scalar(finite(i.protein[6])),
            Self::ProteinFlagellum => Value::Scalar(finite(i.protein[7])),
            Self::ProteinPiercer => Value::Scalar(finite(i.protein[8])),
            Self::ProteinCortex => Value::Scalar(finite(i.protein[9])),
            Self::ProteinRepair => Value::Scalar(finite(i.protein[10])),
            Self::ProteinPhotoreceptor => Value::Scalar(finite(i.protein[11])),
            Self::ProteinThermoreceptor => Value::Scalar(finite(i.protein[12])),
            Self::ProteinChemoreceptor => Value::Scalar(finite(i.protein[13])),
            Self::ProteinMechanoreceptor => Value::Scalar(finite(i.protein[14])),
            Self::ProteinOptical => Value::Scalar(finite(i.protein[15])),
            Self::Energy => Value::Scalar(finite(i.energy)),
            Self::EnergyFraction => Value::Scalar(finite(i.energy_fraction)),
            Self::EnergyCapacity => Value::Scalar(finite(i.energy_capacity)),
            Self::Age => Value::Scalar(finite(i.age)),
            Self::Time => Value::Scalar(finite(i.time)),
            Self::Speed => Value::Scalar(finite(i.speed)),
            Self::Id => Value::Scalar(finite(i.id)),
            Self::Integrity => Value::Scalar(finite(i.integrity)),
            Self::MemoryUsed => Value::Scalar(finite(i.memory_used)),
            Self::MemoryCost => Value::Scalar(finite(i.memory_cost)),
            Self::BodyArea => Value::Scalar(finite(i.body_area)),
            Self::BodyMass => Value::Scalar(finite(i.body_mass)),
            Self::BodyBirthArea => Value::Scalar(finite(i.body_birth_area)),
            Self::BodyMaturity => Value::Scalar(finite(i.body_maturity)),
            Self::BodyDivisionReady => Value::Boolean(i.body_division_ready),
            Self::BodySpaceFree => Value::Scalar(finite(i.body_space_free)),
            Self::BodySpaceUsed => Value::Scalar(finite(i.body_space_used)),
            Self::BodySpaceCapacity => Value::Scalar(finite(i.body_space_capacity)),
            Self::BodySurfaceFree => Value::Scalar(finite(i.body_surface_free)),
            Self::BodySurfaceUsed => Value::Scalar(finite(i.body_surface_used)),
            Self::BodySurfaceCapacity => Value::Scalar(finite(i.body_surface_capacity)),
            Self::NutrientStored => Value::Scalar(finite(i.nutrient_stored)),
            Self::NutrientCapacity => Value::Scalar(finite(i.nutrient_capacity)),
            Self::NutrientFraction => Value::Scalar(finite(i.nutrient_fraction)),
            Self::GutEnergy => Value::Scalar(finite(i.gut_energy)),
            Self::GutCapacity => Value::Scalar(finite(i.gut_capacity)),
            Self::GutFraction => Value::Scalar(finite(i.gut_fraction)),
            Self::DigestionRate => Value::Scalar(finite(i.digestion_rate)),
            Self::GrowthRate => Value::Scalar(finite(i.growth_rate)),
            Self::Lifespan => Value::Scalar(finite(i.lifespan)),
            Self::ExudationRate => Value::Scalar(finite(i.exudation_rate)),
            Self::OrganelleCoreCount => Value::Integer(i.organelle[0].count.max(0)),
            Self::OrganelleCoreMass => Value::Scalar(finite(i.organelle[0].mass)),
            Self::OrganelleCoreCapacity => Value::Scalar(finite(i.organelle[0].capacity)),
            Self::OrganelleCoreDamaged => Value::Scalar(finite(i.organelle[0].damaged)),
            Self::OrganelleCoreBuilding => Value::Scalar(finite(i.organelle[0].building)),
            Self::OrganellePhotosyntheticCount => Value::Integer(i.organelle[1].count.max(0)),
            Self::OrganellePhotosyntheticMass => Value::Scalar(finite(i.organelle[1].mass)),
            Self::OrganellePhotosyntheticCapacity => Value::Scalar(finite(i.organelle[1].capacity)),
            Self::OrganellePhotosyntheticDamaged => Value::Scalar(finite(i.organelle[1].damaged)),
            Self::OrganellePhotosyntheticBuilding => Value::Scalar(finite(i.organelle[1].building)),
            Self::OrganelleMetabolicCount => Value::Integer(i.organelle[2].count.max(0)),
            Self::OrganelleMetabolicMass => Value::Scalar(finite(i.organelle[2].mass)),
            Self::OrganelleMetabolicCapacity => Value::Scalar(finite(i.organelle[2].capacity)),
            Self::OrganelleMetabolicDamaged => Value::Scalar(finite(i.organelle[2].damaged)),
            Self::OrganelleMetabolicBuilding => Value::Scalar(finite(i.organelle[2].building)),
            Self::OrganelleDigestiveCount => Value::Integer(i.organelle[3].count.max(0)),
            Self::OrganelleDigestiveMass => Value::Scalar(finite(i.organelle[3].mass)),
            Self::OrganelleDigestiveCapacity => Value::Scalar(finite(i.organelle[3].capacity)),
            Self::OrganelleDigestiveDamaged => Value::Scalar(finite(i.organelle[3].damaged)),
            Self::OrganelleDigestiveBuilding => Value::Scalar(finite(i.organelle[3].building)),
            Self::OrganelleReserveCount => Value::Integer(i.organelle[4].count.max(0)),
            Self::OrganelleReserveMass => Value::Scalar(finite(i.organelle[4].mass)),
            Self::OrganelleReserveCapacity => Value::Scalar(finite(i.organelle[4].capacity)),
            Self::OrganelleReserveDamaged => Value::Scalar(finite(i.organelle[4].damaged)),
            Self::OrganelleReserveBuilding => Value::Scalar(finite(i.organelle[4].building)),
            Self::OrganelleTransporterCount => Value::Integer(i.organelle[5].count.max(0)),
            Self::OrganelleTransporterMass => Value::Scalar(finite(i.organelle[5].mass)),
            Self::OrganelleTransporterCapacity => Value::Scalar(finite(i.organelle[5].capacity)),
            Self::OrganelleTransporterDamaged => Value::Scalar(finite(i.organelle[5].damaged)),
            Self::OrganelleTransporterBuilding => Value::Scalar(finite(i.organelle[5].building)),
            Self::OrganellePhagocyteCount => Value::Integer(i.organelle[6].count.max(0)),
            Self::OrganellePhagocyteMass => Value::Scalar(finite(i.organelle[6].mass)),
            Self::OrganellePhagocyteCapacity => Value::Scalar(finite(i.organelle[6].capacity)),
            Self::OrganellePhagocyteDamaged => Value::Scalar(finite(i.organelle[6].damaged)),
            Self::OrganellePhagocyteBuilding => Value::Scalar(finite(i.organelle[6].building)),
            Self::OrganelleFlagellumCount => Value::Integer(i.organelle[7].count.max(0)),
            Self::OrganelleFlagellumMass => Value::Scalar(finite(i.organelle[7].mass)),
            Self::OrganelleFlagellumCapacity => Value::Scalar(finite(i.organelle[7].capacity)),
            Self::OrganelleFlagellumDamaged => Value::Scalar(finite(i.organelle[7].damaged)),
            Self::OrganelleFlagellumBuilding => Value::Scalar(finite(i.organelle[7].building)),
            Self::OrganellePiercerCount => Value::Integer(i.organelle[8].count.max(0)),
            Self::OrganellePiercerMass => Value::Scalar(finite(i.organelle[8].mass)),
            Self::OrganellePiercerCapacity => Value::Scalar(finite(i.organelle[8].capacity)),
            Self::OrganellePiercerDamaged => Value::Scalar(finite(i.organelle[8].damaged)),
            Self::OrganellePiercerBuilding => Value::Scalar(finite(i.organelle[8].building)),
            Self::OrganelleCortexCount => Value::Integer(i.organelle[9].count.max(0)),
            Self::OrganelleCortexMass => Value::Scalar(finite(i.organelle[9].mass)),
            Self::OrganelleCortexCapacity => Value::Scalar(finite(i.organelle[9].capacity)),
            Self::OrganelleCortexDamaged => Value::Scalar(finite(i.organelle[9].damaged)),
            Self::OrganelleCortexBuilding => Value::Scalar(finite(i.organelle[9].building)),
            Self::OrganelleRepairCount => Value::Integer(i.organelle[10].count.max(0)),
            Self::OrganelleRepairMass => Value::Scalar(finite(i.organelle[10].mass)),
            Self::OrganelleRepairCapacity => Value::Scalar(finite(i.organelle[10].capacity)),
            Self::OrganelleRepairDamaged => Value::Scalar(finite(i.organelle[10].damaged)),
            Self::OrganelleRepairBuilding => Value::Scalar(finite(i.organelle[10].building)),
            Self::OrganellePhotoreceptorCount => Value::Integer(i.organelle[11].count.max(0)),
            Self::OrganellePhotoreceptorMass => Value::Scalar(finite(i.organelle[11].mass)),
            Self::OrganellePhotoreceptorCapacity => Value::Scalar(finite(i.organelle[11].capacity)),
            Self::OrganellePhotoreceptorDamaged => Value::Scalar(finite(i.organelle[11].damaged)),
            Self::OrganellePhotoreceptorBuilding => Value::Scalar(finite(i.organelle[11].building)),
            Self::OrganelleThermoreceptorCount => Value::Integer(i.organelle[12].count.max(0)),
            Self::OrganelleThermoreceptorMass => Value::Scalar(finite(i.organelle[12].mass)),
            Self::OrganelleThermoreceptorCapacity => {
                Value::Scalar(finite(i.organelle[12].capacity))
            }
            Self::OrganelleThermoreceptorDamaged => Value::Scalar(finite(i.organelle[12].damaged)),
            Self::OrganelleThermoreceptorBuilding => {
                Value::Scalar(finite(i.organelle[12].building))
            }
            Self::OrganelleChemoreceptorCount => Value::Integer(i.organelle[13].count.max(0)),
            Self::OrganelleChemoreceptorMass => Value::Scalar(finite(i.organelle[13].mass)),
            Self::OrganelleChemoreceptorCapacity => Value::Scalar(finite(i.organelle[13].capacity)),
            Self::OrganelleChemoreceptorDamaged => Value::Scalar(finite(i.organelle[13].damaged)),
            Self::OrganelleChemoreceptorBuilding => Value::Scalar(finite(i.organelle[13].building)),
            Self::OrganelleMechanoreceptorCount => Value::Integer(i.organelle[14].count.max(0)),
            Self::OrganelleMechanoreceptorMass => Value::Scalar(finite(i.organelle[14].mass)),
            Self::OrganelleMechanoreceptorCapacity => {
                Value::Scalar(finite(i.organelle[14].capacity))
            }
            Self::OrganelleMechanoreceptorDamaged => Value::Scalar(finite(i.organelle[14].damaged)),
            Self::OrganelleMechanoreceptorBuilding => {
                Value::Scalar(finite(i.organelle[14].building))
            }
            Self::OrganelleOpticalCount => Value::Integer(i.organelle[15].count.max(0)),
            Self::OrganelleOpticalMass => Value::Scalar(finite(i.organelle[15].mass)),
            Self::OrganelleOpticalCapacity => Value::Scalar(finite(i.organelle[15].capacity)),
            Self::OrganelleOpticalDamaged => Value::Scalar(finite(i.organelle[15].damaged)),
            Self::OrganelleOpticalBuilding => Value::Scalar(finite(i.organelle[15].building)),
            Self::LightAvailable => Value::Boolean(i.light_available),
            Self::LightValid => Value::Boolean(i.light_valid),
            Self::LightLevel => Value::Scalar(finite(i.light_level)),
            Self::LightBearing => Value::Scalar(finite(i.light_bearing)),
            Self::HeatAvailable => Value::Boolean(i.heat_available),
            Self::HeatValid => Value::Boolean(i.heat_valid),
            Self::HeatValue => Value::Scalar(finite(i.heat_value)),
            Self::HeatBearing => Value::Scalar(finite(i.heat_bearing)),
            Self::ChemicalAvailable => Value::Boolean(i.chemical_available),
            Self::ChemicalValid => Value::Boolean(i.chemical_valid),
            Self::ChemicalMineral => Value::Scalar(finite(i.chemical_mineral)),
            Self::ChemicalOrganic => Value::Scalar(finite(i.chemical_organic)),
            Self::ChemicalMineralBearing => Value::Scalar(finite(i.chemical_mineral_bearing)),
            Self::ChemicalOrganicBearing => Value::Scalar(finite(i.chemical_organic_bearing)),
            Self::FlowAvailable => Value::Boolean(i.flow_available),
            Self::FlowValid => Value::Boolean(i.flow_valid),
            Self::FlowVector => Value::Vector([finite(i.flow_vector[0]), finite(i.flow_vector[1])]),
            Self::FlowSpeed => Value::Scalar(finite(i.flow_speed)),
            Self::FlowBearing => Value::Scalar(finite(i.flow_bearing)),
            Self::ContactAvailable => Value::Boolean(i.contact_available),
            Self::ContactValid => Value::Boolean(i.contact_valid),
            Self::ContactToken => Value::Text(i.contact_token.clone()),
            Self::ContactOffset => {
                Value::Vector([finite(i.contact_offset[0]), finite(i.contact_offset[1])])
            }
            Self::ContactPressure => Value::Scalar(finite(i.contact_pressure)),
            Self::ContactResistance => Value::Scalar(finite(i.contact_resistance)),
            Self::ObjectAvailable => Value::Boolean(i.object_available),
            Self::ObjectValid => Value::Boolean(i.object_valid),
            Self::ObjectToken => Value::Text(i.object_token.clone()),
            Self::ObjectOffset => {
                Value::Vector([finite(i.object_offset[0]), finite(i.object_offset[1])])
            }
            Self::ObjectArea => Value::Scalar(finite(i.object_area)),
            Self::ObjectContrast => Value::Scalar(finite(i.object_contrast)),
            Self::ObjectPigment => Value::Scalar(finite(i.object_pigment)),
            Self::ObjectMotion => {
                Value::Vector([finite(i.object_motion[0]), finite(i.object_motion[1])])
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Binary {
    Add,
    Subtract,
    Multiply,
    Divide,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
    And,
    Or,
}
impl Binary {
    const ALL: [Self; 12] = [
        Self::Add,
        Self::Subtract,
        Self::Multiply,
        Self::Divide,
        Self::Less,
        Self::LessEqual,
        Self::Greater,
        Self::GreaterEqual,
        Self::Equal,
        Self::NotEqual,
        Self::And,
        Self::Or,
    ];
    fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::And => "&&",
            Self::Or => "||",
        }
    }
}
fn numeric_type(types: &[Type]) -> Option<Type> {
    if types.iter().all(|t| t.numeric()) {
        Some(if types.contains(&Type::Number) {
            Type::Number
        } else {
            Type::Integer
        })
    } else {
        None
    }
}
fn binary_type(op: Binary, a: Type, b: Type) -> Option<Type> {
    use Binary::*;
    match op {
        And | Or => (a == Type::Bool && b == Type::Bool).then_some(Type::Bool),
        Equal | NotEqual => (a == b || (a.numeric() && b.numeric())).then_some(Type::Bool),
        Less | LessEqual | Greater | GreaterEqual => numeric_type(&[a, b]).map(|_| Type::Bool),
        Add if a == Type::String && b == Type::String => Some(Type::String),
        Add | Subtract if a == Type::Vector && b == Type::Vector => Some(Type::Vector),
        Multiply if (a == Type::Vector && b.numeric()) || (b == Type::Vector && a.numeric()) => {
            Some(Type::Vector)
        }
        Divide if a == Type::Vector && b.numeric() => Some(Type::Vector),
        Divide => numeric_type(&[a, b]).map(|_| Type::Number),
        Add | Subtract | Multiply => numeric_type(&[a, b]),
    }
}
fn binary_value(op: Binary, a: &Value, b: &Value, ty: Type) -> Value {
    use Binary::*;
    if matches!(op, Equal | NotEqual) {
        let same = if a.ty().numeric() && b.ty().numeric() {
            a.number() == b.number()
        } else {
            a == b
        };
        return Value::Boolean(if op == Equal { same } else { !same });
    }
    if matches!(op, And | Or) {
        return Value::Boolean(if op == And {
            a.boolean() && b.boolean()
        } else {
            a.boolean() || b.boolean()
        });
    }
    if matches!(op, Less | LessEqual | Greater | GreaterEqual) {
        let (x, y) = (a.number(), b.number());
        return Value::Boolean(match op {
            Less => x < y,
            LessEqual => x <= y,
            Greater => x > y,
            _ => x >= y,
        });
    }
    if ty == Type::String {
        if let (Value::Text(a), Value::Text(b)) = (a, b) {
            return Value::Text(bounded_text(format!("{a}{b}")));
        }
    }
    if ty == Type::Vector {
        let av = if a.ty() == Type::Vector {
            a.vector()
        } else {
            [a.number(); 2]
        };
        let bv = if b.ty() == Type::Vector {
            b.vector()
        } else {
            [b.number(); 2]
        };
        return Value::Vector(std::array::from_fn(|i| {
            finite(match op {
                Add => av[i] + bv[i],
                Subtract => av[i] - bv[i],
                Multiply => av[i] * bv[i],
                Divide => {
                    if bv[i] == 0.0 {
                        0.0
                    } else {
                        av[i] / bv[i]
                    }
                }
                _ => 0.0,
            })
        }));
    }
    if ty == Type::Integer {
        let (Value::Integer(x), Value::Integer(y)) = (a, b) else {
            unreachable!("compiled integer arithmetic must have integer operands")
        };
        let v = match op {
            Add => x.saturating_add(*y),
            Subtract => x.saturating_sub(*y),
            Multiply => x.saturating_mul(*y),
            _ => 0,
        };
        return Value::Integer(v);
    }
    let (x, y) = (a.number(), b.number());
    Value::Scalar(finite(match op {
        Add => x + y,
        Subtract => x - y,
        Multiply => x * y,
        Divide => {
            if y == 0.0 {
                0.0
            } else {
                x / y
            }
        }
        _ => 0.0,
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Function {
    Sin,
    Cos,
    Abs,
    Min,
    Max,
    Clamp,
    Int,
    Float,
    Vec,
    Length,
    Dot,
    Normalize,
    Angle,
    X,
    Y,
    Sqrt,
    Sign,
    Atan2,
    Lerp,
    Noise,
    Identity,
    Activate,
    Repress,
}
impl Function {
    const ALL: [Self; 23] = [
        Self::Sin,
        Self::Cos,
        Self::Abs,
        Self::Min,
        Self::Max,
        Self::Clamp,
        Self::Int,
        Self::Float,
        Self::Vec,
        Self::Length,
        Self::Dot,
        Self::Normalize,
        Self::Angle,
        Self::X,
        Self::Y,
        Self::Sqrt,
        Self::Sign,
        Self::Atan2,
        Self::Lerp,
        Self::Noise,
        Self::Identity,
        Self::Activate,
        Self::Repress,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Sin => "sin",
            Self::Cos => "cos",
            Self::Abs => "abs",
            Self::Min => "min",
            Self::Max => "max",
            Self::Clamp => "clamp",
            Self::Int => "int",
            Self::Float => "float",
            Self::Vec => "vec",
            Self::Length => "length",
            Self::Dot => "dot",
            Self::Normalize => "normalize",
            Self::Angle => "angle",
            Self::X => "x",
            Self::Y => "y",
            Self::Sqrt => "sqrt",
            Self::Sign => "sign",
            Self::Atan2 => "atan2",
            Self::Lerp => "lerp",
            Self::Noise => "noise",
            Self::Identity => "identity",
            Self::Activate => "activate",
            Self::Repress => "repress",
        }
    }
    fn arity(self) -> usize {
        match self {
            Self::Min
            | Self::Max
            | Self::Vec
            | Self::Dot
            | Self::Atan2
            | Self::Noise
            | Self::Activate
            | Self::Repress => 2,
            Self::Clamp | Self::Lerp => 3,
            _ => 1,
        }
    }
    fn lookup(name: &str) -> Option<(Self, usize)> {
        Self::ALL
            .into_iter()
            .find(|f| f.name() == name)
            .map(|f| (f, f.arity()))
    }
    fn ty(self, t: &[Type]) -> Option<Type> {
        if t.len() != self.arity() {
            return None;
        }
        match self {
            Self::Identity => Some(t[0]),
            Self::Int => numeric_type(t).map(|_| Type::Integer),
            Self::Float
            | Self::Sin
            | Self::Cos
            | Self::Sqrt
            | Self::Sign
            | Self::Atan2
            | Self::Lerp
            | Self::Noise
            | Self::Activate
            | Self::Repress => numeric_type(t).map(|_| Type::Number),
            Self::Abs | Self::Min | Self::Max | Self::Clamp => numeric_type(t),
            Self::Vec => numeric_type(t).map(|_| Type::Vector),
            Self::Normalize => (t[0] == Type::Vector).then_some(Type::Vector),
            Self::Dot => t.iter().all(|t| *t == Type::Vector).then_some(Type::Number),
            Self::Angle | Self::X | Self::Y => (t[0] == Type::Vector).then_some(Type::Number),
            Self::Length => match t[0] {
                Type::Vector => Some(Type::Number),
                Type::String => Some(Type::Integer),
                _ => None,
            },
        }
    }
    fn eval(self, args: &[usize; 3], values: &[Value], ty: Type) -> Value {
        let a = &values[args[0]];
        let n = a.number();
        let second = || values[args[1]].number();
        let third = || values[args[2]].number();
        let result = match self {
            Self::Identity => return a.clone(),
            Self::Int => return a.clone().convert(Type::Integer),
            Self::Float => n,
            Self::Sin => n.sin(),
            Self::Cos => n.cos(),
            Self::Abs => n.abs(),
            Self::Min => n.min(second()),
            Self::Max => n.max(second()),
            Self::Clamp => {
                let (b, c) = (second(), third());
                n.clamp(b.min(c), b.max(c))
            }
            Self::Vec => return Value::Vector([n, second()]),
            Self::Length => match a {
                Value::Text(text) => return Value::Integer(text.chars().count() as i32),
                _ => {
                    let v = a.vector();
                    v[0].hypot(v[1])
                }
            },
            Self::Dot => {
                let v = a.vector();
                let w = values[args[1]].vector();
                v[0] * w[0] + v[1] * w[1]
            }
            Self::Normalize => {
                let v = a.vector();
                let len = v[0].hypot(v[1]);
                return Value::Vector(if len == 0.0 {
                    [0.0; 2]
                } else {
                    [v[0] / len, v[1] / len]
                });
            }
            Self::Angle => {
                let v = a.vector();
                if v == [0.0; 2] {
                    0.0
                } else {
                    v[1].atan2(v[0])
                }
            }
            Self::X => a.vector()[0],
            Self::Y => a.vector()[1],
            Self::Sqrt => n.max(0.0).sqrt(),
            Self::Sign => {
                if n == 0.0 {
                    0.0
                } else {
                    n.signum()
                }
            }
            Self::Atan2 => {
                if n == 0.0 && second() == 0.0 {
                    0.0
                } else {
                    n.atan2(second())
                }
            }
            Self::Lerp => n + (second() - n) * third(),
            Self::Noise => smooth_noise(n, second()),
            Self::Activate | Self::Repress => {
                let x = n.max(0.0);
                let k = second().max(0.0);
                let active = if x + k > 0.0 { x / (x + k) } else { 0.0 };
                if self == Self::Repress {
                    1.0 - active
                } else {
                    active
                }
            }
        };
        Value::Scalar(finite(result)).convert(ty)
    }
}
fn smooth_noise(seed: f64, phase: f64) -> f64 {
    fn sample(seed: u64, index: i64) -> f64 {
        let mut x = seed ^ (index as u64).wrapping_mul(0x9e3779b97f4a7c15);
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        x ^= x >> 31;
        ((x >> 11) as f64 / ((1_u64 << 53) - 1) as f64) * 2.0 - 1.0
    }
    let cell = phase.floor() as i64;
    let fraction = phase - phase.floor();
    let blend = fraction * fraction * (3.0 - 2.0 * fraction);
    let a = sample(seed.to_bits(), cell);
    let b = sample(seed.to_bits(), cell + 1);
    a + (b - a) * blend
}
#[derive(Debug, Clone, PartialEq)]
enum Op {
    Constant(Value),
    Sensor(Sensor),
    Pool(usize),
    Concentration(usize),
    Built(usize),
    Receptor(usize, ReceptorField),
    Register(Register),
    Populated(Register),
    Reference(usize),
    Negate(usize),
    Not(usize),
    Binary(Binary, usize, usize),
    Choose(usize, usize, usize),
    Call(Function, [usize; 3]),
}
impl Op {
    fn children(&self) -> ([usize; 3], usize) {
        match *self {
            Self::Constant(_)
            | Self::Sensor(_)
            | Self::Pool(_)
            | Self::Concentration(_)
            | Self::Built(_)
            | Self::Receptor(_, _)
            | Self::Register(_)
            | Self::Populated(_) => ([0; 3], 0),
            Self::Reference(a) | Self::Negate(a) | Self::Not(a) => ([a, 0, 0], 1),
            Self::Binary(_, a, b) => ([a, b, 0], 2),
            Self::Choose(a, b, c) => ([a, b, c], 3),
            Self::Call(f, args) => (args, f.arity()),
        }
    }
}

/// Shared typed semantics for authored evaluation, constant folding and compact
/// execution. Constant folding never substitutes algebraic identities that could
/// change saturation, bounded strings, floating-point order or signed zeros.
fn evaluate_operation(
    op: &Op,
    ty: Type,
    inputs: &Inputs,
    memory: &Registers,
    values: &[Value],
) -> Value {
    let value = match op {
        Op::Constant(value) => value.clone(),
        Op::Sensor(sensor) => sensor.read(inputs),
        Op::Pool(index) => {
            Value::Scalar(finite(inputs.products.get(*index).copied().unwrap_or(0.0)).max(0.0))
        }
        Op::Concentration(index) => {
            let mass = finite(inputs.products.get(*index).copied().unwrap_or(0.0)).max(0.0);
            Value::Scalar(if inputs.body_area.is_finite() && inputs.body_area > 0.0 {
                finite(mass / inputs.body_area)
            } else {
                0.0
            })
        }
        Op::Built(index) => {
            Value::Scalar(finite(inputs.assemblies.get(*index).copied().unwrap_or(0.0)).max(0.0))
        }
        Op::Receptor(index, field) => field.read(inputs.receptors.get(*index)),
        Op::Register(key) => memory.read(*key),
        Op::Populated(key) => Value::Boolean(memory.entries.contains_key(key)),
        Op::Reference(i) => values[*i].clone(),
        Op::Not(i) => Value::Boolean(!values[*i].boolean()),
        Op::Negate(i) => match &values[*i] {
            Value::Integer(v) => Value::Integer(v.saturating_neg()),
            Value::Vector(v) => Value::Vector([-v[0], -v[1]]),
            v => Value::Scalar(-v.number()),
        },
        Op::Binary(op, a, b) => binary_value(*op, &values[*a], &values[*b], ty),
        Op::Choose(condition, yes, no) => values[if values[*condition].boolean() {
            *yes
        } else {
            *no
        }]
        .clone(),
        Op::Call(function, args) => function.eval(args, values, ty),
    };
    value.convert(ty)
}

#[derive(Debug, Clone)]
struct Node {
    op: Op,
    ty: Type,
    scope: usize,
}
#[derive(Debug, Clone)]
struct Binding {
    name: String,
    node: usize,
}
#[derive(Debug, Clone)]
struct WriteRule {
    register: Register,
    value: Option<usize>,
    guard: Option<usize>,
}
pub const OUTPUT_COUNT: usize = 11 + FAMILY_COUNT * 4 + 3 + 3;
const BODY_OUTPUT_START: usize = 11;
const SCAN_OUTPUT_START: usize = BODY_OUTPUT_START + FAMILY_COUNT * 4;
const DIGEST_OUTPUT_START: usize = SCAN_OUTPUT_START + 3;
const OUTPUT_NAMES: [&str; OUTPUT_COUNT] = [
    "thrust",
    "turn",
    "divide",
    "grow",
    "photosynthesize",
    "engulf",
    "bite",
    "defend",
    "repair",
    "absorb",
    "feed",
    "target.core.count",
    "target.core.size",
    "express.core",
    "recycle.core",
    "target.photosynthetic.count",
    "target.photosynthetic.size",
    "express.photosynthetic",
    "recycle.photosynthetic",
    "target.metabolic.count",
    "target.metabolic.size",
    "express.metabolic",
    "recycle.metabolic",
    "target.digestive.count",
    "target.digestive.size",
    "express.digestive",
    "recycle.digestive",
    "target.reserve.count",
    "target.reserve.size",
    "express.reserve",
    "recycle.reserve",
    "target.transporter.count",
    "target.transporter.size",
    "express.transporter",
    "recycle.transporter",
    "target.phagocyte.count",
    "target.phagocyte.size",
    "express.phagocyte",
    "recycle.phagocyte",
    "target.flagellum.count",
    "target.flagellum.size",
    "express.flagellum",
    "recycle.flagellum",
    "target.piercer.count",
    "target.piercer.size",
    "express.piercer",
    "recycle.piercer",
    "target.cortex.count",
    "target.cortex.size",
    "express.cortex",
    "recycle.cortex",
    "target.repair.count",
    "target.repair.size",
    "express.repair",
    "recycle.repair",
    "target.photoreceptor.count",
    "target.photoreceptor.size",
    "express.photoreceptor",
    "recycle.photoreceptor",
    "target.thermoreceptor.count",
    "target.thermoreceptor.size",
    "express.thermoreceptor",
    "recycle.thermoreceptor",
    "target.chemoreceptor.count",
    "target.chemoreceptor.size",
    "express.chemoreceptor",
    "recycle.chemoreceptor",
    "target.mechanoreceptor.count",
    "target.mechanoreceptor.size",
    "express.mechanoreceptor",
    "recycle.mechanoreceptor",
    "target.optical.count",
    "target.optical.size",
    "express.optical",
    "recycle.optical",
    "scan.bearing",
    "scan.width",
    "scan.target",
    "digest.exudate",
    "digest.tissue",
    "digest.waste",
];
/// Older source keeps its spelling while sharing the exact expression output.
fn output_index(name: &str) -> Option<usize> {
    OUTPUT_NAMES.iter().position(|n| *n == name).or_else(|| {
        let family = name.strip_prefix("build.")?;
        OUTPUT_NAMES
            .iter()
            .position(|n| n.strip_prefix("express.") == Some(family))
    })
}
fn output_type(index: usize) -> Type {
    match index {
        2 | 5 => Type::Bool,
        i if (BODY_OUTPUT_START..SCAN_OUTPUT_START).contains(&i)
            && (i - BODY_OUTPUT_START) % 4 == 0 =>
        {
            Type::Integer
        }
        i if i == SCAN_OUTPUT_START + 2 => Type::String,
        _ => Type::Number,
    }
}
fn output_default(index: usize) -> Value {
    match index {
        9 | 10 => Value::Scalar(1.0),
        i if i == DIGEST_OUTPUT_START || i == DIGEST_OUTPUT_START + 1 => Value::Scalar(1.0),
        i if i == DIGEST_OUTPUT_START + 2 => Value::Scalar(0.35),
        i if (BODY_OUTPUT_START..SCAN_OUTPUT_START).contains(&i)
            && (i - BODY_OUTPUT_START) % 4 == 1 =>
        {
            Value::Scalar(MIN_ORGANELLE_MASS[(i - BODY_OUTPUT_START) / 4])
        }
        i if i == SCAN_OUTPUT_START + 1 => Value::Scalar(std::f64::consts::TAU),
        _ => Value::zero(output_type(index)),
    }
}
/// Applied action values use the same conversion in simulation and inspection.
fn clamp_output(index: usize, value: Value) -> Value {
    match (output_type(index), value) {
        (Type::Bool, Value::Boolean(v)) => Value::Boolean(v),
        (Type::String, Value::Text(v)) => Value::Text(v),
        (Type::Integer, Value::Integer(v)) => Value::Integer(v.max(0)),
        (Type::Number, v) => {
            let n = finite(v.number());
            let n = if index == 1 {
                n.clamp(-1.0, 1.0)
            } else if index == SCAN_OUTPUT_START {
                (n + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
            } else if index == SCAN_OUTPUT_START + 1 {
                n.clamp(0.0, std::f64::consts::TAU)
            } else if (BODY_OUTPUT_START..SCAN_OUTPUT_START).contains(&index)
                && (index - BODY_OUTPUT_START) % 4 == 1
            {
                n.clamp(
                    MIN_ORGANELLE_MASS[(index - BODY_OUTPUT_START) / 4],
                    MAX_SIGNAL,
                )
            } else {
                n.clamp(0.0, 1.0)
            };
            Value::Scalar(n)
        }
        _ => unreachable!("compiled output type must match its declaration"),
    }
}

/// Versioned authoritative metadata for editor catalogs and ABI validation.
pub fn language_schema_json() -> String {
    let signals = Sensor::ALL
        .iter()
        .map(|sensor| {
            format!(
                "{{\"name\":{},\"type\":{}}}",
                quote(sensor.name()),
                quote(sensor.ty().label())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let outputs = OUTPUT_NAMES
        .iter()
        .enumerate()
        .map(|(index, name)| {
            format!(
                "{{\"name\":{},\"type\":{},\"required\":{},\"default\":{}}}",
                quote(name),
                quote(output_type(index).label()),
                index < 3,
                output_default(index).json()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let cellular = cellular::schema_json();
    let functions = Function::ALL
        .iter()
        .map(|function| {
            format!(
                "{{\"name\":{},\"arity\":{}}}",
                quote(function.name()),
                function.arity()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"version\":3,\"signals\":[{signals}],\"outputs\":[{outputs}],\"functions\":[{functions}],\"maxNodes\":{MAX_NODES},\"maxSourceBytes\":{MAX_SOURCE_BYTES},\"registersPerType\":{REGISTER_COUNT},\"cellular\":{cellular}}}")
}
#[derive(Debug, Clone)]
pub struct Program {
    nodes: Vec<Node>,
    bindings: Vec<Binding>,
    output_roots: [Option<usize>; OUTPUT_COUNT],
    writes: Vec<WriteRule>,
    // Derived from the immutable authored graph, never written to checkpoints.
    execution: Box<ExecutionPlan>,
    cellular: Option<Box<cellular::CellularGraph>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MutationKind {
    Any = 0,
    Constant = 1,
    Expression = 2,
    Reference = 3,
    GeneSilence = 4,
    GeneDuplicate = 5,
    Geometry = 6,
}
#[derive(Debug, Clone)]
pub struct Mutation {
    pub program: Program,
    pub source: String,
    pub kind: MutationKind,
}

/// Editor metadata is transient; it never enters the executable Program or a checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenomeDiagnostic {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenomeBinding {
    pub name: String,
    pub value_type: &'static str,
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenomeAnalysis {
    pub valid: bool,
    pub diagnostics: Vec<GenomeDiagnostic>,
    pub bindings: Vec<GenomeBinding>,
    pub node_count: usize,
    pub source_bytes: usize,
}

impl GenomeAnalysis {
    /// Input-boundary failures have no readable source range.
    pub(crate) fn invalid_input(source_bytes: usize, message: impl Into<String>) -> Self {
        Self {
            valid: false,
            diagnostics: vec![GenomeDiagnostic {
                message: message.into(),
                line: 1,
                column: 1,
                from: 0,
                to: 0,
            }],
            bindings: Vec::new(),
            node_count: 0,
            source_bytes,
        }
    }

    pub fn json(&self) -> String {
        let diagnostics = self.diagnostics.iter().map(|item| format!(
            "{{\"severity\":\"error\",\"message\":{},\"line\":{},\"column\":{},\"from\":{},\"to\":{}}}",
            quote(&item.message), item.line, item.column, item.from, item.to,
        )).collect::<Vec<_>>().join(",");
        let bindings = self
            .bindings
            .iter()
            .map(|item| {
                format!(
                    "{{\"name\":{},\"type\":{},\"from\":{},\"to\":{}}}",
                    quote(&item.name),
                    quote(item.value_type),
                    item.from,
                    item.to,
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"valid\":{},\"diagnostics\":[{diagnostics}],\"bindings\":[{bindings}],\"nodeCount\":{},\"sourceBytes\":{}}}",
            self.valid, self.node_count, self.source_bytes)
    }
}

/// One linear pass maps the compiler's UTF-8 spans to browser UTF-16 offsets.
/// Avoid rescanning the source separately for each of up to 8,192 declarations.
struct SourceMap {
    offsets: Vec<usize>,
    line_starts: Vec<usize>,
}
impl SourceMap {
    fn new(source: &str) -> Self {
        let mut offsets = vec![0; source.len() + 1];
        let mut line_starts = vec![0];
        let mut position = 0;
        for (byte, character) in source.char_indices() {
            offsets[byte..byte + character.len_utf8()].fill(position);
            position += character.len_utf16();
            if character == '\n' {
                line_starts.push(position);
            }
        }
        offsets[source.len()] = position;
        Self {
            offsets,
            line_starts,
        }
    }
    fn diagnostic(&self, error: CompileError) -> GenomeDiagnostic {
        let from = self.offsets[error.location.from];
        let to = self.offsets[error.location.to];
        let line = self.line_starts.partition_point(|start| *start <= from);
        GenomeDiagnostic {
            message: error.message,
            line,
            column: from - self.line_starts[line - 1] + 1,
            from,
            to,
        }
    }
}

impl Program {
    fn from_parts(
        nodes: Vec<Node>,
        bindings: Vec<Binding>,
        output_roots: [Option<usize>; OUTPUT_COUNT],
        writes: Vec<WriteRule>,
    ) -> Self {
        let execution = ExecutionPlan::compile(&nodes, &output_roots, &writes, &[]);
        Self {
            nodes,
            bindings,
            output_roots,
            writes,
            execution,
            cellular: None,
        }
    }

    pub fn compile(source: &str) -> Result<Self, String> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(format!(
                "Line 1, column 1: program exceeds {MAX_SOURCE_BYTES} bytes."
            ));
        }
        lex(source)
            .and_then(|tokens| Parser::new(tokens).program())
            .map_err(|error| error.to_string())
    }

    /// Uses exactly the same lexer, parser, and type checker as compile().
    /// Successfully completed let declarations remain available after a later parse error.
    pub fn analyze(source: &str) -> GenomeAnalysis {
        if source.len() > MAX_SOURCE_BYTES {
            return GenomeAnalysis::invalid_input(
                source.len(),
                format!("program exceeds {MAX_SOURCE_BYTES} bytes."),
            );
        }
        let map = SourceMap::new(source);
        let tokens = match lex(source) {
            Ok(tokens) => tokens,
            Err(error) => {
                return GenomeAnalysis {
                    valid: false,
                    diagnostics: vec![map.diagnostic(error)],
                    bindings: Vec::new(),
                    node_count: 0,
                    source_bytes: source.len(),
                }
            }
        };
        let mut parser = Parser::new(tokens);
        let parsed = parser.program();
        let (nodes, declarations) = match &parsed {
            Ok(program) => (&program.nodes, &program.bindings),
            Err(_) => (&parser.nodes, &parser.binding_order),
        };
        let bindings = declarations
            .iter()
            .zip(&parser.binding_locations)
            .map(|(binding, location)| GenomeBinding {
                name: binding.name.clone(),
                value_type: nodes[binding.node].ty.label(),
                from: map.offsets[location.from],
                to: map.offsets[location.to],
            })
            .collect();
        GenomeAnalysis {
            valid: parsed.is_ok(),
            node_count: nodes.len(),
            bindings,
            source_bytes: source.len(),
            diagnostics: parsed
                .err()
                .map(|error| map.diagnostic(error))
                .into_iter()
                .collect(),
        }
    }
    pub fn estimated_bytes(&self) -> usize {
        self.nodes.capacity() * std::mem::size_of::<Node>()
            + self.bindings.capacity() * std::mem::size_of::<Binding>()
            + self.writes.capacity() * std::mem::size_of::<WriteRule>()
            + self
                .bindings
                .iter()
                .map(|b| b.name.capacity())
                .sum::<usize>()
            + self
                .nodes
                .iter()
                .map(|n| match &n.op {
                    Op::Constant(Value::Text(s)) => s.len() + 2 * std::mem::size_of::<usize>(),
                    _ => 0,
                })
                .sum::<usize>()
            + self.execution.allocated_bytes
            + self
                .cellular
                .as_ref()
                .map_or(0, |cellular| cellular.allocated_bytes())
    }

    /// Runtime-only storage, including the pointer added to Program itself.
    /// Checkpoint budgets retain the authored allocation estimate, while the
    /// live registry charges both that estimate and the derived plan.
    pub(crate) fn derived_execution_bytes(&self) -> usize {
        std::mem::size_of::<Box<ExecutionPlan>>() + self.execution.allocated_bytes
            // Keep the old authored byte budgets stable for legacy saves.
            + std::mem::size_of::<Option<Box<cellular::CellularGraph>>>()
    }

    /// Describe the compiler's graph without evaluating it or changing memory.
    /// Register reads are old-state leaves; write and clear roots describe the
    /// next-state boundary rather than introducing same-tick feedback edges.
    pub fn graph_json(&self) -> String {
        let reachable = self.reachable();
        let mut bindings = vec![Vec::<&str>::new(); self.nodes.len()];
        for binding in &self.bindings {
            bindings[binding.node].push(&binding.name);
        }
        let mut json = format!(
            "{{\"version\":1,\"mode\":{},\"nodes\":[",
            quote(self.mode())
        );
        for (id, node) in self.nodes.iter().enumerate() {
            if id > 0 {
                json.push(',');
            }
            let (kind, label) = match &node.op {
                Op::Constant(value) => ("constant", value.source()),
                Op::Sensor(sensor) => ("sensor", sensor.name().to_owned()),
                Op::Pool(index) => (
                    "sensor",
                    format!("pool.{}", self.cellular().unwrap().products[*index].name),
                ),
                Op::Concentration(index) => (
                    "sensor",
                    format!(
                        "concentration.{}",
                        self.cellular().unwrap().products[*index].name
                    ),
                ),
                Op::Built(index) => (
                    "sensor",
                    format!("built.{}", self.cellular().unwrap().assemblies[*index].name),
                ),
                Op::Receptor(index, field) => (
                    "sensor",
                    format!(
                        "sensed.{}.{}",
                        self.cellular().unwrap().assemblies[*index].name,
                        field.name()
                    ),
                ),
                Op::Register(register) => ("register", register.name()),
                Op::Populated(register) => ("populated", format!("populated({})", register.name())),
                Op::Reference(target) => (
                    "reference",
                    bindings[*target]
                        .first()
                        .copied()
                        .unwrap_or("reference")
                        .to_owned(),
                ),
                Op::Negate(_) => ("unary", "-".to_owned()),
                Op::Not(_) => ("unary", "!".to_owned()),
                Op::Binary(operator, _, _) => ("binary", operator.symbol().to_owned()),
                Op::Choose(_, _, _) => ("conditional", "if".to_owned()),
                Op::Call(function, _) => ("call", function.name().to_owned()),
            };
            let (children, count) = node.op.children();
            let inputs = children[..count]
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let names = bindings[id]
                .iter()
                .map(|name| quote(name))
                .collect::<Vec<_>>()
                .join(",");
            json.push_str(&format!(
                "{{\"id\":{id},\"kind\":{},\"type\":{},\"label\":{},\"inputs\":[{inputs}],\"bindings\":[{names}],\"reachable\":{}}}",
                quote(kind), quote(node.ty.label()), quote(&label), reachable[id],
            ));
        }
        json.push_str("],\"roots\":[");
        for (index, value) in self
            .output_indices()
            .iter()
            .enumerate()
            .filter(|_| self.cellular.is_none())
        {
            if index > 0 {
                json.push(',');
            }
            let name = OUTPUT_NAMES[index];
            let ty = output_type(index);
            let value_json = value.map_or_else(|| "null".into(), |value| value.to_string());
            let default_json = if value.is_some() {
                "null".into()
            } else {
                quote(&output_default(index).json())
            };
            json.push_str(&format!(
                "{{\"id\":{},\"kind\":\"output\",\"name\":{},\"type\":{},\"value\":{value_json},\"guard\":null,\"default\":{default_json}}}",
                quote(&format!("output:{name}")), quote(name), quote(ty.label()),
            ));
        }
        if let Some(cellular) = &self.cellular {
            json.push_str(&cellular.graph_roots_json());
        }
        for rule in &self.writes {
            let name = rule.register.name();
            let kind = if rule.value.is_some() {
                "write"
            } else {
                "clear"
            };
            let value = rule
                .value
                .map_or_else(|| "null".into(), |value| value.to_string());
            let guard = rule
                .guard
                .map_or_else(|| "null".into(), |value| value.to_string());
            json.push_str(&format!(
                ",{{\"id\":{},\"kind\":{},\"name\":{},\"type\":{},\"value\":{value},\"guard\":{guard},\"default\":null}}",
                quote(&format!("{kind}:{name}")), quote(kind), quote(&name), quote(rule.register.ty.label()),
            ));
        }
        json.push_str("]}");
        json
    }

    pub fn evaluate(&self, inputs: &Inputs) -> Outputs {
        self.evaluate_cell(
            inputs,
            &mut Registers::default(),
            &mut EvalScratch::default(),
        )
    }
    pub fn evaluate_with_scratch(&self, inputs: &Inputs, values: &mut Vec<Value>) -> Outputs {
        self.execution
            .evaluate(inputs, &Registers::default(), values);
        self.evaluated_outputs(values)
    }
    pub fn evaluate_cell(
        &self,
        inputs: &Inputs,
        memory: &mut Registers,
        scratch: &mut EvalScratch,
    ) -> Outputs {
        let mut result = Outputs::default();
        self.evaluate_cell_into(inputs, memory, scratch, &mut result);
        result
    }

    /// Reuse nested cellular control buffers while retaining simultaneous
    /// observation and old-register semantics.
    pub fn evaluate_cell_into(
        &self,
        inputs: &Inputs,
        memory: &mut Registers,
        scratch: &mut EvalScratch,
        result: &mut Outputs,
    ) {
        self.execution.evaluate(inputs, memory, &mut scratch.values);
        self.evaluated_outputs_into(&scratch.values, result);
        Self::commit_writes(&self.execution.writes, memory, &scratch.values);
    }

    fn commit_writes(writes: &[WriteRule], memory: &mut Registers, values: &[Value]) {
        // All RHSs and guards have already evaluated against the same old state.
        // The two passes deliberately make clear win regardless of source order.
        for rule in writes {
            if let Some(value) = rule.value {
                if rule.guard.is_none_or(|g| values[g].boolean()) {
                    memory.entries.insert(
                        rule.register,
                        values[value].clone().convert(rule.register.ty),
                    );
                }
            }
        }
        for rule in writes {
            if rule.value.is_none() && rule.guard.is_none_or(|g| values[g].boolean()) {
                memory.entries.remove(&rule.register);
            }
        }
    }

    /// The full authored evaluator is retained for source inspection and as the
    /// semantic reference for execution-plan differential tests.
    fn evaluate_nodes(&self, inputs: &Inputs, memory: &Registers, values: &mut Vec<Value>) {
        values.clear();
        // reserve() is relative to len (now zero), not to the existing capacity.
        values.reserve(self.nodes.len());
        for node in &self.nodes {
            values.push(evaluate_operation(
                &node.op, node.ty, inputs, memory, values,
            ));
        }
    }
    fn output_value(&self, index: usize, values: &[Value]) -> Value {
        Self::output_value_for(&self.output_roots, index, values)
    }
    fn output_value_for(
        roots: &[Option<usize>; OUTPUT_COUNT],
        index: usize,
        values: &[Value],
    ) -> Value {
        roots[index].map_or_else(
            || output_default(index),
            |root| clamp_output(index, values[root].clone()),
        )
    }
    fn outputs_for(roots: &[Option<usize>; OUTPUT_COUNT], values: &[Value]) -> Outputs {
        let output = |index| Self::output_value_for(roots, index, values);
        let number = |index| output(index).number();
        let boolean = |index| output(index).boolean();
        let mut body = BodyControls::default();
        for family in 0..FAMILY_COUNT {
            let start = BODY_OUTPUT_START + family * 4;
            let Value::Integer(count) = output(start) else {
                unreachable!()
            };
            body.target_counts[family] = count as u32;
            body.target_sizes[family] = number(start + 1);
            body.build[family] = number(start + 2);
            body.recycle[family] = number(start + 3);
        }
        let Value::Text(scan_target) = output(SCAN_OUTPUT_START + 2) else {
            unreachable!()
        };
        Outputs {
            thrust: number(0),
            turn: number(1),
            divide: boolean(2),
            grow: number(3),
            photosynthesize: number(4),
            engulf: boolean(5),
            bite: number(6),
            defend: number(7),
            repair: number(8),
            absorb: number(9),
            feed: number(10),
            digest: std::array::from_fn(|i| number(DIGEST_OUTPUT_START + i)),
            body,
            scan_bearing: number(SCAN_OUTPUT_START),
            scan_width: number(SCAN_OUTPUT_START + 1),
            scan_target,
            cellular: None,
        }
    }
    fn output_indices(&self) -> [Option<usize>; OUTPUT_COUNT] {
        self.output_roots
    }
    fn reachable(&self) -> Vec<bool> {
        let mut reached = vec![false; self.nodes.len()];
        let mut stack: Vec<_> = self.output_indices().into_iter().flatten().collect();
        if let Some(cellular) = &self.cellular {
            stack.extend(cellular.roots());
        }
        for w in &self.writes {
            stack.extend(w.value);
            stack.extend(w.guard);
        }
        while let Some(i) = stack.pop() {
            if reached[i] {
                continue;
            }
            reached[i] = true;
            let (children, count) = self.nodes[i].op.children();
            stack.extend_from_slice(&children[..count]);
        }
        reached
    }
    fn reference_root(&self, mut index: usize) -> usize {
        while let Op::Reference(target) = self.nodes[index].op {
            debug_assert!(target < index);
            index = target;
        }
        index
    }
}

struct MutationRng(u64);
impl MutationRng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 { 0x9e3779b97f4a7c15 } else { seed })
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn index(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}
impl Program {
    fn reference_options(&self, index: usize) -> Vec<Op> {
        let node = &self.nodes[index];
        let current = self.reference_root(index);
        let mut options = Vec::new();
        for s in Sensor::ALL {
            if s.ty() == node.ty
                && self.nodes[current].op != Op::Sensor(s)
                && (self.cellular.is_none() || s.cellular_allowed())
            {
                options.push(Op::Sensor(s));
            }
        }
        for id in 1..=if self.cellular.is_some() {
            0
        } else {
            REGISTER_COUNT
        } {
            let op = Op::Register(Register {
                ty: node.ty,
                index: id as u16,
            });
            if self.nodes[current].op != op {
                options.push(op);
            }
        }
        if node.ty == Type::Number {
            if let Some(cellular) = self.cellular() {
                for op in (0..cellular.products.len())
                    .map(Op::Pool)
                    .chain((0..cellular.products.len()).map(Op::Concentration))
                    .chain((0..cellular.assemblies.len()).map(Op::Built))
                {
                    if self.nodes[current].op != op {
                        options.push(op);
                    }
                }
            }
        }
        if let Some(cellular) = self.cellular() {
            for (index, recipe) in cellular.assemblies.iter().enumerate() {
                for &field in ReceptorField::for_kind(recipe.kind) {
                    let op = Op::Receptor(index, field);
                    if field.ty() == node.ty && self.nodes[current].op != op {
                        options.push(op);
                    }
                }
            }
        }
        if let Op::Populated(key) = node.op {
            for id in 1..=REGISTER_COUNT {
                if id != key.index as usize {
                    options.push(Op::Populated(Register {
                        ty: key.ty,
                        index: id as u16,
                    }));
                }
            }
        }
        for b in &self.bindings[..node.scope] {
            if b.node < index
                && self.nodes[b.node].ty == node.ty
                && self.reference_root(b.node) != current
            {
                options.push(Op::Reference(b.node));
            }
        }
        options
    }
    #[cfg(test)]
    fn to_source(&self) -> Option<String> {
        if self.cellular.is_some() {
            return None;
        }
        if self.output_roots[3].is_none() {
            let mut p = self.clone();
            p.output_roots[3] = Some(p.nodes.len());
            p.nodes.push(Node {
                op: Op::Constant(Value::Scalar(0.0)),
                ty: Type::Number,
                scope: p.bindings.len(),
            });
            return p.to_source();
        }
        let reached = self.reachable();
        let mut names: Vec<Option<String>> = vec![None; self.nodes.len()];
        let mut used: HashSet<String> = self.bindings.iter().map(|b| b.name.clone()).collect();
        for b in &self.bindings {
            let target = self.reference_root(b.node);
            if reached[target] && names[target].is_none() {
                names[target] = Some(b.name.clone());
            }
        }
        let mut serial = 0;
        for (i, n) in self.nodes.iter().enumerate() {
            if !reached[i] || matches!(n.op, Op::Reference(_)) {
                continue;
            }
            if names[i].is_none() {
                let mut name = format!("_g{serial}");
                while used.contains(&name) {
                    name.push('_');
                }
                used.insert(name.clone());
                names[i] = Some(name);
            }
            serial += 1;
        }
        let name = |i| names[self.reference_root(i)].as_deref().unwrap();
        let mut source = String::new();
        let mut tokens = 0;
        let mut compiled_nodes = 0;
        for (i, n) in self.nodes.iter().enumerate() {
            if !reached[i] || matches!(n.op, Op::Reference(_)) {
                continue;
            }
            let (expression, et, en) = match &n.op {
                Op::Constant(v) => {
                    let text = v.source();
                    let tokens = lex(&text).ok()?.len() - 1;
                    let nodes = match v {
                        Value::Vector(a) => {
                            3 + usize::from(a[0].is_sign_negative())
                                + usize::from(a[1].is_sign_negative())
                        }
                        Value::Integer(a) => 1 + usize::from(*a < 0),
                        Value::Scalar(a) => 1 + usize::from(a.is_sign_negative()),
                        _ => 1,
                    };
                    (text, tokens, nodes)
                }
                Op::Sensor(s) => (s.name().to_owned(), 1, 1),
                Op::Pool(_) | Op::Concentration(_) | Op::Built(_) | Op::Receptor(_, _) => {
                    unreachable!()
                }
                Op::Register(r) => (r.name(), 1, 1),
                Op::Populated(r) => (format!("populated({})", r.name()), 4, 1),
                Op::Negate(a) => (format!("-{}", name(*a)), 2, 2),
                Op::Not(a) => (format!("!{}", name(*a)), 2, 2),
                Op::Binary(op, a, b) => {
                    (format!("{} {} {}", name(*a), op.symbol(), name(*b)), 3, 3)
                }
                Op::Choose(c, a, b) => (
                    format!("if {} then {} else {}", name(*c), name(*a), name(*b)),
                    6,
                    4,
                ),
                Op::Call(f, args) => {
                    let arguments = args[..f.arity()]
                        .iter()
                        .map(|&a| name(a))
                        .collect::<Vec<_>>()
                        .join(", ");
                    (
                        format!("{}({arguments})", f.name()),
                        2 * f.arity() + 2,
                        f.arity() + 1,
                    )
                }
                Op::Reference(_) => unreachable!(),
            };
            tokens += 4 + et;
            compiled_nodes += en;
            if tokens > MAX_TOKENS || compiled_nodes > MAX_NODES {
                return None;
            }
            source.push_str(&format!("let {} = {expression};\n", name(i)));
            if source.len() > MAX_SOURCE_BYTES {
                return None;
            }
        }
        for (i, root) in self.output_indices().iter().enumerate() {
            if let Some(root) = root {
                tokens += 4;
                compiled_nodes += 1;
                source.push_str(&format!("{} = {};\n", OUTPUT_NAMES[i], name(*root)));
            }
        }
        for rule in &self.writes {
            match rule.value {
                Some(i) => {
                    tokens += 4;
                    compiled_nodes += 1;
                    source.push_str(&format!("{} = {}", rule.register.name(), name(i)));
                }
                None => {
                    tokens += 3;
                    source.push_str(&format!("clear {}", rule.register.name()));
                }
            }
            if let Some(g) = rule.guard {
                tokens += 2;
                compiled_nodes += 1;
                source.push_str(&format!(" when {}", name(g)));
            }
            source.push_str(";\n");
        }
        if source.len() > MAX_SOURCE_BYTES || tokens > MAX_TOKENS || compiled_nodes > MAX_NODES {
            return None;
        }
        Some(source)
    }
}
fn mutate_constant(value: &Value, rng: &mut MutationRng) -> Value {
    let fraction = 0.05 + (rng.next() % 4501) as f64 / 10000.0;
    let sign = if rng.next() & 1 == 0 { -1.0 } else { 1.0 };
    let perturb = |v: f64| {
        let changed = finite(v + sign * v.abs().max(0.2) * fraction);
        if changed == v {
            v * 0.8
        } else {
            changed
        }
    };
    match value {
        Value::Boolean(v) => Value::Boolean(!v),
        Value::Scalar(v) => Value::Scalar(perturb(*v)),
        Value::Integer(v) => {
            let step = ((*v as f64).abs() * fraction).round().max(1.0) as i64;
            let next = (*v as i64 + if sign < 0.0 { -step } else { step })
                .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
            Value::Integer(if next == *v {
                v.saturating_add(if *v < 0 { 1 } else { -1 })
            } else {
                next
            })
        }
        Value::Vector(v) => {
            let mut a = *v;
            let i = rng.index(2);
            a[i] = perturb(a[i]);
            Value::Vector(a)
        }
        Value::Text(s) => {
            let mut text = s.to_string();
            if text.len() < MAX_STRING_BYTES {
                text.push(if rng.next() & 1 == 0 { 'a' } else { 'b' });
            } else {
                let first = text.chars().next().unwrap();
                text.replace_range(..first.len_utf8(), if first == 'a' { "b" } else { "a" });
            }
            Value::Text(bounded_text(text))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Number(f64),
    Integer(i64),
    Text(Rc<str>),
    Name(String),
    Plus,
    Minus,
    Star,
    Slash,
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    Colon,
    Comma,
    Semicolon,
    Assign,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
    And,
    Or,
    Not,
    End,
}
#[derive(Debug, Clone, Copy)]
struct Location {
    line: usize,
    column: usize,
    from: usize,
    to: usize,
}
impl Location {
    fn error(self, message: impl AsRef<str>) -> CompileError {
        CompileError {
            location: self,
            message: message.as_ref().to_owned(),
        }
    }
    fn through(self, to: usize) -> Self {
        Self {
            to: to.max(self.to),
            ..self
        }
    }
}
#[derive(Debug)]
struct CompileError {
    location: Location,
    message: String,
}
impl std::fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Line {}, column {}: {}",
            self.location.line, self.location.column, self.message
        )
    }
}
#[derive(Debug, Clone)]
struct Token {
    kind: Kind,
    location: Location,
}

fn lex(source: &str) -> Result<Vec<Token>, CompileError> {
    let bytes = source.as_bytes();
    let (mut position, mut line, mut column) = (0, 1, 1);
    let mut tokens = Vec::new();
    while position < bytes.len() {
        let byte = bytes[position];
        if byte == b'\n' {
            line += 1;
            column = 1;
            position += 1;
            continue;
        }
        if byte.is_ascii_whitespace() {
            position += 1;
            column += 1;
            continue;
        }
        if byte == b'#' {
            while position < bytes.len() && bytes[position] != b'\n' {
                position += 1;
                column += 1;
            }
            continue;
        }
        let location = Location {
            line,
            column,
            from: position,
            to: position + source[position..].chars().next().unwrap().len_utf8(),
        };
        if tokens.len() >= MAX_TOKENS {
            return Err(location.error(format!("program exceeds {MAX_TOKENS} tokens.")));
        }
        let start = position;
        let kind = if byte.is_ascii_digit()
            || (byte == b'.' && bytes.get(position + 1).is_some_and(u8::is_ascii_digit))
        {
            while bytes.get(position).is_some_and(u8::is_ascii_digit) {
                position += 1;
            }
            let mut integral = true;
            if bytes.get(position) == Some(&b'.') {
                integral = false;
                position += 1;
                while bytes.get(position).is_some_and(u8::is_ascii_digit) {
                    position += 1;
                }
            }
            if matches!(bytes.get(position), Some(b'e' | b'E')) {
                integral = false;
                position += 1;
                if matches!(bytes.get(position), Some(b'+' | b'-')) {
                    position += 1;
                }
                let exponent = position;
                while bytes.get(position).is_some_and(u8::is_ascii_digit) {
                    position += 1;
                }
                if exponent == position {
                    return Err(location
                        .through(position)
                        .error("expected digits in the exponent."));
                }
            }
            let spelling = &source[start..position];
            let value = spelling
                .parse::<f64>()
                .map_err(|_| location.through(position).error("invalid number."))?;
            if !value.is_finite() {
                return Err(location.through(position).error("number must be finite."));
            }
            if integral {
                spelling
                    .parse::<i64>()
                    .map_or(Kind::Number(value), Kind::Integer)
            } else {
                Kind::Number(value)
            }
        } else if byte == b'"' {
            position += 1;
            let mut value = String::new();
            let mut closed = false;
            while position < bytes.len() {
                if bytes[position] == b'"' {
                    position += 1;
                    closed = true;
                    break;
                }
                if bytes[position] == b'\\' {
                    position += 1;
                    let escaped = *bytes.get(position).ok_or_else(|| {
                        location
                            .through(position)
                            .error("unterminated string escape.")
                    })?;
                    position += 1;
                    match escaped {
                        b'"' => value.push('"'),
                        b'\\' => value.push('\\'),
                        b'n' => value.push('\n'),
                        b'r' => value.push('\r'),
                        b't' => value.push('\t'),
                        b'u' => {
                            let end = position + 4;
                            if end > bytes.len()
                                || !bytes[position..end].iter().all(u8::is_ascii_hexdigit)
                            {
                                return Err(location
                                    .through(position)
                                    .error("a Unicode escape needs four hexadecimal digits."));
                            }
                            let scalar = u32::from_str_radix(&source[position..end], 16).unwrap();
                            let character = char::from_u32(scalar).ok_or_else(|| {
                                location
                                    .through(end)
                                    .error("Unicode escape is not a valid character.")
                            })?;
                            value.push(character);
                            position = end;
                        }
                        _ => {
                            return Err(location
                                .through(
                                    position - 1
                                        + source[position - 1..].chars().next().unwrap().len_utf8(),
                                )
                                .error("unsupported string escape."))
                        }
                    }
                } else {
                    let c = source[position..].chars().next().unwrap();
                    if c.is_control() {
                        return Err(location.through(position + c.len_utf8()).error(
                            "literal control characters are not allowed in strings; use an escape.",
                        ));
                    }
                    value.push(c);
                    position += c.len_utf8();
                }
                if value.len() > MAX_STRING_BYTES {
                    return Err(location.through(position).error(format!(
                        "strings are limited to {MAX_STRING_BYTES} UTF-8 bytes."
                    )));
                }
            }
            if !closed {
                return Err(location
                    .through(position)
                    .error("unterminated string literal."));
            }
            Kind::Text(Rc::from(value))
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            position += 1;
            while bytes
                .get(position)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'.')
            {
                position += 1;
            }
            Kind::Name(source[start..position].to_owned())
        } else {
            position += 1;
            match byte {
                b'+' => Kind::Plus,
                b'-' => Kind::Minus,
                b'*' => Kind::Star,
                b'/' => Kind::Slash,
                b'(' => Kind::LeftParen,
                b')' => Kind::RightParen,
                b'{' => Kind::LeftBrace,
                b'}' => Kind::RightBrace,
                b':' => Kind::Colon,
                b',' => Kind::Comma,
                b';' => Kind::Semicolon,
                b'=' if bytes.get(position) == Some(&b'=') => {
                    position += 1;
                    Kind::Equal
                }
                b'=' => Kind::Assign,
                b'!' if bytes.get(position) == Some(&b'=') => {
                    position += 1;
                    Kind::NotEqual
                }
                b'!' => Kind::Not,
                b'<' if bytes.get(position) == Some(&b'=') => {
                    position += 1;
                    Kind::LessEqual
                }
                b'<' => Kind::Less,
                b'>' if bytes.get(position) == Some(&b'=') => {
                    position += 1;
                    Kind::GreaterEqual
                }
                b'>' => Kind::Greater,
                b'&' if bytes.get(position) == Some(&b'&') => {
                    position += 1;
                    Kind::And
                }
                b'|' if bytes.get(position) == Some(&b'|') => {
                    position += 1;
                    Kind::Or
                }
                _ => {
                    return Err(location.error(format!(
                        "unexpected character '{}'.",
                        source[start..].chars().next().unwrap_or('?')
                    )))
                }
            }
        };
        column += source[start..position].chars().count();
        tokens.push(Token {
            kind,
            location: location.through(position),
        });
    }
    tokens.push(Token {
        kind: Kind::End,
        location: Location {
            line,
            column,
            from: position,
            to: position,
        },
    });
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
    nodes: Vec<Node>,
    bindings: HashMap<String, usize>,
    binding_order: Vec<Binding>,
    binding_locations: Vec<Location>,
    node_locations: Vec<source::NodeLocation>,
    write_locations: Vec<Location>,
    cellular_mode: bool,
    product_names: HashMap<String, usize>,
    assembly_names: HashMap<String, usize>,
    gene_locations: Vec<cellular::GeneLocation>,
    geometry_locations: Vec<cellular::GeometryLocation>,
    receptor_references: Vec<(usize, ReceptorField, Location)>,
    functions: HashMap<String, functions::UserFunction>,
    function_scope: Option<usize>,
    function_depth: usize,
}
impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
            nodes: Vec::new(),
            bindings: HashMap::new(),
            binding_order: Vec::new(),
            binding_locations: Vec::new(),
            node_locations: Vec::new(),
            write_locations: Vec::new(),
            cellular_mode: false,
            product_names: HashMap::new(),
            assembly_names: HashMap::new(),
            gene_locations: Vec::new(),
            geometry_locations: Vec::new(),
            receptor_references: Vec::new(),
            functions: HashMap::new(),
            function_scope: None,
            function_depth: 0,
        }
    }
    fn peek(&self) -> &Token {
        &self.tokens[self.position]
    }
    fn take(&mut self) -> Token {
        let token = self.peek().clone();
        if token.kind != Kind::End {
            self.position += 1;
        }
        token
    }
    fn expect(&mut self, kind: Kind, description: &str) -> Result<(), CompileError> {
        let token = self.take();
        if token.kind == kind {
            Ok(())
        } else {
            Err(token.location.error(format!("expected {description}.")))
        }
    }
    fn keyword(&mut self, name: &str) -> Result<(), CompileError> {
        self.expect(Kind::Name(name.to_owned()), &format!("'{name}'"))
    }
    fn add(&mut self, op: Op, ty: Type, location: Location) -> Result<usize, CompileError> {
        if self.nodes.len() >= MAX_NODES {
            return Err(location.error(format!("program exceeds {MAX_NODES} expression nodes.")));
        }
        let index = self.nodes.len();
        self.nodes.push(Node {
            op,
            ty,
            scope: self.function_scope.unwrap_or(self.binding_order.len()),
        });
        let end = self.tokens[self.position.saturating_sub(1)]
            .location
            .to
            .max(location.to);
        self.node_locations.push(source::NodeLocation {
            from: location.from,
            to: end,
            head: location,
            authored: true,
        });
        Ok(index)
    }
    fn require(
        &self,
        index: usize,
        expected: Type,
        location: Location,
    ) -> Result<(), CompileError> {
        let actual = self.nodes[index].ty;
        if actual == expected {
            Ok(())
        } else {
            Err(location.error(format!(
                "expected {}, found {}.",
                expected.description(),
                actual.description()
            )))
        }
    }
    fn assignable(
        &self,
        index: usize,
        expected: Type,
        location: Location,
    ) -> Result<(), CompileError> {
        if expected == Type::Number && self.nodes[index].ty == Type::Integer {
            Ok(())
        } else {
            self.require(index, expected, location)
        }
    }
    fn guard(&mut self) -> Result<Option<usize>, CompileError> {
        if self.peek().kind == Kind::Name("when".to_owned()) {
            let location = self.take().location;
            let guard = self.expression(0, 0)?;
            self.require(guard, Type::Bool, location)?;
            Ok(Some(guard))
        } else {
            Ok(None)
        }
    }
    fn register(name: &str, location: Location) -> Result<Register, CompileError> {
        Register::parse(name).ok_or_else(|| {
            location.error(format!(
                "invalid register '{name}'; use ri/rf/rb/rs/rv followed by 001 through 999."
            ))
        })
    }
    fn program(&mut self) -> Result<Program, CompileError> {
        if self.peek().kind == Kind::Name("cellular".to_string()) {
            return self.cellular_program();
        }
        let mut outputs = [None; OUTPUT_NAMES.len()];
        let mut writes = Vec::new();
        let (mut assigned, mut cleared) = (HashSet::new(), HashSet::new());
        while self.peek().kind != Kind::End {
            let mut token = self.take();
            if token.kind == Kind::Name("clear".to_owned()) {
                token = self.take();
                let Kind::Name(name) = token.kind else {
                    return Err(token.location.error("expected a register after 'clear'."));
                };
                let register = Self::register(&name, token.location)?;
                if !cleared.insert(register) {
                    return Err(token
                        .location
                        .error(format!("register '{name}' already has a clear rule.")));
                }
                let guard = self.guard()?;
                self.expect(Kind::Semicolon, "';' after clear")?;
                self.write_locations.push(token.location);
                writes.push(WriteRule {
                    register,
                    value: None,
                    guard,
                });
                continue;
            }
            let is_binding = token.kind == Kind::Name("let".to_owned());
            if is_binding {
                token = self.take();
            }
            let Kind::Name(name) = token.kind else {
                return Err(token
                    .location
                    .error("expected 'let', a register write, 'clear', or an output name."));
            };
            let output = output_index(&name);
            let register = Register::parse(&name);
            if is_binding {
                if output.is_some()
                    || Sensor::lookup(&name).is_some()
                    || Function::lookup(&name).is_some()
                    || Register::reserved(&name)
                    || name.contains('.')
                    || matches!(
                        name.as_str(),
                        "if" | "then"
                            | "else"
                            | "let"
                            | "true"
                            | "false"
                            | "when"
                            | "clear"
                            | "populated"
                            | "food"
                            | "neighbor"
                            | "prey"
                            | "threat"
                            | "flow"
                            | "light"
                            | "memory"
                            | "temperature"
                    )
                {
                    return Err(token
                        .location
                        .error(format!("'{name}' is reserved; choose another signal name.")));
                }
                if self.bindings.contains_key(&name) {
                    return Err(token.location.error(format!(
                        "signal '{name}' is already defined; bindings are immutable."
                    )));
                }
            } else if let Some(output) = output {
                if outputs[output].is_some() {
                    return Err(token
                        .location
                        .error(format!("output '{name}' is already defined.")));
                }
            } else if let Some(register) = register {
                if !assigned.insert(register) {
                    return Err(token
                        .location
                        .error(format!("register '{name}' already has a write rule.")));
                }
            } else if Register::reserved(&name) {
                return Err(Self::register(&name, token.location).unwrap_err());
            } else {
                return Err(token.location.error(format!(
                    "unknown output '{name}'; use 'let {name} = ...;' for a signal."
                )));
            }
            self.expect(Kind::Assign, "'='")?;
            let value = self.expression(0, 0)?;
            let guard = if !is_binding && register.is_some() {
                self.guard()?
            } else {
                None
            };
            self.expect(Kind::Semicolon, "';' after the expression")?;
            if is_binding {
                self.binding_order.push(Binding {
                    name: name.clone(),
                    node: value,
                });
                self.binding_locations.push(token.location);
                self.bindings.insert(name, value);
            } else if let Some(register) = register {
                self.assignable(value, register.ty, token.location)?;
                self.write_locations.push(token.location);
                writes.push(WriteRule {
                    register,
                    value: Some(value),
                    guard,
                });
            } else {
                let output = output.unwrap();
                self.assignable(value, output_type(output), token.location)?;
                outputs[output] = Some(value);
            }
        }
        for (index, name) in OUTPUT_NAMES[..3].iter().enumerate() {
            if outputs[index].is_none() {
                return Err(self
                    .peek()
                    .location
                    .error(format!("missing required output '{name}'.")));
            }
        }
        Ok(Program::from_parts(
            std::mem::take(&mut self.nodes),
            std::mem::take(&mut self.binding_order),
            outputs,
            writes,
        ))
    }
    fn expression(&mut self, minimum: u8, depth: usize) -> Result<usize, CompileError> {
        if depth >= MAX_DEPTH {
            return Err(self
                .peek()
                .location
                .error(format!("expression nesting exceeds {MAX_DEPTH} levels.")));
        }
        let token = self.take();
        let location = token.location;
        let mut left = match token.kind {
            Kind::Number(value) => self.add(
                Op::Constant(Value::Scalar(finite(value))),
                Type::Number,
                location,
            )?,
            Kind::Integer(value) => {
                if let Ok(integer) = i32::try_from(value) {
                    self.add(
                        Op::Constant(Value::Integer(integer)),
                        Type::Integer,
                        location,
                    )?
                } else {
                    self.add(
                        Op::Constant(Value::Scalar(finite(value as f64))),
                        Type::Number,
                        location,
                    )?
                }
            }
            Kind::Text(value) => {
                self.add(Op::Constant(Value::Text(value)), Type::String, location)?
            }
            Kind::Minus => {
                if self.peek().kind == Kind::Integer(2_147_483_648) {
                    self.take();
                    self.add(
                        Op::Constant(Value::Integer(i32::MIN)),
                        Type::Integer,
                        location,
                    )?
                } else {
                    let child = self.expression(7, depth + 1)?;
                    let ty = self.nodes[child].ty;
                    if !ty.numeric() && ty != Type::Vector {
                        return Err(location.error("unary '-' needs a number or vector."));
                    }
                    self.add(Op::Negate(child), ty, location)?
                }
            }
            Kind::Plus => {
                let child = self.expression(7, depth + 1)?;
                if !self.nodes[child].ty.numeric() {
                    return Err(location.error("unary '+' needs a number."));
                }
                child
            }
            Kind::Not => {
                let child = self.expression(7, depth + 1)?;
                self.require(child, Type::Bool, location)?;
                self.add(Op::Not(child), Type::Bool, location)?
            }
            Kind::LeftParen => {
                let child = self.expression(0, depth + 1)?;
                self.expect(Kind::RightParen, "')'")?;
                child
            }
            Kind::Name(name) if name == "true" || name == "false" => self.add(
                Op::Constant(Value::Boolean(name == "true")),
                Type::Bool,
                location,
            )?,
            Kind::Name(name) if name == "if" => {
                let condition = self.expression(0, depth + 1)?;
                self.require(condition, Type::Bool, location)?;
                self.keyword("then")?;
                let yes = self.expression(0, depth + 1)?;
                self.keyword("else")?;
                let no = self.expression(0, depth + 1)?;
                let (a, b) = (self.nodes[yes].ty, self.nodes[no].ty);
                let ty = if a == b {
                    a
                } else {
                    numeric_type(&[a, b]).ok_or_else(|| {
                        location.error("'if' branches must have the same type, or both be numeric.")
                    })?
                };
                self.add(Op::Choose(condition, yes, no), ty, location)?
            }
            Kind::Name(name) => {
                if self.peek().kind == Kind::LeftParen {
                    self.call(&name, location, depth + 1)?
                } else if self.cellular_mode && name.starts_with("pool.") {
                    let index = self.product_names.get(&name[5..]).copied().ok_or_else(|| {
                        location.error(format!("unknown product stock '{name}'."))
                    })?;
                    self.add(Op::Pool(index), Type::Number, location)?
                } else if self.cellular_mode && name.starts_with("concentration.") {
                    let index = self
                        .product_names
                        .get(&name[14..])
                        .copied()
                        .ok_or_else(|| {
                            location.error(format!("unknown product concentration '{name}'."))
                        })?;
                    self.add(Op::Concentration(index), Type::Number, location)?
                } else if self.cellular_mode && name.starts_with("sensed.") {
                    let (recipe, field) = name[7..].split_once('.').ok_or_else(|| {
                        location.error("named receptor reads use sensed.Recipe.field.")
                    })?;
                    let index = self.assembly_names.get(recipe).copied().ok_or_else(|| {
                        location.error(format!("unknown receptor assembly '{recipe}'."))
                    })?;
                    let field = ReceptorField::lookup(field).ok_or_else(|| {
                        location.error(format!("unknown receptor measurement '{name}'."))
                    })?;
                    self.receptor_references.push((index, field, location));
                    self.add(Op::Receptor(index, field), field.ty(), location)?
                } else if self.cellular_mode && name.starts_with("built.") {
                    let index = self
                        .assembly_names
                        .get(&name[6..])
                        .copied()
                        .ok_or_else(|| {
                            location.error(format!("unknown installed assembly '{name}'."))
                        })?;
                    self.add(Op::Built(index), Type::Number, location)?
                } else if let Some(register) = Register::parse(&name) {
                    if self.cellular_mode {
                        return Err(location.error("cellular genes use product stocks for persistent state; registers are unavailable."));
                    }
                    self.add(Op::Register(register), register.ty, location)?
                } else if Register::reserved(&name) {
                    return Err(Self::register(&name, location).unwrap_err());
                } else if let Some(&target) = self.bindings.get(&name) {
                    self.add(Op::Reference(target), self.nodes[target].ty, location)?
                } else if let Some((sensor, ty)) = Sensor::lookup(&name) {
                    if self.cellular_mode && !sensor.cellular_allowed() {
                        return Err(location.error(format!("'{name}' is not a cellular measurement; read named pool.* or built.* stocks and local sensors.")));
                    }
                    self.add(Op::Sensor(sensor), ty, location)?
                } else {
                    return Err(location.error(format!(
                        "unknown signal '{name}'. Define signals with 'let' before using them."
                    )));
                }
            }
            _ => {
                return Err(location
                    .error("expected a literal, signal, register, function, or 'if' expression."))
            }
        };
        loop {
            let (operation, precedence) = match self.peek().kind {
                Kind::Or => (Binary::Or, 1),
                Kind::And => (Binary::And, 2),
                Kind::Equal => (Binary::Equal, 3),
                Kind::NotEqual => (Binary::NotEqual, 3),
                Kind::Less => (Binary::Less, 4),
                Kind::LessEqual => (Binary::LessEqual, 4),
                Kind::Greater => (Binary::Greater, 4),
                Kind::GreaterEqual => (Binary::GreaterEqual, 4),
                Kind::Plus => (Binary::Add, 5),
                Kind::Minus => (Binary::Subtract, 5),
                Kind::Star => (Binary::Multiply, 6),
                Kind::Slash => (Binary::Divide, 6),
                _ => break,
            };
            if precedence < minimum {
                break;
            }
            let operator_location = self.take().location;
            let right = self.expression(precedence + 1, depth + 1)?;
            let (a, b) = (self.nodes[left].ty, self.nodes[right].ty);
            let ty = binary_type(operation, a, b).ok_or_else(|| {
                operator_location.error(format!(
                    "'{}' cannot combine {} with {}.",
                    operation.symbol(),
                    a.description(),
                    b.description()
                ))
            })?;
            left = self.add(Op::Binary(operation, left, right), ty, operator_location)?;
            self.node_locations[left].from = location.from;
        }
        Ok(left)
    }
    fn call(
        &mut self,
        name: &str,
        location: Location,
        depth: usize,
    ) -> Result<usize, CompileError> {
        if self.functions.contains_key(name) {
            return self.user_call(name, location, depth);
        }
        if name == "populated" {
            if self.cellular_mode {
                return Err(location.error("cellular genes use product stocks for persistent state; populated() is unavailable."));
            }
            self.expect(Kind::LeftParen, "'('")?;
            let token = self.take();
            let Kind::Name(name) = token.kind else {
                return Err(token
                    .location
                    .error("populated() expects a register address."));
            };
            let register = Self::register(&name, token.location)?;
            self.expect(Kind::RightParen, "')' after the register address")?;
            return self.add(Op::Populated(register), Type::Bool, location);
        }
        let (function, arity) = Function::lookup(name)
            .ok_or_else(|| location.error(format!("unknown function '{name}'.")))?;
        self.expect(Kind::LeftParen, "'('")?;
        let mut args = [0; 3];
        for index in 0..arity {
            if index > 0 {
                self.expect(Kind::Comma, "',' between arguments")?;
            }
            if self.peek().kind == Kind::RightParen {
                return Err(location.error(format!("'{name}' expects {arity} argument(s).")));
            }
            args[index] = self.expression(0, depth)?;
        }
        if self.peek().kind == Kind::Comma {
            return Err(location.error(format!("'{name}' expects {arity} argument(s).")));
        }
        self.expect(Kind::RightParen, "')' after function arguments")?;
        let types = args[..arity]
            .iter()
            .map(|&i| self.nodes[i].ty)
            .collect::<Vec<_>>();
        let ty = function.ty(&types).ok_or_else(|| {
            location.error(format!("argument types do not match function '{name}'."))
        })?;
        self.add(Op::Call(function, args), ty, location)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn eval(source: &str, inputs: Inputs) -> Outputs {
        Program::compile(source).unwrap().evaluate(&inputs)
    }

    #[test]
    fn analysis_infers_all_primitive_bindings_and_utf16_declaration_ranges() {
        let source = "# café 🌱\r\nlet flag = chemical.valid;\nlet counter = ri001 + 1;\nlet fuel = energy * 0.5;\nlet label = \"é🌱\"; let drift = vec(x(flow.vector), y(flow.vector));\nlet alias = counter;\nthrust=fuel;turn=x(drift);divide=flag;";
        let analysis = Program::analyze(source);
        assert!(analysis.valid);
        assert!(analysis.diagnostics.is_empty());
        assert_eq!(
            analysis.node_count,
            Program::compile(source).unwrap().nodes.len()
        );
        assert_eq!(analysis.source_bytes, source.len());
        let expected = [
            ("flag", "boolean"),
            ("counter", "integer"),
            ("fuel", "scalar"),
            ("label", "string"),
            ("drift", "vec2"),
            ("alias", "integer"),
        ];
        assert_eq!(analysis.bindings.len(), expected.len());
        for (binding, (name, value_type)) in analysis.bindings.iter().zip(expected) {
            let byte = source.find(&format!("let {name} =")).unwrap() + 4;
            let from = source[..byte].encode_utf16().count();
            assert_eq!(
                binding,
                &GenomeBinding {
                    name: name.into(),
                    value_type,
                    from,
                    to: from + name.len()
                }
            );
        }
    }

    #[test]
    fn analysis_keeps_completed_bindings_and_real_type_error_spans() {
        let source = "# café 🌱\r\nlet label = \"é🌱\"; let count = 1; let broken = count + false;\nthrust=0;turn=0;divide=false;";
        let analysis = Program::analyze(source);
        assert!(!analysis.valid);
        assert_eq!(
            analysis
                .bindings
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            ["label", "count"]
        );
        assert!(analysis.node_count > 0);
        let diagnostic = &analysis.diagnostics[0];
        let byte = source.find('+').unwrap();
        let line_start = source[..byte].rfind('\n').unwrap() + 1;
        assert_eq!(diagnostic.line, 2);
        assert_eq!(
            diagnostic.column,
            source[line_start..byte].encode_utf16().count() + 1
        );
        assert_eq!(diagnostic.from, source[..byte].encode_utf16().count());
        assert_eq!(diagnostic.to, diagnostic.from + 1);
        assert!(Program::compile(source)
            .unwrap_err()
            .ends_with(&diagnostic.message));
        let unknown = "let label=\"🌱\";thrust=missing_signal;turn=0;divide=false;";
        let diagnostic = Program::analyze(unknown).diagnostics.remove(0);
        let from = unknown[..unknown.find("missing_signal").unwrap()]
            .encode_utf16()
            .count();
        assert_eq!(
            (diagnostic.from, diagnostic.to),
            (from, from + "missing_signal".len())
        );
    }

    #[test]
    fn analysis_handles_empty_incomplete_and_unicode_lexical_input() {
        for source in [
            "",
            "# café 🌱",
            "let",
            "let signal = ",
            "let note = \"é🌱\"; thrust =",
            "thrust=0;turn=0;\r\n",
        ] {
            let analysis = Program::analyze(source);
            assert!(!analysis.valid, "{source}");
            assert_eq!(analysis.diagnostics.len(), 1);
            let error = &analysis.diagnostics[0];
            let end = source.encode_utf16().count();
            let line_start = source.rfind('\n').map_or(0, |index| index + 1);
            assert_eq!((error.from, error.to), (end, end), "{source}");
            assert_eq!(
                error.line,
                source.bytes().filter(|byte| *byte == b'\n').count() + 1
            );
            assert_eq!(
                error.column,
                source[line_start..].encode_utf16().count() + 1
            );
            assert!(Program::compile(source)
                .unwrap_err()
                .ends_with(&error.message));
        }
        for (source, token) in [
            ("# 🌱\nλ", "λ"),
            ("let text=\"é🌱", "\"é🌱"),
            ("let text=\"\\🌱", "\"\\🌱"),
        ] {
            let analysis = Program::analyze(source);
            let byte = source.find(token).unwrap();
            let from = source[..byte].encode_utf16().count();
            assert_eq!(
                (analysis.diagnostics[0].from, analysis.diagnostics[0].to),
                (from, from + token.encode_utf16().count())
            );
            assert!(Program::compile(source)
                .unwrap_err()
                .ends_with(&analysis.diagnostics[0].message));
        }
        let oversized = " ".repeat(MAX_SOURCE_BYTES + 1);
        let analysis = Program::analyze(&oversized);
        assert!(!analysis.valid);
        assert_eq!(analysis.source_bytes, MAX_SOURCE_BYTES + 1);
        assert_eq!(
            (analysis.diagnostics[0].from, analysis.diagnostics[0].to),
            (0, 0)
        );
    }

    #[test]
    fn analysis_obeys_expression_budget_without_changing_program_storage() {
        let mut source = (0..MAX_NODES - 3)
            .map(|index| format!("let a{index}={index};"))
            .collect::<String>();
        source.push_str("thrust=0;turn=0;divide=false;");
        let program = Program::compile(&source).unwrap();
        let mut before = crate::checkpoint::Writer::new();
        program.write_checkpoint(&mut before);
        let analysis = Program::analyze(&source);
        assert!(analysis.valid);
        assert_eq!(analysis.node_count, MAX_NODES);
        assert_eq!(analysis.bindings.len(), MAX_NODES - 3);
        let compiled = Program::compile(&source).unwrap();
        let mut after = crate::checkpoint::Writer::new();
        compiled.write_checkpoint(&mut after);
        assert_eq!(before.0, after.0);
        assert_eq!(program.estimated_bytes(), compiled.estimated_bytes());
        source.push_str("let extra=1;");
        let invalid = Program::analyze(&source);
        assert!(!invalid.valid);
        assert_eq!(invalid.node_count, MAX_NODES);
        assert!(invalid.diagnostics[0]
            .message
            .contains("8192 expression nodes"));
    }

    #[test]
    fn graph_preserves_operands_aliases_defaults_and_state_boundaries() {
        let program = Program::compile(
            "let sensor = energy; let alias = sensor; let unused = 42;\n\
             ri001 = ri001 + 1 when chemical.valid;\n\
             clear ri001 when populated(ri001);\n\
             thrust = if rb001 then alias else 0.2; turn = 0; divide = false;",
        )
        .unwrap();
        let graph = program.graph_json();
        assert!(graph.starts_with("{\"version\":1,\"mode\":\"legacy\",\"nodes\":["));
        assert_eq!(graph.matches("\"reachable\":").count(), 14);
        assert_eq!(graph.matches("\"kind\":\"output\"").count(), OUTPUT_COUNT);
        assert!(graph.contains(r#"{"id":0,"kind":"sensor","type":"scalar","label":"energy","inputs":[],"bindings":["sensor"],"reachable":true}"#));
        assert!(graph.contains(r#"{"id":1,"kind":"reference","type":"scalar","label":"sensor","inputs":[0],"bindings":["alias"],"reachable":true}"#));
        assert!(graph.contains(r#"{"id":2,"kind":"constant","type":"integer","label":"42","inputs":[],"bindings":["unused"],"reachable":false}"#));
        assert!(graph.contains(r#"{"id":3,"kind":"register","type":"integer","label":"ri001","inputs":[],"bindings":[],"reachable":true}"#));
        assert!(graph
            .contains(r#"{"id":5,"kind":"binary","type":"integer","label":"+","inputs":[3,4]"#));
        assert!(graph.contains(
            r#"{"id":7,"kind":"populated","type":"boolean","label":"populated(ri001)","inputs":[]"#
        ));
        assert!(graph.contains(
            r#"{"id":11,"kind":"conditional","type":"scalar","label":"if","inputs":[8,9,10]"#
        ));
        assert!(graph.contains(r#"{"id":"output:thrust","kind":"output","name":"thrust","type":"scalar","value":11,"guard":null,"default":null}"#));
        assert!(graph.contains(r#"{"id":"output:grow","kind":"output","name":"grow","type":"scalar","value":null,"guard":null,"default":"0"}"#));
        assert!(graph.contains(r#"{"id":"output:engulf","kind":"output","name":"engulf","type":"boolean","value":null,"guard":null,"default":"false"}"#));
        for (name, default) in [
            ("absorb", "1"),
            ("express.flagellum", "0"),
            ("express.photosynthetic", "0"),
            ("express.digestive", "0"),
            ("express.cortex", "0"),
            ("express.optical", "0"),
            ("feed", "1"),
            ("express.piercer", "0"),
        ] {
            assert!(graph.contains(&format!(
                "{{\"id\":\"output:{name}\",\"kind\":\"output\",\"name\":\"{name}\",\"type\":\"scalar\",\"value\":null,\"guard\":null,\"default\":\"{default}\"}}"
            )));
        }
        assert!(graph.contains(r#"{"id":"write:ri001","kind":"write","name":"ri001","type":"integer","value":5,"guard":6,"default":null}"#));
        assert!(graph.contains(r#"{"id":"clear:ri001","kind":"clear","name":"ri001","type":"integer","value":null,"guard":7,"default":null}"#));
        assert_eq!(program.graph_json(), graph, "inspection is deterministic");
    }

    #[test]
    fn trophic_outputs_are_optional_typed_and_survive_mutation_roundtrips() {
        let legacy = Program::compile("thrust=0;turn=0;divide=false;").unwrap();
        let defaults = legacy.evaluate(&Inputs::default());
        assert_eq!(defaults.feed, 1.0);
        assert_eq!(defaults.body.build[8], 0.0);
        assert!(!legacy.to_source().unwrap().contains("feed ="));
        assert!(!legacy.to_source().unwrap().contains("build.piercer ="));
        let source = "thrust=0;turn=0;divide=false;feed=2;build.piercer=-1;";
        let output = Program::compile(source)
            .unwrap()
            .evaluate(&Inputs::default());
        assert_eq!(output.feed, 1.0);
        assert_eq!(output.body.build[8], 0.0);
        for invalid in [
            "feed=true;",
            "build.piercer=false;",
            "feed=0;feed=1;",
            "build.piercer=0;build.piercer=1;",
            "let feed=1;",
            "let body.maturity=1;",
        ] {
            assert!(Program::compile(&format!("thrust=0;turn=0;divide=false;{invalid}")).is_err());
        }
        let source = "thrust=.2;turn=chemical.organic_bearing;divide=body.maturity>=1;feed=1-gut.fraction;build.piercer=contact.resistance*.5;rf001=digestion.rate+growth.rate+exudation.rate;";
        for kind in [
            MutationKind::Any,
            MutationKind::Constant,
            MutationKind::Expression,
            MutationKind::Reference,
        ] {
            for seed in 1..50 {
                if let Some(mutation) = Program::mutate_source(source, seed, kind) {
                    assert_roundtrip(&mutation);
                }
            }
        }
    }

    #[test]
    fn organelle_and_physical_sensors_are_typed_and_read_current_inputs() {
        let inputs = Inputs {
            body_area: 1.25,
            body_mass: 0.7,
            body_division_ready: true,
            gut_energy: 32.0,
            gut_capacity: 80.0,
            gut_fraction: 0.4,
            digestion_rate: 4.1,
            growth_rate: 0.03,
            body_birth_area: 0.8,
            body_maturity: 0.5,
            lifespan: 300.0,
            exudation_rate: 0.8,
            contact_pressure: 0.3,
            contact_token: Rc::from("contact:17"),
            object_token: Rc::from("track:23"),
            object_offset: [1.0, -3.0],
            flow_vector: [-2.0, 0.4],
            object_motion: [0.2, 0.1],
            organelle: std::array::from_fn(|i| OrganelleInput {
                count: i as i32,
                mass: i as f64 * 0.1,
                capacity: i as f64 * 0.2,
                damaged: 0.01,
                building: 0.02,
            }),
            ..Inputs::default()
        };
        let mut names = HashSet::new();
        for sensor in Sensor::ALL {
            assert!(names.insert(sensor.name()));
            let value = sensor.read(&inputs);
            assert_eq!(value.ty(), sensor.ty());
            let register = format!("{}001", sensor.ty().prefix());
            let source = format!("{register}={};thrust=0;turn=0;divide=false;", sensor.name());
            let program = Program::compile(&source).unwrap();
            let mut memory = Registers::default();
            program.evaluate_cell(&inputs, &mut memory, &mut EvalScratch::default());
            assert_eq!(memory.value(&register), Some(&value));
            assert!(Program::compile(&format!(
                "{}=0;thrust=0;turn=0;divide=false;",
                sensor.name()
            ))
            .is_err());
        }
        assert_eq!(Sensor::ALL.len(), 169);
        assert_eq!(
            Sensor::OrganelleFlagellumCount.read(&inputs),
            Value::Integer(7)
        );
        assert_eq!(
            Sensor::ObjectToken.read(&inputs),
            Value::Text(Rc::from("track:23"))
        );
    }

    #[test]
    fn graph_keeps_duplicate_edges_and_all_binding_aliases() {
        let mut program =
            Program::compile("let a = 1; thrust = a + a; turn = 0; divide = false;").unwrap();
        // Typed mutations can share the same child in multiple operand slots.
        program.nodes[3].op = Op::Binary(Binary::Subtract, 0, 0);
        program.bindings.push(Binding {
            name: "second_name".into(),
            node: 0,
        });
        let graph = program.graph_json();
        assert!(graph.contains(r#""bindings":["a","second_name"]"#));
        assert!(graph
            .contains(r#"{"id":3,"kind":"binary","type":"integer","label":"-","inputs":[0,0]"#));
        assert!(graph.contains(r#"{"id":1,"kind":"reference","type":"integer","label":"a","inputs":[0],"bindings":[],"reachable":false}"#));
    }

    #[test]
    fn graph_string_labels_escape_source_literals_as_json() {
        let program = Program::compile(
            r#"rs001 = "\"line\n\t\u0000雪<svg>\\"; thrust = 0; turn = 0; divide = false;"#,
        )
        .unwrap();
        let graph = program.graph_json();
        assert!(graph.contains(r#""label":"\"\\\"line\\n\\t\\u0000雪<svg>\\\\\"""#));
        assert!(!graph.chars().any(char::is_control));
        assert!(graph.contains(r#""name":"rs001","type":"string""#));
    }

    #[test]
    fn graph_exports_the_entire_maximum_graph_without_recursive_traversal() {
        let source = format!(
            "let unused = 0; thrust = {}; turn = 0; divide = false;",
            vec!["1"; 4095].join(" + ")
        );
        let program = Program::compile(&source).unwrap();
        assert_eq!(program.nodes.len(), MAX_NODES);
        let graph = program.graph_json();
        assert_eq!(graph.matches("\"reachable\":").count(), MAX_NODES);
        assert_eq!(graph.matches("\"reachable\":false").count(), 1);
        assert!(graph.contains(r#""value":8189,"guard":null"#));
        assert!(graph.contains(r#"{"id":8191,"kind":"constant","type":"boolean""#));
        assert!(
            graph.len() < 2 * 1024 * 1024,
            "bounded metadata for a maximum-body.area graph"
        );
    }

    #[test]
    fn program_reacts_to_new_sensor_samples_without_mutable_program_state() {
        let program = Program::compile("thrust=if chemical.valid then .8 else .45; turn=chemical.organic_bearing*1.8;divide=energy>250;").unwrap();
        let original = Inputs {
            energy: 300.0,
            chemical_valid: true,
            chemical_organic_bearing: 0.2,
            ..Inputs::default()
        };
        let first = program.evaluate(&original);
        assert_eq!(first.thrust, 0.8);
        assert!((first.turn - 0.36).abs() < 1e-10);
        assert!(first.divide);
        let second = program.evaluate(&Inputs {
            chemical_valid: false,
            energy: 30.0,
            ..original.clone()
        });
        assert_eq!(second.thrust, 0.45);
        assert!(!second.divide);
        assert_eq!(program.evaluate(&original), first);
    }

    #[test]
    fn existing_three_output_programs_default_to_no_growth() {
        let program = Program::compile(
            "thrust = 0.8; turn = chemical.organic_bearing; divide = energy > 120;",
        )
        .unwrap();
        for size in [0.5, 1.0, 2.0, 4.0] {
            let result = program.evaluate(&Inputs {
                body_area: size,
                energy: 150.0,
                chemical_organic_bearing: 0.2,
                ..Inputs::default()
            });
            assert_eq!(result.grow, 0.0);
            assert_eq!(result.thrust, 0.8);
            assert_eq!(result.turn, 0.2);
            assert!(result.divide);
        }
        let restored = Program::compile(&program.to_source().unwrap()).unwrap();
        for inputs in mutation_samples() {
            assert_eq!(program.evaluate(&inputs), restored.evaluate(&inputs));
        }
    }

    #[test]
    fn optional_growth_preserves_legacy_source_at_the_exact_node_limit() {
        let source = format!(
            "thrust = {}; turn = -1; divide = false;",
            vec!["1"; 4095].join("+")
        );
        let program = Program::compile(&source).unwrap();
        assert_eq!(program.nodes.len(), MAX_NODES);
        assert_eq!(program.evaluate(&Inputs::default()).grow, 0.0);
        // Adding an authored expression still obeys the unchanged node budget.
        assert!(Program::compile(&(source + " grow = 0;")).is_err());
    }

    #[test]
    fn grow_is_optional_numeric_and_unique_and_size_is_a_readonly_sensor() {
        let program =
            Program::compile("grow = body.area / 2; divide = false; turn = 0; thrust = 0;")
                .unwrap();
        assert_eq!(
            program
                .evaluate(&Inputs {
                    body_area: 1.0,
                    ..Inputs::default()
                })
                .grow,
            0.5
        );
        for source in [
            "thrust = 0; turn = 0; divide = false; grow = true;",
            "thrust = 0; turn = 0; divide = false; grow = 0; grow = 1;",
            "let grow = 1; thrust = 0; turn = 0; divide = false;",
            "let body.area = 1; thrust = 0; turn = 0; divide = false;",
            "thrust = grow; turn = 0; grow = 1; divide = false;",
            "thrust = 0; turn = 0; divide = body.area;",
            "thrust = 0; turn = 0; divide = false; grow = energy > 50;",
        ] {
            assert!(Program::compile(source).is_err(), "accepted {source}");
        }
    }

    #[test]
    fn growth_reacts_to_size_and_energy_and_clamps_to_zero_one() {
        let program = Program::compile(
            "thrust = 0; turn = 0; divide = false; grow = (2 - body.area) * energy / 100;",
        )
        .unwrap();
        for (size, energy, expected) in [
            (0.5, 100.0, 1.0),
            (1.5, 100.0, 0.5),
            (3.0, 100.0, 0.0),
            (1.0, 0.0, 0.0),
        ] {
            assert_eq!(
                program
                    .evaluate(&Inputs {
                        body_area: size,
                        energy,
                        ..Inputs::default()
                    })
                    .grow,
                expected
            );
        }
        assert_eq!(
            program
                .evaluate(&Inputs {
                    body_area: 1.0,
                    energy: f64::NAN,
                    ..Inputs::default()
                })
                .grow,
            0.0
        );
        let unlimited =
            Program::compile("thrust = 0; turn = 0; divide = false; grow = 1e300;").unwrap();
        assert_eq!(unlimited.evaluate(&Inputs::default()).grow, 1.0);
        let negative =
            Program::compile("thrust = 0; turn = 0; divide = false; grow = -1e300;").unwrap();
        assert_eq!(negative.evaluate(&Inputs::default()).grow, 0.0);
    }

    #[test]
    fn default_producer_requests_paid_construction_and_physical_division() {
        let program = Program::compile(DEFAULT_PROGRAM).unwrap();
        let mut inputs = Inputs {
            energy: 68.0,
            energy_fraction: 0.85,
            nutrient_stored: 0.6,
            body_area: 1.0,
            body_birth_area: 1.0,
            integrity: 1.0,
            ..Inputs::default()
        };
        inputs.organelle[0] = OrganelleInput {
            count: 1,
            mass: 0.04,
            capacity: 1.0,
            ..OrganelleInput::default()
        };
        let newborn = program.evaluate(&inputs);
        assert_eq!(newborn.body.build[1], 1.0, "start an energy source");
        assert_eq!(newborn.body.build[0], 0.0, "do not expand storage first");
        assert_eq!(newborn.grow, 0.0);
        assert!(!newborn.divide);
        assert_eq!(newborn.photosynthesize, 1.0);
        assert_eq!(newborn.body.target_counts[0], 2);
        assert_eq!(newborn.body.target_counts[1], 2);
        assert_eq!(newborn.body.recycle, [0.0; FAMILY_COUNT]);

        inputs.energy = 130.0;
        inputs.body_division_ready = true;
        assert!(
            !program.evaluate(&inputs).divide,
            "reserves cannot replace missing organs"
        );
        for index in [0, 1, 2, 5] {
            inputs.organelle[index].count = 2;
            inputs.organelle[index].capacity = 2.0;
        }
        assert!(program.evaluate(&inputs).divide);
        inputs.body_division_ready = false;
        assert!(!program.evaluate(&inputs).divide);
        assert_eq!(program.evaluate(&inputs).grow, 0.8);
        inputs.body_division_ready = true;
        inputs.organelle[1].building = 0.01;
        assert!(
            !program.evaluate(&inputs).divide,
            "unfinished copies are insufficient"
        );
        inputs.organelle[1].building = 0.0;
        inputs.energy = 20.0;
        assert!(!program.evaluate(&inputs).divide);
        assert_eq!(program.evaluate(&inputs).grow, 0.0);
    }

    #[test]
    fn precedence_and_boolean_composition_are_typed() {
        let result = eval("let ready = energy > 5 && !chemical.valid; thrust = (2 + 3 * 4) / 20; turn = 0.9 - 0.3 - 0.2; divide = ready || false;", Inputs { energy: 6.0, ..Inputs::default() });
        assert!((result.thrust - 0.7).abs() < 1e-10);
        assert!((result.turn - 0.4).abs() < 1e-10);
        assert!(result.divide);
    }

    #[test]
    fn branch_types_must_match_and_outputs_have_fixed_types() {
        for source in [
            "thrust = true; turn = 0; divide = false;",
            "thrust = 0; turn = 0; divide = 1;",
            "thrust = if true then 1 else false; turn = 0; divide = false;",
            "thrust = 0; turn = 0; divide = 1 && true;",
            "thrust = true + false; turn = 0; divide = false;",
            "thrust = 0; turn = 0; divide = true == 1;",
            "thrust = sin(true); turn = 0; divide = false;",
        ] {
            let error = Program::compile(source).unwrap_err();
            assert!(error.starts_with("Line 1, column "), "{error}");
        }
    }

    #[test]
    fn construction_targets_are_integers_and_scan_commands_keep_string_and_angle_types() {
        let source = "thrust=0;turn=0;divide=false;target.flagellum.count=ri001-3;target.flagellum.size=-1;build.flagellum=9;recycle.flagellum=-2;scan.bearing=10;scan.width=20;scan.target=if object.valid then object.token else rs001;";
        let program = Program::compile(source).unwrap();
        let inputs = Inputs {
            object_valid: true,
            object_token: Rc::from("track:17"),
            ..Inputs::default()
        };
        let output = program.evaluate(&inputs);
        assert_eq!(output.body.target_counts[7], 0);
        assert_eq!(output.body.target_sizes[7], MIN_ORGANELLE_MASS[7]);
        assert_eq!(output.body.build[7], 1.0);
        assert_eq!(output.body.recycle[7], 0.0);
        assert_eq!(&*output.scan_target, "track:17");
        assert_eq!(output.scan_width, std::f64::consts::TAU);
        assert!((-std::f64::consts::PI..std::f64::consts::PI).contains(&output.scan_bearing));
        for invalid in [
            "target.flagellum.count=2.0;",
            "target.flagellum.count=2/1;",
            "scan.target=123;",
            "scan.width=true;",
        ] {
            assert!(Program::compile(&format!("thrust=0;turn=0;divide=false;{invalid}")).is_err());
        }
        let huge = eval("thrust=0;turn=0;divide=false;target.optical.count=2147483647;target.optical.size=1e300;", Inputs::default());
        assert_eq!(huge.body.target_counts[15], i32::MAX as u32);
        assert_eq!(huge.body.target_sizes[15], MAX_SIGNAL);
        for family in 0..FAMILY_COUNT {
            let index = BODY_OUTPUT_START + family * 4;
            let source = format!(
                "thrust=0;turn=0;divide=false;{}={};{}=.5;{}=.7;{}=.2;",
                OUTPUT_NAMES[index],
                family + 1,
                OUTPUT_NAMES[index + 1],
                OUTPUT_NAMES[index + 2],
                OUTPUT_NAMES[index + 3]
            );
            let actual = eval(&source, Inputs::default());
            assert_eq!(actual.body.target_counts[family], family as u32 + 1);
            assert_eq!(actual.body.target_sizes[family], 0.5);
            assert_eq!(actual.body.build[family], 0.7);
            assert_eq!(actual.body.recycle[family], 0.2);
        }
    }

    #[test]
    fn semantic_target_oracles_and_investment_aliases_are_rejected() {
        for name in [
            "size",
            "food.visible",
            "food.energy",
            "prey.id",
            "prey.energy",
            "prey.same_genome",
            "threat.size",
            "neighbor.visible",
            "temperature.value",
            "flow.x",
            "nutrient.level",
            "body.propulsion",
            "birth.size",
            "maturity",
        ] {
            assert!(Sensor::lookup(name).is_none(), "legacy signal {name}");
            assert!(Program::compile(&format!("thrust={name};turn=0;divide=false;")).is_err());
        }
        for name in [
            "invest.propulsion",
            "invest.photosynthesis",
            "invest.digestion",
            "invest.armor",
            "invest.sensing",
            "invest.attack",
        ] {
            assert!(Program::compile(&format!("thrust=0;turn=0;divide=false;{name}=1;")).is_err());
        }
    }

    #[test]
    fn schema_includes_every_typed_signal_and_output_once() {
        let schema = language_schema_json();
        assert!(schema.starts_with("{\"version\":3,"));
        assert!(schema.contains("\"name\":\"target.flagellum.count\",\"type\":\"integer\",\"required\":false,\"default\":0"));
        assert!(schema.contains(
            "\"name\":\"scan.target\",\"type\":\"string\",\"required\":false,\"default\":\"\""
        ));
        let legacy_catalog = schema.split(",\"cellular\":").next().unwrap();
        for name in Sensor::ALL
            .into_iter()
            .map(|s| s.name())
            .chain(OUTPUT_NAMES)
        {
            assert_eq!(
                legacy_catalog
                    .matches(&format!("\"name\":{}", quote(name)))
                    .count(),
                1
            );
        }
        assert_eq!(OUTPUT_COUNT, 81);
    }

    #[test]
    fn expression_and_substrate_controls_are_typed_and_old_build_spelling_is_an_alias() {
        let input = Inputs {
            protein: std::array::from_fn(|i| i as f64 * 0.01),
            gut_exudate_energy: 3.0,
            gut_tissue_energy: 5.0,
            gut_waste_energy: 7.0,
            gut_exudate_nutrients: 0.1,
            gut_tissue_nutrients: 0.2,
            gut_waste_nutrients: 0.3,
            expression_rate: 0.01,
            assembly_rate: 0.02,
            assembly_capacity: 0.03,
            ..Inputs::default()
        };
        let source = "thrust=0;turn=0;divide=false;express.flagellum=if protein.flagellum<0.1 then 0.4 else 0;digest.exudate=-1;digest.tissue=2;digest.waste=0.25;";
        let current = eval(source, input.clone());
        let legacy = eval(
            &source.replace("express.flagellum", "build.flagellum"),
            input.clone(),
        );
        assert_eq!(current, legacy);
        assert_eq!(current.body.build[7], 0.4);
        assert_eq!(current.digest, [0.0, 1.0, 0.25]);
        for name in [
            "protein.flagellum",
            "gut.tissue.energy",
            "gut.waste.nutrients",
            "expression.rate",
            "assembly.capacity",
        ] {
            let (sensor, ty) = Sensor::lookup(name).unwrap();
            assert_eq!(ty, Type::Number);
            assert!(sensor.read(&input).number() > 0.0);
        }
        for invalid in [
            "express.flagellum=true;",
            "digest.tissue=vec(1,0);",
            "protein.flagellum=1;",
            "express.flagellum=1;build.flagellum=1;",
        ] {
            assert!(Program::compile(&format!("thrust=0;turn=0;divide=false;{invalid}")).is_err());
        }
        for kind in [
            MutationKind::Constant,
            MutationKind::Expression,
            MutationKind::Reference,
        ] {
            for seed in 0..24 {
                if let Some(mutation) = Program::mutate_source(source, seed, kind) {
                    assert_roundtrip(&mutation);
                }
            }
        }
    }

    #[test]
    fn body_controls_default_consistently_and_explicit_targets_remain_independent() {
        let idle = "thrust=0;turn=0;divide=false;";
        let defaults = eval(idle, Inputs::default());
        assert_eq!(defaults, Outputs::default());
        assert_eq!(defaults.body.target_counts, [0; FAMILY_COUNT]);
        assert_eq!(defaults.body.target_sizes, MIN_ORGANELLE_MASS);
        assert_eq!(defaults.body.build, [0.0; FAMILY_COUNT]);
        assert_eq!(defaults.body.recycle, [0.0; FAMILY_COUNT]);
        let explicit = format!("{idle}absorb=0;build.flagellum=-1;build.photosynthetic=2;build.digestive=.75;build.cortex=.375;build.optical=1;feed=.25;build.piercer=.625;target.flagellum.count=2;target.flagellum.size=.08;recycle.optical=.25;");
        let p = Program::compile(&explicit).unwrap();
        let o = p.evaluate(&Inputs::default());
        assert_eq!(o.absorb, 0.0);
        assert_eq!(o.body.build[7], 0.0);
        assert_eq!(o.body.build[1], 1.0);
        assert_eq!(o.body.build[3], 0.75);
        assert_eq!(o.body.build[9], 0.375);
        assert_eq!(o.body.build[15], 1.0);
        assert_eq!(o.body.recycle[15], 0.25);
        assert_eq!(o.body.target_counts[7], 2);
        assert_eq!(o.body.target_sizes[7], 0.08);
        assert_eq!(o.feed, 0.25);
        assert_eq!(o.body.build[8], 0.625);
        // Allocation fairness belongs to the physical body; requests are independent.
        assert!(o.body.build[1] + o.body.build[3] + o.body.build[9] > 1.0);
        assert_eq!(o, eval(&p.to_source().unwrap(), Inputs::default()));
        let graph = p.graph_json();
        for (i, name) in OUTPUT_NAMES.iter().enumerate() {
            let marker = format!("\"id\":\"output:{name}\"");
            let root = graph
                .split(&marker)
                .nth(1)
                .unwrap()
                .split('}')
                .next()
                .unwrap();
            assert_eq!(
                root.contains("\"default\":null"),
                p.output_roots[i].is_some()
            );
            assert_eq!(root.contains("\"value\":null"), p.output_roots[i].is_none());
        }
    }

    #[test]
    fn all_optional_actions_reject_wrong_types_duplicate_assignments_and_shadowing() {
        let idle = "thrust=0;turn=0;divide=false;";
        for (index, name) in OUTPUT_NAMES.iter().enumerate().skip(9) {
            let invalid = match output_type(index) {
                Type::Integer => ["true", "2.0", "vec(1,2)"],
                Type::String => ["true", "2", "vec(1,2)"],
                _ => ["true", "\"text\"", "vec(1,2)"],
            };
            for value in invalid {
                assert!(Program::compile(&format!("{idle}{name}={value};")).is_err());
            }
            let value = output_default(index).source();
            assert!(Program::compile(&format!("{idle}{name}={value};{name}={value};")).is_err());
            assert!(Program::compile(&format!("let {name}=0;{idle}")).is_err());
            assert!(Program::compile(&format!("{idle}rf001={name};")).is_err());
        }
    }

    #[test]
    fn immutable_bindings_and_reserved_names_are_enforced() {
        for source in [
            "let x = 1; let x = 2; thrust = x; turn = 0; divide = false;",
            "let energy = 1; thrust = energy; turn = 0; divide = false;",
            "let thrust = 1; turn = 0; divide = false;",
            "let sin = 1; thrust = 1; turn = 0; divide = false;",
            "let food.foo = 1; thrust = 1; turn = 0; divide = false;",
            "let x = x + 1; thrust = x; turn = 0; divide = false;",
            "thrust = 1; thrust = 0; turn = 0; divide = false;",
            "thrust = 1; turn = 0;",
            "thrust = mystery; turn = 0; divide = false;",
            "thrust = 1; turn = 0; divide = false; unexpected = 0;",
        ] {
            assert!(Program::compile(source).is_err(), "accepted {source}");
        }
    }

    #[test]
    fn division_by_zero_and_extreme_numbers_stay_finite() {
        let result = eval(
            "thrust = 1 / 0; turn = 1e300 * 1e300; divide = true;",
            Inputs::default(),
        );
        assert_eq!(result.thrust, 0.0);
        assert_eq!(result.turn, 1.0);
        assert!(result.divide);
        assert!(Program::compile("thrust = 1e999; turn = 0; divide = false;").is_err());
        let result = eval(
            "thrust = energy; turn = speed; divide = false;",
            Inputs {
                energy: f64::NAN,
                speed: f64::INFINITY,
                ..Inputs::default()
            },
        );
        assert_eq!(result.thrust, 0.0);
        assert_eq!(result.turn, 1.0);
    }

    #[test]
    fn builtins_nested_conditionals_and_reversed_clamps_are_safe() {
        let result = eval("let value = if true then if false then 0 else .4 else .9; thrust = clamp(value, 1, 0); turn = max(-1, min(1, cos(0) - abs(-.25))); divide = flow.valid && flow.speed < 20;", Inputs { flow_valid: true, flow_speed: 10.0, ..Inputs::default() });
        assert_eq!(result.thrust, 0.4);
        assert_eq!(result.turn, 0.75);
        assert!(result.divide);
    }

    #[test]
    fn errors_have_real_line_and_column() {
        let error = Program::compile("# comment\nthrust = 1;\nturn = unknown;\ndivide = false;")
            .unwrap_err();
        assert!(error.starts_with("Line 3, column 8:"), "{error}");
    }

    #[test]
    fn malformed_and_unbounded_programs_are_rejected() {
        for source in [
            "thrust = 1 turn = 0; divide = false;",
            "thrust = sin(); turn = 0; divide = false;",
            "thrust = min(1); turn = 0; divide = false;",
            "thrust = cos(1, 2); turn = 0; divide = false;",
            "thrust = 1e; turn = 0; divide = false;",
            "thrust = unknown(1); turn = 0; divide = false;",
            "thrust = 1 & 2; turn = 0; divide = false;",
            "thrust = 1; turn = 0; divide = true; @",
        ] {
            assert!(Program::compile(source).is_err(), "accepted {source}");
        }
        let nested = format!(
            "thrust = {}1{}; turn = 0; divide = false;",
            "(".repeat(100),
            ")".repeat(100)
        );
        assert!(Program::compile(&nested).unwrap_err().contains("nesting"));
        assert!(Program::compile(&" ".repeat(MAX_SOURCE_BYTES + 1)).is_err());
        let nodes = format!(
            "thrust = {}; turn = 0; divide = false;",
            vec!["1"; 4200].join("+")
        );
        assert!(Program::compile(&nodes)
            .unwrap_err()
            .contains("expression nodes"));
        let tokens = format!(
            "thrust = 1{}; turn = 0; divide = false;",
            ";".repeat(MAX_TOKENS)
        );
        assert!(Program::compile(&tokens).unwrap_err().contains("tokens"));
    }

    #[test]
    fn reusable_buffer_is_safe_across_different_program_sizes() {
        let small = Program::compile("thrust = 1; turn = 0; divide = false;").unwrap();
        let big = Program::compile(DEFAULT_PROGRAM).unwrap();
        let mut buffer = Vec::new();
        let inputs = Inputs::default();
        assert_eq!(
            small.evaluate(&inputs),
            small.evaluate_with_scratch(&inputs, &mut buffer)
        );
        assert_eq!(
            big.evaluate(&inputs),
            big.evaluate_with_scratch(&inputs, &mut buffer)
        );
        assert_eq!(
            small.evaluate(&inputs),
            small.evaluate_with_scratch(&inputs, &mut buffer)
        );
    }

    fn assert_graph_types(program: &Program) {
        for (index, node) in program.nodes.iter().enumerate() {
            assert!(node.scope <= program.bindings.len());
            let (children, count) = node.op.children();
            for &child in &children[..count] {
                assert!(child < index, "forward edge/cycle at {index} -> {child}");
            }
            let ty = |child: usize| program.nodes[child].ty;
            match &node.op {
                Op::Constant(value) => {
                    assert_eq!(value.ty(), node.ty);
                    match value {
                        Value::Scalar(x) => assert!(x.is_finite() && x.abs() <= MAX_SIGNAL),
                        Value::Vector(v) => {
                            assert!(v.iter().all(|x| x.is_finite() && x.abs() <= MAX_SIGNAL))
                        }
                        Value::Text(s) => assert!(s.len() <= MAX_STRING_BYTES),
                        _ => {}
                    }
                }
                Op::Sensor(sensor) => assert_eq!(node.ty, sensor.ty()),
                Op::Pool(_) | Op::Concentration(_) | Op::Built(_) => {
                    assert_eq!(node.ty, Type::Number)
                }
                Op::Receptor(_, field) => assert_eq!(node.ty, field.ty()),
                Op::Register(register) => assert_eq!(register.ty, node.ty),
                Op::Populated(_) => assert_eq!(node.ty, Type::Bool),
                Op::Reference(child) => assert_eq!(node.ty, ty(*child)),
                Op::Negate(child) => {
                    assert_eq!(node.ty, ty(*child));
                    assert!(node.ty.numeric() || node.ty == Type::Vector);
                }
                Op::Not(child) => {
                    assert_eq!(node.ty, Type::Bool);
                    assert_eq!(ty(*child), Type::Bool);
                }
                Op::Choose(condition, yes, no) => {
                    assert_eq!(ty(*condition), Type::Bool);
                    assert_eq!(
                        Some(node.ty),
                        if ty(*yes) == ty(*no) {
                            Some(ty(*yes))
                        } else {
                            numeric_type(&[ty(*yes), ty(*no)])
                        }
                    );
                }
                Op::Call(function, args) => assert_eq!(
                    Some(node.ty),
                    function.ty(&args[..function.arity()]
                        .iter()
                        .map(|&i| ty(i))
                        .collect::<Vec<_>>())
                ),
                Op::Binary(op, a, b) => assert_eq!(Some(node.ty), binary_type(*op, ty(*a), ty(*b))),
            }
        }
        for (index, root) in program.output_indices().iter().enumerate() {
            if let Some(root) = root {
                let ty = program.nodes[*root].ty;
                assert!(
                    ty == output_type(index)
                        || (output_type(index) == Type::Number && ty == Type::Integer)
                );
            }
        }
        for rule in &program.writes {
            if let Some(v) = rule.value {
                let ty = program.nodes[v].ty;
                assert!(
                    rule.register.ty == ty
                        || (rule.register.ty == Type::Number && ty == Type::Integer)
                );
            }
            if let Some(g) = rule.guard {
                assert_eq!(program.nodes[g].ty, Type::Bool);
            }
        }
    }

    fn mutation_samples() -> [Inputs; 4] {
        [
            Inputs::default(),
            Inputs {
                energy: 130.0,
                age: 8.5,
                speed: 30.0,
                time: 120.0,
                id: 44.0,
                body_area: 1.4,
                chemical_valid: true,
                chemical_organic: 37.0,
                chemical_organic_bearing: -1.4,
                flow_valid: true,
                flow_speed: 11.0,
                flow_bearing: 2.5,
                ..Inputs::default()
            },
            Inputs {
                energy: 18.0,
                age: 4.1,
                speed: 0.5,
                time: 182.0,
                id: 7.0,
                body_area: 0.6,
                chemical_valid: false,
                chemical_organic: 120.0,
                chemical_organic_bearing: 0.0,
                flow_valid: true,
                flow_speed: 89.0,
                flow_bearing: -3.1,
                ..Inputs::default()
            },
            Inputs {
                energy: f64::INFINITY,
                age: f64::NAN,
                speed: -f64::INFINITY,
                id: 1e300,
                ..Inputs::default()
            },
        ]
    }

    fn assert_roundtrip(mutation: &Mutation) -> Program {
        assert!(mutation.source.len() <= MAX_SOURCE_BYTES);
        assert!(lex(&mutation.source).unwrap().len() - 1 <= MAX_TOKENS);
        assert_graph_types(&mutation.program);
        let compiled = Program::compile(&mutation.source).unwrap_or_else(|error| {
            panic!(
                "typed mutation emitted invalid source: {error}\n{}",
                mutation.source
            )
        });
        assert_graph_types(&compiled);
        assert!(compiled.nodes.len() <= MAX_NODES);
        for inputs in mutation_samples() {
            let original = mutation.program.evaluate(&inputs);
            let restored = compiled.evaluate(&inputs);
            assert_eq!(
                original, restored,
                "serialized source changed behavior:\n{}",
                mutation.source
            );
            assert!(restored.thrust.is_finite() && (0.0..=1.0).contains(&restored.thrust));
            assert!(restored.turn.is_finite() && (-1.0..=1.0).contains(&restored.turn));
            for value in [
                restored.grow,
                restored.photosynthesize,
                restored.bite,
                restored.defend,
                restored.repair,
                restored.absorb,
                restored.body.build[7],
                restored.body.build[1],
                restored.body.build[3],
                restored.body.build[9],
                restored.body.build[15],
                restored.feed,
                restored.body.build[8],
            ] {
                assert!(value.is_finite() && (0.0..=1.0).contains(&value));
            }
        }
        compiled
    }

    #[test]
    fn thousands_of_seeded_mutations_preserve_types_bounds_and_source_behavior() {
        let sources = [
            DEFAULT_PROGRAM,
            "thrust = 0.7; turn = 0; divide = false;",
            "thrust = energy; turn = chemical.organic_bearing; divide = chemical.valid;",
            "let a = energy / 100; let b = a; let c = b; let flag = flow.valid; thrust = clamp(c, 0, 1); turn = if flag then flow.bearing else chemical.organic_bearing; divide = flag && energy > 110;",
            "let tiny = 1e-300; let huge = 1e300; thrust = tiny; turn = -huge; divide = chemical.valid != flow.valid;",
        ];
        let kinds = [
            MutationKind::Any,
            MutationKind::Constant,
            MutationKind::Expression,
            MutationKind::Reference,
        ];
        let mut returned = 0;
        for source in sources {
            for kind in kinds {
                for seed in 0..300 {
                    if let Some(mutation) = Program::mutate_source(source, seed, kind) {
                        assert!(kind == MutationKind::Any || mutation.kind == kind);
                        assert_ne!(mutation.kind, MutationKind::Any);
                        assert_roundtrip(&mutation);
                        returned += 1;
                    }
                }
            }
        }
        assert!(returned > 5000, "only {returned} mutations applied");
    }

    #[test]
    fn mutation_chains_and_export_recompile_chains_remain_valid() {
        let mut internal_source = DEFAULT_PROGRAM.to_string();
        let mut exported_source = internal_source.clone();
        let mut accepted = [0, 0];
        let kinds = [
            MutationKind::Constant,
            MutationKind::Reference,
            MutationKind::Expression,
            MutationKind::Any,
        ];
        for seed in 1..=1800 {
            let kind = kinds[seed as usize % kinds.len()];
            if let Some(mutation) = Program::mutate_source(&internal_source, seed * 97, kind) {
                assert_roundtrip(&mutation);
                internal_source = mutation.source;
                accepted[0] += 1;
            }
            if let Some(mutation) = Program::mutate_source(&exported_source, seed * 101, kind) {
                assert_roundtrip(&mutation);
                exported_source = mutation.source;
                accepted[1] += 1;
            }
        }
        assert!(
            accepted[0] > 1000 && accepted[1] > 1000,
            "accepted {accepted:?}"
        );
    }

    #[test]
    fn exporting_does_not_accumulate_aliases_or_expand_shared_subtrees() {
        let mut source = String::from("let _g0 = energy; let alias = _g0; let x0 = alias;\n");
        for index in 1..35 {
            source.push_str(&format!(
                "let x{index} = x{} + x{};\n",
                index - 1,
                index - 1
            ));
        }
        source.push_str("thrust = x34; turn = alias; divide = chemical.valid;");
        let mut program = Program::compile(&source).unwrap();
        let first = program.to_source().unwrap();
        assert!(first.len() < 10_000, "shared DAG was expanded");
        let mut expected = None;
        for _ in 0..50 {
            let printed = program.to_source().unwrap();
            program = Program::compile(&printed).unwrap();
            if let Some((ref prior, prior_count)) = expected {
                assert_eq!(&printed, prior);
                assert_eq!(program.nodes.len(), prior_count);
            }
            expected = Some((printed, program.nodes.len()));
        }
    }

    #[test]
    fn reference_substitution_respects_type_lexical_scope_and_alias_targets() {
        let source = "let early = 0.2; let current = energy; let future = age; let alias = current; let flag = chemical.valid; thrust = alias; turn = future; divide = flag;";
        let program = Program::compile(source).unwrap();
        let early = program.bindings[0].node;
        let current = program.bindings[1].node;
        let future = program.bindings[2].node;
        let options = program.reference_options(current);
        assert!(options.contains(&Op::Reference(early)));
        assert!(!options.contains(&Op::Reference(future)));
        assert!(!options.contains(&Op::Reference(current)));
        assert!(!options.contains(&Op::Sensor(Sensor::ChemicalValid)));
        for sensor in Sensor::ALL
            .iter()
            .filter(|s| s.ty() == Type::Number && **s != Sensor::Energy)
        {
            assert!(
                options.contains(&Op::Sensor(*sensor)),
                "{} is not an eligible scalar reference",
                sensor.name()
            );
        }
        for (index, node) in program.nodes.iter().enumerate() {
            if !matches!(node.op, Op::Reference(_) | Op::Sensor(_)) {
                continue;
            }
            for candidate in program.reference_options(index) {
                match candidate {
                    Op::Sensor(sensor) => assert_eq!(sensor.ty(), node.ty),
                    Op::Register(register) => assert_eq!(register.ty, node.ty),
                    Op::Populated(_) => assert_eq!(node.ty, Type::Bool),
                    Op::Reference(target) => {
                        assert!(target < index);
                        assert_eq!(program.nodes[target].ty, node.ty);
                        assert!(program.bindings[..node.scope]
                            .iter()
                            .any(|binding| binding.node == target));
                        assert_ne!(
                            program.reference_root(target),
                            program.reference_root(index)
                        );
                    }
                    _ => panic!("reference mutation constructed an expression"),
                }
            }
        }
        for seed in 0..500 {
            assert_roundtrip(
                &Program::mutate_source(source, seed, MutationKind::Reference).unwrap(),
            );
        }
    }

    #[test]
    fn source_mutation_preserves_scope_boundaries_and_avoids_unused_targets() {
        let source = "let unused = 999; let first = energy; let dead = false; let later = age; thrust = first; turn = later; grow = body.area; divide = chemical.valid;";
        for seed in 0..150 {
            let mutation = Program::mutate_source(source, seed, MutationKind::Reference).unwrap();
            assert_roundtrip(&mutation);
            for (index, node) in mutation.program.nodes.iter().enumerate() {
                for candidate in mutation.program.reference_options(index) {
                    if let Op::Reference(target) = candidate {
                        assert!(mutation.program.bindings[..node.scope]
                            .iter()
                            .any(|b| b.node == target));
                        assert!(target < index);
                    }
                }
            }
        }
        // A dead constant is not an eligible mutation target.
        assert!(Program::mutate_source(source, 1, MutationKind::Constant).is_none());
    }

    #[test]
    fn expression_mutation_can_insert_or_remove_one_typed_unary_operation() {
        let source = "thrust=-energy;turn=0;divide=!body.division_ready;";
        let parent = Program::compile(source).unwrap();
        let mut grew = false;
        let mut shrank = false;
        for seed in 0..500 {
            let mutation = Program::mutate_source(source, seed, MutationKind::Expression).unwrap();
            grew |= mutation.program.nodes.len() > parent.nodes.len();
            shrank |= mutation.program.nodes.len() < parent.nodes.len();
            assert_roundtrip(&mutation);
        }
        assert!(
            grew && shrank,
            "expression mutations must include one-node insertion and removal"
        );
        let source = "thrust = 0; turn = 1; divide = false;";
        let leaf = Program::compile(source).unwrap();
        let mutation = Program::mutate_source(source, 3, MutationKind::Expression).unwrap();
        assert!(mutation.program.nodes.len() > leaf.nodes.len());
        assert_roundtrip(&mutation);
    }

    #[test]
    fn constant_mutations_are_distinct_and_finite_even_at_numeric_limits() {
        let sources = [
            "thrust = 1e300; turn = -1e300; divide = true;",
            "thrust = 0; turn = 1e-300; divide = false;",
        ];
        for source in sources {
            let parent = Program::compile(source).unwrap();
            for seed in 0..500 {
                let mutation =
                    Program::mutate_source(source, seed, MutationKind::Constant).unwrap();
                assert_roundtrip(&mutation);
                assert_ne!(parent.to_source(), Some(mutation.source));
            }
        }
    }

    #[test]
    fn source_limits_are_checked_without_serialization_expansion() {
        let source = format!(
            "thrust = {}; turn = 0; divide = false;",
            vec!["energy"; 3000].join(" + ")
        );
        let parent = Program::compile(&source).unwrap();
        assert!(parent.to_source().is_none());
        for seed in 0..50 {
            assert_roundtrip(
                &Program::mutate_source(&source, seed, MutationKind::Constant).unwrap(),
            );
            if let Some(mutation) = Program::mutate_source(&source, seed, MutationKind::Expression)
            {
                assert_roundtrip(&mutation);
            }
        }
        assert!(Program::mutate_source(
            "thrust=energy;turn=age;grow=body.area;divide=chemical.valid;",
            1,
            MutationKind::Constant
        )
        .is_none());
        assert!(Program::mutate_source(
            "thrust=0;turn=0;divide=false;",
            1,
            MutationKind::Reference
        )
        .is_none());
        let prefix = "thrust=.9;turn=0;divide=false;#";
        let source = format!("{prefix}{}", "x".repeat(MAX_SOURCE_BYTES - prefix.len()));
        assert!(Program::compile(&source).is_ok());
        for seed in 0..50 {
            if let Some(mutation) = Program::mutate_source(&source, seed, MutationKind::Constant) {
                assert_roundtrip(&mutation);
            }
        }
    }

    #[test]
    fn flat_long_expressions_serialize_without_recursive_printer_depth() {
        let source = format!(
            "thrust = {}; turn = 0; divide = true;",
            vec!["0.001"; 120].join(" + ")
        );
        for seed in 0..30 {
            assert_roundtrip(
                &Program::mutate_source(&source, seed, MutationKind::Constant).unwrap(),
            );
        }
    }

    #[test]
    fn mutation_is_deterministic_across_fresh_compilation_and_hash_seeds() {
        for seed in 0..100 {
            let first = Program::mutate_source(DEFAULT_PROGRAM, seed, MutationKind::Any).unwrap();
            let second = Program::mutate_source(DEFAULT_PROGRAM, seed, MutationKind::Any).unwrap();
            assert_eq!(first.kind, second.kind);
            assert_eq!(first.source, second.source);
        }
    }

    #[test]
    fn omitted_growth_is_not_added_by_a_mutation() {
        let source = "thrust=energy;turn=age;divide=chemical.valid;";
        assert!(Program::mutate_source(source, 1, MutationKind::Constant).is_none());
        for seed in 0..100 {
            let variant = Program::mutate_source(source, seed, MutationKind::Any).unwrap();
            assert!(!variant.source.contains("grow"));
            assert!(variant.program.output_roots[3].is_none());
            assert_roundtrip(&variant);
        }
    }

    #[test]
    fn size_is_an_eligible_numeric_reference_and_growth_mutations_roundtrip() {
        let source = "let scale = energy; thrust = scale; turn = age; grow = body.area / 4; divide = chemical.valid;";
        let parent = Program::compile(source).unwrap();
        let energy_node = parent.bindings[0].node;
        assert!(parent
            .reference_options(energy_node)
            .contains(&Op::Sensor(Sensor::BodyArea)));
        let visible_node = parent
            .nodes
            .iter()
            .position(|n| n.op == Op::Sensor(Sensor::ChemicalValid))
            .unwrap();
        assert!(!parent
            .reference_options(visible_node)
            .contains(&Op::Sensor(Sensor::BodyArea)));
        for kind in [
            MutationKind::Any,
            MutationKind::Constant,
            MutationKind::Expression,
            MutationKind::Reference,
        ] {
            for seed in 0..300 {
                let mutation = Program::mutate_source(source, seed, kind).unwrap();
                let restored = assert_roundtrip(&mutation);
                assert_eq!(
                    restored.nodes[restored.output_roots[3].unwrap()].ty,
                    Type::Number
                );
                assert!(mutation.source.contains("grow = "));
            }
        }
    }
}

#[cfg(test)]
mod register_tests {
    use super::*;
    const IDLE: &str = "thrust=0;turn=0;divide=false;";
    fn program(body: &str) -> Program {
        Program::compile(&format!("{body}{IDLE}")).unwrap()
    }
    fn tick(program: &Program, memory: &mut Registers, inputs: Inputs) -> Outputs {
        program.evaluate_cell(&inputs, memory, &mut EvalScratch::default())
    }
    fn value(memory: &Registers, name: &str) -> Value {
        memory.value(name).unwrap().clone()
    }
    fn transition_roundtrip(program: &Program, source: &str) {
        let restored = Program::compile(source).unwrap_or_else(|error| panic!("{error}\n{source}"));
        let (mut original_memory, mut restored_memory) =
            (Registers::default(), Registers::default());
        let (mut original_scratch, mut restored_scratch) =
            (EvalScratch::default(), EvalScratch::default());
        // Seed every bank, including a populated default, so copying, occupancy,
        // conditional clears, and default reads are all observed by the test.
        for name in [
            "ri001", "rf001", "rb001", "rs001", "rv001", "ri999", "rf999", "rb999", "rs999",
            "rv999",
        ] {
            let register = Register::parse(name).unwrap();
            let v = match register.ty {
                Type::Integer => Value::Integer(7),
                Type::Number => Value::Scalar(0.375),
                Type::Bool => Value::Boolean(false),
                Type::String => Value::Text(Rc::from("巡航")),
                Type::Vector => Value::Vector([0.3, -0.8]),
            };
            original_memory.entries.insert(register, v.clone());
            restored_memory.entries.insert(register, v);
        }
        for tick in 0..24 {
            let used = original_memory.populated_count() as f64;
            let inputs = Inputs {
                energy: 80.0 + tick as f64 * 2.5,
                energy_fraction: 0.65,
                age: tick as f64 / 30.0,
                time: tick as f64 / 30.0,
                body_area: 1.5,
                id: 17.0,
                chemical_valid: tick % 3 == 0,
                chemical_organic: 20.0,
                chemical_organic_bearing: 0.4,
                flow_valid: tick % 2 == 0,
                object_valid: tick % 4 == 0,
                object_available: tick % 5 == 0,
                object_contrast: 10.0,
                object_area: 0.6,
                object_pigment: 2.0,
                heat_valid: tick % 6 == 0,
                memory_used: used,
                memory_cost: used * REGISTER_ENERGY_COST,
                light_level: 0.7,
                light_bearing: -0.3,
                heat_value: 0.4,
                heat_bearing: 0.2,
                flow_vector: [0.25, -0.1],
                flow_speed: 0.269,
                flow_bearing: -0.38,
                integrity: 0.9,
                nutrient_stored: 0.1 + tick as f64 * 0.03,
                nutrient_capacity: 3.0,
                nutrient_fraction: (0.1 + tick as f64 * 0.03) / 3.0,
                chemical_mineral: 0.05 + (tick % 5) as f64 * 0.1,
                chemical_mineral_bearing: (tick as f64).sin(),
                organelle: std::array::from_fn(|i| OrganelleInput {
                    mass: if i == 7 {
                        0.1 + tick as f64 * 0.001
                    } else {
                        i as f64 * 0.03
                    },
                    count: 2,
                    capacity: 100.0,
                    damaged: 0.01,
                    building: 0.02,
                }),
                body_space_free: 0.1 - tick as f64 * 0.001,
                ..Inputs::default()
            };
            let a = program.evaluate_cell(&inputs, &mut original_memory, &mut original_scratch);
            let b = restored.evaluate_cell(&inputs, &mut restored_memory, &mut restored_scratch);
            assert_eq!(a, b, "output mismatch at tick {tick}\n{source}");
            assert_eq!(
                original_memory, restored_memory,
                "memory transition mismatch at tick {tick}\n{source}"
            );
        }
    }
    #[test]
    fn all_primitive_types_populate_at_both_address_bounds_and_defaults_still_cost_occupancy() {
        let p = program(
            r#"ri001=0;ri999=-2147483648;rf001=0;rf999=0.125;rb001=false;rb999=true;rs001="";rs999="雪";rv001=vec(0,0);rv999=vec(-1,2);"#,
        );
        let mut memory = Registers::default();
        tick(&p, &mut memory, Inputs::default());
        assert_eq!(memory.populated_count(), 10);
        assert_eq!(value(&memory, "ri001"), Value::Integer(0));
        assert_eq!(value(&memory, "ri999"), Value::Integer(i32::MIN));
        assert_eq!(value(&memory, "rf001"), Value::Scalar(0.0));
        assert_eq!(value(&memory, "rf999"), Value::Scalar(0.125));
        assert_eq!(value(&memory, "rb001"), Value::Boolean(false));
        assert_eq!(value(&memory, "rb999"), Value::Boolean(true));
        assert_eq!(value(&memory, "rs001"), Value::Text(Rc::from("")));
        assert_eq!(value(&memory, "rs999"), Value::Text(Rc::from("雪")));
        assert_eq!(value(&memory, "rv001"), Value::Vector([0.0; 2]));
        assert_eq!(value(&memory, "rv999"), Value::Vector([-1.0, 2.0]));
        assert!(memory
            .to_json()
            .contains("\"name\":\"rv999\",\"type\":\"vec2\",\"value\":[-1,2]"));
        assert!(memory.to_json().contains("\"value\":false"));
        assert!(memory.value("é001").is_none());
        transition_roundtrip(&p, &p.to_source().unwrap());
    }
    #[test]
    fn every_4995_register_can_be_populated_within_the_expression_budget() {
        let mut source = String::from(IDLE);
        for index in 1..=999 {
            source.push_str(&format!("ri{index:03}=0;rf{index:03}=0;rb{index:03}=false;rs{index:03}=\"\";rv{index:03}=vec(0,0);"));
        }
        let p = Program::compile(&source).unwrap();
        assert_eq!(p.nodes.len(), 6996);
        let mut memory = Registers::default();
        tick(&p, &mut memory, Inputs::default());
        assert_eq!(memory.populated_count(), 4995);
        assert_eq!(
            memory.populated_count() as f64 * REGISTER_ENERGY_COST,
            12.4875
        );
        let mut clear = String::from(IDLE);
        for index in 1..=999 {
            for prefix in ["ri", "rf", "rb", "rs", "rv"] {
                clear.push_str(&format!("clear {prefix}{index:03};"));
            }
        }
        let clearing = Program::compile(&clear).unwrap();
        assert_eq!(clearing.nodes.len(), 3);
        tick(&clearing, &mut memory, Inputs::default());
        assert_eq!(memory.populated_count(), 0);
        assert_eq!(memory.to_json(), "[]");
    }
    #[test]
    fn all_register_reads_and_outputs_observe_the_same_pre_tick_snapshot() {
        let p = Program::compile(
            "ri001=ri002+1;ri002=ri001+2;thrust=ri001/10;turn=ri002/10;divide=populated(ri001);",
        )
        .unwrap();
        let mut memory = Registers::default();
        let first = tick(&p, &mut memory, Inputs::default());
        assert_eq!(first.thrust, 0.0);
        assert_eq!(first.turn, 0.0);
        assert!(!first.divide);
        assert_eq!(value(&memory, "ri001"), Value::Integer(1));
        assert_eq!(value(&memory, "ri002"), Value::Integer(2));
        let second = tick(&p, &mut memory, Inputs::default());
        assert_eq!(second.thrust, 0.1);
        assert_eq!(second.turn, 0.2);
        assert!(second.divide);
        assert_eq!(value(&memory, "ri001"), Value::Integer(3));
        assert_eq!(value(&memory, "ri002"), Value::Integer(3));
        let reversed = Program::compile(
            "turn=ri002/10;divide=populated(ri001);ri002=ri001+2;thrust=ri001/10;ri001=ri002+1;",
        )
        .unwrap();
        let (mut a, mut b) = (Registers::default(), Registers::default());
        for _ in 0..20 {
            assert_eq!(
                tick(&p, &mut a, Inputs::default()),
                tick(&reversed, &mut b, Inputs::default())
            );
            assert_eq!(a, b);
        }
    }
    #[test]
    fn guarded_writes_hold_memory_and_clear_wins_independently_of_statement_order() {
        let a = program("rf001=energy when chemical.valid;clear rf001 when flow.valid;");
        let b = program("clear rf001 when flow.valid;rf001=energy when chemical.valid;");
        let (mut ma, mut mb) = (Registers::default(), Registers::default());
        for (write, clear, count, expected) in [
            (false, false, 0, None),
            (true, false, 1, Some(10.0)),
            (false, false, 1, Some(10.0)),
            (true, true, 0, None),
            (true, false, 1, Some(50.0)),
            (false, true, 0, None),
        ] {
            let energy = if expected == Some(50.0) { 50.0 } else { 10.0 };
            let input = Inputs {
                energy,
                chemical_valid: write,
                flow_valid: clear,
                ..Inputs::default()
            };
            tick(&a, &mut ma, input.clone());
            tick(&b, &mut mb, input);
            assert_eq!(ma, mb);
            assert_eq!(ma.populated_count(), count);
            assert_eq!(ma.value("rf001").map(Value::number), expected);
        }
        let occupied =
            program("rf001=10 when !populated(rf001);clear rf001 when populated(rf001);");
        let mut m = Registers::default();
        for count in [1, 0, 1, 0, 1, 0] {
            tick(&occupied, &mut m, Inputs::default());
            assert_eq!(m.populated_count(), count);
        }
    }
    #[test]
    fn program_and_scratch_share_no_hidden_per_cell_memory() {
        let p = program("ri001=ri001+1;");
        let (mut a, mut b) = (Registers::default(), Registers::default());
        let mut scratch = EvalScratch::default();
        for _ in 0..3 {
            p.evaluate_cell(&Inputs::default(), &mut a, &mut scratch);
        }
        p.evaluate_cell(&Inputs::default(), &mut b, &mut scratch);
        assert_eq!(value(&a, "ri001"), Value::Integer(3));
        assert_eq!(value(&b, "ri001"), Value::Integer(1));
        let mut reset = Registers::default();
        p.evaluate_cell(&Inputs::default(), &mut reset, &mut scratch);
        assert_eq!(reset, b);
        let clone = a.clone();
        tick(&p, &mut a, Inputs::default());
        assert_eq!(value(&clone, "ri001"), Value::Integer(3));
    }
    #[test]
    fn register_addresses_types_duplicate_rules_and_guards_are_validated() {
        for body in [
            "ri000=1;",
            "ri1000=1;",
            "ri1=1;",
            "ri01=1;",
            "ri0001=1;",
            "rf000=0;",
            "rv1000=vec(0,0);",
            "let ri001=1;",
            "let ri1000=1;",
            "let ri001abc=1;",
            "ri001=ri000;",
            "clear ri000;",
            "clear chemical.valid;",
            "ri001=0.5;",
            "ri001=ri001/2;",
            "rf001=false;",
            "rb001=1;",
            "rs001=2;",
            "rv001=0;",
            "ri001=1;ri001=2;",
            "ri001=1 when true;ri001=2 when false;",
            "clear ri001;clear ri001;",
            "ri001=1 when 2;",
            "clear ri001 when 2;",
            "ri001=populated(1);",
            "rb001=populated(energy);",
            "rb001=populated(ri001+1);",
            "let early=later;let later=1;",
            "rb001=rs001==1;",
            "rv001=vec(true,1);",
            "rf001=normalize(1);",
        ] {
            assert!(
                Program::compile(&format!("{body}{IDLE}")).is_err(),
                "accepted {body}"
            );
        }
        assert!(Program::compile(&format!(
            "ri001=int(energy/2);rf001=ri001;clear ri001 when false;{IDLE}"
        ))
        .is_ok());
    }
    #[test]
    fn integer_promotions_saturation_and_total_numeric_helpers_are_explicit() {
        let p=program("ri001=2147483647+1;ri002=-2147483648-1;ri003=2147483647*2147483647;ri004=int(-1.9);ri005=abs(-2147483648);rf001=3/2;rf002=1+0.25;rf003=2147483648;rf004=sqrt(-1);rf005=atan2(0,0);rf006=sign(0);rf007=lerp(2,4,0.25);rf008=1/0;ri006=int(1e300);ri007=int(-1e300);");
        let mut m = Registers::default();
        tick(&p, &mut m, Inputs::default());
        for (name, n) in [
            ("ri001", i32::MAX),
            ("ri002", i32::MIN),
            ("ri003", i32::MAX),
            ("ri004", -1),
            ("ri005", i32::MAX),
            ("ri006", i32::MAX),
            ("ri007", i32::MIN),
        ] {
            assert_eq!(value(&m, name), Value::Integer(n));
        }
        for (name, n) in [
            ("rf001", 1.5),
            ("rf002", 1.25),
            ("rf003", 2147483648.0),
            ("rf004", 0.0),
            ("rf005", 0.0),
            ("rf006", 0.0),
            ("rf007", 2.5),
            ("rf008", 0.0),
        ] {
            assert_eq!(value(&m, name), Value::Scalar(n));
        }
        transition_roundtrip(&p, &p.to_source().unwrap());
        let source = "ri001=ri001+1;rf001=ri001/2;thrust=0;turn=0;divide=false;";
        for seed in 0..100 {
            let variant = Program::mutate_source(source, seed, MutationKind::Expression).unwrap();
            let root = variant
                .program
                .writes
                .iter()
                .find(|rule| rule.register.name() == "rf001")
                .unwrap()
                .value
                .unwrap();
            assert_eq!(variant.program.nodes[root].ty, Type::Number);
            transition_roundtrip(&variant.program, &variant.source);
        }
    }
    #[test]
    fn strings_roundtrip_escape_and_bound_utf8_without_splitting_characters() {
        let exact = "雪".repeat(42) + "ab";
        assert_eq!(exact.len(), 128);
        let p = program(&format!(
            "rs001={};rs002=rs001+\"雪\";rs003=\"\\\"\\\\\\n\\r\\t\\u0000\";ri001=length(rs001);",
            quote(&exact)
        ));
        let mut m = Registers::default();
        tick(&p, &mut m, Inputs::default());
        tick(&p, &mut m, Inputs::default());
        assert_eq!(value(&m, "rs001"), Value::Text(Rc::from(exact.as_str())));
        assert_eq!(value(&m, "rs002"), Value::Text(Rc::from(exact.as_str())));
        assert_eq!(value(&m, "ri001"), Value::Integer(44));
        let json = m.to_json();
        assert!(json.contains("\\\"\\\\\\n\\r\\t\\u0000"));
        assert!(!json.contains('\0'));
        transition_roundtrip(&p, &p.to_source().unwrap());
        for literal in [
            format!("\"{}\"", "雪".repeat(43)),
            "\"\\q\"".into(),
            "\"\\ud800\"".into(),
            "\"unfinished".into(),
            "\"a\nb\"".into(),
        ] {
            assert!(
                Program::compile(&format!("rs001={literal};{IDLE}")).is_err(),
                "accepted {literal}"
            );
        }
        let truncate = program(&format!("rs001={}+\"雪\";", quote(&"a".repeat(127))));
        let mut t = Registers::default();
        tick(&truncate, &mut t, Inputs::default());
        assert_eq!(value(&t, "rs001"), Value::Text(Rc::from("a".repeat(127))));
    }
    #[test]
    fn vectors_are_typed_finite_and_safe_at_zero_length() {
        let p=program("rv001=normalize(vec(0,0));rv002=normalize(vec(3,4));rv003=vec(2,-3)*2;rv004=2*vec(2,-3);rv005=vec(1,2)/0;rv006=vec(1,2)+vec(3,4);rf001=dot(vec(2,3),vec(4,5));rf002=angle(vec(0,0));rf003=length(vec(3,4));rf004=x(vec(5,6));rf005=y(vec(5,6));rb001=vec(1,2)==vec(1,2);");
        let mut m = Registers::default();
        tick(&p, &mut m, Inputs::default());
        for (name, v) in [
            ("rv001", [0.0, 0.0]),
            ("rv002", [0.6, 0.8]),
            ("rv003", [4.0, -6.0]),
            ("rv004", [4.0, -6.0]),
            ("rv005", [0.0, 0.0]),
            ("rv006", [4.0, 6.0]),
        ] {
            assert_eq!(value(&m, name), Value::Vector(v));
        }
        for (name, n) in [
            ("rf001", 23.0),
            ("rf002", 0.0),
            ("rf003", 5.0),
            ("rf004", 5.0),
            ("rf005", 6.0),
        ] {
            assert_eq!(value(&m, name), Value::Scalar(n));
        }
        assert_eq!(value(&m, "rb001"), Value::Boolean(true));
        transition_roundtrip(&p, &p.to_source().unwrap());
    }
    #[test]
    fn deterministic_noise_is_bounded_continuous_and_does_not_advance_random_state() {
        for seed in [-123.0, 0.0, 1.5, 1e12] {
            for phase in [-1e12, -1.0, -0.01, 0.0, 0.5, 1.0, 100.1, 1e12] {
                let n = smooth_noise(seed, phase);
                assert!(n.is_finite() && (-1.0..=1.0).contains(&n));
                assert_eq!(n, smooth_noise(seed, phase));
            }
        }
        for i in -20..20 {
            let phase = i as f64;
            assert!(
                (smooth_noise(3.0, phase - 1e-6) - smooth_noise(3.0, phase + 1e-6)).abs() < 1e-8
            );
        }
        let p = program("rf001=noise(id,time);rf002=noise(id,time);");
        transition_roundtrip(&p, &p.to_source().unwrap());
        let mut m = Registers::default();
        tick(
            &p,
            &mut m,
            Inputs {
                id: 7.0,
                time: 0.7,
                ..Inputs::default()
            },
        );
        assert_eq!(value(&m, "rf001"), value(&m, "rf002"));
    }
    #[test]
    fn new_sensors_outputs_and_omitted_actions_remain_typed_and_bounded() {
        let p=Program::compile("thrust=energy.fraction;turn=flow.bearing;divide=object.valid&&!object.available;photosynthesize=light.level;engulf=object.valid;bite=2;defend=-1;repair=integrity;rf001=memory.used+memory.cost;rf002=heat.value+heat.bearing+light.bearing+x(flow.vector)+y(flow.vector)+flow.speed+object.pigment+light.bearing+object.contrast+object.area+heat.bearing+heat.value+chemical.mineral;rb001=heat.valid;").unwrap();
        let mut m = Registers::default();
        let o = tick(
            &p,
            &mut m,
            Inputs {
                energy_fraction: 0.7,
                flow_bearing: -0.4,
                object_valid: true,
                light_level: 0.6,
                integrity: 0.8,
                memory_used: 3.0,
                memory_cost: 0.0075,
                ..Inputs::default()
            },
        );
        assert_eq!(o.thrust, 0.7);
        assert_eq!(o.turn, -0.4);
        assert!(o.divide && o.engulf);
        assert_eq!(o.photosynthesize, 0.6);
        assert_eq!(o.bite, 1.0);
        assert_eq!(o.defend, 0.0);
        assert_eq!(o.repair, 0.8);
        assert_eq!(value(&m, "rf001"), Value::Scalar(3.0075));
        let defaults = program("").evaluate(&Inputs::default());
        assert_eq!(
            defaults.photosynthesize + defaults.bite + defaults.defend + defaults.repair,
            0.0
        );
        assert!(!defaults.engulf);
        transition_roundtrip(&p, &p.to_source().unwrap());
    }
    #[test]
    fn nutrient_and_body_sensors_are_readonly_scalars_and_sample_current_inputs() {
        let names = [
            "nutrient.stored",
            "nutrient.capacity",
            "nutrient.fraction",
            "chemical.mineral",
            "chemical.mineral_bearing",
            "organelle.flagellum.mass",
            "organelle.photosynthetic.mass",
            "organelle.digestive.mass",
            "organelle.cortex.mass",
            "organelle.optical.mass",
            "body.space.free",
            "organelle.optical.capacity",
        ];
        let source = names
            .iter()
            .enumerate()
            .map(|(index, name)| format!("rf{:03}={name};", index + 1))
            .collect::<String>();
        let p = program(&source);
        let mut memory = Registers::default();
        tick(
            &p,
            &mut memory,
            Inputs {
                nutrient_stored: 1.5,
                nutrient_capacity: 3.0,
                nutrient_fraction: 0.5,
                chemical_mineral: 0.8,
                chemical_mineral_bearing: -1.2,
                organelle: std::array::from_fn(|i| OrganelleInput {
                    mass: match i {
                        7 => 0.05,
                        1 => 0.15,
                        3 => 0.25,
                        9 => 0.35,
                        15 => 0.12,
                        _ => 0.0,
                    },
                    capacity: 88.0,
                    ..OrganelleInput::default()
                }),
                body_space_free: 0.08,
                ..Inputs::default()
            },
        );
        let expected = [
            1.5, 3.0, 0.5, 0.8, -1.2, 0.05, 0.15, 0.25, 0.35, 0.12, 0.08, 88.0,
        ];
        let graph = p.graph_json();
        for (index, (name, expected)) in names.iter().zip(expected).enumerate() {
            assert_eq!(
                value(&memory, &format!("rf{:03}", index + 1)),
                Value::Scalar(expected),
                "{name}"
            );
            assert!(graph.contains(&format!(
                "\"kind\":\"sensor\",\"type\":\"scalar\",\"label\":\"{name}\""
            )));
            assert!(
                Program::compile(&format!("{IDLE}{name}=1;")).is_err(),
                "{name} allowed write"
            );
            assert!(
                Program::compile(&format!("{IDLE}let {name}=1;")).is_err(),
                "{name} allowed shadow"
            );
            assert!(
                Program::compile(&format!("{IDLE}ri001={name};")).is_err(),
                "{name} allowed scalar-to-integer assignment"
            );
        }
        // Same compiled graph samples new tick inputs rather than retaining
        // sensor values from the previous evaluation or from register writes.
        tick(&p, &mut memory, Inputs::default());
        for index in 1..=names.len() {
            assert_eq!(value(&memory, &format!("rf{index:03}")), Value::Scalar(0.0));
        }
        tick(
            &p,
            &mut memory,
            Inputs {
                nutrient_stored: f64::NAN,
                nutrient_capacity: f64::INFINITY,
                organelle: std::array::from_fn(|_| OrganelleInput {
                    capacity: -f64::INFINITY,
                    ..OrganelleInput::default()
                }),
                ..Inputs::default()
            },
        );
        assert_eq!(value(&memory, "rf001"), Value::Scalar(0.0));
        assert_eq!(value(&memory, "rf002"), Value::Scalar(MAX_SIGNAL));
        assert_eq!(value(&memory, "rf012"), Value::Scalar(-MAX_SIGNAL));
        transition_roundtrip(&p, &p.to_source().unwrap());
    }
    #[test]
    fn typed_mutation_serialization_preserves_multitick_memory_transitions() {
        let source = r#"
            let remembered=if populated(rf001) then rf001 else energy;
            let count=ri001+1;
            ri001=count when !heat.valid;
            rf001=remembered*0.8+energy*0.2 when chemical.valid;
            rb001=if chemical.valid then true else rb001;
            rs001=if rb001 then "feed" else rs001+".";
            rv001=normalize(rv001+vec(x(flow.vector),y(flow.vector)));
            ri999=int(energy);rf999=length(rv001);rb999=populated(rs001);rs999="雪";rv999=vec(-1,1);
            clear ri001 when count>15;clear rf001 when heat.valid;
            clear rb001 when flow.valid;clear rs001 when length(rs001)>30;
            clear rv001 when object.available;
            rf002=if populated(rf002) then rf002*0.75+nutrient.stored/nutrient.capacity*0.25 else nutrient.fraction;
            rf003=chemical.mineral+chemical.mineral_bearing/organelle.optical.capacity;
            rb002=nutrient.fraction>0.1;
            clear rf002 when nutrient.fraction>0.2;
            thrust=if rb001 then 0.7 else 0.2;turn=noise(id,time)*0.2+chemical.organic_bearing;
            divide=energy>140&&ri001>5;grow=if body.area<2 then 1 else 0;
            photosynthesize=light.level;engulf=object.valid;bite=if object.available then 0 else 0.8;defend=if heat.valid then 1 else 0;repair=1-integrity;
            absorb=if rb002 then 1-rf002 else 0;
            build.flagellum=organelle.flagellum.mass+rf003*0.05;
            build.photosynthetic=organelle.photosynthetic.mass+light.level*0.2;
            build.digestive=organelle.digestive.mass+0.1*nutrient.fraction;
            build.cortex=if heat.valid then 0.6 else organelle.cortex.mass;
            build.optical=organelle.optical.mass+body.space.free;
        "#;
        let parent = Program::compile(source).unwrap();
        transition_roundtrip(&parent, &parent.to_source().unwrap());
        let mut applied = 0;
        for kind in [
            MutationKind::Any,
            MutationKind::Constant,
            MutationKind::Expression,
            MutationKind::Reference,
        ] {
            for seed in 0..160 {
                let mutation = Program::mutate_source(source, seed, kind).unwrap();
                transition_roundtrip(&mutation.program, &mutation.source);
                applied += 1;
            }
        }
        assert_eq!(applied, 640);
        let mut internal_source = source.to_string();
        let mut restored_source = source.to_string();
        for seed in 1..=250 {
            if let Some(m) = Program::mutate_source(&internal_source, seed * 97, MutationKind::Any)
            {
                transition_roundtrip(&m.program, &m.source);
                internal_source = m.source;
            }
            if let Some(m) = Program::mutate_source(&restored_source, seed * 101, MutationKind::Any)
            {
                transition_roundtrip(&Program::compile(&m.source).unwrap(), &m.source);
                restored_source = m.source;
            }
        }
    }
    #[test]
    fn source_byte_depth_and_node_limits_cover_register_rules_too() {
        let exact = format!("{IDLE}#{}", "x".repeat(MAX_SOURCE_BYTES - IDLE.len() - 1));
        assert_eq!(exact.len(), MAX_SOURCE_BYTES);
        assert!(Program::compile(&exact).is_ok());
        assert!(Program::compile(&(exact + "x")).is_err());
        let exact_nodes = format!("ri001={};{IDLE}", vec!["1"; 4095].join("+"));
        assert_eq!(Program::compile(&exact_nodes).unwrap().nodes.len(), 8192);
        assert!(Program::compile(&(exact_nodes + "rb001=true;")).is_err());
        let nested = format!(
            "ri001={}1{};{IDLE}",
            "(".repeat(MAX_DEPTH),
            ")".repeat(MAX_DEPTH)
        );
        assert!(Program::compile(&nested).unwrap_err().contains("nesting"));
    }
}
