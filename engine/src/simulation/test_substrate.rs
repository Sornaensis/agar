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
