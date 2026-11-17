use super::*;
use crate::checkpoint::{ensure, Reader, Result, Writer};

impl World {
    pub(crate) fn write_checkpoint(&self, w: &mut Writer) {
        for count in [self.ticks, self.births, self.deaths, self.eaten, self.mutations, self.mutations_skipped, self.predations, self.attacks] { w.u64(count); }
        for value in [self.food_rate, self.mutation_rate, self.photosynthesis_total, self.nutrient_imports, self.food_credit] { w.f64(value); }
        w.u32(self.next_id); w.u32(self.rng.0); w.u32(self.seed);
        self.environment.write_checkpoint(w);
        w.count(self.cells.len());
        for cell in &self.cells {
            for id in [cell.id, cell.genome_id, cell.generation, cell.parent_id] { w.u32(id); }
            for value in [cell.x, cell.y, cell.vx, cell.vy, cell.angle, cell.energy, cell.size, cell.age, cell.cooldown,
                cell.integrity, cell.photosynthesis_rate, cell.nutrient_stored, cell.uptake_rate, cell.remodeling_rate, cell.engulf_cooldown] { w.f64(value); }
            for value in cell.body { w.f64(value); }
            cell.memory.write_checkpoint(w);
        }
        w.count(self.food.len());
        for food in &self.food { for value in [food.x, food.y, food.energy, food.nutrients] { w.f64(value); } }
    }
    pub(crate) fn read_checkpoint(r: &mut Reader<'_>, genome_count: usize) -> Result<Self> {
        let mut counts = [0; 8];
        for count in &mut counts { *count = r.u64()?; ensure(*count < u64::MAX / 2, "counter is outside its safe range")?; }
        let food_rate = r.range(0.0, 60.0)?; let mutation_rate = r.range(0.0, 1.0)?;
        let photosynthesis_total = r.range(0.0, 1e100)?; let nutrient_imports = r.range(0.0, 1e100)?;
        let food_credit = r.range(0.0, 1.0)?;
        let next_id = r.u32()?; let random_state = r.u32()?; let seed = r.u32()?;
        ensure(next_id > 0 && random_state != 0, "invalid cell or random sequence")?;
        let environment = Environment::read_checkpoint(r)?;
        let count = r.count(MAX_CELLS)?; let mut cells = Vec::with_capacity(count);
        let mut ids = std::collections::HashSet::new();
        for _ in 0..count {
            let id = r.u32()?; let genome_id = r.u32()?; let generation = r.u32()?; let parent_id = r.u32()?;
            ensure(id > 0 && id < next_id && ids.insert(id), "invalid or duplicate cell ID")?;
            ensure(genome_id > 0 && genome_id as usize <= genome_count, "cell refers to an unavailable genome")?;
            ensure(parent_id < id && ((parent_id == 0) == (generation == 0)), "invalid cell ancestry")?;
            let x = r.range(0.0, WIDTH)?; let y = r.range(0.0, HEIGHT)?;
            ensure(x < WIDTH && y < HEIGHT, "cell is outside the habitat")?;
            let vx = r.range(-1e12, 1e12)?; let vy = r.range(-1e12, 1e12)?; let angle = r.range(0.0, TAU)?;
            let energy = r.range(0.0, MAX_ENERGY * MAX_SIZE)?; let size = r.range(MIN_SIZE, MAX_SIZE)?;
            let age = r.range(0.0, 1e100)?; let cooldown = r.range(0.0, DIVISION_COOLDOWN)?;
            let integrity = r.range(0.0, 1.0)?; let photosynthesis_rate = r.range(0.0, 1e12)?;
            let nutrient_stored = r.range(0.0, 2.0 * size + 1e-12)?;
            let uptake_rate = r.range(0.0, 1e12)?; let remodeling_rate = r.range(0.0, 1e12)?;
            let engulf_cooldown = r.range(0.0, 8.0)?;
            let mut body = [0.0; 5]; for value in &mut body { *value = r.range(0.0, 1.0)?; }
            ensure(energy > 0.0 && energy <= MAX_ENERGY * size + 1e-9 && integrity > 0.0 && body.iter().sum::<f64>() <= 1.0 + 1e-12, "cell violates biological limits")?;
            let memory = Registers::read_checkpoint(r)?;
            cells.push(Cell { id, genome_id, x, y, vx, vy, angle, energy, size, age, generation, parent_id, cooldown,
                memory, integrity, photosynthesis_rate, nutrient_stored, body, uptake_rate, remodeling_rate, engulf_cooldown });
        }
        let count = r.count(MAX_FOOD)?; let mut food = Vec::with_capacity(count);
        for _ in 0..count {
            let x = r.range(0.0, WIDTH)?; let y = r.range(0.0, HEIGHT)?;
            let energy = r.range(0.0, 24.0)?; let nutrients = r.range(0.0, 1e100)?;
            ensure(x < WIDTH && y < HEIGHT && energy > 0.0, "invalid food particle")?;
            food.push(Food { x, y, energy, nutrients });
        }
        Ok(Self { cells, food, ticks: counts[0], births: counts[1], deaths: counts[2], eaten: counts[3], food_rate, mutation_rate,
            mutations: counts[4], mutations_skipped: counts[5], environment, predations: counts[6], attacks: counts[7],
            photosynthesis_total, nutrient_imports, food_credit, next_id, rng: Rng(random_state), seed })
    }
}
