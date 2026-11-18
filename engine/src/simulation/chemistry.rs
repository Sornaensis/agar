//! Conserved carbon and oxygen reactions in abstract amount units.
use super::*;

impl World {
    pub fn organic_cell_total(&self) -> f64 { self.cells.iter().map(Cell::organic_carbon_total).sum() }
    pub fn organic_food_total(&self) -> f64 { self.food.iter().map(|f| f.energy / CHEMICAL_ENERGY_PER_CARBON).sum() }
    pub fn internal_inorganic_carbon_total(&self) -> f64 { self.cells.iter().map(|c| c.internal_inorganic_carbon).sum() }
    pub fn oxygen_cell_total(&self) -> f64 { self.cells.iter().map(|c| c.internal_oxygen).sum() }
    pub fn carbon_total(&self) -> f64 {
        self.environment.inorganic_carbon_total() + self.internal_inorganic_carbon_total()
            + self.organic_cell_total() + self.organic_food_total()
    }
    pub fn carbon_input_total(&self) -> f64 {
        self.environment.carbon_input_total() + self.carbon_imports + self.legacy_carbon_imports
    }

    /// All membrane requests use one pre-exchange field. Outward transfers are
    /// committed together before proportional, supply-limited inward transfers.
    /// O2/DIC diffuse passively; neither pumps nor a species identity are needed.
    pub(super) fn exchange_gases_in(&mut self, workspace: &mut PhaseWorkspace) {
        let groups = &mut workspace.groups;
        groups.resize_with(self.environment.patch_count(), Vec::new);
        for oxygen in [true, false] {
            for group in groups.iter_mut() { group.clear(); }
            for (index, cell) in self.cells.iter().enumerate() {
                if !cell.alive() { continue; }
                let patch = self.environment.patch_index(cell.x, cell.y);
                let volume = cell.gas_volume();
                let internal = if oxygen { cell.internal_oxygen } else { cell.internal_inorganic_carbon };
                let outside = if oxygen { self.environment.oxygen_patch(patch) } else { self.environment.inorganic_carbon_patch(patch) };
                let conductance = (0.6 * cell.body.area.sqrt() * DT).min(volume * 0.5);
                let amount = (outside - internal / volume) * conductance;
                if amount != 0.0 { groups[patch].push((index, amount.max(-internal))); }
            }
            for (patch, group) in groups.iter().enumerate() {
                if group.is_empty() { continue; }
                let mut demand = 0.0;
                for &(index, amount) in group {
                    if amount >= 0.0 { demand += amount; continue; }
                    let cell = &mut self.cells[index];
                    if oxygen {
                        cell.internal_oxygen += amount;
                        self.environment.release_oxygen(cell.x, cell.y, -amount);
                    } else {
                        cell.internal_inorganic_carbon += amount;
                        self.environment.release_inorganic_carbon(cell.x, cell.y, -amount);
                    }
                }
                if demand == 0.0 { continue; }
                let supplied = if oxygen { self.environment.take_oxygen(patch, demand) } else { self.environment.take_inorganic_carbon(patch, demand) };
                let factor = (supplied / demand).min(1.0);
                let mut remaining = supplied;
                for &(index, amount) in group {
                    if amount <= 0.0 { continue; }
                    let received = (amount * factor).min(remaining);
                    remaining = (remaining - received).max(0.0);
                    if oxygen { self.cells[index].internal_oxygen += received; }
                    else { self.cells[index].internal_inorganic_carbon += received; }
                }
                if remaining > 0.0 {
                    let cell = &self.cells[group[0].0];
                    if oxygen { self.environment.release_oxygen(cell.x, cell.y, remaining); }
                    else { self.environment.release_inorganic_carbon(cell.x, cell.y, remaining); }
                }
            }
        }
    }

    pub(super) fn respire_in(&mut self, caps: &[BodyCapacities]) {
        for (cell, cap) in self.cells.iter_mut().zip(caps) {
            cell.respiration_rate = 0.0;
            if !cell.alive() { continue; }
            // The minimal core maintains a small ATP working pool. Dedicated
            // respiratory machinery can fill the larger store under its own
            // authored activity control; a core does not automatically burn
            // every biosynthetic reserve just to saturate storage capacity.
            let basal = cap.basal_respiration.min(cap.respiration);
            let basal_room = (cap.energy * 0.25 - cell.energy).max(0.0);
            let capacity = (cap.respiration - basal) * DT + (basal * DT).min(basal_room);
            let carbon = (capacity / RESPIRATION_ENERGY_PER_CARBON)
                .min(cell.organic_carbon).min(cell.internal_oxygen)
                .min((cap.energy - cell.energy).max(0.0) / RESPIRATION_ENERGY_PER_CARBON);
            if carbon <= 0.0 { continue; }
            cell.organic_carbon -= carbon;
            cell.internal_oxygen -= carbon;
            cell.internal_inorganic_carbon += carbon;
            cell.energy += carbon * RESPIRATION_ENERGY_PER_CARBON;
            cell.respiration_rate = carbon / DT;
            self.carbon_respired_total += carbon;
        }
    }

    /// An explicit background aerobic decay reaction. No oxygen means no decay;
    /// particles retain their carbon. Its extent and O2 demand are accounted.
    pub(super) fn decay_food_in(&mut self, workspace: &mut PhaseWorkspace) {
        let groups = &mut workspace.groups;
        groups.resize_with(self.environment.patch_count(), Vec::new);
        for group in groups.iter_mut() { group.clear(); }
        let fractions = ORGANIC_DECAY.map(|rate| -(-rate * DT).exp_m1());
        for (index, food) in self.food.iter().enumerate() {
            let carbon = food.energy / CHEMICAL_ENERGY_PER_CARBON;
            let requested = if food.energy <= 1e-6 { carbon } else { carbon * fractions[food.kind as usize] };
            if requested > 0.0 { groups[self.environment.patch_index(food.x, food.y)].push((index, requested)); }
        }
        for (patch, group) in groups.iter().enumerate() {
            let demand: f64 = group.iter().map(|(_, amount)| amount).sum();
            if demand == 0.0 { continue; }
            let supplied = self.environment.take_oxygen(patch, demand);
            let factor = (supplied / demand).min(1.0);
            let mut remaining = supplied;
            for &(index, requested) in group {
                let food = &mut self.food[index];
                let carbon = food.energy / CHEMICAL_ENERGY_PER_CARBON;
                let consumed = (requested * factor).min(remaining).min(carbon);
                if consumed <= 0.0 { continue; }
                let fraction = (consumed / carbon).min(1.0);
                let minerals = food.nutrients * fraction;
                food.energy = (food.energy - consumed * CHEMICAL_ENERGY_PER_CARBON).max(0.0);
                food.nutrients = (food.nutrients - minerals).max(0.0);
                self.environment.recycle_nutrients(food.x, food.y, minerals);
                self.environment.release_inorganic_carbon(food.x, food.y, consumed);
                self.environment.emit_organic(food.x, food.y, consumed * CHEMICAL_ENERGY_PER_CARBON * 0.08);
                self.decay_carbon_respired_total += consumed;
                remaining = (remaining - consumed).max(0.0);
                if food.energy == 0.0 {
                    self.environment.recycle_nutrients(food.x, food.y, food.nutrients);
                    food.nutrients = 0.0;
                }
            }
            if remaining > 0.0 {
                let food = &self.food[group[0].0];
                self.environment.release_oxygen(food.x, food.y, remaining);
            }
        }
        self.food.retain(|food| food.energy > 0.0);
    }
}
