//! Surface mechanisms interact from their actual attachment, while legacy
//! organs retain the existing whole-cell targeting and protection rules.
use super::*;

impl World {
    pub(super) fn legacy_shield_effort(&self, victim: usize, output: &Outputs) -> f64 {
        if output.cellular.is_none() { return output.defend; }
        let body = &self.cells[victim].body;
        if !body.cellular.as_ref().is_some_and(|state| state.assemblies.iter().any(|assembly|
            assembly.recipe.kind == AssemblyKind::Mechanism(MechanismKind::Shield))) {
            return output.defend;
        }
        let total = body.functional_mass(Kind::Cortex);
        if total <= 0.0 { return 0.0; }
        let active = body.cellular.as_ref().map_or(0.0, |state|
            state.assemblies.iter().filter(|assembly| assembly.recipe.kind == AssemblyKind::Organelle(Kind::Cortex))
                .map(|assembly| assembly.functional_mass() * assembly.activity).sum::<f64>());
        (active / total).clamp(0.0, 1.0)
    }

    pub(super) fn contact_protection(&self, victim: usize, angle: f64,
        defend: f64, capacities: &[BodyCapacities]) -> f64 {
        let body = &self.cells[victim].body;
        if body.cellular.as_ref().is_some_and(|state| state.assemblies.iter().any(|assembly|
            assembly.recipe.kind == AssemblyKind::Mechanism(MechanismKind::Shield))) {
            body.protection_at(angle, defend)
        } else {
            (capacities[victim].protection * (0.5 + 0.5 * defend)).min(0.9)
        }
    }

    pub(super) fn primitive_piercing_targets(
        &self,
        outputs: &[Outputs],
        genomes: &[Genome],
        workspace: &mut PhaseWorkspace,
    ) {
        let PhaseWorkspace { geometry, cell_grid, primitive_hits, .. } = workspace;
        primitive_hits.clear();
        let max_radius = geometry.radii.iter().copied().fold(0.0_f64, f64::max);
        for (attacker, cell) in self.cells.iter().enumerate() {
            if !cell.alive() {
                continue;
            }
            let Some(state) = &cell.body.cellular else {
                continue;
            };
            for (instance, assembly) in state.assemblies.iter().enumerate() {
                if assembly.recipe.kind != AssemblyKind::Mechanism(MechanismKind::Pierce) {
                    continue;
                }
                let mass = assembly.working_mass();
                let activity = if outputs[attacker].cellular.is_some() {
                    assembly.activity
                } else {
                    outputs[attacker].bite * assembly.activity
                };
                let power = mass * activity;
                if power <= 0.0 {
                    continue;
                }
                let at = cell.angle + assembly.at();
                let outward = [at.cos(), at.sin()];
                let x = wrap(cell.x + cell.radius() * outward[0], self.environment.width);
                let y = wrap(cell.y + cell.radius() * outward[1], self.environment.height);
                let reach = 0.75 + 12.0 * mass.sqrt();
                let guard = genomes.get((cell.genome_id - 1) as usize).and_then(|g| {
                    if !g.program.has_contact_guards() {
                        return None;
                    }
                    let def = cell.body.recipe_definition(g.program.cellular()?, instance)?;
                    g.program.has_contact_guard(def).then_some((&g.program, def))
                });
                let mut best: Option<(usize, f64)> = None;
                for victim in cell_grid.nearby_radius(x, y, reach + max_radius) {
                    if victim == attacker || !self.cells[victim].alive() {
                        continue;
                    }
                    let dx = delta(geometry.positions[victim][0] - x, self.environment.width);
                    let dy = delta(geometry.positions[victim][1] - y, self.environment.height);
                    if self.environment.segment_blocked((x, y), (x + dx, y + dy)) {
                        continue;
                    }
                    let gap = dx.hypot(dy) - geometry.radii[victim];
                    if gap > reach
                        || dx * outward[0] + dy * outward[1] < -1e-9
                        || relative_angle(dy.atan2(dx) - cell.angle - assembly.direction()).abs()
                            > PI * 0.4
                    {
                        continue;
                    }
                    if let Some((program, def)) = guard {
                        if !program.allows_contact(
                            def,
                            &workspace.signals[attacker].registers,
                            self.peer_at(attacker, victim, [x,y], self.division_interface(cell.id, self.cells[victim].id), &workspace.signals),
                            &mut workspace.contact_scratch,
                        ) {
                            continue;
                        }
                    }
                    if best.is_none_or(|(old, distance)| {
                        gap < distance || (gap == distance && geometry.ids[victim] < geometry.ids[old])
                    }) {
                        best = Some((victim, gap));
                    }
                }
                if let Some((victim, _)) = best {
                    let angle = relative_angle(
                        delta(y - geometry.positions[victim][1], self.environment.height)
                            .atan2(delta(x - geometry.positions[victim][0], self.environment.width))
                            - geometry.angles[victim],
                    );
                    primitive_hits.push((attacker, victim, power, angle));
                }
            }
        }
    }


}
