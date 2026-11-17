use super::*;
use crate::checkpoint::{ensure, Reader, Result, Writer};
impl World {
    pub(crate) fn write_checkpoint(&self, w: &mut Writer) {
        for v in [
            self.ticks,
            self.births,
            self.deaths,
            self.eaten,
            self.mutations,
            self.mutations_skipped,
            self.predations,
            self.attacks,
        ] {
            w.u64(v);
        }
        for v in [
            self.food_rate,
            self.mutation_rate,
            self.photosynthesis_total,
            self.nutrient_imports,
            self.energy_imports,
            self.food_credit,
        ] {
            w.f64(v);
        }
        w.u32(self.next_id);
        w.u64(self.next_food_id);
        w.u32(self.rng.0);
        w.u32(self.seed);
        self.environment.write_checkpoint(w);
        w.count(self.cells.len());
        for c in &self.cells {
            if w.failed() {
                return;
            }
            for id in [c.id, c.genome_id, c.generation, c.parent_id] {
                w.u32(id);
            }
            for v in [
                c.x,
                c.y,
                c.vx,
                c.vy,
                c.angle,
                c.energy,
                c.age,
                c.cooldown,
                c.integrity,
                c.nutrient_stored,
                c.gut_energy,
                c.gut_nutrients,
                c.birth_area,
                c.photosynthesis_rate,
                c.uptake_rate,
                c.digestion_rate,
                c.growth_rate,
                c.exudation_rate,
                c.construction_rate,
                c.recycling_rate,
                c.repair_rate,
                c.engulf_cooldown,
                c.scan_bearing,
                c.scan_width,
            ] {
                w.f64(v);
            }
            c.body.write_checkpoint(w);
            c.memory.write_checkpoint(w);
            w.text(&c.scan_target);
            for i in 0..FAMILY_COUNT {
                w.u32(c.requested.target_counts[i]);
                for v in [
                    c.requested.target_sizes[i],
                    c.requested.build[i],
                    c.requested.recycle[i],
                ] {
                    w.f64(v);
                }
            }
            w.u64(c.tracking.next_serial);
            w.count(c.tracking.entries.len());
            for t in &c.tracking.entries {
                w.u64(t.entity);
                w.u64(t.serial);
                w.u64(t.seen);
            }
        }
        w.count(self.food.len());
        for f in &self.food {
            w.u64(f.id);
            for v in [f.x, f.y, f.energy, f.nutrients] {
                w.f64(v);
            }
        }
    }
    pub(crate) fn read_checkpoint(r: &mut Reader<'_>, genome_count: usize) -> Result<Self> {
        let mut counts = [0; 8];
        for v in &mut counts {
            *v = r.u64()?;
            ensure(*v < u64::MAX / 2, "counter outside safe range")?;
        }
        let food_rate = r.range(0.0, 60.0)?;
        let mutation_rate = r.range(0.0, 1.0)?;
        let photosynthesis_total = r.range(0.0, 1e100)?;
        let nutrient_imports = r.range(0.0, 1e100)?;
        let energy_imports = r.range(0.0, 1e100)?;
        let food_credit = r.range(0.0, 1.0)?;
        let next_id = r.u32()?;
        let next_food_id = r.u64()?;
        let rng = r.u32()?;
        let seed = r.u32()?;
        ensure(
            next_id > 0 && next_food_id > 0 && next_food_id <= u64::MAX / 2 && rng != 0,
            "invalid identifier or random sequence",
        )?;
        let environment = Environment::read_checkpoint(r)?;
        let count = r.u32()? as usize;
        ensure(
            count <= r.remaining().saturating_sub(4) / 700,
            "cell count exceeds checkpoint data",
        )?;
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(count)
            .map_err(|_| "Not enough memory to restore organisms".to_owned())?;
        let mut ids = std::collections::HashSet::new();
        for _ in 0..count {
            let id = r.u32()?;
            let genome_id = r.u32()?;
            let generation = r.u32()?;
            let parent_id = r.u32()?;
            ensure(
                id > 0 && id < next_id && ids.insert(id),
                "invalid or duplicate cell ID",
            )?;
            ensure(
                genome_id > 0 && genome_id as usize <= genome_count,
                "unavailable cell genome",
            )?;
            ensure(
                parent_id < id && ((parent_id == 0) == (generation == 0)),
                "invalid ancestry",
            )?;
            let x = r.range(0.0, WIDTH)?;
            let y = r.range(0.0, HEIGHT)?;
            ensure(x < WIDTH && y < HEIGHT, "cell outside habitat")?;
            let vx = r.range(-1e12, 1e12)?;
            let vy = r.range(-1e12, 1e12)?;
            let angle = r.range(0.0, TAU)?;
            let energy = r.range(0.0, 1e12)?;
            let age = r.range(0.0, LIFESPAN)?;
            let cooldown = r.range(0.0, DIVISION_COOLDOWN)?;
            let integrity = r.range(0.0, 1.0)?;
            let nutrient_stored = r.range(0.0, 1e12)?;
            let gut_energy = r.range(0.0, 1e12)?;
            let gut_nutrients = r.range(0.0, 1e12)?;
            let birth_area = r.range(0.0, MAX_AREA)?;
            let mut rates = [0.0; 8];
            for v in &mut rates {
                *v = r.range(0.0, 1e12)?;
            }
            let engulf_cooldown = r.range(0.0, 2.0)?;
            let scan_bearing = r.range(-PI, PI)?;
            let scan_width = r.range(0.0, TAU)?;
            let body = Body::read_checkpoint(r)?;
            let memory = Registers::read_checkpoint(r)?;
            let scan_target = Rc::from(r.text(128)?);
            let mut requested = BodyControls::default();
            for i in 0..FAMILY_COUNT {
                requested.target_counts[i] = r.u32()?;
                ensure(
                    requested.target_counts[i] <= i32::MAX as u32,
                    "invalid target count",
                )?;
                requested.target_sizes[i] = r.range(0.0, 1e12)?;
                requested.build[i] = r.range(0.0, 1.0)?;
                requested.recycle[i] = r.range(0.0, 1.0)?;
            }
            let next_serial = r.u64()?;
            let track_count = r.count(TRACK_LIMIT)?;
            let mut entries = Vec::with_capacity(track_count);
            let mut entities = std::collections::HashSet::new();
            let mut serials = std::collections::HashSet::new();
            for _ in 0..track_count {
                let entity = r.u64()?;
                let serial = r.u64()?;
                let seen = r.u64()?;
                ensure(
                    entity > 1
                        && entities.insert(entity)
                        && serial > 0
                        && serial <= next_serial
                        && serials.insert(serial)
                        && seen <= counts[0],
                    "invalid observation track",
                )?;
                // Expired tracks are allowed at save time but cannot grant sight.
                if entity & 1 == 0 {
                    ensure(entity >> 1 < next_id as u64, "track names an unissued cell")?;
                } else {
                    ensure(
                        entity >> 1 < next_food_id,
                        "track names an unissued particle",
                    )?;
                }
                entries.push(Track {
                    entity,
                    serial,
                    seen,
                });
            }
            let c = Cell {
                id,
                genome_id,
                generation,
                parent_id,
                x,
                y,
                vx,
                vy,
                angle,
                energy,
                age,
                cooldown,
                integrity,
                body,
                memory,
                nutrient_stored,
                gut_energy,
                gut_nutrients,
                birth_area,
                photosynthesis_rate: rates[0],
                uptake_rate: rates[1],
                digestion_rate: rates[2],
                growth_rate: rates[3],
                exudation_rate: rates[4],
                construction_rate: rates[5],
                recycling_rate: rates[6],
                repair_rate: rates[7],
                engulf_cooldown,
                requested,
                scan_bearing,
                scan_width,
                scan_target,
                tracking: Tracking {
                    entries,
                    next_serial,
                },
            };
            let caps = c.body.capacities();
            ensure(
                c.alive()
                    && c.body.area <= MAX_AREA
                    && birth_area > 0.0
                    && birth_area <= c.body.area + 1e-10
                    && age < LIFESPAN,
                "invalid living body",
            )?;
            ensure(
                energy <= caps.energy + 1e-9
                    && nutrient_stored <= caps.minerals + 1e-9
                    && gut_energy <= caps.gut_energy + 1e-9
                    && gut_nutrients <= caps.gut_minerals + 1e-9,
                "contents exceed physical compartments",
            )?;
            ensure(
                gut_energy > 0.0 || gut_nutrients == 0.0,
                "empty gut contains minerals",
            )?;
            cells.push(c);
        }
        let count = r.count(MAX_FOOD)?;
        let mut food = Vec::with_capacity(count);
        let mut food_ids = std::collections::HashSet::new();
        for _ in 0..count {
            let id = r.u64()?;
            let x = r.range(0.0, WIDTH)?;
            let y = r.range(0.0, HEIGHT)?;
            let energy = r.range(0.0, 24.0)?;
            let nutrients = r.range(0.0, 1e100)?;
            ensure(
                id > 0
                    && id < next_food_id
                    && food_ids.insert(id)
                    && x < WIDTH
                    && y < HEIGHT
                    && energy > 0.0,
                "invalid particle",
            )?;
            food.push(Food {
                id,
                x,
                y,
                energy,
                nutrients,
            });
        }
        let world = Self {
            cells,
            food,
            ticks: counts[0],
            births: counts[1],
            deaths: counts[2],
            eaten: counts[3],
            mutations: counts[4],
            mutations_skipped: counts[5],
            predations: counts[6],
            attacks: counts[7],
            food_rate,
            mutation_rate,
            photosynthesis_total,
            nutrient_imports,
            energy_imports,
            food_credit,
            next_id,
            next_food_id,
            rng: Rng(rng),
            seed,
            environment,
        };
        // Individual compartment bounds cannot detect a forged but locally
        // plausible reserve or particle. Preserve the complete world's ledger.
        // Tolerance accommodates accumulated floating-point transfer rounding,
        // without making the admissible error depend on the forged total.
        let material = world.nutrient_total();
        let introduced_material = world.nutrient_input_total();
        let material_tolerance = 1e-6 + introduced_material.abs() * 1e-9;
        ensure(
            material.is_finite()
                && introduced_material.is_finite()
                && (material - introduced_material).abs() <= material_tolerance,
            "material ledger does not match recorded introductions",
        )?;
        // Introduction energy includes fabrication heat; metabolism, conversion
        // losses and particle decay also dissipate energy. Equality is incorrect.
        let energy = world.energy_total();
        let supplied = world.energy_input_total() + world.photosynthesis_total;
        let energy_tolerance = 1e-6 + supplied.abs() * 1e-9;
        ensure(
            energy.is_finite() && supplied.is_finite() && energy <= supplied + energy_tolerance,
            "stored energy exceeds recorded introductions and photosynthesis",
        )?;
        Ok(world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{checkpoint, Engine};

    #[test]
    fn aggregate_restore_ledgers_reject_checksum_correct_resource_injection_atomically() {
        let mut live = Engine::new();
        let id = live.compile("thrust=0;turn=0;divide=false;feed=0;absorb=0;");
        live.world.reset_empty(97, 0);
        live.world
            .try_seed_body(id, 1, 200.0, 200.0, BodyRecipe::Bare)
            .unwrap();
        live.world.add_food(300.0, 200.0, 1);
        let before = checkpoint::save(&live).unwrap();
        assert_eq!(
            checkpoint::save(&checkpoint::restore(&before).unwrap()).unwrap(),
            before
        );
        for variation in 0..5 {
            let mut forged = checkpoint::restore(&before).unwrap();
            match variation {
                0 => forged.world.food[0].nutrients += 1.0,
                1 => forged.world.cells[0].nutrient_stored += 0.1,
                2 => {
                    // This bare recipe's remaining energy capacity exceeds its
                    // initial fabrication losses; filling it grants unrecorded
                    // energy while remaining inside the compartment bound.
                    let cell = &mut forged.world.cells[0];
                    cell.energy = cell.energy_capacity();
                    assert!(forged.world.energy_total() > forged.world.energy_input_total() + 1e-3);
                }
                3 => forged.world.cells[0].body.organs[0].embodied_energy += 1.0,
                _ => forged.world.cells[0].body.scaffold_energy += 1.0,
            }
            // Saving the forged in-memory state supplies a valid checksum; the
            // rejection must come from semantic validation, not corruption.
            let bytes = checkpoint::save(&forged).unwrap();
            assert!(
                !live.restore_state(&bytes),
                "accepted resource injection {variation}"
            );
            assert!(!live.error.contains("checksum"));
            if variation <= 1 {
                assert!(live.error.contains("material ledger"), "{}", live.error);
            }
            if variation == 2 {
                assert!(live.error.contains("stored energy"), "{}", live.error);
            }
            if variation >= 3 {
                assert!(live.error.contains("energy ledger"), "{}", live.error);
            }
            assert_eq!(
                checkpoint::save(&live).unwrap(),
                before,
                "failed restore changed live state"
            );
        }
    }
}
