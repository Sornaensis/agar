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
            self.mutation_rate,
            self.photosynthesis_total,
            self.nutrient_imports,
            self.energy_imports,
        ] {
            w.f64(v);
        }
        for v in [self.carbon_fixed_total, self.carbon_respired_total,
            self.decay_carbon_respired_total, self.carbon_imports, self.legacy_carbon_imports] { w.f64(v); }
        w.f64(self.energy_withdrawn);
        w.f64(self.mineral_withdrawn);
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
            for pool in &c.gut {
                w.f64(pool.energy);
                w.f64(pool.nutrients);
                w.f64(pool.extra_energy);
            }
            w.f64(c.synthesis_rate);
            for v in [c.organic_carbon, c.internal_oxygen, c.internal_inorganic_carbon,
                c.respiration_rate, c.carbon_fixation_rate] { w.f64(v); }
            w.f64(c.reserve_enrichment);
            w.f64(c.structural_enrichment);
            w.u32(c.adhered_neighbors);
            for amount in c.junction_net { w.f64(amount); }
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
            w.u8(f.kind as u8);
            w.f64(f.extra_energy);
        }
        w.count(self.contacts.len());
        for (&(a,b),&ticks) in &self.contacts {
            w.u32(a); w.u32(b); w.u8(ticks);
        }
    }
    pub(crate) fn read_checkpoint(
        r: &mut Reader<'_>,
        genome_count: usize,
        version: u32,
    ) -> Result<Self> {
        let mut counts = [0; 8];
        for v in &mut counts {
            *v = r.u64()?;
            ensure(*v < u64::MAX / 2, "counter outside safe range")?;
        }
        if version < 14 { r.range(0.0, 60.0)?; } // Obsolete provisioning rate.
        let mutation_rate = r.range(0.0, 1.0)?;
        let photosynthesis_total = r.range(0.0, 1e100)?;
        let nutrient_imports = r.range(0.0, 1e100)?;
        let energy_imports = r.range(0.0, 1e100)?;
        if version < 14 { r.range(0.0, 1.0)?; } // Obsolete fractional provisioning credit.
        let mut chemistry = [0.0; 5];
        if version >= 9 { for value in &mut chemistry { *value = r.range(0.0, 1e100)?; } }
        let (energy_withdrawn, mineral_withdrawn) = if version >= 12 {
            (r.range(0.0, 1e100)?, r.range(0.0, 1e100)?)
        } else { (0.0, 0.0) };
        let next_id = r.u32()?;
        let next_food_id = r.u64()?;
        let rng = r.u32()?;
        let seed = r.u32()?;
        ensure(
            next_id > 0 && next_food_id > 0 && next_food_id <= u64::MAX / 2 && rng != 0,
            "invalid identifier or random sequence",
        )?;
        let environment = Environment::read_checkpoint(r, version)?;
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
            let x = r.range(0.0, environment.width)?;
            let y = r.range(0.0, environment.height)?;
            ensure(x < environment.width && y < environment.height, "cell outside habitat")?;
            let vx = r.range(-1e12, 1e12)?;
            let vy = r.range(-1e12, 1e12)?;
            let angle = r.range(0.0, TAU)?;
            let energy = r.range(0.0, 1e12)?;
            let age = r.range(0.0, 1e100)?;
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
            let mut gut = [OrganicMatter::default(); 3];
            let synthesis_rate;
            if version >= 4 {
                for pool in &mut gut {
                    pool.energy = r.range(0.0, 1e12)?;
                    pool.nutrients = r.range(0.0, 1e12)?;
                    pool.extra_energy = read_substrate_enrichment(r, version, pool.energy)?;
                    ensure(
                        pool.energy > 0.0 || pool.nutrients == 0.0,
                        "empty substrate pool contains minerals",
                    )?;
                }
                synthesis_rate = r.range(0.0, 1e100)?;
                let pooled_energy: f64 = gut.iter().map(|pool| pool.energy).sum();
                let pooled_nutrients: f64 = gut.iter().map(|pool| pool.nutrients).sum();
                ensure(
                    (pooled_energy - gut_energy).abs() <= 1e-9 + gut_energy * 1e-10
                        && (pooled_nutrients - gut_nutrients).abs() <= 1e-9 + gut_nutrients * 1e-10,
                    "gut substrate totals disagree",
                )?;
            } else {
                // Version 3 did not retain provenance. Preserve the complete
                // mixed meal as tissue; never infer history from its ratio.
                gut[OrganicKind::Tissue as usize] = OrganicMatter {
                    energy: gut_energy,
                    nutrients: gut_nutrients,
                    extra_energy: 0.0,
                };
                synthesis_rate = 0.0;
            }
            let mut gases = [0.0; 5];
            if version >= 9 { for value in &mut gases { *value = r.range(0.0, 1e12)?; } }
            let (reserve_enrichment, structural_enrichment) = if version >= 13 {
                (r.range(0.0, 1e100)?, r.range(0.0, 1e100)?)
            } else { (0.0, 0.0) };
            ensure(gases[0] > 0.0 || reserve_enrichment == 0.0, "reserve energy without organic carbon")?;
            let mut adhered_neighbors = 0;
            let mut junction_net = [0.0; 3];
            if version >= 11 {
                adhered_neighbors = r.u32()?;
                ensure(adhered_neighbors < next_id.saturating_sub(1), "invalid adhesive neighbor count")?;
                for amount in &mut junction_net { *amount = r.range(-1e12, 1e12)?; }
                ensure(version >= 15 || adhered_neighbors > 0 || junction_net == [0.0; 3], "junction transfer without a contact")?;
            }
            let body = Body::read_checkpoint_version(r, version)?;
            ensure(body.structural_mass() > 0.0 || structural_enrichment == 0.0, "stored energy without body material")?;
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
                adhered_neighbors, junction_net, reserve_enrichment, structural_enrichment,
                organic_carbon: gases[0], internal_oxygen: gases[1], internal_inorganic_carbon: gases[2],
                respiration_rate: gases[3], carbon_fixation_rate: gases[4],
                gut_energy,
                gut_nutrients,
                gut,
                birth_area,
                photosynthesis_rate: rates[0],
                uptake_rate: rates[1],
                digestion_rate: rates[2],
                growth_rate: rates[3],
                exudation_rate: rates[4],
                construction_rate: rates[5],
                synthesis_rate,
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
                    && birth_area <= c.body.area + 1e-10,
                "invalid living body",
            )?;
            ensure(
                energy <= caps.energy + 1e-9
                    && c.organic_carbon <= caps.energy / CHEMICAL_ENERGY_PER_CARBON + 1e-9
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
        let count = r.u32()? as usize;
        let particle_bytes = if version >= 13 { 49 } else if version >= 4 { 41 } else { 40 };
        ensure(
            count <= r.remaining() / particle_bytes,
            "particle count exceeds checkpoint data",
        )?;
        let mut food = Vec::new();
        food.try_reserve_exact(count)
            .map_err(|_| "Not enough memory to restore organic particles".to_owned())?;
        let mut food_ids = std::collections::HashSet::new();
        food_ids
            .try_reserve(count)
            .map_err(|_| "Not enough memory to restore organic particle identities".to_owned())?;
        for _ in 0..count {
            let id = r.u64()?;
            let x = r.range(0.0, environment.width)?;
            let y = r.range(0.0, environment.height)?;
            let energy = r.range(0.0, 24.0)?;
            let nutrients = r.range(0.0, 1e100)?;
            let kind = if version >= 4 {
                OrganicKind::from_index(r.u8()?)
                    .ok_or("Invalid simulation save: unknown organic substrate.")?
            } else {
                OrganicKind::Tissue
            };
            ensure(
                id > 0
                    && id < next_food_id
                    && food_ids.insert(id)
                    && x < environment.width
                    && y < environment.height
                    && energy > 0.0,
                "invalid particle",
            )?;
            let extra_energy = read_substrate_enrichment(r, version, energy)?;
            food.push(Food {
                id,
                kind,
                x,
                y,
                energy,
                nutrients,
                extra_energy,
            });
        }
        let mut contacts = std::collections::BTreeMap::new();
        if version >= 15 {
            let pairs = r.u32()? as usize;
            let max_pairs = cells.len().saturating_mul(cells.len().saturating_sub(1)) / 2;
            ensure(pairs <= max_pairs && pairs <= r.remaining()/9, "invalid adhesion pair count")?;
            let mut previous = (0,0);
            for _ in 0..pairs {
                let pair = (r.u32()?,r.u32()?); let ticks = r.u8()?;
                ensure(previous < pair && pair.0 < pair.1 && ids.contains(&pair.0) && ids.contains(&pair.1)
                    && (1..=adhesion::CLONAL).contains(&ticks), "invalid adhesion pair")?;
                contacts.insert(pair,ticks);
                previous=pair;
            }
        }
        let mut world = Self {
            contacts,
            had_adhesive_contacts: cells.iter().any(|c| c.adhered_neighbors != 0),
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
            mutation_rate,
            photosynthesis_total,
            nutrient_imports,
            energy_imports, energy_withdrawn, mineral_withdrawn,
            carbon_fixed_total: chemistry[0], carbon_respired_total: chemistry[1],
            decay_carbon_respired_total: chemistry[2], carbon_imports: chemistry[3], legacy_carbon_imports: chemistry[4],
            next_id,
            next_food_id,
            rng: Rng(rng),
            seed,
            environment,
            eval_scratch: EvalScratch::default(),
            workspace: TickWorkspace::default(),
        };
        if version < 9 {
            // Old files had no carbon chemistry. Classify their existing body,
            // gut and particle inventories once, without interpreting ATP as C.
            world.legacy_carbon_imports = world.organic_cell_total() + world.organic_food_total();
            world.energy_imports += world.cells.iter().map(|c|
                c.body.structural_mass() * STRUCTURAL_CARBON_PER_MASS * CHEMICAL_ENERGY_PER_CARBON).sum::<f64>();
        }
        let introduced_carbon = world.carbon_input_total();
        let carbon_error = world.carbon_total() + world.environment.carbon_output_total() - introduced_carbon;
        ensure(carbon_error.abs() <= 1e-6 + introduced_carbon.abs() * 1e-9,
            "carbon ledger does not match recorded inventories and exchange")?;
        let oxygen_expected = world.environment.oxygen_input_total() + world.carbon_fixed_total
            - world.carbon_respired_total - world.decay_carbon_respired_total - world.environment.oxygen_output_total();
        let oxygen_error = world.environment.oxygen_total() + world.oxygen_cell_total() - oxygen_expected;
        ensure(oxygen_error.abs() <= 1e-6 + (world.environment.oxygen_input_total() + world.carbon_fixed_total) * 1e-9,
            "oxygen ledger does not match recorded reactions and exchange")?;
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
                && (material + world.environment.nutrient_removed_total() + world.mineral_withdrawn - introduced_material).abs() <= material_tolerance,
            "material ledger does not match recorded introductions",
        )?;
        // Introduction energy includes fabrication heat; metabolism, conversion
        // losses and particle decay also dissipate energy. Equality is incorrect.
        let energy = world.energy_total();
        let supplied = world.energy_input_total() + world.photosynthesis_total;
        let energy_tolerance = 1e-6 + supplied.abs() * 1e-9;
        ensure(
            energy.is_finite() && supplied.is_finite() && energy + world.energy_withdrawn <= supplied + energy_tolerance,
            "stored energy exceeds recorded introductions and photosynthesis",
        )?;
        Ok(world)
    }
}

/// The pre-v13 particle and gut energy values were entirely carbon-backed.
fn read_substrate_enrichment(reader: &mut Reader<'_>, version: u32, energy: f64) -> Result<f64> {
    if version < 13 { return Ok(0.0); }
    let extra = reader.range(0.0, energy)?;
    ensure(extra == 0.0 || extra < energy, "substrate energy without carrier carbon")?;
    Ok(extra)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{checkpoint, Engine};

    #[test]
    fn enrichment_roundtrips_and_requires_carrier_carbon() {
        let mut live = Engine::new();
        let genome = live.compile("thrust=0;turn=0;divide=false;feed=0;");
        live.world.reset_empty(8);
        live.world.try_seed_cells(genome, 1, 100.0, 100.0).unwrap();
        // Move some reserve carbon into a particle so the supply ledgers stay exact.
        live.world.cells[0].organic_carbon -= 0.1;
        assert!(live.world.particle(OrganicKind::Tissue, 200.0, 200.0, 2.0, 0.0));
        let energy = live.world.energy_total();
        let carbon = live.world.carbon_total();
        live.world.cells[0].energy -= 3.0;
        live.world.cells[0].reserve_enrichment = 1.0;
        live.world.cells[0].structural_enrichment = 1.0;
        live.world.food[0].energy += 1.0;
        live.world.food[0].extra_energy = 1.0;
        let good = checkpoint::save(&live).unwrap();
        let restored = checkpoint::restore(&good).unwrap();
        assert!((restored.world.energy_total() - energy).abs() < 1e-9);
        assert!((restored.world.carbon_total() - carbon).abs() < 1e-9);
        assert_eq!(checkpoint::save(&restored).unwrap(), good);
        for case in 0..3 {
            let mut forged = checkpoint::restore(&good).unwrap();
            match case {
                0 => forged.world.food[0].extra_energy = forged.world.food[0].energy,
                1 => forged.world.cells[0].organic_carbon = 0.0,
                _ => forged.world.cells[0].gut[0].extra_energy = 1.0,
            }
            assert!(!live.restore_state(&checkpoint::save(&forged).unwrap()));
            assert_eq!(checkpoint::save(&live).unwrap(), good);
        }
    }

    #[test]
    fn mildly_heat_damaged_universal_core_survives_and_roundtrips_without_free_headroom() {
        let mut live = Engine::new();
        let id = live.compile("thrust=0;turn=0;divide=true;grow=1;absorb=0;feed=0;");

        live.world.environment.configure(0.9, 0.0, 0.0);
        live.world.try_seed_cells(id, 1, 200.0, 200.0).unwrap();
        live.world.step(&mut live.genomes);
        assert_eq!(live.world.cells.len(), 1);
        let cell = &live.world.cells[0];
        assert_eq!(cell.body.organs.len(), 1);
        assert_eq!(cell.body.organs[0].mass, Kind::Core.min_mass());
        assert!(cell.body.organs[0].integrity < 1.0);
        assert!(
            !cell.body.viable(),
            "full support is required for new frame, not survival"
        );
        assert!(cell.alive());
        assert_eq!(cell.body.area, 1.0);
        let saved = checkpoint::save(&live).unwrap();
        let restored = checkpoint::restore(&saved).unwrap();
        assert_eq!(checkpoint::save(&restored).unwrap(), saved);
    }

    #[test]
    fn aggregate_restore_ledgers_reject_checksum_correct_resource_injection_atomically() {
        let mut live = Engine::new();
        let id = live.compile("thrust=0;turn=0;divide=false;feed=0;absorb=0;");
        live.world.reset_empty(97);
        live.world
            .try_seed_fixture(id, 1, 200.0, 200.0, BodyFixture::Bare)
            .unwrap();
        live.world.fixture_exudate(300.0, 200.0, 1);
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
    #[test]
    fn old_cells_and_uncapped_particles_roundtrip_with_exact_gut_substrates() {
        let mut live = Engine::new();
        let id = live.compile("thrust=0;turn=0;divide=false;feed=0;absorb=0;");
        live.world.reset_empty(31);
        live.world.fixture_exudate(-1.0,-1.0,2501);
        live.world
            .try_seed_fixture(id, 1, 200.0, 200.0, BodyFixture::Scavenger)
            .unwrap();
        live.world.cells[0].age = 401.0;
        for kind in OrganicKind::ALL {
            let food = live.world.food.pop().unwrap();
            live.world.cells[0].ingest(kind, food.energy, food.nutrients);
        }
        let before = checkpoint::save(&live).unwrap();
        let restored = checkpoint::restore(&before).unwrap();
        assert_eq!(restored.world.food.len(), 2498);
        assert_eq!(restored.world.cells[0].age, 401.0);
        assert_eq!(restored.world.cells[0].gut, live.world.cells[0].gut);
        assert_eq!(checkpoint::save(&restored).unwrap(), before);
        let mut forged = checkpoint::restore(&before).unwrap();
        forged.world.cells[0].gut[0].energy += 1.0;
        let bytes = checkpoint::save(&forged).unwrap();
        assert!(!live.restore_state(&bytes));
        assert!(
            live.error.contains("gut substrate totals"),
            "{}",
            live.error
        );
        assert_eq!(checkpoint::save(&live).unwrap(), before);
    }

    #[test]
    fn empty_reset_stays_organic_free_and_has_no_external_carbon_or_energy_provision() {
        let mut engine = Engine::new();
        engine.world.reset_empty(12);
        let initial_carbon = engine.world.carbon_input_total();
        for _ in 0..240 { engine.world.step(&mut engine.genomes); }
        assert!(engine.world.food.is_empty());
        assert_eq!(engine.world.energy_input_total(), 0.0);
        assert_eq!(engine.world.carbon_input_total(), initial_carbon);
        assert_eq!(engine.world.organic_food_total(), 0.0);
        let saved = checkpoint::save(&engine).unwrap();
        assert_eq!(checkpoint::save(&checkpoint::restore(&saved).unwrap()).unwrap(), saved);
    }
}
