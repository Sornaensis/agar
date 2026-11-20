//! Two coarse-grained carbon isomers with identical counted carbon and stored energy.
//! Food-compartment ligand competes for the binding sites used by ordinary hydrolases.
use super::*;
use crate::counts::{floor, rate, CountMeal};

impl OrganicMatter {
    fn carbon(self) -> f64 {
        (self.energy - self.extra_energy).max(0.0) / CHEMICAL_ENERGY_PER_CARBON
    }

    /// Minerals and enrichment travel with the molecules in a mixed packet.
    pub(super) fn take_carbon(&mut self, carbon: f64) -> Self {
        let total = self.carbon();
        if total <= 0.0 || carbon <= 0.0 { return Self::default(); }
        let fraction = (carbon / total).min(1.0);
        let part = Self { energy: self.energy * fraction, extra_energy: self.extra_energy * fraction,
            nutrients: self.nutrients * fraction };
        self.energy -= part.energy;
        self.extra_energy -= part.extra_energy;
        self.nutrients -= part.nutrients;
        part
    }
}

impl Cell {
    pub(super) fn ligand_carbon(&self) -> f64 {
        self.gut[LIGAND].carbon()
    }

    pub(super) fn hydrolase_availability(&self, substrate: usize, gut_capacity: f64) -> f64 {
        // Ratio of the inhibited rate to its uninhibited rate. Concentrations
        // are expressed relative to the compartment's nominal carbon capacity.
        let half_saturation = 0.05 * gut_capacity / CHEMICAL_ENERGY_PER_CARBON;
        let substrate = self.gut[substrate].carbon();
        let available = half_saturation + substrate;
        if available == 0.0 { return 0.0; }
        available / (available + 5.0 * self.ligand_carbon())
    }

    fn convert_counted(&mut self, cap: &BodyCapacities, direction: usize) -> u32 {
        let q = self.counted.as_mut().unwrap();
        let extent = rate(&mut q.rate_credit[29 + direction], cap.isomerization[direction] * DT)
            .min(q.atp);
        let moved = if direction == 0 {
            let source = CountMeal { carbon: q.carbon, mineral: 0, extra: q.reserve_extra };
            let (room, _) = q.gut_room(cap);
            let n = source.fitting(extent, room, u32::MAX);
            let meal = source.portion(n);
            q.carbon -= n;
            q.reserve_extra -= meal.extra;
            q.gut[LIGAND].join(meal);
            n
        } else {
            let source = q.gut[LIGAND];
            let max_carbon = extent.min((floor(cap.energy) / 20).saturating_sub(q.carbon));
            let mut n = source.fitting(max_carbon, u64::MAX, floor(cap.minerals).saturating_sub(q.mineral));
            // Largest n for which floor(extra*n/carbon) still fits.
            if source.extra > 0 {
                let extra_room = u32::MAX - q.reserve_extra;
                let max = ((u64::from(extra_room) + 1) * u64::from(source.carbon) - 1) / u64::from(source.extra);
                n = n.min(max.min(u64::from(u32::MAX)) as u32);
            }
            let meal = q.gut[LIGAND].take(n);
            q.carbon += n;
            q.reserve_extra += meal.extra;
            q.mineral += meal.mineral;
            n
        };
        q.atp -= moved;
        moved
    }

    fn convert_continuous(&mut self, cap: &BodyCapacities, direction: usize) {
        let extent = (cap.isomerization[direction] * DT).min(self.energy);
        let moved = if direction == 0 {
            if self.organic_carbon <= 0.0 { return; }
            let density = CHEMICAL_ENERGY_PER_CARBON + self.reserve_enrichment / self.organic_carbon;
            let n = extent.min(self.organic_carbon).min((cap.gut_energy - self.gut_energy).max(0.0) / density);
            let extra = self.reserve_extra_for(n);
            self.organic_carbon -= n;
            self.reserve_enrichment -= extra;
            self.gut[LIGAND].energy += n * CHEMICAL_ENERGY_PER_CARBON + extra;
            self.gut[LIGAND].extra_energy += extra;
            n
        } else {
            let held = self.gut[LIGAND];
            let mut n = held.carbon().min(extent).min((cap.energy / CHEMICAL_ENERGY_PER_CARBON - self.organic_carbon).max(0.0));
            if held.nutrients > 0.0 {
                n = n.min(held.carbon() * (cap.minerals - self.nutrient_stored).max(0.0) / held.nutrients);
            }
            let part = self.gut[LIGAND].take_carbon(n);
            self.organic_carbon += n;
            self.reserve_enrichment += part.extra_energy;
            self.nutrient_stored += part.nutrients;
            n
        };
        self.energy -= moved;
        self.refresh_gut_totals();
    }
}
impl World {
    pub(super) fn isomerize_in(&mut self, capacities: &[BodyCapacities]) {
        for (cell, cap) in self.cells.iter_mut().zip(capacities) {
            if cap.isomerization == [0.0; 2] || !cell.alive() { continue; }
            for (direction, &capacity) in cap.isomerization.iter().enumerate() {
                if capacity <= 0.0 { continue; }
                if cell.counted.is_some() {
                    self.energy_loss.work += u64::from(cell.convert_counted(cap, direction));
                    cell.project_counts();
                } else {
                    cell.convert_continuous(cap, direction);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::mechanism_tests::equipped;

    const GUT: &str = "assembly Gut:store(food){Wall:0.04;}";
    fn close(a: f64, b: f64) { assert!((a - b).abs() < 1e-9, "{a} versus {b}"); }

    #[test]
    fn isomerization_and_secretion_require_separate_built_machinery() {
        let parts = format!("{GUT} product Pump=transporter();
            assembly Synth:isomerize(ligand){{Catalyst:0.025;}}
            assembly Outlet:export(ligand){{Mount:0.005;Pump:0.02;at:0;}}");
        let (mut w, genomes) = equipped(&parts);
        let snapshot = |w: &World| (w.carbon_total(), w.energy_total(), w.nutrient_total());
        let before = snapshot(&w);
        let caps = [w.cells[0].body.capacities()];
        w.export_carbon_in(&mut PhaseWorkspace::default());
        assert!(w.food.is_empty(), "an empty outlet cannot release reserve carbon");
        w.isomerize_in(&caps);
        let made = w.cells[0].ligand_carbon();
        close(made, 2.0 * 0.025 * DT);
        assert!(w.food.is_empty());
        w.export_carbon_in(&mut PhaseWorkspace::default());
        let packet = &w.food[0];
        assert!(packet.carbon() > 0.0 && packet.x > w.cells[0].x);
        close(packet.composition.ligand, 1.0);
        close(packet.carbon() + w.cells[0].ligand_carbon(), made);
        let after = snapshot(&w);
        close(before.0, after.0);
        close(before.1 - after.1, made + packet.carbon());
        close(before.2, after.2);
        let mut engine = crate::Engine::new();
        engine.world = w;
        engine.genomes = genomes;
        let saved = crate::checkpoint::save(&engine).unwrap();
        let restored = crate::checkpoint::restore(&saved).unwrap();
        assert_eq!(crate::checkpoint::save(&restored).unwrap(), saved);
    }

    #[test]
    fn captured_ligand_reversibly_inhibits_hydrolysis_with_substrate_competition() {
        let source = format!("{GUT} assembly Hydro:digest(soluble){{Catalyst:0.04;}}
            assembly Recover:isomerize(carbon){{Catalyst:0.04;}}");
        let (mut w, _) = equipped(&source);
        let caps = [w.cells[0].body.capacities()];
        let cell = &mut w.cells[0];
        cell.organic_carbon = 0.0;
        cell.ingest_mixture(Composition::SOLUBLE, 3.0, 0.0, 0.0);
        let (mut control, _) = equipped(&source);
        control.cells[0] = w.cells[0].clone();
        w.cells[0].ingest_mixture(Composition::pool(LIGAND), 3.0, 0.0, 0.0);
        let start = w.cells[0].ligand_carbon();
        let before = w.cells[0].hydrolase_availability(0, caps[0].gut_energy);
        assert!(before > 0.0 && before < 0.5);
        let mut excess_substrate = w.cells[0].clone();
        excess_substrate.ingest_mixture(Composition::SOLUBLE, 3.0, 0.0, 0.0);
        assert!(excess_substrate.hydrolase_availability(0, caps[0].gut_energy) > before);

        let outputs = [Outputs::default()];
        w.digest(&outputs, &caps, &mut [0.0]);
        control.digest(&outputs, &caps, &mut [0.0]);
        assert!(control.cells[0].digestion_rate > 0.0);
        close(w.cells[0].digestion_rate / control.cells[0].digestion_rate, before);
        close(w.cells[0].ligand_carbon(), start);
        for _ in 0..60 { w.isomerize_in(&caps); }
        close(w.cells[0].ligand_carbon(), 0.0);
        close(w.cells[0].hydrolase_availability(0, caps[0].gut_energy), 1.0);
    }
}
