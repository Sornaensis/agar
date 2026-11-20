//! Census events are recorded only when a cell enters, divides or dies. Mutants
//! keep their own records internally, then roll up to their root at sample time.
use super::*;
use crate::checkpoint::{ensure, Reader, Result, Writer};
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(super) struct Events {
    pub opening: u64,
    pub births: u64,
    pub deaths: u64,
    pub predation: u64,
    pub depleted: u64,
    pub kills: u64,
    pub last_birth: u64, // Zero means no observed birth; first possible birth is tick 1.
}

#[derive(Default)]
pub(super) struct Census {
    pub since: u64,
    by_genome: BTreeMap<u32, Events>,
}

impl Census {
    pub fn events(&mut self, genome: u32) -> &mut Events {
        self.by_genome.entry(genome).or_default()
    }

    pub fn write(&self, w: &mut Writer) {
        w.u64(self.since);
        w.count(self.by_genome.len());
        for (&id, e) in &self.by_genome {
            w.u32(id);
            for n in [e.opening, e.births, e.deaths, e.predation, e.depleted, e.kills, e.last_birth] {
                w.u64(n);
            }
        }
    }

    pub fn read(r: &mut Reader, genome_count: usize, ticks: u64, cells: &[Cell]) -> Result<Self> {
        let mut census = Self { since: r.u64()?, ..Self::default() };
        ensure(census.since <= ticks, "census starts in the future")?;
        let n = r.count(genome_count)?;
        let mut population = vec![0_u64; genome_count + 1];
        for c in cells { population[c.genome_id as usize] += 1; }
        let mut previous = 0;
        for _ in 0..n {
            let id = r.u32()?;
            ensure(id > previous && id as usize <= genome_count, "invalid census genome")?;
            previous = id;
            let e = Events { opening: r.u64()?, births: r.u64()?, deaths: r.u64()?,
                predation: r.u64()?, depleted: r.u64()?, kills: r.u64()?, last_birth: r.u64()? };
            ensure([e.opening,e.births,e.deaths,e.predation,e.depleted,e.kills].iter().all(|&v| v <= u64::from(u32::MAX)),
                "census exceeds cell identifier space")?;
            ensure(e.predation + e.depleted <= e.deaths
                && e.opening + e.births == population[id as usize] + e.deaths
                && (e.births == 0 && e.last_birth == 0 || e.births > 0 && e.last_birth > census.since && e.last_birth <= ticks),
                "census does not match population or event times")?;
            population[id as usize] = 0;
            census.by_genome.insert(id,e);
        }
        ensure(population.iter().all(|&n| n == 0), "missing living census")?;
        ensure(census.by_genome.values().map(|e| e.kills).sum::<u64>()
            == census.by_genome.values().map(|e| e.predation).sum(), "predator kills do not match victims")?;
        Ok(census)
    }

    pub fn from_opening(ticks: u64, cells: &[Cell]) -> Self {
        let mut c = Self { since: ticks, ..Self::default() };
        for cell in cells { c.events(cell.genome_id).opening += 1; }
        c
    }
}

#[derive(Default)]
struct Lineage {
    events: Events,
    alive: u32,
    max_generation: u32,
    near_full: u32,
    maturity: f64,
    digestion: f64,
    fixation: f64,
    no_free_minerals: u32,
    free_minerals: f64,
    reserve_carbon: f64,
    gut_energy: f64,
    low_atp_with_fuel: u32,
}

impl World {
    pub fn energy_loss_counts(&self) -> Option<[i64; 7]> {
        self.uses_count_chemistry().then(|| self.energy_loss.counts())
    }

    pub fn ecology_snapshot(&self, genomes: &[Genome], target: &mut Vec<f64>) {
        let mut roots = Vec::with_capacity(genomes.len() + 1);
        roots.push(0);
        for (idx, genome) in genomes.iter().enumerate() {
            roots.push(if genome.parent_id == 0 { idx as u32 + 1 } else { roots[genome.parent_id as usize] });
        }
        let mut rows = BTreeMap::<u32,Lineage>::new();
        for (&id, e) in &self.census.by_genome {
            let total = &mut rows.entry(roots[id as usize]).or_default().events;
            total.opening += e.opening;
            total.births += e.births;
            total.deaths += e.deaths;
            total.predation += e.predation;
            total.depleted += e.depleted;
            total.kills += e.kills;
            total.last_birth = total.last_birth.max(e.last_birth);
        }
        for cell in &self.cells {
            let row = rows.entry(roots[cell.genome_id as usize]).or_default();
            row.alive += 1;
            row.max_generation = row.max_generation.max(cell.generation);
            let capacity = cell.energy_capacity();
            row.near_full += u32::from(capacity > 0.0 && cell.energy >= 0.95 * capacity);
            row.maturity += cell.maturity();
            row.digestion += cell.digestion_rate;
            row.fixation += cell.carbon_fixation_rate;
            let empty = cell.counted.as_ref().map_or(cell.nutrient_stored <= 1e-12, |q| q.mineral == 0);
            row.no_free_minerals += u32::from(empty);
            row.free_minerals += cell.nutrient_stored;
            row.reserve_carbon += cell.organic_carbon;
            row.gut_energy += cell.gut_energy;
            // Count the chemical potential of reserve carbon alone. Gut contents,
            // tissue enrichment and ATP obtainable after conversion are separate.
            let low_atp_with_fuel = if let Some(stores) = &cell.counted {
                let capacity = u64::from(crate::counts::floor(capacity));
                capacity > 0 && u64::from(stores.atp) * 10 <= capacity
                    && u64::from(stores.carbon) * CHEMICAL_ENERGY_PER_CARBON as u64 * 4 >= capacity
            } else {
                capacity > 0.0 && cell.energy <= capacity / 10.0
                    && cell.organic_carbon * CHEMICAL_ENERGY_PER_CARBON >= capacity / 4.0
            };
            row.low_atp_with_fuel += u32::from(low_atp_with_fuel);

        }
        target.clear();
        target.extend([self.census.since as f64 * DT, rows.len() as f64]);
        for (id,row) in rows {
            let e = row.events;
            target.extend([id as f64, row.alive as f64, e.opening as f64, e.births as f64, e.deaths as f64,
                e.predation as f64, e.depleted as f64, (e.deaths - e.predation - e.depleted) as f64, e.kills as f64,
                if e.last_birth == 0 { -1.0 } else { e.last_birth as f64 * DT }, row.max_generation as f64,
                row.near_full as f64, row.maturity / f64::from(row.alive.max(1)), row.digestion, row.fixation,
                row.no_free_minerals as f64, row.free_minerals, row.reserve_carbon, row.gut_energy,
                row.low_atp_with_fuel as f64]);
        }
    }
}

/// Whole energy counts leaving ATP or chemical potential, measured independently
/// at the reactions themselves. Fabrication can recognize a previously prepaid
/// sub-count of bound investment on a later tick, so its net change is signed.
#[derive(Default)]
pub(super) struct EnergyLoss {
    pub opening: u64,
    pub maintenance: u64,
    pub respiration: u64,
    pub decay: u64,
    pub fabrication: i64,
    pub work: u64,
    pub overflow: u64,
}
impl EnergyLoss {
    pub fn counts(&self) -> [i64;7] {
        [self.opening as i64, self.maintenance as i64, self.respiration as i64, self.decay as i64,
            self.fabrication, self.work as i64, self.overflow as i64]
    }
    pub fn write(&self, w: &mut Writer) {
        for n in self.counts() { w.u64(n as u64); }
    }
    pub fn read(r: &mut Reader) -> Result<Self> {
        let s = Self { opening:r.u64()?, maintenance:r.u64()?, respiration:r.u64()?, decay:r.u64()?,
            fabrication:r.u64()? as i64, work:r.u64()?, overflow:r.u64()? };
        ensure([s.opening,s.maintenance,s.respiration,s.decay,s.work,s.overflow,s.fabrication.unsigned_abs()]
            .iter().all(|n| *n <= i64::MAX as u64), "invalid energy loss counters")?;
        Ok(s)
    }
}
