//! Named receptors sample their own paid, physical attachment point. Readings
//! are derived from the same immutable world as the legacy observation packet.
use super::*;
use crate::genome::ReceptorField;

impl World {
    /// Player inspection uses the same sampling rules without advancing time or
    /// committing tracking, regulation, or physical state.
    pub fn sense_program(&self, id: u32, program: &Program) -> Option<Inputs> {
        self.sense_program_in_registry(id, program, &[])
    }
    pub fn sense_program_in_registry(&self, id: u32, program: &Program, genomes: &[Genome]) -> Option<Inputs> {
        let index = self.cells.iter().position(|cell| cell.id == id && cell.alive())?;
        let capacities: Vec<_> = self.cells.iter().map(|cell| cell.body.capacities()).collect();
        let mut workspace = ObservationWorkspace::default();
        self.observations_into(Some(index), &capacities, &mut workspace, None);
        let input = &mut workspace.inputs[0];
        if let Some(definition) = program.cellular() {
            let mut signals = Vec::new();
            self.freeze_contact_signals(genomes, &mut signals);
            self.cells[index].body.cellular_inputs_into(definition, &mut input.products, &mut input.assemblies);
            self.receptor_inputs_into(index, program, input, &capacities,
                &workspace.geometry, &workspace.cell_grid, &workspace.food_grid,
                workspace.max_radius, &mut workspace.receptor_masses, true, &signals);
        }
        workspace.inputs.pop()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn receptor_inputs_into(
        &self, index: usize, program: &Program, input: &mut Inputs,
        capacities: &[BodyCapacities], geometry: &CellGeometry,
        cell_grid: &Grid, food_grid: &Grid, max_radius: f64, masses: &mut Vec<f64>,
        complete: bool, signals: &[CellSignal],
    ) {
        let Some(definition) = program.cellular() else { return; };
        input.receptors.resize(definition.assemblies.len(), ReceptorInput::default());
        input.receptors.fill(ReceptorInput::default());
        let mut requested = 0_u64;
        let mut fields = [0_u32; 64];
        for &(recipe, field) in program.receptor_reads() {
            if recipe < 64 {
                requested |= 1_u64 << recipe;
                fields[recipe] |= 1_u32 << field as u8;
            }
        }
        if requested == 0 { return; }
        let cell = &self.cells[index];
        cell.body.active_recipe_masses_into(definition, masses);
        while requested != 0 {
            let recipe_index = requested.trailing_zeros() as usize;
            requested &= requested - 1;
            let fields = if complete { u32::MAX } else { fields[recipe_index] };
            let reads = |field: ReceptorField| fields & (1_u32 << field as u8) != 0;
            let Some(recipe) = definition.assemblies.get(recipe_index) else { continue; };
            let mass = masses.get(recipe_index).copied().unwrap_or(0.0);
            if mass <= 0.0 { continue; }
            let kind = match recipe.kind {
                AssemblyKind::Mechanism(kind) => kind,
                AssemblyKind::Organelle(Kind::Photoreceptor) => MechanismKind::SenseLight,
                AssemblyKind::Organelle(Kind::Thermoreceptor) => MechanismKind::SenseHeat,
                AssemblyKind::Organelle(Kind::Chemoreceptor) => MechanismKind::SenseChemical,
                AssemblyKind::Organelle(Kind::Mechanoreceptor) => MechanismKind::SenseContact,
                AssemblyKind::Organelle(Kind::Optical) => MechanismKind::SenseOptical,
                _ => continue,
            };
            let attachment = cell.angle + recipe.at;
            let outward = [attachment.cos(), attachment.sin()];
            let x = wrap(cell.x + cell.radius() * outward[0], self.environment.width);
            let y = wrap(cell.y + cell.radius() * outward[1], self.environment.height);
            let reading = &mut input.receptors[recipe_index];
            reading.available = true;
            match kind {
                MechanismKind::SenseLight => {
                    reading.valid = true;
                    if reads(ReceptorField::Level) {
                        reading.level = measure(self.environment.light_at(x, y), mass, 0.0005).clamp(0.0, 1.0);
                    }
                }
                MechanismKind::SenseHeat => {
                    reading.valid = true;
                    if reads(ReceptorField::Value) {
                        reading.value = measure(self.environment.temperature_at(x, y), mass, 0.0005).clamp(0.0, 1.0);
                    }
                }
                MechanismKind::SenseChemical => {
                    reading.valid = true;
                    if reads(ReceptorField::Mineral) {
                        reading.mineral = measure(self.environment.nutrient_at(x, y), mass, 0.001).max(0.0);
                    }
                    if reads(ReceptorField::Organic) {
                        reading.organic = measure(self.environment.organic_at(x, y), mass, 0.001).max(0.0);
                    }
                }
                MechanismKind::SenseContact | MechanismKind::SenseOptical => {
                    // Flow is observable even when nothing touches this receptor.
                    if kind == MechanismKind::SenseContact &&
                        (reads(ReceptorField::Flow) || reads(ReceptorField::Speed) || reads(ReceptorField::Bearing)) {
                        let (vx, vy) = self.environment.flow_at(x, y, self.time());
                        reading.flow = local_vector(vx - cell.vx, vy - cell.vy, cell.angle)
                            .map(|v| measure(v, mass, 0.03));
                        reading.speed = reading.flow[0].hypot(reading.flow[1]);
                        reading.bearing = if reading.speed > 1e-12 {
                            reading.flow[1].atan2(reading.flow[0])
                        } else { 0.0 };
                    }
                    // Availability and ambient flow do not require a target.
                    if ![ReceptorField::Valid, ReceptorField::Offset, ReceptorField::Pressure,
                        ReceptorField::Resistance, ReceptorField::Area, ReceptorField::Contrast,
                        ReceptorField::Pigment, ReceptorField::Motion, ReceptorField::MessageValid,
                        ReceptorField::Message, ReceptorField::SignalValid, ReceptorField::Signal]
                        .iter().any(|&field| reads(field)) {
                        continue;
                    }
                    if geometry.positions.len() != self.cells.len() { continue; }
                    let range = if kind == MechanismKind::SenseOptical {
                        (35.0 + 300.0 * mass.sqrt()).min(190.0)
                    } else { 0.9 };
                    let mut selected: Option<(Observation, f64, Option<usize>)> = None;
                    let mut consider = |mut observation: Observation, target: Option<usize>| {
                        let distance = observation.dx.hypot(observation.dy);
                        let gap = distance - observation.radius;
                        let score;
                        if kind == MechanismKind::SenseContact {
                            if gap > 0.9 { return; }
                            score = -gap;
                        } else {
                            let angle = relative_angle(observation.dy.atan2(observation.dx)
                                - cell.angle - recipe.direction);
                            if angle.abs() > PI * 0.5 || distance > range + observation.radius {
                                return;
                            }
                            // A receptor cannot see through the cell it is mounted on.
                            if observation.dx * outward[0] + observation.dy * outward[1] < -1e-9 {
                                return;
                            }
                            let illumination = self.environment.light_at(
                                wrap(x + observation.dx, self.environment.width), wrap(y + observation.dy, self.environment.height)
                            ).clamp(0.0, 1.0);
                            observation.contrast *= illumination;
                            if observation.contrast < 0.005 / (1.0 + 10.0 * mass.sqrt())
                                || distance > range * illumination.sqrt() + observation.radius {
                                return;
                            }
                            score = observation.contrast / (10.0 + distance) / (1.0 + angle.abs());
                        }
                        if selected.as_ref().is_none_or(|(old, best, _)|
                            score > *best || (score == *best && observation.entity < old.entity))
                            && (kind != MechanismKind::SenseOptical ||
                                self.optical_line_clear_from(index, [x, y], &observation,
                                    cell_grid, max_radius, geometry)) {
                            selected = Some((observation, score, target));
                        }
                    };
                    for other in cell_grid.nearby_radius(x, y, range + max_radius) {
                        if other == index || !geometry.alive[other] { continue; }
                        let [ox, oy] = geometry.positions[other];
                        let [vx, vy] = geometry.velocities[other];
                        let pigment = geometry.pigments[other];
                        consider(Observation {
                            entity: (geometry.ids[other] as u64) << 1,
                            dx: delta(ox - x, self.environment.width), dy: delta(oy - y, self.environment.height),
                            radius: geometry.radii[other], area: geometry.areas[other],
                            contrast: 0.45 + 0.4 * pigment, pigment,
                            motion: ObservedMotion::Relative(vx - cell.vx, vy - cell.vy),
                            resistance: 0.0,
                        }, Some(other));
                    }
                    for particle in food_grid.nearby_radius(x, y, range + 3.5) {
                        let food = &self.food[particle];
                        consider(Observation {
                            entity: (food.id << 1) | 1,
                            dx: delta(food.x - x, self.environment.width), dy: delta(food.y - y, self.environment.height),
                            radius: 2.5, area: (2.5 / CELL_RADIUS).powi(2),
                            contrast: [0.28, 0.4, 0.32][food.kind as usize],
                            pigment: [0.1, 0.24, 0.06][food.kind as usize],
                            motion: ObservedMotion::ParticleAt(food.x, food.y),
                            resistance: [0.02, 0.07, 0.04][food.kind as usize],
                        }, None);
                    }
                    if let Some((mut observation, _, target)) = selected {
                        reading.valid = true;
                        let (vx, vy) = if (kind == MechanismKind::SenseContact && reads(ReceptorField::Pressure))
                            || (kind == MechanismKind::SenseOptical && reads(ReceptorField::Motion)) {
                            observation.relative_motion(cell, &self.environment, self.time())
                        } else { (0.0, 0.0) };
                        if reads(ReceptorField::Offset) {
                            reading.offset = local_vector(observation.dx, observation.dy, cell.angle)
                                .map(|v| measure(v, mass, 0.08));
                        }
                        if kind == MechanismKind::SenseContact {
                            let identity = reads(ReceptorField::MessageValid) || reads(ReceptorField::Message);
                            let payload = reads(ReceptorField::SignalValid) || reads(ReceptorField::Signal);
                            if let Some(peer) = target.and_then(|other| signals.get(other)) {
                                if ((identity && peer.published.is_some()) || (payload && peer.signal.is_some()))
                                    && !self.environment.segment_blocked((x,y),(x+observation.dx,y+observation.dy)) {
                                    if identity {
                                        reading.message_valid = peer.published.is_some();
                                        reading.message = peer.published.unwrap_or(0.0);
                                    }
                                    if payload {
                                        reading.signal_valid = peer.signal.is_some();
                                        reading.signal = peer.signal.unwrap_or(0.0);
                                    }
                                }
                            }
                            if reads(ReceptorField::Pressure) { reading.pressure = measure(
                                (observation.radius - observation.dx.hypot(observation.dy)).max(0.0)
                                + (-vx * observation.dx - vy * observation.dy).max(0.0)
                                    / (1.0 + observation.dx.hypot(observation.dy)), mass, 0.01); }
                            if reads(ReceptorField::Resistance) {
                                if let Some(other) = target {
                                    let [ox, oy] = geometry.positions[other];
                                    let contact_angle = relative_angle(delta(y - oy, self.environment.height)
                                        .atan2(delta(x - ox, self.environment.width)) - geometry.angles[other]);
                                    observation.resistance = self.contact_protection(other, contact_angle, 1.0, capacities) + 0.15;
                                }
                                reading.resistance = measure(observation.resistance, mass, 0.002);
                            }
                        } else {
                            if reads(ReceptorField::Area) { reading.area = measure(observation.area, mass, 0.002).max(0.02); }
                            if reads(ReceptorField::Contrast) { reading.contrast = measure(observation.contrast, mass, 0.001).clamp(0.0, 1.0); }
                            if reads(ReceptorField::Pigment) { reading.pigment = measure(observation.pigment, mass, 0.001).clamp(0.0, 1.0); }
                            if reads(ReceptorField::Motion) { reading.motion = local_vector(vx, vy, cell.angle).map(|v| measure(v, mass, 0.03)); }
                        }
                    }
                }
                _ => reading.available = false,
            }
        }
    }

    fn optical_line_clear_from(&self, observer: usize, origin: [f64; 2],
        target: &Observation, grid: &Grid, max_radius: f64, geometry: &CellGeometry) -> bool {
        let [x, y] = origin;
        let length_squared = target.dx * target.dx + target.dy * target.dy;
        if self.environment.segment_blocked((x,y),(x+target.dx,y+target.dy)) { return false; }
        let midpoint_x = wrap(x + target.dx * 0.5, self.environment.width);
        let midpoint_y = wrap(y + target.dy * 0.5, self.environment.height);
        let reach = length_squared.sqrt() * 0.5 + max_radius;
        for index in grid.nearby_radius(midpoint_x, midpoint_y, reach) {
            if index == observer || (geometry.ids[index] as u64) << 1 == target.entity || !geometry.alive[index] {
                continue;
            }
            let dx = delta(geometry.positions[index][0] - x, self.environment.width);
            let dy = delta(geometry.positions[index][1] - y, self.environment.height);
            let t = if length_squared > 1e-12 {
                ((dx * target.dx + dy * target.dy) / length_squared).clamp(0.0, 1.0)
            } else { 0.0 };
            let nx = dx - t * target.dx;
            let ny = dy - t * target.dy;
            if nx * nx + ny * ny + 1e-9 < geometry.radii[index].powi(2) { return false; }
        }
        true
    }
}
