//! Portable, versioned checkpoints. No pointers or linear-memory images are saved.
//! Decode into a temporary engine and publish it only after complete validation.
use crate::{
    genome::{Program, MAX_SOURCE_BYTES},
    simulation::{registry_bytes, Genome, World, MAX_GENOMES, MAX_GENOME_BYTES},
    Engine,
};

pub(crate) const MAX_STATE_BYTES: usize = 128 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"CELLAR01";
const VERSION: u32 = 6;
pub(crate) type Result<T> = std::result::Result<T, String>;

pub(crate) fn ensure(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(format!("Invalid simulation save: {message}."))
    }
}

pub(crate) struct Writer(pub Vec<u8>, Option<&'static str>);
impl Writer {
    pub fn new() -> Self {
        Self(Vec::new(), None)
    }
    pub fn failed(&self) -> bool {
        self.1.is_some()
    }
    fn bytes(&mut self, bytes: &[u8]) {
        if self.failed() {
            return;
        }
        let Some(length) = self
            .0
            .len()
            .checked_add(bytes.len())
            .filter(|&length| length <= MAX_STATE_BYTES)
        else {
            self.1 = Some("save exceeds 128 MiB");
            return;
        };
        if length > self.0.capacity() {
            let capacity = length
                .max(self.0.capacity().saturating_mul(2))
                .max(64)
                .min(MAX_STATE_BYTES);
            if self.0.try_reserve_exact(capacity - self.0.len()).is_err() {
                self.1 = Some("not enough memory to create the simulation save");
                return;
            }
        }
        self.0.extend_from_slice(bytes);
    }
    fn finish(self) -> Result<Vec<u8>> {
        match self.1 {
            Some(message) => Err(format!("Cannot save simulation: {message}.")),
            None => Ok(self.0),
        }
    }
    pub fn u8(&mut self, value: u8) {
        self.bytes(&[value]);
    }
    pub fn u16(&mut self, value: u16) {
        self.bytes(&value.to_le_bytes());
    }
    pub fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }
    pub fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    pub fn i32(&mut self, value: i32) {
        self.bytes(&value.to_le_bytes());
    }
    pub fn f64(&mut self, value: f64) {
        self.u64(value.to_bits());
    }
    pub fn count(&mut self, value: usize) {
        self.u32(value as u32);
    }
    pub fn option(&mut self, value: Option<usize>) {
        self.u32(value.map_or(u32::MAX, |n| n as u32));
    }
    pub fn text(&mut self, text: &str) {
        self.count(text.len());
        self.bytes(text.as_bytes());
    }
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or("Invalid simulation save: length overflow.")?;
        ensure(end <= self.bytes.len(), "truncated data")?;
        let result = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(result)
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn f64(&mut self) -> Result<f64> {
        let n = f64::from_bits(self.u64()?);
        ensure(n.is_finite(), "non-finite number")?;
        Ok(n)
    }
    pub fn range(&mut self, low: f64, high: f64) -> Result<f64> {
        let n = self.f64()?;
        ensure(n >= low && n <= high, "number outside its allowed range")?;
        Ok(n)
    }
    pub fn count(&mut self, max: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        ensure(n <= max, "collection exceeds its limit")?;
        Ok(n)
    }
    pub fn index(&mut self, length: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        ensure(n < length, "invalid expression reference")?;
        Ok(n)
    }
    pub fn option(&mut self, length: usize) -> Result<Option<usize>> {
        let n = self.u32()?;
        if n == u32::MAX {
            Ok(None)
        } else {
            ensure((n as usize) < length, "invalid expression reference")?;
            Ok(Some(n as usize))
        }
    }
    pub fn text(&mut self, max: usize) -> Result<String> {
        let len = self.count(max)?;
        String::from_utf8(self.take(len)?.to_vec())
            .map_err(|_| "Invalid simulation save: text is not UTF-8.".into())
    }
    pub fn finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

fn checksum(bytes: &[u8]) -> u32 {
    // FNV-1a detects accidental corruption; validation does not rely on it for trust.
    bytes.iter().fold(0x811c9dc5_u32, |hash, byte| {
        (hash ^ *byte as u32).wrapping_mul(0x01000193)
    })
}

pub(crate) fn save(engine: &Engine) -> Result<Vec<u8>> {
    let mut w = Writer::new();
    w.bytes(MAGIC);
    w.u32(VERSION);
    w.u32(0);
    w.u32(0);
    w.count(engine.genomes.len());
    for genome in &engine.genomes {
        if w.failed() {
            break;
        }
        w.u32(genome.parent_id);
        w.u32(genome.mutation_kind);
        // Persist the authored allocation budget. Runtime execution plans are
        // rebuilt on load and charged separately to the live registry limit.
        let authored_bytes = genome
            .bytes
            .checked_sub(genome.program.derived_execution_bytes())
            .ok_or("Invalid simulation state: genome allocation budget is incomplete.")?;
        w.count(authored_bytes);
        w.text(&genome.source);
        genome.program.write_checkpoint(&mut w);
    }
    if !w.failed() {
        engine.world.write_checkpoint(&mut w);
    }
    let mut bytes = w.finish()?;
    let len = bytes.len() as u32;
    bytes[12..16].copy_from_slice(&len.to_le_bytes());
    let hash = checksum(&bytes[20..]);
    bytes[16..20].copy_from_slice(&hash.to_le_bytes());
    Ok(bytes)
}

pub(crate) fn restore(bytes: &[u8]) -> Result<Engine> {
    ensure(
        bytes.len() >= 20 && bytes.len() <= MAX_STATE_BYTES,
        "save size is outside its limit",
    )?;
    let mut r = Reader::new(bytes);
    ensure(r.take(8)? == MAGIC, "unknown save format")?;
    let version = r.u32()?;
    if !matches!(version, 3 | 4 | 5 | VERSION) {
        return Err(format!("Unsupported simulation version {version}. Agar supports physical-organelle saves 3 and 4, cellular saves 5, and mechanism saves 6. The saved simulation is preserved."));
    }
    ensure(r.u32()? as usize == bytes.len(), "incorrect save length")?;
    ensure(r.u32()? == checksum(&bytes[20..]), "checksum mismatch")?;
    let count = r.count(MAX_GENOMES)?;
    let mut genomes = Vec::with_capacity(count);
    for index in 0..count {
        let parent = r.u32()?;
        let kind = r.u32()?;
        ensure(
            parent as usize <= index
                && kind <= if version >= 5 { 5 } else { 3 }
                && ((parent == 0) == (kind == 0)),
            "invalid genome lineage",
        )?;
        let saved_bytes = r.count(MAX_GENOME_BYTES)?;
        let source = r.text(MAX_SOURCE_BYTES)?;
        // Both the retained exact expression graph and its editable source are validated.
        let compiled =
            Program::compile(&source).map_err(|error| format!("Invalid saved genome: {error}"))?;
        let program = Program::read_checkpoint_version(&mut r, version)?;
        ensure(
            program.checkpoint_source_matches(&compiled, &source, parent == 0),
            "genome source does not match its compiled program",
        )?;
        let mut genome = Genome::new(source, program, parent, kind);
        if version == 3 {
            // Added digestion roots enlarge Program storage. Older source and
            // expression graphs stay exact; charge current decoded allocations.
            ensure(
                genome.bytes <= MAX_GENOME_BYTES,
                "migrated genome exceeds its allocation budget",
            )?;
        } else {
            // Preserve retained authored allocation capacities, including saves
            // produced before derived execution plans existed. Validate that
            // budget before adding the current runtime-only plan storage.
            let derived_bytes = genome.program.derived_execution_bytes();
            ensure(
                saved_bytes >= genome.bytes - derived_bytes,
                "invalid genome budget",
            )?;
            genome.bytes = saved_bytes
                .checked_add(derived_bytes)
                .ok_or("Invalid simulation save: genome budget overflow.")?;
        }
        genomes.push(genome);
        ensure(
            registry_bytes(&genomes) <= MAX_GENOME_BYTES,
            "genome registry exceeds 64 MiB",
        )?;
    }
    let world = World::read_checkpoint(&mut r, genomes.len(), version)?;
    ensure(r.finished(), "trailing data")?;
    Ok(Engine {
        genomes,
        world,
        ..Engine::new()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::MutationKind;
    use crate::simulation::mutate_registered;

    #[test]
    fn cellular_checkpoint_retains_products_recipes_and_gene_mutation_lineage() {
        let mut original = Engine::new();
        let source = include_str!("../../examples/solar.cell");
        let id = original.compile(source);
        assert_ne!(id, 0, "{}", original.error);
        original.world.environment.configure(0.5, 0.85, 0.0);
        original.world.environment.set_initial_nutrients(2.0);
        original.world.reset_empty(49, 0);
        original.world.set_food_rate(0.0);
        original.world.set_mutation_rate(0.0);
        original.world.seed_cells(id, 2, 600.0, 380.0);
        for _ in 0..75 { original.world.step(&mut original.genomes); }
        let duplicate = original.mutate(id, 17, 5);
        let silent = original.mutate(id, 19, 4);
        assert_ne!(duplicate, id, "{}", original.error);
        assert_ne!(silent, id, "{}", original.error);
        assert_eq!(original.genomes[(duplicate - 1) as usize].mutation_kind, 5);
        assert_eq!(original.genomes[(silent - 1) as usize].mutation_kind, 4);
        let bytes = save(&original).unwrap();
        let mut restored = restore(&bytes).unwrap();
        assert_eq!(save(&restored).unwrap(), bytes);
        for _ in 0..600 {
            original.world.step(&mut original.genomes);
            restored.world.step(&mut restored.genomes);
        }
        assert_eq!(save(&restored).unwrap(), save(&original).unwrap());
        assert!(original.world.photosynthesis_total > 0.0);
        assert_eq!(original.world.body_json(1, &original.genomes),
            restored.world.body_json(1, &restored.genomes));
    }

    const PROGRAM: &str = "ri001 = ri001 + 1; rf001 = light.bearing; rb001 = light.valid; rs001 = if light.valid then \"light\" else \"search\"; rv001 = flow.vector; clear rb001 when ri001 == 9; thrust = 0.6; turn = if light.valid then light.bearing else sin(age); grow = if body.area < 2 then 1 else 0; divide = body.division_ready; photosynthesize = 1; absorb = 1;";

    fn live_engine() -> Engine {
        let mut e = Engine::new();
        let id = e.compile(PROGRAM);
        assert_eq!(id, 1);
        e.world.environment.configure(0.5, 0.8, 7.0);
        e.world.environment.set_initial_nutrients(3.0);
        for (kind, x, y, strength, radius) in [
            (0, 700.0, 400.0, 0.6, 210.0),
            (1, 900.0, 300.0, -0.3, 110.0),
            (2, 200.0, 300.0, 0.5, 150.0),
            (3, 600.0, 250.0, 0.8, 130.0),
        ] {
            assert!(e.world.environment.add_source(kind, x, y, strength, radius) > 0);
        }
        e.world.reset_empty(56311, 600);
        e.world.set_food_rate(27.5);
        e.world.set_mutation_rate(1.0);
        e.world
            .try_seed_fixture(id, 12, -1.0, -1.0, crate::body::BodyFixture::Producer)
            .unwrap();
        for c in &mut e.world.cells {
            for _ in 0..1000 {
                c.body.work(
                    1.0,
                    &crate::body::BodyControls::default(),
                    0.0,
                    &mut c.energy,
                    &mut c.nutrient_stored,
                    crate::simulation::DT,
                    2.0,
                );
                if c.body.area >= 2.0 - 1e-10 {
                    break;
                }
            }
            c.cooldown = 0.0;
        }
        for _ in 0..37 {
            e.world.step(&mut e.genomes);
        }
        e
    }

    #[test]
    fn checkpoint_resumes_all_dynamics_and_future_mutations_exactly() {
        let mut original = live_engine();
        assert!(
            original.world.births > 0 && original.world.mutations > 0 && original.genomes.len() > 1
        );
        assert!(original
            .world
            .cells
            .iter()
            .any(|c| c.memory.populated_count() == 5));
        // Include nested AST mutations: their printed source has a different
        // structural graph, so simply recompiling source would lose continuity.
        let mut parent = 1;
        for seed in 1..15 {
            parent = mutate_registered(
                &mut original.genomes,
                parent,
                seed,
                MutationKind::Expression,
            )
            .genome_id;
        }
        original.world.seed_cells(parent, 2, -1.0, -1.0);
        let bytes = save(&original).unwrap();
        let mut restored = restore(&bytes).unwrap();
        assert_eq!(
            save(&restored).unwrap(),
            bytes,
            "checkpoint is a lossless roundtrip"
        );
        for (a, b) in original.genomes.iter().zip(&restored.genomes) {
            assert_eq!(a.program.graph_json(), b.program.graph_json());
        }
        for _ in 0..240 {
            original.world.step(&mut original.genomes);
            restored.world.step(&mut restored.genomes);
        }
        assert_eq!(
            save(&original).unwrap(),
            save(&restored).unwrap(),
            "future physics, fields, memory, RNG, and mutated genomes stay identical"
        );
        let original_next = original
            .world
            .environment
            .add_source(1, 1.0, 1.0, 0.5, 40.0);
        assert_eq!(
            restored
                .world
                .environment
                .add_source(1, 1.0, 1.0, 0.5, 40.0),
            original_next,
            "source sequence is retained"
        );
    }

    #[test]
    fn checkpoint_preserves_every_register_type_and_populated_zero() {
        let mut e = Engine::new();
        let mut program = String::from("thrust = 0; turn = 0; divide = false;");
        for index in 1..=999 {
            program.push_str(&format!("ri{index:03}=0;rf{index:03}=0.0;rb{index:03}=false;rs{index:03}=\"\";rv{index:03}=vec(0,0);"));
        }
        // Vector constructors add three nodes, keeping all 4,995 addresses below the 8,192-node ceiling.
        let id = e.compile(&program);
        assert_ne!(id, 0, "{}", e.error);
        e.world.seed_cells(id, 1, -1.0, -1.0);
        e.world.step(&mut e.genomes);
        assert_eq!(e.world.cells[0].memory.populated_count(), 4995);
        let copy = restore(&save(&e).unwrap()).unwrap();
        assert_eq!(copy.world.cells[0].memory, e.world.cells[0].memory);
        assert_eq!(copy.world.cells[0].memory.populated_count(), 4995);
    }

    #[test]
    fn pre_plan_checkpoints_keep_authored_bytes_and_rebuild_runtime_plans() {
        // Produced by the unchanged phase-one engine, not reconstructed by the
        // current serializer. Its stored budget contains no execution plan.
        let bytes = include_bytes!("../fixtures/execution-plan-v4.bin");
        let mut restored = restore(bytes).unwrap();
        let migrated_bytes = save(&restored).unwrap();
        assert_eq!(u32::from_le_bytes(migrated_bytes[8..12].try_into().unwrap()), 6);
        assert_eq!(save(&restore(&migrated_bytes).unwrap()).unwrap(), migrated_bytes);
        let saved_budget = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
        let genome = &restored.genomes[0];
        assert!(genome.program.derived_execution_bytes() > 0);
        assert_eq!(genome.bytes, saved_budget + genome.program.derived_execution_bytes());
        assert_eq!(restored.world.ticks, 17);
        for cell in &restored.world.cells {
            assert_eq!(cell.memory.value("ri001"), Some(&crate::genome::Value::Integer(17)));
        }
        restored.world.step(&mut restored.genomes);
        for cell in &restored.world.cells {
            assert_eq!(cell.memory.value("ri001"), Some(&crate::genome::Value::Integer(18)));
        }

        let version3 = include_bytes!("../fixtures/organelle-v3.bin");
        let migrated = restore(version3).unwrap();
        assert!(migrated.genomes.iter().all(|genome| {
            genome.program.derived_execution_bytes() > 0
                && genome.bytes > genome.program.derived_execution_bytes()
        }));
        let updated = save(&migrated).unwrap();
        assert_eq!(save(&restore(&updated).unwrap()).unwrap(), updated);
    }

    #[test]
    fn checkpoint_budgets_include_runtime_plans_and_reject_forged_authoring_costs() {
        let mut engine = Engine::new();
        assert_eq!(engine.compile(PROGRAM), 1);
        // A decoded Vec/String may have less capacity; preserve the retained
        // authored budget across saves while still charging rebuilt plan bytes.
        engine.genomes[0].source.reserve_exact(4096);
        let genome = &mut engine.genomes[0];
        genome.bytes = std::mem::size_of::<Genome>()
            + genome.source.capacity()
            + genome.program.estimated_bytes();
        let bytes = save(&engine).unwrap();
        let restored = restore(&bytes).unwrap();
        assert_eq!(restored.genomes[0].bytes, engine.genomes[0].bytes);
        assert_eq!(save(&restored).unwrap(), bytes);

        let genome = &engine.genomes[0];
        let mut graph = Writer::new();
        genome.program.write_checkpoint(&mut graph);
        let decoded = Program::read_checkpoint(&mut Reader::new(&graph.0)).unwrap();
        let decoded = Genome::new(genome.source.clone(), decoded, 0, 0);
        let minimum_authored = decoded.bytes - decoded.program.derived_execution_bytes();
        for (budget, expected_error) in [
            (minimum_authored - 1, "invalid genome budget"),
            (MAX_GENOME_BYTES, "registry exceeds 64 MiB"),
        ] {
            let mut forged = bytes.clone();
            forged[32..36].copy_from_slice(&(budget as u32).to_le_bytes());
            let hash = checksum(&forged[20..]);
            forged[16..20].copy_from_slice(&hash.to_le_bytes());
            assert!(!engine.restore_state(&forged));
            assert!(engine.error.contains(expected_error), "{}", engine.error);
            assert_eq!(save(&engine).unwrap(), bytes, "budget rejection must be atomic");
        }
    }

    #[test]
    fn old_body_formats_are_explicitly_unsupported_without_overwriting_data() {
        let bytes = include_bytes!("../fixtures/trophic-v1.bin");
        let mut live = live_engine();
        let before = save(&live).unwrap();
        assert!(!live.restore_state(bytes));
        assert!(live.error.contains("physical-organelle saves 3 and 4"));
        assert_eq!(save(&live).unwrap(), before);
        let mut version2 = before.clone();
        version2[8..12].copy_from_slice(&2u32.to_le_bytes());
        assert!(!live.restore_state(&version2));
        assert!(live.error.contains("preserved"));
        assert_eq!(save(&live).unwrap(), before);
    }

    #[test]
    fn invalid_checkpoints_are_atomic_including_semantically_invalid_data() {
        let mut e = live_engine();
        let before = save(&e).unwrap();
        let mut cases = vec![Vec::new(), before[..before.len() - 1].to_vec()];
        for index in [0, 8, 12, 16, before.len() - 1] {
            let mut bad = before.clone();
            bad[index] ^= 0xff;
            cases.push(bad);
        }
        for invalid in 0..6 {
            let mut bad = restore(&before).unwrap();
            match invalid {
                0 => bad.world.cells[0].vx = f64::NAN,
                1 => bad.world.cells[0].genome_id = u32::MAX,
                2 => bad.world.cells[0].body.organs[0].mass = f64::INFINITY,
                3 => bad.genomes[0].source = "thrust = true; turn = 0; divide = false;".into(),
                4 => bad.world.ticks = u64::MAX,
                _ => bad.genomes[0].source = "thrust = 0; turn = 0; divide = false;".into(),
            }
            cases.push(save(&bad).unwrap());
        }
        let mut trailing = before.clone();
        trailing.push(0);
        let len = trailing.len() as u32;
        trailing[12..16].copy_from_slice(&len.to_le_bytes());
        let hash = checksum(&trailing[20..]);
        trailing[16..20].copy_from_slice(&hash.to_le_bytes());
        cases.push(trailing);
        for bad in cases {
            assert!(!e.restore_state(&bad), "invalid save was accepted");
            assert!(!e.error.is_empty());
            assert_eq!(
                save(&e).unwrap(),
                before,
                "rejection must preserve the live engine"
            );
        }
    }

    #[test]
    fn oversized_save_writer_rejects_before_growing_its_buffer() {
        let oversized = vec![0u8; MAX_STATE_BYTES + 1];
        let mut writer = Writer::new();
        writer.bytes(&oversized);
        assert!(writer.failed());
        assert!(writer.0.is_empty());
        assert_eq!(writer.0.capacity(), 0);
        writer.u32(42);
        assert!(
            writer.0.is_empty(),
            "writes after rejection cannot produce a partial successful save"
        );
        assert!(writer.finish().unwrap_err().contains("128 MiB"));
    }

    #[test]
    fn population_restore_uses_available_bytes_instead_of_a_fixed_cell_ceiling() {
        let mut engine = Engine::new();
        let id = engine
            .compile("rf001=chemical.organic;rf002=chemical.mineral;thrust=0;turn=0;divide=false;");
        assert_eq!(engine.world.seed_cells(id, 2049, -1.0, -1.0), 2049);
        let bytes = save(&engine).unwrap();
        let restored = restore(&bytes).unwrap();
        assert_eq!(restored.world.cells.len(), 2049);
        assert_eq!(save(&restored).unwrap(), bytes);

        // An empty world ends with cell_count and food_count. Recompute the
        // checksum so rejection tests the count boundary, not corruption checks.
        let mut malformed = save(&Engine::new()).unwrap();
        let offset = malformed.len() - 8;
        malformed[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let hash = checksum(&malformed[20..]);
        malformed[16..20].copy_from_slice(&hash.to_le_bytes());
        assert!(!engine.restore_state(&malformed));
        assert!(engine.error.contains("checkpoint data"));
        assert_eq!(
            save(&engine).unwrap(),
            bytes,
            "rejected counts cannot replace the live population"
        );
    }

    #[test]
    fn empty_reset_keeps_recipe_and_rates_but_clears_dynamic_state() {
        let mut e = live_engine();
        let genome_count = e.genomes.len();
        e.world.reset_empty(72, 13);
        assert_eq!(e.world.cells.len(), 0);
        assert_eq!(e.world.food.len(), 13);
        assert_eq!(e.world.ticks, 0);
        assert_eq!(e.world.environment.sources.len(), 4);
        assert_eq!(e.world.food_rate, 27.5);
        assert_eq!(e.world.mutation_rate, 1.0);
        assert_eq!(e.genomes.len(), genome_count);
        assert_eq!(e.world.seed_cells(1, 1, -1.0, -1.0), 1);
        assert_eq!(e.world.cells.len(), 1);
        let bytes = save(&e).unwrap();
        assert_eq!(save(&restore(&bytes).unwrap()).unwrap(), bytes);
        let empty = Engine::new();
        let bytes = save(&empty).unwrap();
        assert_eq!(save(&restore(&bytes).unwrap()).unwrap(), bytes);
    }
}
