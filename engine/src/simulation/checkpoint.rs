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
            for value in [cell.gut_energy, cell.gut_nutrients, cell.digestion_rate, cell.growth_rate,
                cell.birth_size, cell.exudation_rate] { w.f64(value); }
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
            let mut body = [0.0; 6];
            let body_count = if r.version() >= 2 { 6 } else { 5 };
            for value in &mut body[..body_count] { *value = r.range(0.0, 1.0)?; }
            let (gut_energy, gut_nutrients, digestion_rate, growth_rate, birth_size, exudation_rate) = if r.version() >= 2 {
                (r.range(0.0, 100.0 * size * body[DIGESTION] / 0.2 + 1e-9)?, r.range(0.0, 1e100)?,
                    r.range(0.0, 1e12)?, r.range(0.0, 1e12)?, r.range(MIN_SIZE, size)?, r.range(0.0, 1e12)?)
            } else { (0.0, 0.0, 0.0, 0.0, size.min(1.0), 0.0) };
            ensure(gut_energy > 0.0 || gut_nutrients == 0.0, "empty gut contains undigested minerals")?;
            ensure(energy > 0.0 && energy <= MAX_ENERGY * size + 1e-9 && integrity > 0.0 && body.iter().sum::<f64>() <= 1.0 + 1e-12, "cell violates biological limits")?;
            let memory = Registers::read_checkpoint(r)?;
            cells.push(Cell { id, genome_id, x, y, vx, vy, angle, energy, size, age, generation, parent_id, cooldown,
                memory, integrity, photosynthesis_rate, nutrient_stored, body, uptake_rate, remodeling_rate, engulf_cooldown,
                gut_energy, gut_nutrients, digestion_rate, growth_rate, birth_size, exudation_rate });
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

#[cfg(test)]
mod tests {
    use crate::checkpoint::{restore, save};

    #[test]
    fn trophic_fields_roundtrip_exactly_and_invalid_guts_or_birth_mass_are_atomic() {
        let mut engine = crate::Engine::new();
        let id = engine.compile("thrust = 0; turn = 0; divide = false; grow = .5; photosynthesize = 1; invest.photosynthesis = 0; invest.attack = .2;");
        engine.world.seed_cells(id, 1, -1.0, -1.0);
        let cell = &mut engine.world.cells[0];
        cell.body = [0.2, 0.0, 0.2, 0.2, 0.2, 0.2];
        cell.age = 37.25; cell.size = 1.8; cell.birth_size = 0.9;
        cell.gut_energy = 42.75; cell.gut_nutrients = 0.625;
        cell.digestion_rate = 1.25; cell.growth_rate = 0.03; cell.exudation_rate = 0.125;
        let body = cell.body;
        let bytes = save(&engine).unwrap();
        let mut copy = restore(&bytes).unwrap();
        let actual = &copy.world.cells[0];
        assert_eq!(actual.body, body);
        assert_eq!((actual.gut_energy, actual.gut_nutrients, actual.digestion_rate,
            actual.growth_rate, actual.birth_size, actual.exudation_rate),
            (42.75, 0.625, 1.25, 0.03, 0.9, 0.125));
        assert_eq!(save(&copy).unwrap(), bytes);
        for _ in 0..60 { engine.world.step(&mut engine.genomes); copy.world.step(&mut copy.genomes); }
        assert_eq!(save(&engine).unwrap(), save(&copy).unwrap());

        let before = save(&engine).unwrap();
        for invalid in 0..7 {
            let mut bad = restore(&before).unwrap();
            let cell = &mut bad.world.cells[0];
            match invalid {
                0 => cell.gut_energy = cell.gut_capacity() + 1.0,
                1 => cell.gut_energy = f64::NAN,
                2 => cell.gut_nutrients = -1.0,
                3 => cell.birth_size = 0.0,
                4 => cell.birth_size = cell.size + 0.1,
                5 => cell.growth_rate = f64::INFINITY,
                _ => { cell.gut_energy = 0.0; cell.gut_nutrients = 1.0; }
            }
            assert!(!engine.restore_state(&save(&bad).unwrap()));
            assert_eq!(save(&engine).unwrap(), before);
        }
    }

    #[test]
    fn full_guts_roundtrip_after_division_and_capacity_reducing_remodeling() {
        let mut engine = crate::Engine::new();
        let id = engine.compile("thrust = 0; turn = 0; divide = true; feed = 0; absorb = 0;");
        engine.world.seed_cells(id, 1, -1.0, -1.0);
        engine.world.food_rate = 0.0; engine.world.mutation_rate = 0.0;
        let cell = &mut engine.world.cells[0];
        cell.size = 2.0; cell.energy = 300.0; cell.cooldown = 0.0; cell.age = 20.0;
        cell.gut_energy = cell.gut_capacity(); cell.gut_nutrients = 1.0;
        let minerals = engine.world.nutrient_total();
        engine.world.step(&mut engine.genomes);
        assert_eq!(engine.world.cells.len(), 2);
        for cell in &engine.world.cells {
            assert_eq!(cell.gut_energy, cell.gut_capacity());
            assert_eq!(cell.gut_nutrients, 0.5);
        }
        let divided = save(&engine).unwrap();
        assert_eq!(save(&restore(&divided).unwrap()).unwrap(), divided);
        let smaller_gut = engine.compile("thrust = 0; turn = 0; divide = false; feed = 0; absorb = 0; invest.digestion = .1; invest.attack = .1;");
        for cell in &mut engine.world.cells { cell.genome_id = smaller_gut; }
        engine.world.step(&mut engine.genomes);
        assert!(engine.world.cells.iter().all(|cell| cell.gut_capacity() < 100.0
            && (cell.gut_energy - cell.gut_capacity()).abs() < 1e-9));
        assert!(!engine.world.food.is_empty(), "remodeling spills the digestive contents that no longer fit");
        assert!((engine.world.nutrient_total() - minerals).abs() < 1e-9);
        let remodeled = save(&engine).unwrap();
        let mut restored = restore(&remodeled).unwrap();
        assert_eq!(save(&restored).unwrap(), remodeled);
        for _ in 0..30 { engine.world.step(&mut engine.genomes); restored.world.step(&mut restored.genomes); }
        assert_eq!(save(&restored).unwrap(), save(&engine).unwrap());
    }
}
