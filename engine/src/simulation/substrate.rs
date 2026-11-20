//! Carbon fractions in extracellular packets. Origin is a player-facing trace;
//! digestion, decay and observations all use the packet's actual material.
use super::*;

#[derive(Clone, Copy, Default, Debug)]
pub(super) struct Composition {
    pub structural: f64,
    pub bound: f64,
    pub ligand: f64,
}

impl Composition {
    pub const SOLUBLE: Self = Self { structural: 0.0, bound: 0.0, ligand: 0.0 };
    pub fn pool(index: usize) -> Self {
        Self { structural: f64::from(index == 1), bound: f64::from(index == 2), ligand: f64::from(index == LIGAND) }
    }
    pub fn fractions(self) -> [f64; 4] {
        [(1.0 - self.structural - self.bound - self.ligand).max(0.0), self.structural, self.bound, self.ligand]
    }
    pub fn from_body(structural: f64, reserve: f64) -> Self {
        Self { structural: if structural + reserve > 0.0 { structural / (structural + reserve) } else { 0.0 }, bound: 0.0, ligand: 0.0 }
    }
    pub fn mix(self, held: f64, incoming: Self, received: f64) -> Self {
        if received <= 0.0 { return self; }
        let fraction = received / (held + received);
        let bound = self.bound + fraction * (incoming.bound - self.bound);
        let structural = self.structural + fraction * (incoming.structural - self.structural);
        let ligand = (self.ligand + fraction * (incoming.ligand - self.ligand)).clamp(0.0, 1.0);
        let bound = bound.clamp(0.0, 1.0 - ligand);
        Self { ligand, bound, structural: structural.clamp(0.0, (1.0 - ligand - bound).max(0.0)) }
    }
    // Hydrolysis leaves some unprocessed original carrier plus less accessible
    // residue. The different hydrolases thus make different mixtures of waste.
    pub fn residue(pool: usize) -> Self {
        match pool {
            0 => Self { structural: 0.0, bound: 0.82, ligand: 0.0 },
            1 => Self { structural: 0.28, bound: 0.72, ligand: 0.0 },
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
    pub(super) fn counts(self, total: u64) -> [u64; 4] {
        let ligand = ((self.ligand * total as f64).round() as u64).min(total);
        let total = total - ligand;
        let bound_fraction = if self.ligand < 1.0 { self.bound / (1.0 - self.ligand) } else { 0.0 };
        let bound = (bound_fraction.clamp(0.0, 1.0) * total as f64).round() as u64;
        let free = total - bound.min(total);
        let fraction = if self.bound + self.ligand < 1.0 { self.structural / (1.0 - self.ligand - self.bound) } else { 0.0 };
        let structural = (fraction.clamp(0.0, 1.0) * free as f64).round() as u64;
        [free - structural, structural, total - free, ligand]
    }
    pub fn taken(self, before: u64, after: u64) -> [u64; 4] {
        let initial = self.counts(before);
        let retained = self.counts(after);
        std::array::from_fn(|i| initial[i] - retained[i])
    }
    pub(super) fn from_counts(parts: [u64; 4]) -> Self {
        let total = parts.iter().sum::<u64>() as f64;
        if total == 0.0 { return Self::default(); }
        Self { structural: parts[1] as f64 / total, bound: parts[2] as f64 / total, ligand: parts[3] as f64 / total }
    }

    pub fn portion(self, before: u64, after: u64) -> Self {
        Self::from_counts(self.taken(before, after))
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
                let ligand = match assembly.recipe.kind {
                    AssemblyKind::Mechanism(MechanismKind::CarbonExport) => false,
                    AssemblyKind::Mechanism(MechanismKind::LigandExport) => true,
                    _ => continue,
                };
                let request = assembly.working_mass() * assembly.activity * DT;
                if request <= 0.0 { continue; }
                let at = cell.angle + cell.body.mount_at(assembly);
                let outlet = (cell.x + (radius + 2.5) * at.cos(), cell.y + (radius + 2.5) * at.sin());
                if self.environment.segment_blocked((cell.x,cell.y), outlet) { continue; }
                let (energy, minerals, extra) = if let Some(q) = &mut cell.counted {
                    use crate::counts::{proportional, rate, units, wide_units, CountMeal};
                    let credit = if ligand { 31 } else { 28 };
                    let extent = rate(&mut q.rate_credit[credit], request).min(q.atp);
                    let meal = if ligand {
                        q.gut[LIGAND].take(extent)
                    } else {
                        let n = extent.min(q.carbon);
                        let extra = proportional(q.reserve_extra, n, q.carbon);
                        q.carbon -= n;
                        q.reserve_extra -= extra;
                        CountMeal {carbon:n, mineral:0, extra}
                    };
                    q.atp -= meal.carbon;
                    self.energy_loss.work += u64::from(meal.carbon);
                    (wide_units(meal.energy()), units(meal.mineral), units(meal.extra))
                } else if ligand {
                    let part = cell.gut[LIGAND].take_carbon(request.min(cell.energy));
                    cell.energy -= (part.energy - part.extra_energy) / CHEMICAL_ENERGY_PER_CARBON;
                    (part.energy, part.nutrients, part.extra_energy)
                } else {
                    let n = request.min(cell.organic_carbon).min(cell.energy);
                    let extra = cell.reserve_extra_for(n);
                    cell.organic_carbon -= n;
                    cell.energy -= n;
                    cell.reserve_enrichment -= extra;
                    (n * CHEMICAL_ENERGY_PER_CARBON + extra, 0.0, extra)
                };
                if energy <= 0.0 { continue; }
                let mut emission = OrganicEmission::new(OrganicKind::Exudate,
                    outlet.0, outlet.1, energy, minerals).enriched(extra);
                if ligand { emission.composition = Composition::pool(LIGAND); }
                emission.merge_radius = 5.0; // One local packet diameter.
                emission.outside = Some((cell.x, cell.y, radius));
                workspace.emissions.push(emission);
                transferred += energy;
            }
            if transferred > 0.0 {
                cell.exudation_rate = transferred / DT;
                cell.project_counts();
                if cell.counted.is_none() { cell.refresh_gut_totals(); }
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
        assert_eq!(f.composition.fractions(), [0.5,0.5,0.0,0.0]);
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
    fn single_count_bites_preserve_all_four_component_totals() {
        let mix = Composition { structural:0.37, bound:0.32, ligand:0.10 };
        let mut sum = [0u64;4];
        for before in (1..=997).rev() {
            let removed = mix.taken(before, before-1);
            assert_eq!(removed.iter().sum::<u64>(), 1);
            for (s, r) in sum.iter_mut().zip(removed) { *s += r; }
        }
        assert_eq!(sum, mix.counts(997));
    }
}
