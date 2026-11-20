//! Membrane mouths own their position, usable rate and per-contact predicate.
use super::*;
use crate::genome::PeerContact;

pub(super) struct ParticleInlet {
    cell: usize,
    id: u32,
    origin: [f64; 2],
    reach: f64,
    budget: f64,
    guard: Option<usize>,
}

fn mount(cell: &Cell, a: &crate::cellular::AssemblyInstance) -> [f64; 2] {
    let angle = cell.angle + cell.body.mount_at(a);
    [cell.x + cell.radius() * angle.cos(), cell.y + cell.radius() * angle.sin()]
}
fn guard_of(cell: &Cell, genomes: &[Genome], instance: usize) -> Option<usize> {
    let p = &genomes.get((cell.genome_id - 1) as usize)?.program;
    if !p.has_contact_guards() { return None; }
    let definition = cell.body.recipe_definition(p.cellular()?, instance)?;
    p.has_contact_guard(definition).then_some(definition)
}

impl World {
    pub(super) fn eat_in(&mut self, outputs: &[Outputs], caps: &[BodyCapacities], genomes: &[Genome], ws: &mut PhaseWorkspace) {
        if self.food.is_empty() || self.cells.is_empty() { return; }
        for cell in &mut self.cells {
            if let Some(q) = &mut cell.counted {
                q.capture_credit.retain(|_, entry| self.ticks.saturating_sub(entry.1) <= 1);
            }
        }
        let ports = &mut ws.inlets;
        ports.clear();
        let grid = &mut ws.cell_grid;
        grid.configure(self.environment.width, self.environment.height);
        grid.clear();
        let mut max_reach: f64 = 0.0;
        for (i, c) in self.cells.iter().enumerate() {
            if !ws.geometry.alive[i] || caps[i].capture <= 0.0 { continue; }
            let feed = if outputs[i].cellular.is_some() { 1.0 } else { outputs[i].feed };
            if feed <= 0.0 { continue; }
            let mut add = |position: [f64; 2], reach: f64, power: f64, guard, id| {
                if power <= 0.0 { return; }
                let origin = [wrap(position[0], self.environment.width), wrap(position[1], self.environment.height)];
                grid.insert(origin[0], origin[1], ports.len());
                max_reach = max_reach.max(reach);
                ports.push(ParticleInlet { cell:i, id, origin, reach, budget:power * PARTICLE_CAPTURE_RATE * DT, guard });
            };
            let mut mounted = false;
            if let Some(state) = &c.body.cellular {
                for (instance, a) in state.assemblies.iter().enumerate() {
                    if !matches!(a.recipe.kind, AssemblyKind::Mechanism(MechanismKind::ParticleCapture)
                        | AssemblyKind::Organelle(Kind::Phagocyte)) { continue; }
                    mounted = true;
                    let mass = a.working_mass();
                    add(mount(c, a), 2.5 + 6.0 * mass.sqrt(), mass * a.activity * feed,
                        guard_of(c, genomes, instance), a.id);
                }
            }
            // Historical, unmounted organelles carry no attachment coordinates.
            let legacy = if mounted {
                c.body.organs.iter().filter(|o| o.kind == Kind::Phagocyte).map(|o| o.functional_mass()).sum()
            } else { caps[i].capture };
            add([c.x, c.y], c.radius() + 2.5, legacy * feed, None, 0);
        }
        if ports.is_empty() { return; }
        self.food.retain_mut(|food| {
            let contacts = &mut ws.inlet_contacts;
            contacts.clear();
            for index in grid.nearby_radius(food.x, food.y, max_reach) {
                let port = &ports[index];
                let cell = &self.cells[port.cell];
                if port.budget <= 0.0 || cell.gut_fraction_for(food.energy, food.nutrients, &caps[port.cell]) <= 0.0 { continue; }
                let dx = delta(food.x - port.origin[0], self.environment.width);
                let dy = delta(food.y - port.origin[1], self.environment.height);
                let distance = dx.hypot(dy);
                if distance > port.reach || self.environment.segment_blocked((port.origin[0], port.origin[1]), (port.origin[0]+dx, port.origin[1]+dy)) { continue; }
                if let Some(def) = port.guard {
                    let resistance = contact::measured_resistance(cell, port.origin, [food.x, food.y], 2.5, &self.environment, || food.traits().0);
                    let peer = PeerContact { resistance, ..Default::default() };
                    if !genomes[(cell.genome_id - 1) as usize].program.allows_contact(def,
                        &ws.signals[port.cell].registers, peer, &mut ws.contact_scratch) { continue; }
                }
                contacts.push((index, distance));
            }
            contacts.sort_unstable_by(|&(a, da), &(b, db)| {
                let tie = |n: usize| {
                    let p = &ports[n];
                    (self.cells[p.cell].id, p.id)
                };
                da.total_cmp(&db).then_with(|| tie(a).cmp(&tie(b)))
            });
            let mounted = contacts.iter().any(|&(n,_)| ports[n].id != 0);
            if !mounted { contacts.truncate(1); }
            if contacts.len() > 1 {
                // Equal-distance mouths take turns on small packets; a larger packet
                // can service every contacting mouth up to its individual budget.
                for group in contacts.chunk_by_mut(|a,b| a.1 == b.1) {
                    let rotation = self.ticks.wrapping_add(food.id) as usize % group.len();
                    group.rotate_left(rotation);
                }
            }
            for &(index, _) in contacts.iter() {
                let port = &mut ports[index];
                let c = &mut self.cells[port.cell];
                let used = if c.counted.is_some() {
                    c.count_capture(food, &caps[port.cell], port.budget, port.id, self.ticks)
                } else {
                    let fraction = c.gut_fraction_for(food.energy, food.nutrients, &caps[port.cell])
                        .min(port.budget / food.energy).clamp(0.0, 1.0);
                    let energy = food.energy * fraction;
                    let minerals = food.nutrients * fraction;
                    let extra = food.extra_energy * fraction;
                    c.ingest_mixture(food.composition, energy, minerals, extra);
                    food.energy = (food.energy - energy).max(0.0);
                    food.extra_energy = (food.extra_energy - extra).max(0.0);
                    food.nutrients = (food.nutrients - minerals).max(0.0);
                    energy
                };
                port.budget = (port.budget - used).max(0.0);
                if food.energy == 0.0 { break; }
            }
            if contacts.is_empty() || food.energy > 0.0 { return true; }
            self.environment.recycle_nutrients(food.x, food.y, food.nutrients);
            self.eaten += 1;
            false
        });
    }

    pub(super) fn can_swallow(&self, i: usize, j: usize, aperture: f64, cap: &BodyCapacities) -> bool {
        let a = &self.cells[i];
        let b = &self.cells[j];
        if b.integrity > 0.35 || b.body.area > aperture * 40.0 || a.body.area < 1.6 * b.body.area
            || a.energy <= 8.0 * b.body.area { return false; }
        if let (Some(gut), Some(prey)) = (&a.counted, &b.counted) {
            let (energy_room, mineral_room) = gut.gut_room(cap);
            let body = u64::from(b.body.count_material());
            let energy = 20 * (body + u64::from(prey.carbon)) + u64::from(b.body.count_investment()) +
                u64::from(prey.structural_extra) + u64::from(prey.reserve_extra) + u64::from(prey.atp) +
                prey.gut.iter().map(|p| p.energy()).sum::<u64>();
            let mineral = body + u64::from(prey.mineral) + prey.gut.iter().map(|p| u64::from(p.mineral)).sum::<u64>();
            return energy <= energy_room && mineral <= u64::from(mineral_room);
        }
        a.gut_fraction_for(b.food_potential().total(), b.mass(), cap) >= 1.0
    }

    pub(super) fn mounted_engulf_targets(&self, outputs: &[Outputs], caps: &[BodyCapacities], genomes: &[Genome], ws: &mut PhaseWorkspace) {
        ws.mouth_candidates.resize_with(self.cells.len(), Vec::new);
        for (list, target) in ws.mouth_candidates.iter_mut().zip(&ws.engulf_targets) {
            list.clear();
            if let Some((j, power)) = target { list.push((0, *j, *power, 0.0)); }
        }
        let max_radius = ws.geometry.radii.iter().copied().fold(0.0_f64, f64::max);
        for (i, cell) in self.cells.iter().enumerate() {
            if !outputs[i].engulf || !cell.alive() { continue; }
            let Some(state) = &cell.body.cellular else { continue; };
            for (n, a) in state.assemblies.iter().enumerate() {
                if !matches!(a.recipe.kind, AssemblyKind::Mechanism(MechanismKind::Engulf) | AssemblyKind::Organelle(Kind::Phagocyte)) { continue; }
                if a.recipe.kind == AssemblyKind::Organelle(Kind::Phagocyte) && outputs[i].feed <= 0.0 { continue; }
                let power = a.working_mass() * a.activity;
                if power <= 0.0 || cell.mouth_ready.contains_key(&a.id) { continue; }
                let [x, y] = mount(cell, a);
                let aperture_reach = 0.75 + 3.0 * power.sqrt();
                let guard = guard_of(cell, genomes, n);
                for j in ws.cell_grid.nearby_radius(x, y, max_radius + aperture_reach) {
                    if i == j || !ws.geometry.alive[j] { continue; }
                    let other = &self.cells[j];
                    let dx = delta(other.x - x, self.environment.width);
                    let dy = delta(other.y - y, self.environment.height);
                    let gap = dx.hypot(dy) - other.radius();
                    if gap > aperture_reach || self.environment.segment_blocked((x, y), (x+dx, y+dy))
                        || !self.can_swallow(i, j, power, &caps[i]) { continue; }
                    if let Some(def) = guard {
                        let peer = self.peer_at(i, j, [x, y], self.division_interface(cell.id,other.id), &ws.signals);
                        if !genomes[(cell.genome_id-1) as usize].program.allows_contact(def,
                            &ws.signals[i].registers, peer, &mut ws.contact_scratch) { continue; }
                    }
                    ws.mouth_candidates[i].push((a.id, j, power, gap));
                }
            }
            let candidates = &mut ws.mouth_candidates[i];
            candidates.sort_unstable_by(|a,b| a.0.cmp(&b.0).then_with(|| a.3.total_cmp(&b.3))
                .then_with(|| self.cells[a.1].id.cmp(&self.cells[b.1].id)));
            ws.engulf_targets[i] = candidates.first().map(|v| (v.1, v.2));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::mechanism_tests::equipped;
    const STORE: &str = "assembly Gut:store(food){Wall:0.12;}";
    const TWO_PORTS: &str = "
        product F=filament();
        assembly Touch:sense(contact){Mount:0.005;Detector:0.01;F:0.005;at:0;}
        assembly Front:import(particle){Mount:0.01;Rotor:0.02;at:0;
            engage_when:peer.resistance_valid && peer.resistance < 0.075;}
        assembly Back:import(particle){Mount:0.01;Rotor:0.02;at:3.14159;}";
    fn capture(w: &mut World, genomes: &[Genome]) {
        let caps: Vec<_> = w.cells.iter().map(|c| c.body.capacities()).collect();
        let output = vec![Outputs { feed: 1.0, ..Default::default() }; w.cells.len()];
        let mut ws = PhaseWorkspace::default();
        ws.geometry.rebuild(&w.cells, false, false);
        w.freeze_contact_signals(genomes, &mut ws.signals);
        w.eat_in(&output, &caps, genomes, &mut ws);
    }

    #[test]
    fn each_inlet_applies_its_own_reach_guard_and_throughput() {
        let (mut w, genomes) = equipped(&format!("{STORE}{TWO_PORTS}"));
        let c = &w.cells[0];
        let (x,y,r) = (c.x,c.y,c.radius());
        assert!(w.particle(OrganicKind::Tissue, x+r+2.0, y, 10.0, 0.0));
        w.food[0].composition = Composition::pool(2);
        assert!(w.particle(OrganicKind::Waste, x-r-2.0, y, 10.0, 0.0));
        w.food[1].composition = Composition::pool(1);
        assert!(w.particle(OrganicKind::Exudate, x, y+r, 10.0, 0.0));
        capture(&mut w, &genomes);
        assert_eq!(w.food[0].energy, 10.0, "resistant specimen blocked at front");
        assert_eq!(w.food[2].energy, 10.0, "no opening at the flank");
        let budget = 0.03 * PARTICLE_CAPTURE_RATE * DT;
        assert!((10.0 - w.food[1].energy - budget).abs() < 1e-10);
        assert!((w.cells[0].gut[1].energy - budget).abs() < 1e-10, "back takes structural carbon even with waste origin");
        w.food[0].composition = Composition::SOLUBLE;
        capture(&mut w, &genomes);
        assert!(w.cells[0].gut[0].energy > 0.0, "same front inlet accepts a soft specimen");
    }

    #[test]
    fn an_oversized_nearer_cell_cannot_hide_a_fitting_meal_at_a_rear_mouth() {
        let (mut w, genomes) = equipped("assembly Gut:store(food){Wall:0.20;} assembly Mouth:engulf(){Mount:0.02;Rotor:0.04;Wall:0.03;at:3.14159;}");
        let c = &mut w.cells[0];
        c.body.area = 2.0;
        let (x, y, r) = (c.x, c.y, c.radius());
        w.seed_cells(1, 2, x, y);
        for (i, area) in [(1, 1.4), (2, 1.0)] {
            let b = &mut w.cells[i];
            b.body.area = area;
            b.integrity = 0.2;
            b.organic_carbon = 0.01;
            b.energy = 1.0;
            b.x = x - r - b.radius() - if i == 1 { 0.0 } else { 0.5 };
            b.y = y;
        }
        let caps: Vec<_> = w.cells.iter().map(|c| c.body.capacities()).collect();
        let mut ws = PhaseWorkspace::default();
        ws.geometry.rebuild(&w.cells, false, false);
        let mut outputs = vec![Outputs::default(); 3];
        outputs[0].engulf = true;
        w.resolve_predation_in(&outputs, &caps, &genomes, &mut ws);
        assert_eq!(ws.consumed, [false, false, true]);
    }
}
