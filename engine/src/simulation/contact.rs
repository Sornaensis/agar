use super::*;
use crate::genome::{PeerContact, Value};

#[derive(Default)]
pub(super) struct CellSignal {
    pub(super) published: Option<f64>,
    pub(super) signal: Option<f64>,
    pub(super) registers: Vec<Option<Value>>,
}

impl CellSignal {
    pub(super) fn freeze(&mut self, cell: &Cell, program: &Program) {
        self.published = program.published_message(&cell.memory);
        self.signal = program.contact_signal(&cell.memory);
        program.freeze_contact_registers(&cell.memory, &mut self.registers);
    }

    pub(super) fn from_cell(cell: &Cell, program: &Program) -> Self {
        let mut snapshot = Self::default();
        snapshot.freeze(cell, program);
        snapshot
    }

    pub(super) fn peer(&self, division_interface: bool) -> PeerContact {
        PeerContact { message: self.published, signal: self.signal, division_interface }
    }
}

impl World {
    /// Both ordinary receptors and per-surface predicates read pre-write state.
    pub(super) fn freeze_contact_signals(&self, genomes: &[Genome], target: &mut Vec<CellSignal>) {
        if !genomes.iter().any(|genome| {
            genome.program.participates_in_contact_signaling() || genome.program.has_contact_guards()
        }) {
            target.clear();
            return;
        }
        target.resize_with(self.cells.len(), CellSignal::default);
        for (cell, snapshot) in self.cells.iter().zip(target) {
            snapshot.freeze(cell, &genomes[(cell.genome_id - 1) as usize].program);
        }
    }
}

/// Per-target contributions from whole-cell engulfing and legacy piercing.
/// Mounted piercers make independent geometry queries in surface.rs.
#[derive(Clone, Copy)]
pub(super) struct ContactCapacity {
    pub engulf: f64,
    pub independent: f64,
    pub bite: f64,
    pub piercing: f64,
}

impl ContactCapacity {
    pub(super) fn for_peer(
        self,
        cell: &Cell,
        output: &Outputs,
        program: &Program,
        local: &CellSignal,
        peer: PeerContact,
        scratch: &mut Vec<Value>,
    ) -> Self {
        let Some(definition) = program.cellular() else {
            return self;
        };
        let Some(state) = &cell.body.cellular else {
            return self;
        };
        let mut physical = Self { engulf: 0.0, independent: 0.0, bite: 0.0, piercing: 0.0 };
        let mut rejected_piercer = false;
        let mut rejected_mouth = false;
        for organ in &cell.body.organs {
            let mass = organ.functional_mass();
            match organ.kind {
                Kind::Piercer => {
                    physical.piercing += mass;
                    if output.cellular.is_none() {
                        physical.bite += mass * output.bite;
                    }
                }
                Kind::Phagocyte if output.cellular.is_none() => physical.engulf += mass,
                _ => {}
            }
        }
        for (instance, a) in state.assemblies.iter().enumerate() {
            let kind = a.recipe.kind;
            if !matches!(
                kind,
                AssemblyKind::Mechanism(MechanismKind::Engulf)
                    | AssemblyKind::Organelle(Kind::Phagocyte | Kind::Piercer)
            ) {
                continue;
            }
            let mass = a.functional_mass();
            if mass == 0.0 {
                continue;
            }
            if cell.body.recipe_definition(definition, instance).is_some_and(|d| {
                program.has_contact_guard(d) && !program.allows_contact(d, &local.registers, peer, scratch)
            }) {
                if kind == AssemblyKind::Organelle(Kind::Piercer) {
                    rejected_piercer = true;
                } else {
                    rejected_mouth = true;
                }
                continue;
            }
            let power = mass * a.activity;
            match kind {
                AssemblyKind::Organelle(Kind::Piercer) => {
                    physical.piercing += mass;
                    physical.bite += if output.cellular.is_some() { power } else { mass * output.bite };
                }
                AssemblyKind::Mechanism(MechanismKind::Engulf) => {
                    physical.engulf += power;
                    physical.independent += power;
                }
                _ => physical.engulf += power,
            }
        }
        // Preserve the ordinary aggregate, including its rounding, when every
        // contributing mechanism accepts. Otherwise sum only accepted hardware.
        if !rejected_mouth {
            physical.engulf = self.engulf;
            physical.independent = self.independent;
        }
        if !rejected_piercer {
            physical.piercing = self.piercing;
            physical.bite = self.bite;
        }
        physical
    }
}

