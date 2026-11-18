//! Named receptors sample their own paid, physical attachment point. Readings
//! are derived from the same immutable world as the legacy observation packet.
use super::*;

impl World {
    /// Player inspection uses the same sampling rules without advancing time or
    /// committing tracking, regulation, or physical state.
    pub fn sense_program(&self, id: u32, program: &Program) -> Option<Inputs> {
        let index = self.cells.iter().position(|cell| cell.id == id && cell.alive())?;
        let capacities: Vec<_> = self.cells.iter().map(|cell| cell.body.capacities()).collect();
        let mut workspace = ObservationWorkspace::default();
        self.observations_into(Some(index), &capacities, &mut workspace);
        let input = &mut workspace.inputs[0];
        if let Some(definition) = program.cellular() {
            self.cells[index].body.cellular_inputs_into(definition, &mut input.products, &mut input.assemblies);
            self.receptor_inputs_into(index, program, input, &capacities,
                &workspace.geometry, &workspace.cell_grid, &workspace.food_grid,
                workspace.max_radius, &mut workspace.receptor_masses);
        }
        workspace.inputs.pop()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn receptor_inputs_into(
        &self, index: usize, program: &Program, input: &mut Inputs,
        capacities: &[BodyCapacities], geometry: &CellGeometry,
        cell_grid: &Grid, food_grid: &Grid, max_radius: f64, masses: &mut Vec<f64>,
    ) {
        let Some(definition) = program.cellular() else { return; };
        input.receptors.resize(definition.assemblies.len(), ReceptorInput::default());
        input.receptors.fill(ReceptorInput::default());
        let mut requested = 0_u64;
        for &(recipe, _) in program.receptor_reads() {
            if recipe < 64 { requested |= 1_u64 << recipe; }
        }
        if requested == 0 { return; }
        let cell = &self.cells[index];
        cell.body.active_recipe_masses_into(definition, masses);
        while requested != 0 {
            let recipe_index = requested.trailing_zeros() as usize;
            requested &= requested - 1;
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
            let x = wrap(cell.x + cell.radius() * outward[0], WIDTH);
            let y = wrap(cell.y + cell.radius() * outward[1], HEIGHT);
            let reading = &mut input.receptors[recipe_index];
            reading.available = true;
            match kind {
                MechanismKind::SenseLight => {
                    reading.valid = true;
                    reading.level = measure(self.environment.light_at(x, y), mass, 0.0005).clamp(0.0, 1.0);
                }
                MechanismKind::SenseHeat => {
                    reading.valid = true;
                    reading.value = measure(self.environment.temperature_at(x, y), mass, 0.0005).clamp(0.0, 1.0);
                }
                MechanismKind::SenseChemical => {
                    reading.valid = true;
                    reading.mineral = measure(self.environment.nutrient_at(x, y), mass, 0.001).max(0.0);
                    reading.organic = measure(self.environment.organic_at(x, y), mass, 0.001).max(0.0);
                }
                MechanismKind::SenseContact | MechanismKind::SenseOptical => {
                    // Flow is observable even when nothing touches this receptor.
                    if kind == MechanismKind::SenseContact {
                        let (vx, vy) = self.environment.flow_at(x, y, self.time());
                        reading.flow = local_vector(vx - cell.vx, vy - cell.vy, cell.angle)
                            .map(|v| measure(v, mass, 0.03));
                        reading.speed = reading.flow[0].hypot(reading.flow[1]);
                        reading.bearing = if reading.speed > 1e-12 {
                            reading.flow[1].atan2(reading.flow[0])
                        } else { 0.0 };
                    }
                    if geometry.positions.len() != self.cells.len() { continue; }
                    let range = if kind == MechanismKind::SenseOptical {
                        (35.0 + 300.0 * mass.sqrt()).min(190.0)
                    } else { 0.9 };
                    let mut selected: Option<(Observation, f64)> = None;
                    let mut consider = |mut observation: Observation| {
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
                                wrap(x + observation.dx, WIDTH), wrap(y + observation.dy, HEIGHT)
                            ).clamp(0.0, 1.0);
                            observation.contrast *= illumination;
                            if observation.contrast < 0.005 / (1.0 + 10.0 * mass.sqrt())
                                || distance > range * illumination.sqrt() + observation.radius {
                                return;
                            }
                            score = observation.contrast / (10.0 + distance) / (1.0 + angle.abs());
                            if !self.optical_line_clear_from(index, [x, y], &observation,
                                cell_grid, max_radius, geometry) { return; }
                        }
                        if selected.as_ref().is_none_or(|(old, best)|
                            score > *best || (score == *best && observation.entity < old.entity)) {
                            selected = Some((observation, score));
                        }
                    };
                    for other in cell_grid.nearby_radius(x, y, range + max_radius) {
                        if other == index || !geometry.alive[other] { continue; }
                        let [ox, oy] = geometry.positions[other];
                        let [vx, vy] = geometry.velocities[other];
                        let pigment = geometry.pigments[other];
                        let contact_angle = relative_angle(delta(y - oy, HEIGHT)
                            .atan2(delta(x - ox, WIDTH)) - geometry.angles[other]);
                        consider(Observation {
                            entity: (geometry.ids[other] as u64) << 1,
                            dx: delta(ox - x, WIDTH), dy: delta(oy - y, HEIGHT),
                            radius: geometry.radii[other], area: geometry.areas[other],
                            contrast: 0.45 + 0.4 * pigment, pigment,
                            motion: ObservedMotion::Relative(vx - cell.vx, vy - cell.vy),
                            resistance: self.contact_protection(other, contact_angle, 1.0, capacities) + 0.15,
                        });
                    }
                    for particle in food_grid.nearby_radius(x, y, range + 3.5) {
                        let food = &self.food[particle];
                        consider(Observation {
                            entity: (food.id << 1) | 1,
                            dx: delta(food.x - x, WIDTH), dy: delta(food.y - y, HEIGHT),
                            radius: 2.5, area: (2.5 / CELL_RADIUS).powi(2),
                            contrast: [0.28, 0.4, 0.32][food.kind as usize],
                            pigment: [0.1, 0.24, 0.06][food.kind as usize],
                            motion: ObservedMotion::ParticleAt(food.x, food.y),
                            resistance: [0.02, 0.07, 0.04][food.kind as usize],
                        });
                    }
                    if let Some((observation, _)) = selected {
                        reading.valid = true;
                        let (vx, vy) = observation.relative_motion(cell, &self.environment, self.time());
                        reading.offset = local_vector(observation.dx, observation.dy, cell.angle)
                            .map(|v| measure(v, mass, 0.08));
                        if kind == MechanismKind::SenseContact {
                            reading.pressure = measure(
                                (observation.radius - observation.dx.hypot(observation.dy)).max(0.0)
                                + (-vx * observation.dx - vy * observation.dy).max(0.0)
                                    / (1.0 + observation.dx.hypot(observation.dy)), mass, 0.01);
                            reading.resistance = measure(observation.resistance, mass, 0.002);
                        } else {
                            reading.area = measure(observation.area, mass, 0.002).max(0.02);
                            reading.contrast = measure(observation.contrast, mass, 0.001).clamp(0.0, 1.0);
                            reading.pigment = measure(observation.pigment, mass, 0.001).clamp(0.0, 1.0);
                            reading.motion = local_vector(vx, vy, cell.angle).map(|v| measure(v, mass, 0.03));
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
        let midpoint_x = wrap(x + target.dx * 0.5, WIDTH);
        let midpoint_y = wrap(y + target.dy * 0.5, HEIGHT);
        let reach = length_squared.sqrt() * 0.5 + max_radius;
        for index in grid.nearby_radius(midpoint_x, midpoint_y, reach) {
            if index == observer || (geometry.ids[index] as u64) << 1 == target.entity || !geometry.alive[index] {
                continue;
            }
            let dx = delta(geometry.positions[index][0] - x, WIDTH);
            let dy = delta(geometry.positions[index][1] - y, HEIGHT);
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
