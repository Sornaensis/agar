//! Accounted specimens used by isolated sensing, capture, digestion and load
//! tests. The live engine obtains particles exclusively from organism activity.
use super::*;

impl World {
    pub(crate) fn fixture_exudate(&mut self, x: f64, y: f64, count: usize) -> usize {
        for _ in 0..count {
            let (fx, fy) = if x < 0.0 || y < 0.0 {
                (self.rng.unit() * self.environment.width, self.rng.unit() * self.environment.height)
            } else {
                let angle = self.rng.unit() * TAU;
                let distance = self.rng.unit().sqrt() * 45.0;
                (wrap(x + angle.cos() * distance, self.environment.width),
                    wrap(y + angle.sin() * distance, self.environment.height))
            };
            let potential = 18.0 + self.rng.unit() * 6.0;
            assert!(self.particle(OrganicKind::Exudate, fx, fy, potential, FIXTURE_MINERALS));
            self.nutrient_imports += FIXTURE_MINERALS;
            self.energy_imports += potential;
            self.carbon_imports += potential / CHEMICAL_ENERGY_PER_CARBON;
        }
        count
    }
}

impl Cell {
    /// Place supplied assay material at an actual feeding opening.
    pub(super) fn inlet_fixture_position(&self) -> (f64, f64) {
        let angle = self.angle + self.body.cellular.as_ref().and_then(|state| state.assemblies.iter()
            .find(|a| a.recipe.kind == AssemblyKind::Mechanism(MechanismKind::ParticleCapture) && a.working_mass() > 0.0))
            .map_or(0.0, |a| self.body.mount_at(a));
        (wrap(self.x+self.radius()*angle.cos(), WIDTH), wrap(self.y+self.radius()*angle.sin(), HEIGHT))
    }
}
