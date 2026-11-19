use super::*;
use crate::checkpoint::{ensure, Reader, Result, Writer};
fn stored_contents_fit(cell: &Cell, caps: &BodyCapacities) -> bool {
    cell.energy <= caps.energy + 1e-9
        && cell.organic_carbon <= caps.energy / CHEMICAL_ENERGY_PER_CARBON + 1e-9
        && cell.nutrient_stored <= caps.minerals + 1e-9
        && cell.gut_energy <= caps.gut_energy + 1e-9
        && cell.gut_nutrients <= caps.gut_minerals + 1e-9
}

impl World {
    /// Old tolerance occasionally activated a store without all its ingredients.
    /// Its actual contents were validated under the historical physical rule.
    /// Preserve excess ATP as potential on the existing structural carbon carrier
    /// and use normal closed-system spill/lysis for other displaced material.
    pub(crate) fn migrate_pre_v16_compartments(&mut self) -> Result<()> {
        let mut changed = Vec::new();
        let mut packet_energy = 0.0;
        let mut packet_overhead = 0usize;
        for (row, cell) in self.cells.iter_mut().enumerate() {
            let caps = cell.body.capacities();
            if cell.alive() && stored_contents_fit(cell, &caps) { continue; }
            changed.push((row, caps));
            let excess_atp = (cell.energy - caps.energy).max(0.0);
            cell.energy -= excess_atp;
            cell.structural_enrichment += excess_atp;
            if !cell.alive() {
                packet_energy += cell.food_potential().total();
                packet_overhead += 4;
            } else {
                let carbon = (cell.organic_carbon - caps.energy / CHEMICAL_ENERGY_PER_CARBON).max(0.0);
                if carbon > 0.0 {
                    packet_energy += carbon * CHEMICAL_ENERGY_PER_CARBON + cell.reserve_extra_for(carbon);
                    packet_overhead += 1;
                }
                if cell.gut_energy > caps.gut_energy || cell.gut_nutrients > caps.gut_minerals {
                    packet_energy += cell.gut_energy;
                    packet_overhead += 3;
                }
            }
        }
        if !changed.is_empty() {
            if packet_overhead != 0 { self.reserve_organic_packets(packet_energy, packet_overhead)?; }
            let mut phases = PhaseWorkspace::default();
            for (row, caps) in changed {
                Self::collect_store_overflow(&mut self.cells[row], &caps, &mut self.environment, &mut phases.emissions);
            }
            self.emit_organic_pending(&mut phases);
            self.remove_dead_in(&mut phases);
        }
        Ok(())
    }

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
            if let Some(q)=&c.counted{q.write(w);c.body.count_funding.as_ref().expect("counted body funding").write(w);}
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
            if let Some(q)=f.counted {q.meal.write(w);w.u32(q.decay_credit);}
        }
        w.count(self.contacts.len());
        for (&(a,b),&ticks) in &self.contacts {
            w.u32(a); w.u32(b); w.u8(ticks);
        }
        if self.uses_count_chemistry(){
            let f=self.count_flux;
            for n in [f.mineral_in,f.carbon_in,f.energy_in,f.energy_out,f.mineral_out,f.fixed,f.respired,f.decay]{w.u64(n);}
            w.count(self.photon_credit.len());
            for &credit in &self.photon_credit { w.u32(credit); }
            w.count(self.deposits.len());
            for d in &self.deposits {w.f64(d.x);w.f64(d.y);for n in d.quantities {w.u32(n);}}
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
            let mut body = Body::read_checkpoint_version(r, version)?;
            let counted=if environment.uses_count_chemistry() {
                let q=crate::counts::CellCounts::read(r)?;
                body.count_funding=Some(crate::body::BodyFunding::read(r)?);
                body.validate_count_funding()?;
                let raw=[q.atp,q.mineral,q.carbon,q.oxygen,q.dic,q.reserve_extra,q.structural_extra];
                let views=[energy,nutrient_stored,gases[0],gases[1],gases[2],reserve_enrichment,structural_enrichment];
                ensure(raw.iter().zip(views).all(|(&n,v)|crate::counts::units(n)==v),"counted cell projections disagree with inventory")?;
                for (q,v) in q.gut.iter().zip(&gut){
                    ensure(crate::counts::wide_units(q.energy())==v.energy && crate::counts::units(q.mineral)==v.nutrients
                        &&crate::counts::units(q.extra)==v.extra_energy,"counted gut projection disagrees with inventory")?;
                }
                Some(q)
            } else {
                if version >= 17 {
                    body.validate()?;
                }
                None
            };
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
                counted,
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
                (c.alive() || version < 16 && c.energy > 0.0 && c.integrity > 0.0
                    && c.body.cellular.as_ref().is_some_and(|state| state.assemblies.iter().any(|a|
                        matches!(a.recipe.kind, AssemblyKind::Organelle(Kind::Core) | AssemblyKind::Mechanism(MechanismKind::Support))
                            && a.pre_v16_functional_mass() > 0.0)))
                    && c.body.area <= MAX_AREA
                    && birth_area > 0.0
                    && birth_area <= c.body.area + 1e-10,
                "invalid living body",
            )?;
            ensure(
                stored_contents_fit(&c, &caps)
                    || version < 16 && stored_contents_fit(&c, &c.body.pre_v16_capacities()),
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
            let energy = r.range(0.0, if environment.uses_count_chemistry() {crate::counts::wide_units(u64::from(u32::MAX)*21)} else {24.0})?;
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
            let counted=if environment.uses_count_chemistry(){
                let q=crate::counts::CountMeal::read(r)?;
                ensure(q.carbon>0 && energy==crate::counts::wide_units(q.energy()) && nutrients==crate::counts::units(q.mineral)
                    &&extra_energy==crate::counts::units(q.extra),"counted particle projections disagree with inventory")?;
                Some(crate::counts::FoodCounts{meal:q,decay_credit:r.u32()?})
            }else{None};
            food.push(Food {
                counted,
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
        let mut count_flux=counted::Flux::default();
        let mut deposits=Vec::new();
        let mut photon_credit = Vec::new();
        if environment.uses_count_chemistry(){
            let f=&mut count_flux;
            for v in [&mut f.mineral_in,&mut f.carbon_in,&mut f.energy_in,&mut f.energy_out,&mut f.mineral_out,&mut f.fixed,&mut f.respired,&mut f.decay]{
                *v=r.u64()?;ensure(*v<=u64::MAX/128,"count reaction ledger outside safe range")?;
            }
            let patch_count = environment.patch_count();
            let light_columns = r.count(patch_count)?;
            ensure(light_columns == 0 || light_columns == patch_count, "invalid light credit geometry")?;
            for _ in 0..light_columns { photon_credit.push(r.u32()?); }
            let n=r.u32()? as usize;ensure(n<=r.remaining()/28,"invalid delayed material deposit count")?;
            for _ in 0..n{
                let x=r.range(0.0,environment.width)?;let y=r.range(0.0,environment.height)?;
                let quantities=[r.u32()?,r.u32()?,r.u32()?];
                ensure(x<environment.width&&y<environment.height&&quantities!=[0;3],"invalid delayed material deposit")?;
                deposits.push(counted::LocalDeposit{x,y,quantities});
            }
        }
        let mut world = Self {
            count_flux, deposits, photon_credit,
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
        if let Some(a)=world.count_audit(){
            for axis in [0,3,6] {ensure(a[axis].checked_add(a[axis+2])==Some(a[axis+1]),"integer chemical inventory does not balance")?;}
            ensure(a[9].checked_add(a[11]).is_some_and(|held|held<=a[10]),"integer energy exceeds its inputs")?;
            let f=world.count_flux;
            for (n,v) in [(f.mineral_in,world.nutrient_imports),(f.carbon_in,world.carbon_imports),(f.energy_in,world.energy_imports),
                (f.energy_out,world.energy_withdrawn),(f.mineral_out,world.mineral_withdrawn),(f.fixed,world.carbon_fixed_total),
                (f.respired,world.carbon_respired_total),(f.decay,world.decay_carbon_respired_total)] {
                ensure(crate::counts::wide_units(n)==v,"projected count-flow ledger disagrees with raw ledger")?;
            }
        }
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
    fn continuous_v17_rejects_an_ingredient_rounded_up_to_a_whole_count() {
        let mut engine = Engine::new();
        let id = engine.compile("cellular; product Wall=membrane(); product Trace=scaffold();
            gene make: Wall=1; assembly E: store(energy) { Wall:0.04; Trace:0.00001; }");
        engine.world.reset_empty(41);
        engine.world.place_cell(id, 200.0, 100.0, &engine.genomes).unwrap();
        for _ in 0..100 { engine.world.step(&mut engine.genomes); }
        let body = &mut engine.world.cells[0].body;
        let assembly = &mut body.cellular.as_mut().unwrap().assemblies[0];
        assembly.ingredients[1].mass = 0.0001;
        let bytes = checkpoint::save(&engine).unwrap();
        assert!(checkpoint::restore(&bytes).err().unwrap().contains("assembly ingredient exceeds recipe"));
    }

    #[test]
    fn v15_trace_storage_validates_historical_capacity_and_moves_only_real_contents() {
        let mut old = Engine::new();
        let id = old.compile("cellular; product Wall=membrane(); product Trace=scaffold(); gene make: Wall=1;
            assembly E: store(energy) { Wall:0.04; Trace:0.0000000000001; }
            assembly M: store(mineral) { Wall:0.04; Trace:0.0000000000001; }
            assembly G: store(food) { Wall:0.04; Trace:0.0000000000001; }");
        old.world.reset_empty(41);
        old.world.place_cell(id, 200.0, 100.0, &old.genomes).unwrap();
        for _ in 0..1200 { old.world.step(&mut old.genomes); }
        let c = &mut old.world.cells[0];
        let historical = c.body.pre_v16_capacities();
        let corrected = c.body.capacities();
        assert!(historical.energy > corrected.energy + 20.0 && historical.minerals > corrected.minerals + 0.1
            && historical.gut_energy >= 3.0 && corrected.gut_energy == 0.0);
        let energy = corrected.energy + 20.0;
        let carbon = corrected.energy / CHEMICAL_ENERGY_PER_CARBON + 0.5;
        let mineral = corrected.minerals + 0.1;
        old.world.energy_imports += energy - c.energy + (carbon - c.organic_carbon) * CHEMICAL_ENERGY_PER_CARBON + 3.0;
        old.world.carbon_imports += carbon - c.organic_carbon + 0.1;
        old.world.nutrient_imports += mineral - c.nutrient_stored + 0.01;
        c.energy = energy;
        c.organic_carbon = carbon;
        c.nutrient_stored = mineral;
        c.ingest_enriched(OrganicKind::Tissue, 3.0, 0.01, 1.0);
        let as_v15 = |engine: &Engine| {
            // This fixture uses only pre-v16 graph/body tags. Version 17 adds
            // one environment mode byte to the continuous-world payload.
            let mut bytes = checkpoint::save(engine).unwrap();
            let mut world = Writer::new();
            engine.world.write_checkpoint(&mut world);
            let mut env = Writer::new();
            engine.world.environment.write_checkpoint(&mut env);
            let offset = world.0.windows(env.0.len()).position(|w| w == env.0).unwrap();
            let position = bytes.len() - world.0.len() + offset;
            assert_eq!(bytes.remove(position), 0);
            bytes[8..12].copy_from_slice(&15_u32.to_le_bytes());
            let length = bytes.len() as u32;
            bytes[12..16].copy_from_slice(&length.to_le_bytes());
            let checksum = checkpoint::checksum(&bytes[20..]);
            bytes[16..20].copy_from_slice(&checksum.to_le_bytes());
            bytes
        };
        let mut migrated = checkpoint::restore(&as_v15(&old)).unwrap();
        let c = &migrated.world.cells[0];
        let close = |a: f64, b: f64| assert!((a-b).abs() < 1e-8, "{a} != {b}");
        close(c.energy, corrected.energy);
        close(c.structural_enrichment, 20.0);
        close(c.nutrient_stored, corrected.minerals);
        close(c.organic_carbon, corrected.energy / CHEMICAL_ENERGY_PER_CARBON);
        close(c.gut_energy, 0.0);
        assert!(migrated.world.food.iter().any(|p| p.kind == OrganicKind::Tissue && p.extra_energy > 0.0));
        close(old.world.energy_total(), migrated.world.energy_total());
        close(old.world.nutrient_total(), migrated.world.nutrient_total());
        close(old.world.carbon_total(), migrated.world.carbon_total());
        let canonical = checkpoint::save(&migrated).unwrap();
        assert_eq!(checkpoint::save(&checkpoint::restore(&canonical).unwrap()).unwrap(), canonical);
        let impossible = historical.energy + 1.0;
        old.world.energy_imports += impossible - old.world.cells[0].energy;
        old.world.cells[0].energy = impossible;
        assert!(!migrated.restore_state(&as_v15(&old)));
        assert!(migrated.error.contains("contents exceed physical compartments"));
        assert_eq!(checkpoint::save(&migrated).unwrap(), canonical);
    }

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
