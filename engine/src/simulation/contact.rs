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
        self.published = program.published_message(&cell.memory)
            .filter(|_| has_active_surface(cell, MechanismKind::Display));
        self.signal = Self::outgoing_payload(cell, program);
        program.freeze_contact_registers(&cell.memory, &mut self.registers);
    }

    pub(super) fn outgoing_payload(cell: &Cell, program: &Program) -> Option<f64> {
        program.contact_signal(&cell.memory).filter(|_| has_active_surface(cell, MechanismKind::Display)
            || has_active_surface(cell, MechanismKind::JunctionSignal))
    }

    pub(super) fn from_cell(cell: &Cell, program: &Program) -> Self {
        let mut snapshot = Self::default();
        snapshot.freeze(cell, program);
        snapshot
    }

    /// Read at the position of an actual working touch receptor. Registers
    /// are frozen at tick start; the read also needs a facing, active display.
    pub(super) fn displayed_at(&self, remote: &Cell, receptor: [f64; 2], env: &Environment) -> PeerContact {
        let mut result = PeerContact::default();
        if self.published.is_none() && self.signal.is_none() { return result; }
        let Some(state) = &remote.body.cellular else { return result; };
        for assembly in &state.assemblies {
            if assembly.recipe.kind != AssemblyKind::Mechanism(MechanismKind::Display) { continue; }
            let active = assembly.working_mass() * assembly.activity;
            if active <= 0.0 { continue; }
            let patch = mount_point(remote, assembly);
            let distance = patch_distance(patch, receptor, env);
            if distance <= 0.9 + patch_radius(active, remote.radius())
                && !env.segment_blocked((patch[0], patch[1]), (receptor[0], receptor[1])) {
                result.message = self.published;
                result.signal = self.signal;
                break;
            }
        }
        result
    }

    pub(super) fn peer_at(&self, remote: &Cell, actor: &Cell, point: [f64; 2], env: &Environment,
        division_interface: bool) -> PeerContact {
        let mut result = PeerContact { division_interface, ..Default::default() };
        let Some(state) = &actor.body.cellular else { return result; };
        for assembly in &state.assemblies {
            if !matches!(assembly.recipe.kind, AssemblyKind::Mechanism(MechanismKind::SenseContact)
                | AssemblyKind::Organelle(Kind::Mechanoreceptor)) { continue; }
            let active = assembly.working_mass() * assembly.activity;
            if active <= 0.0 { continue; }
            let receptor = mount_point(actor, assembly);
            if patch_distance(receptor, point, env) > patch_radius(active, actor.radius())
                || patch_distance(receptor, [remote.x,remote.y], env) > remote.radius() + 0.9 { continue; }
            if env.segment_blocked((receptor[0],receptor[1]), (remote.x,remote.y)) { continue; }
            if result.resistance.is_none() {
                let dx = delta(receptor[0] - remote.x, env.width);
                let dy = delta(receptor[1] - remote.y, env.height);
                let value = remote.body.protection_at(relative_angle(dy.atan2(dx) - remote.angle), 1.0) + 0.15;
                result.resistance = Some(measure(value, active, 0.002));
            }
            if self.published.is_none() && self.signal.is_none() { break; }
            let observed = self.displayed_at(remote, receptor, env);
            if observed.message.is_some() || observed.signal.is_some() {
                result.message = observed.message;
                result.signal = observed.signal;
                break;
            }
        }
        result
    }
}

fn has_active_surface(cell: &Cell, kind: MechanismKind) -> bool {
    cell.body.cellular.as_ref().is_some_and(|state| state.assemblies.iter().any(|a|
        a.recipe.kind == AssemblyKind::Mechanism(kind) && a.activity > 0.0 && a.working_mass() > 0.0))
}
fn mount_point(cell: &Cell, assembly: &crate::cellular::AssemblyInstance) -> [f64; 2] {
    let at = cell.angle + cell.body.mount_at(assembly);
    [cell.x + cell.radius() * at.cos(), cell.y + cell.radius() * at.sin()]
}
fn patch_radius(active: f64, body_radius: f64) -> f64 {
    (1.0 + 15.0 * active.sqrt()).min(body_radius * 0.75)
}
fn patch_distance(a: [f64; 2], b: [f64; 2], env: &Environment) -> f64 {
    delta(a[0] - b[0], env.width).hypot(delta(a[1] - b[1], env.height))
}
pub(super) fn measured_resistance(actor: &Cell, point: [f64; 2], target: [f64; 2],
    radius: f64, env: &Environment, property: impl FnOnce() -> f64) -> Option<f64> {
    let state = actor.body.cellular.as_ref()?;
    for assembly in &state.assemblies {
        if !matches!(assembly.recipe.kind, AssemblyKind::Mechanism(MechanismKind::SenseContact)
            | AssemblyKind::Organelle(Kind::Mechanoreceptor)) { continue; }
        let active = assembly.working_mass() * assembly.activity;
        let receptor = mount_point(actor, assembly);
        if active > 0.0 && patch_distance(receptor, point, env) <= patch_radius(active, actor.radius())
            && patch_distance(receptor, target, env) <= radius + 0.9
            && !env.segment_blocked((receptor[0], receptor[1]), (target[0], target[1])) {
            return Some(measure(property(), active, 0.002));
        }
    }
    None
}
impl World {
    pub(super) fn peer_at(&self, actor: usize, other: usize, point: [f64; 2], division: bool,
        signals: &[CellSignal]) -> PeerContact {
        signals.get(other).map_or(PeerContact { division_interface: division, ..Default::default() }, |signal|
            signal.peer_at(&self.cells[other], &self.cells[actor], point, &self.environment, division))
    }
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

/// Mounted primitive piercers and mouths query from their own membrane patches.
/// Older piercer families use their historical forward attack geometry.
#[derive(Clone, Copy)]
pub(super) struct ContactCapacity {
    pub bite: f64,
    pub piercing: f64,
}

impl ContactCapacity {
    pub(super) fn for_peer(self, cell: &Cell, output: &Outputs, program: &Program,
        local: &CellSignal, peer: PeerContact, scratch: &mut Vec<Value>) -> Self {
        let (Some(definition), Some(state)) = (program.cellular(), &cell.body.cellular) else { return self; };
        let unmounted: f64 = cell.body.organs.iter().filter(|o| o.kind == Kind::Piercer)
            .map(|o| o.functional_mass()).sum();
        let mut physical = Self { piercing: unmounted, bite: if output.cellular.is_none() { unmounted * output.bite } else { 0.0 } };
        let mut rejected = false;
        for (instance, a) in state.assemblies.iter().enumerate() {
            if a.recipe.kind != AssemblyKind::Organelle(Kind::Piercer) { continue; }
            let mass = a.working_mass();
            if mass <= 0.0 { continue; }
            if cell.body.recipe_definition(definition, instance).is_some_and(|d|
                program.has_contact_guard(d) && !program.allows_contact(d, &local.registers, peer, scratch)) {
                rejected = true;
                continue;
            }
            physical.piercing += mass;
            physical.bite += mass * if output.cellular.is_some() { a.activity } else { output.bite };
        }
        if rejected { physical } else { self }
    }
}
