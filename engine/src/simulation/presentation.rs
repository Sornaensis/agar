//! Read-only presentation projections. These buffers are not simulation state.
use super::*;
use std::collections::BTreeMap;

pub(super) const RENDER_HEADER: usize = 12;
pub(super) const RENDER_CELL_STRIDE: usize = 21;
pub(super) const METRICS_HEADER: usize = 54;

impl World {
    pub(crate) fn cell_snapshot(&self, id: u32, target: &mut Vec<f64>) -> bool {
        target.clear();
        let Some(cell) = self.cells.iter().find(|cell| cell.id == id && cell.alive()) else {
            return false;
        };
        self.snapshot_cell_into(cell, target);
        true
    }

    /// Geometry and visual capabilities only: no inspection, fields or metrics.
    pub(crate) fn render_snapshot(&self, target: &mut Vec<f64>) {
        target.clear();
        target.extend_from_slice(&[
            self.ticks as f64,
            self.time(),
            self.cells.len() as f64,
            self.food.len() as f64,
            self.births as f64,
            self.deaths as f64,
            self.eaten as f64,
            self.environment.width,
            self.environment.height,
            0.0, // Number of sparse outgoing signals appended after particles.
            self.mutation_rate,
            0.0,
        ]);
        let mut generation = 0;
        for cell in &self.cells {
            let mut family = cell.body.family_summaries();
            let capacities = cell.body.capacities_from_summaries(&family);
            cell.body.add_visual_mechanisms(&mut family);
            let flagellum = family[Kind::Flagellum as usize];
            let cortex = family[Kind::Cortex as usize];
            generation = generation.max(cell.generation);
            target.extend_from_slice(&[
                cell.id as f64,
                cell.genome_id as f64,
                cell.x,
                cell.y,
                cell.vx,
                cell.vy,
                cell.angle,
                cell.radius(),
                cell.energy,
                capacities.energy,
                cell.gut_energy,
                capacities.gut_energy,
                cell.integrity,
                cell.photosynthesis_rate,
                flagellum.count as f64,
                flagellum.capacity,
                cortex.mass,
                cortex.capacity,
                family[Kind::Photosynthetic as usize].capacity,
                family[Kind::Piercer as usize].capacity,
                cell.generation as f64,
            ]);
        }
        target[11] = generation as f64;
        for food in &self.food {
            target.extend_from_slice(&[
                food.x,
                food.y,
                food.energy,
                food.nutrients,
                food.kind as u8 as f64,
            ]);
        }
        debug_assert_eq!(
            target.len(),
            RENDER_HEADER + self.cells.len() * RENDER_CELL_STRIDE + self.food.len() * 5
        );
    }

    /// Outgoing signals and physical bonds are separate observations. A signal
    /// may come from a solitary cell; only the accepted registry supplies links.
    pub(crate) fn append_colony_activity(&self, genomes: &[Genome], target: &mut Vec<f64>, typed: bool) {
        let mut signal_count = 0;
        if genomes.iter().any(|g| g.program.participates_in_contact_signaling()) {
            for (index, cell) in self.cells.iter().enumerate() {
                let program = &genomes[(cell.genome_id - 1) as usize].program;
                if let Some(value) = program.contact_signal(&cell.memory) {
                    target.extend_from_slice(&[index as f64, value]);
                    signal_count += 1;
                }
            }
        }
        target[9] = signal_count as f64;
        if typed {
            for (a, b, kind) in self.presented_bonds(None) {
                target.extend_from_slice(&[a as f64, b as f64, kind as u8 as f64]);
            }
        } else {
            for (a, b) in self.bond_pairs() {
                target.extend_from_slice(&[a as f64, b as f64]);
            }
        }
    }

    /// Small save metadata projection, captured beside checkpoint bytes.
    pub(crate) fn summary_snapshot(&self, target: &mut Vec<f64>) {
        target.clear();
        target.extend_from_slice(&[
            self.time(),
            self.cells.len() as f64,
            self.food.len() as f64,
            self.births as f64,
            self.deaths as f64,
            self.cells
                .iter()
                .map(|cell| cell.generation)
                .max()
                .unwrap_or(0) as f64,
            self.ticks as f64,
        ]);
    }

    /// Aggregates only; no cell records, particles or environmental grid copy.
    pub(crate) fn metrics_snapshot(&self, genomes: &[Genome], target: &mut Vec<f64>) {
        let mut generation = 0;
        let mut trophic = [0_u64; 4];
        let mut flow = [0.0; 3];
        let mut groups: BTreeMap<u32, (usize, u32)> = BTreeMap::new();
        let mut cell_material = -0.0;
        let mut cell_energy = -0.0;
        for cell in &self.cells {
            let cap = cell.body.capacities();
            let photosynthetic = cap.photosynthesis > 0.0 || cap.light_harvest > 0.0;
            let digestive = cap.digestion > 0.0 || cap.substrate_digestion.iter().any(|rate| *rate > 0.0);
            match (photosynthetic, digestive) {
                (true, false) => trophic[0] += 1,
                (false, true) => trophic[1] += 1,
                (true, true) => trophic[2] += 1,
                _ => {}
            }
            trophic[3] += u64::from(cap.piercing > 0.0);
            flow[0] += cell.photosynthesis_rate;
            flow[1] += cell.digestion_rate;
            flow[2] += cell.exudation_rate;
            cell_material += cell.mass();
            cell_energy += cell.energy + cell.gut_energy + cell.chemical_body_investment()
                + (cell.organic_carbon + cell.chemical_body_mass() * STRUCTURAL_CARBON_PER_MASS) * CHEMICAL_ENERGY_PER_CARBON
                + cell.reserve_enrichment + cell.structural_enrichment;
            generation = generation.max(cell.generation);
            let group = groups.entry(cell.genome_id).or_default();
            group.0 += 1;
            group.1 = group.1.max(cell.generation);
        }
        let mut organic_counts = [0_u64; 3];
        let mut food_material = -0.0;
        let mut food_energy = -0.0;
        for food in &self.food {
            organic_counts[food.kind as usize] += 1;
            food_material += food.nutrients;
            food_energy += food.energy;
        }
        let dissolved = self.dissolved_minerals();
        target.clear();
        target.extend_from_slice(&[
            self.ticks as f64,
            self.time(),
            self.cells.len() as f64,
            self.food.len() as f64,
            self.births as f64,
            self.deaths as f64,
            self.eaten as f64,
            generation as f64,
            self.mutation_rate,
            self.mutations as f64,
            self.mutations_skipped as f64,
            genomes.len() as f64,
            MAX_GENOMES as f64,
            registry_bytes(genomes) as f64,
            MAX_GENOME_BYTES as f64,
            self.predations as f64,
            self.attacks as f64,
            self.photosynthesis_total,
            food_energy + cell_energy,
            self.energy_input_total(),
            dissolved + food_material + cell_material,
            self.nutrient_input_total(),
            dissolved,
            food_material,
            cell_material,
            trophic[0] as f64,
            trophic[1] as f64,
            trophic[2] as f64,
            trophic[3] as f64,
            flow[0],
            flow[1],
            flow[2],
            organic_counts[0] as f64,
            organic_counts[1] as f64,
            organic_counts[2] as f64,
            groups.len() as f64,
            self.carbon_total(), self.carbon_input_total(), self.organic_cell_total(), self.organic_food_total(),
            self.carbon_fixed_total, self.carbon_respired_total, self.decay_carbon_respired_total, self.legacy_carbon_imports,
            self.oxygen_cell_total(), self.dissolved_oxygen(), self.dissolved_carbon(),
            self.environment.oxygen_input_total(), self.environment.oxygen_output_total(), self.environment.carbon_output_total(),
            self.internal_inorganic_carbon_total(), self.energy_withdrawn, self.mineral_withdrawn, food_energy,
        ]);
        debug_assert_eq!(target.len(), METRICS_HEADER);
        for (id, (population, max_generation)) in groups {
            target.extend_from_slice(&[id as f64, population as f64, max_generation as f64]);
        }
    }

    /// Shared exact legacy layout for a single selected cell or the full snapshot.
    pub(super) fn snapshot_cell_into(&self, c: &Cell, target: &mut Vec<f64>) {
        let mut families = c.body.family_summaries();
        let cap = c.body.capacities_from_summaries(&families);
        c.body.add_visual_mechanisms(&mut families);
        let space = c.body.space();
        let surface = c.body.surface();
        let used = c.memory.populated_count() as f64;
        let values = [
            c.id as f64,
            c.genome_id as f64,
            c.x,
            c.y,
            c.vx,
            c.vy,
            c.angle,
            c.energy,
            c.age,
            c.generation as f64,
            c.parent_id as f64,
            c.radius(),
            c.body.area,
            c.mass(),
            c.birth_area,
            cap.energy,
            c.basal_metabolism() * metabolic_factor(self.environment.temperature_at(c.x, c.y)),
            used,
            used * MEMORY_REGISTER_COST,
            c.integrity,
            c.photosynthesis_rate,
            c.nutrient_stored,
            cap.minerals,
            c.uptake_rate,
            c.gut_energy,
            cap.gut_energy,
            c.gut_nutrients,
            c.digestion_rate,
            c.growth_rate,
            c.maturity(),
            LIFESPAN,
            c.exudation_rate,
            c.body.upkeep(),
            space.used,
            space.capacity,
            surface.used,
            surface.capacity,
            if c.division_ready() { 1.0 } else { 0.0 },
            cap.optical_range,
        ];
        debug_assert_eq!(values.len(), 39);
        target.extend(values);
        for family in families {
            target.extend(
                [
                    family.count as f64,
                    family.mass,
                    family.capacity,
                    family.damaged,
                    family.building,
                    family.functional_mass,
                ]
                .map(|v| v as f64),
            );
        }
        target.extend(c.gut.iter().map(|pool| pool.energy));
        target.extend(c.gut.iter().map(|pool| pool.nutrients));
        target.extend(c.body.precursor_mass);
        target.push(c.synthesis_rate);
        target.extend([c.organic_carbon, c.internal_oxygen, c.internal_inorganic_carbon, c.respiration_rate, c.carbon_fixation_rate]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::Writer;

    fn state(world: &World) -> Vec<u8> {
        let mut writer = Writer::new();
        world.write_checkpoint(&mut writer);
        assert!(!writer.failed());
        writer.0
    }

    #[test]
    fn projections_match_full_snapshots_and_leave_simulation_unchanged() {
        let source = "thrust=0;turn=0;divide=false;";
        let genomes = vec![
            Genome::new(source.to_owned(), Program::compile(source).unwrap(), 0, 0),
            Genome::new(source.to_owned(), Program::compile(source).unwrap(), 0, 0),
        ];
        let mut world = World::empty(829);
        world.environment.configure(0.4, 0.9, 6.0);
        for (index, fixture) in BodyFixture::ALL.into_iter().enumerate() {
            world
                .try_seed_fixture((index % 2 + 1) as u32, 1, -1.0, -1.0, fixture)
                .unwrap();
            world.cells[index].generation = index as u32;
            world.cells[index].photosynthesis_rate = index as f64 * 0.25;
            world.cells[index].digestion_rate = index as f64 * 0.5;
            world.cells[index].exudation_rate = index as f64 * 0.125;
        }
        world.fixture_exudate(-1.0, -1.0, 9);
        for (index, food) in world.food.iter_mut().enumerate() {
            food.kind = OrganicKind::ALL[index % 3];
        }
        let before = state(&world);
        let mut full = Vec::new();
        let mut compact = Vec::new();
        world.snapshot(&mut full);
        world.render_snapshot(&mut compact);
        assert_eq!(
            compact.len(),
            RENDER_HEADER + 5 * RENDER_CELL_STRIDE + 9 * 5
        );
        let mapping = [
            0,
            1,
            2,
            3,
            4,
            5,
            6,
            11,
            7,
            15,
            24,
            25,
            19,
            20,
            39 + Kind::Flagellum as usize * 6,
            39 + Kind::Flagellum as usize * 6 + 2,
            39 + Kind::Cortex as usize * 6 + 1,
            39 + Kind::Cortex as usize * 6 + 2,
            39 + Kind::Photosynthetic as usize * 6 + 2,
            39 + Kind::Piercer as usize * 6 + 2,
            9,
        ];
        let mut selected = Vec::new();
        for index in 0..world.cells.len() {
            let old = &full[12 + index * CELL_STRIDE..12 + (index + 1) * CELL_STRIDE];
            let new = &compact[RENDER_HEADER + index * RENDER_CELL_STRIDE
                ..RENDER_HEADER + (index + 1) * RENDER_CELL_STRIDE];
            for (output, input) in mapping.into_iter().enumerate() {
                assert_eq!(new[output].to_bits(), old[input].to_bits());
            }
            assert!(world.cell_snapshot(world.cells[index].id, &mut selected));
            assert_eq!(selected.len(), CELL_STRIDE);
            assert!(selected
                .iter()
                .zip(old)
                .all(|(a, b)| a.to_bits() == b.to_bits()));
        }
        assert_eq!(
            &compact[RENDER_HEADER + world.cells.len() * RENDER_CELL_STRIDE..],
            &full[12 + world.cells.len() * CELL_STRIDE..]
        );
        assert!(!world.cell_snapshot(u32::MAX, &mut selected));
        assert!(selected.is_empty());

        let mut metrics = Vec::new();
        world.metrics_snapshot(&genomes, &mut metrics);
        assert_eq!(metrics.len(), METRICS_HEADER + 2 * 3);
        assert_eq!(metrics[2], 5.0);
        assert_eq!(metrics[3], 9.0);
        assert_eq!(metrics[7], 4.0);
        assert_eq!(metrics[18], world.energy_total());
        assert_eq!(metrics[19], world.energy_input_total());
        assert_eq!(metrics[20], world.nutrient_total());
        assert_eq!(metrics[21], world.nutrient_input_total());
        assert_eq!(metrics[22], world.environment.nutrient_total());
        assert_eq!(metrics[23], world.nutrient_food_total());
        assert_eq!(metrics[24], world.nutrient_cell_total());
        for (offset, kind) in [(25, Kind::Photosynthetic), (26, Kind::Digestive)] {
            let other = if kind == Kind::Photosynthetic {
                Kind::Digestive
            } else {
                Kind::Photosynthetic
            };
            let count = world
                .cells
                .iter()
                .filter(|cell| cell.body.capacity(kind) > 0.0 && cell.body.capacity(other) == 0.0)
                .count();
            assert_eq!(metrics[offset], count as f64);
        }
        assert_eq!(
            metrics[27],
            world
                .cells
                .iter()
                .filter(|cell| cell.body.capacity(Kind::Photosynthetic) > 0.0
                    && cell.body.capacity(Kind::Digestive) > 0.0)
                .count() as f64
        );
        assert_eq!(
            metrics[28],
            world
                .cells
                .iter()
                .filter(|cell| cell.body.capacity(Kind::Piercer) > 0.0)
                .count() as f64
        );
        assert_eq!(&metrics[29..35], &[2.5, 5.0, 1.25, 3.0, 3.0, 3.0]);
        assert_eq!(&metrics[METRICS_HEADER..], &[1.0, 3.0, 4.0, 2.0, 2.0, 3.0]);
        let mut summary = Vec::new();
        world.summary_snapshot(&mut summary);
        assert_eq!(
            summary,
            [
                world.time(),
                5.0,
                9.0,
                world.births as f64,
                world.deaths as f64,
                4.0,
                world.ticks as f64,
            ]
        );
        assert_eq!(state(&world), before);
    }

    #[test]
    fn empty_projections_have_bounded_headers_and_no_groups() {
        let world = World::empty(13);
        let mut output = Vec::new();
        world.render_snapshot(&mut output);
        assert_eq!(output.len(), RENDER_HEADER);
        world.metrics_snapshot(&[], &mut output);
        assert_eq!(output.len(), METRICS_HEADER);
        assert_eq!(output[35], 0.0);
        world.summary_snapshot(&mut output);
        assert_eq!(output, [0.0; 7]);
        assert!(!world.cell_snapshot(1, &mut output));
        assert!(output.is_empty());
    }
}
