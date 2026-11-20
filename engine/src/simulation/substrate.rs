//! Carbon fractions in extracellular packets. Origin is a player-facing trace;
//! digestion, decay and observations all use the packet's actual material.
use super::*;

#[derive(Clone, Copy, Default, Debug)]
pub(super) struct Composition {
    pub structural: f64,
    pub bound: f64,
}

impl Composition {
    pub const SOLUBLE: Self = Self { structural: 0.0, bound: 0.0 };
    pub fn pool(index: usize) -> Self {
        Self { structural: f64::from(index == 1), bound: f64::from(index == 2) }
    }
    pub fn fractions(self) -> [f64; 3] {
        [(1.0 - self.structural - self.bound).max(0.0), self.structural, self.bound]
    }
    pub fn from_body(structural: f64, reserve: f64) -> Self {
        Self { structural: if structural + reserve > 0.0 { structural / (structural + reserve) } else { 0.0 }, bound: 0.0 }
    }
    pub fn mix(self, held: f64, incoming: Self, received: f64) -> Self {
        if received <= 0.0 { return self; }
        let fraction = received / (held + received);
        let bound = self.bound + fraction * (incoming.bound - self.bound);
        let structural = self.structural + fraction * (incoming.structural - self.structural);
        Self { bound: bound.clamp(0.0, 1.0), structural: structural.clamp(0.0, (1.0 - bound).max(0.0)) }
    }
    // Hydrolysis leaves some unprocessed original carrier plus less accessible
    // residue. The different hydrolases thus make different mixtures of waste.
    pub fn residue(pool: usize) -> Self {
        match pool {
            0 => Self { structural: 0.0, bound: 0.82 },
            1 => Self { structural: 0.28, bound: 0.72 },
            _ => Self::pool(2),
        }
    }
    pub fn decay_fraction(self) -> f64 {
        let f = self.fractions();
        let rate = f.iter().zip(ORGANIC_DECAY).map(|(p, k)| p * k).sum::<f64>();
        -(-rate * DT).exp_m1()
    }
    /// Nested rounding is monotone in the total number of carriers. Removing a
    /// one-count bite therefore never removes negative counts from any pool.
    fn counts(self, total: u64) -> [u64; 3] {
        let bound = (self.bound * total as f64).round() as u64;
        let free = total - bound.min(total);
        let fraction = if self.bound < 1.0 { self.structural / (1.0 - self.bound) } else { 0.0 };
        let structural = (fraction.clamp(0.0, 1.0) * free as f64).round() as u64;
        [free - structural, structural, total - free]
    }
    pub fn taken(self, before: u64, after: u64) -> [u64; 3] {
        let initial = self.counts(before);
        let retained = self.counts(after);
        std::array::from_fn(|i| initial[i] - retained[i])
    }
    pub fn portion(self, before: u64, after: u64) -> Self {
        let n = (before - after) as f64;
        if n == 0.0 { return Self::default(); }
        let parts = self.taken(before, after);
        Self { structural: parts[1] as f64 / n, bound: parts[2] as f64 / n }
    }
}

impl Food {
    pub(super) fn traits(&self) -> (f64, f64, f64) {
        let carbon = self.carbon();
        let mineral = self.nutrients / (self.nutrients + carbon).max(f64::MIN_POSITIVE);
        let enrichment = self.extra_energy / self.energy.max(f64::MIN_POSITIVE);
        let mix = self.composition;
        let resistance = 0.016 + 0.046 * mix.structural + 0.084 * mix.bound + 0.01 * mineral;
        let pigment = 0.08 + 0.13 * mix.structural + 0.05 * mineral + 0.10 * enrichment;
        let contrast = 0.22 + 0.22 * pigment + 0.10 * mix.bound + 0.10 * enrichment;
        (resistance, pigment, contrast)
    }
}
impl Cell {
    pub(super) fn ingest_mixture(&mut self, mix: Composition, energy: f64, minerals: f64, extra: f64) {
        for (pool, fraction) in self.gut.iter_mut().zip(mix.fractions()) {
            pool.energy += energy * fraction;
            pool.nutrients += minerals * fraction;
            pool.extra_energy += extra * fraction;
        }
        self.refresh_gut_totals();
    }
}

impl World {
    pub(super) fn export_carbon_in(&mut self, workspace: &mut PhaseWorkspace) {
        workspace.emissions.clear();
        for cell in &mut self.cells {
            if !cell.alive() { continue; }
            let Some(state) = &cell.body.cellular else { continue };
            let radius = cell.radius();
            let mut transferred = 0.0;
            for assembly in &state.assemblies {
                if assembly.recipe.kind != AssemblyKind::Mechanism(MechanismKind::CarbonExport) { continue; }
                let request = assembly.working_mass() * assembly.activity * DT;
                if request <= 0.0 { continue; }
                let at = cell.angle + cell.body.mount_at(assembly);
                let outlet = (cell.x + (radius + 2.5) * at.cos(), cell.y + (radius + 2.5) * at.sin());
                if self.environment.segment_blocked((cell.x,cell.y), outlet) { continue; }
                let (carbon, extra) = if let Some(q) = &mut cell.counted {
                    use crate::counts::{proportional, rate, units};
                    let n = rate(&mut q.rate_credit[28], request).min(q.carbon).min(q.atp);
                    let extra = proportional(q.reserve_extra, n, q.carbon);
                    q.carbon -= n;
                    q.atp -= n;
                    q.reserve_extra -= extra;
                    self.energy_loss.work += u64::from(n);
                    (units(n), units(extra))
                } else {
                    let n = request.min(cell.organic_carbon).min(cell.energy);
                    let extra = cell.reserve_extra_for(n);
                    cell.organic_carbon -= n;
                    cell.energy -= n;
                    cell.reserve_enrichment -= extra;
                    (n, extra)
                };
                if carbon <= 0.0 { continue; }
                let energy = CHEMICAL_ENERGY_PER_CARBON * carbon + extra;
                let mut emission = OrganicEmission::new(OrganicKind::Exudate,
                    outlet.0, outlet.1, energy, 0.0).enriched(extra);
                emission.merge_radius = 5.0; // One local packet diameter.
                emission.outside = Some((cell.x, cell.y, radius));
                workspace.emissions.push(emission);
                transferred += energy;
            }
            if transferred > 0.0 {
                cell.exudation_rate = transferred / DT;
                cell.project_counts();
            }
        }
        self.emit_organic_pending(workspace);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::mechanism_tests::equipped;

    #[test]
    fn export_is_optional_local_and_paid_even_without_light() {
        let (mut w, _) = equipped("product Pump=transporter();assembly Outlet:export(carbon){Mount:0.005;Pump:0.02;at:3.14159;}");
        let atp = w.cells[0].energy;
        let carbon = w.cells[0].organic_carbon;
        w.cells[0].body.cellular.as_mut().unwrap().assemblies.last_mut().unwrap().activity = 0.0;
        w.export_carbon_in(&mut PhaseWorkspace::default());
        assert!(w.food.is_empty());
        assert_eq!((w.cells[0].energy,w.cells[0].organic_carbon), (atp,carbon));
        w.cells[0].body.cellular.as_mut().unwrap().assemblies.last_mut().unwrap().activity = 1.0;
        w.export_carbon_in(&mut PhaseWorkspace::default());
        let moved = w.organic_food_total();
        assert!(moved > 0.0 && w.food[0].x < w.cells[0].x);
        assert!((atp - w.cells[0].energy - moved).abs() < 1e-10);
        assert!((carbon - w.cells[0].organic_carbon - moved).abs() < 1e-10);
    }

    #[test]
    fn opposite_exporters_release_two_local_packets() {
        let (mut w, _) = equipped("product Pump=transporter();
            assembly Front:export(carbon){Mount:0.005;Pump:0.02;at:0;}
            assembly Rear:export(carbon){Mount:0.005;Pump:0.02;at:3.14159;}");
        w.export_carbon_in(&mut PhaseWorkspace::default());
        let x = w.cells[0].x;
        assert_eq!(w.food.len(), 2);
        assert!(w.food.iter().any(|f| f.x > x + w.cells[0].radius()));
        assert!(w.food.iter().any(|f| f.x < x - w.cells[0].radius()));
    }

    #[test]
    fn composition_survives_merging_and_ignores_particle_origin() {
        let mut w = World::empty(41);
        let mut ws = PhaseWorkspace::default();
        for material in [Composition::SOLUBLE, Composition::pool(1)] {
            ws.emissions.push(OrganicEmission::new(OrganicKind::Waste, 300.0, 200.0, 7.0, 0.02).material(material));
        }
        w.emit_organic_pending(&mut ws);
        assert_eq!(w.food.len(), 1);
        let f = &mut w.food[0];
        assert_eq!(f.composition.fractions(), [0.5,0.5,0.0]);
        let traits = f.traits();
        let decay = f.composition.decay_fraction();
        f.kind = OrganicKind::Exudate;
        assert_eq!(f.traits(), traits);
        assert_eq!(f.composition.decay_fraction(), decay);
        f.composition = Composition::pool(2);
        assert_ne!(f.traits(), traits);
        assert!(f.composition.decay_fraction() < decay);
    }

    #[test]
    fn single_count_bites_preserve_all_three_component_totals() {
        let mix = Composition { structural:0.37, bound:0.42 };
        let mut sum = [0u64;3];
        for before in (1..=997).rev() {
            let removed = mix.taken(before, before-1);
            assert_eq!(removed.iter().sum::<u64>(), 1);
            for (s, r) in sum.iter_mut().zip(removed) { *s += r; }
        }
        assert_eq!(sum, mix.counts(997));
    }
}
