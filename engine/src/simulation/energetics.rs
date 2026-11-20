//! Carbon carries a base chemical potential. Fabrication and recovered ATP can
//! add energy to that same material, without adding carbon or minerals.
use super::*;

pub(super) struct FoodPotential {
    pub body: f64,
    pub reserve: f64,
    pub gut: f64,
    pub atp: f64,
}
impl FoodPotential {
    pub fn total(&self) -> f64 { self.body + self.reserve + self.gut + self.atp }
}
impl Cell {
    pub(super) fn chemical_body_mass(&self)->f64 {
        if self.counted.is_some() {crate::counts::units(self.body.count_material())} else {self.body.structural_mass()}
    }
    pub(super) fn chemical_body_investment(&self)->f64 {
        if self.counted.is_some() {crate::counts::units(self.body.count_investment())} else {self.body.embodied_energy()}
    }
    pub(super) fn food_potential(&self) -> FoodPotential {
        FoodPotential {
            body: self.chemical_body_mass() * STRUCTURAL_CARBON_PER_MASS * CHEMICAL_ENERGY_PER_CARBON
                + self.chemical_body_investment() + self.structural_enrichment,
            reserve: self.organic_carbon * CHEMICAL_ENERGY_PER_CARBON + self.reserve_enrichment,
            gut: self.gut_energy,
            atp: self.energy,
        }
    }
    pub(super) fn tissue_extra_energy(&self) -> f64 {
        self.chemical_body_investment() + self.structural_enrichment + self.reserve_enrichment + self.energy
    }
    pub(super) fn reserve_extra_for(&self, carbon: f64) -> f64 {
        if self.organic_carbon == 0.0 { 0.0 } else {
            self.reserve_enrichment * (carbon / self.organic_carbon).clamp(0.0, 1.0)
        }
    }
    pub(super) fn move_construction_carbon(&mut self, previous_mass: f64) {
        let delta = (self.body.structural_mass() - previous_mass) * STRUCTURAL_CARBON_PER_MASS;
        if delta > 0.0 {
            let extra = self.reserve_extra_for(delta);
            self.reserve_enrichment -= extra;
            self.structural_enrichment += extra;
        } else if delta < 0.0 {
            let extra = self.structural_enrichment * (-delta / (previous_mass * STRUCTURAL_CARBON_PER_MASS)).min(1.0);
            self.structural_enrichment -= extra;
            self.reserve_enrichment += extra;
        }
        self.organic_carbon = (self.organic_carbon - delta).max(0.0);
    }
}
impl Food {
    pub(super) fn carbon(&self) -> f64 { (self.energy - self.extra_energy) / CHEMICAL_ENERGY_PER_CARBON }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) { assert!((a-b).abs() < 1e-8, "{a} != {b}"); }

    #[test]
    fn enriched_digestion_preserves_waste_and_needs_oxygen_to_pay_atp() {
        let mut w = World::empty(12);
        w.seed_cells(1, 1, 200.0, 200.0);
        let mut cap = w.cells[0].body.capacities();
        cap.substrate_digestion[OrganicKind::Tissue as usize] = 1000.0;
        let c = &mut w.cells[0];
        // Move a known amount of already introduced carbon and ATP into a meal.
        c.organic_carbon -= 0.5;
        c.energy -= 8.0;
        c.ingest_enriched(OrganicKind::Tissue, 18.0, 0.0, 8.0);
        let before = (w.energy_total(), w.carbon_total());
        w.digest(&[Outputs::default()], &[cap], &mut [1.0]);
        close(w.cells[0].gut_energy, 0.0);
        close(w.cells[0].reserve_enrichment, 0.65 * 8.0);
        close(w.organic_food_total(), 0.35 * 0.5);
        close(w.food.iter().map(|f| f.extra_energy).sum(), 0.35 * 8.0);
        close(w.energy_total(), before.0 - 10.0 * 0.02);
        close(w.carbon_total(), before.1);
        let energy = w.cells[0].energy;
        cap.respiration = 1000.0;
        w.respire_in(&[cap]);
        close(w.cells[0].energy, energy);
        let c = &w.cells[0];
        let density = c.food_potential().reserve / c.organic_carbon;
        let patch = w.environment.patch_index(c.x,c.y);
        let oxygen = w.environment.take_oxygen(patch,0.05);
        w.cells[0].internal_oxygen += oxygen;
        let held = w.energy_total();
        w.respire_in(&[cap]);
        close(w.carbon_respired_total, oxygen);
        close(w.cells[0].energy - energy, 0.8 * oxygen * density);
        close(held - w.energy_total(), 0.2 * oxygen * density);
        close(w.carbon_total(), before.1);
    }

    #[test]
    fn lysis_restores_all_stores_and_merged_packets_preserve_their_carbon() {
        let mut w = World::empty(42);
        w.seed_cells(1,1,100.0,100.0);
        let c = &mut w.cells[0];
        let (x,y) = (c.x,c.y);
        c.energy -= 10.0;
        c.reserve_enrichment = 6.0;
        c.structural_enrichment = 4.0;
        c.organic_carbon -= 0.2;
        c.energy -= 3.0;
        c.ingest_enriched(OrganicKind::Waste,7.0,0.0,3.0);
        c.integrity = 0.0;
        let expected = c.food_potential();
        close(expected.body, 0.39 * 20.0 + 31.2 * 0.90 + 4.0);
        let held = w.energy_total();
        let carbon = w.carbon_total();
        assert!(w.particle(OrganicKind::Tissue,x,y,1.0,0.0));
        w.remove_dead_in(&mut PhaseWorkspace::default(), None);
        close(w.energy_total(),held+1.0);
        close(w.carbon_total(),carbon+1.0/20.0);
        close(w.food.iter().map(|f|f.energy).sum(),expected.total()+1.0);
        close(w.food.iter().map(|f|f.carbon()*f.composition.bound).sum(),0.2);
        assert!(w.food.iter().all(|f|f.kind==OrganicKind::Tissue));
        assert!(w.food.iter().all(|f| f.carbon()>0.0 && f.energy<=24.0));
        assert_eq!(w.deaths,1);
    }

    #[test]
    fn enrichment_follows_structural_growth_and_release_proportionally() {
        let mut w = World::empty(52);
        w.seed_cells(1,1,200.0,200.0);
        let c = &mut w.cells[0];
        c.energy -= 16.0;
        c.reserve_enrichment = 16.0;
        let mass = c.body.structural_mass();
        let carbon = c.organic_carbon;
        let before = c.food_potential().total();
        // Isolate the material transfer performed immediately after paid body work.
        c.body.area += 0.1;
        let grown = c.body.structural_mass();
        c.move_construction_carbon(mass);
        close(c.structural_enrichment,16.0*(grown-mass)/carbon);
        close(c.structural_enrichment+c.reserve_enrichment,16.0);
        close(c.food_potential().total(),before);
        c.body.area -= 0.1;
        c.move_construction_carbon(grown);
        close(c.structural_enrichment+c.reserve_enrichment,16.0);
        close(c.organic_carbon,carbon);
        close(c.food_potential().total(),before);
    }
}
