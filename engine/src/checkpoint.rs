//! Portable, versioned checkpoints. No pointers or linear-memory images are saved.
//! Decode into a temporary engine and publish it only after complete validation.
use crate::{Engine, genome::{Program, MAX_SOURCE_BYTES}, simulation::{Genome, World, MAX_GENOMES, MAX_GENOME_BYTES, registry_bytes}};

pub(crate) const MAX_STATE_BYTES: usize = 128 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"CELLAR01";
const VERSION: u32 = 1;
pub(crate) type Result<T> = std::result::Result<T, String>;

pub(crate) fn ensure(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(format!("Invalid simulation save: {message}.")) }
}

pub(crate) struct Writer(pub Vec<u8>);
impl Writer {
    pub fn u8(&mut self, value: u8) { self.0.push(value); }
    pub fn u16(&mut self, value: u16) { self.0.extend(value.to_le_bytes()); }
    pub fn u32(&mut self, value: u32) { self.0.extend(value.to_le_bytes()); }
    pub fn u64(&mut self, value: u64) { self.0.extend(value.to_le_bytes()); }
    pub fn i32(&mut self, value: i32) { self.0.extend(value.to_le_bytes()); }
    pub fn f64(&mut self, value: f64) { self.u64(value.to_bits()); }
    pub fn count(&mut self, value: usize) { self.u32(value as u32); }
    pub fn option(&mut self, value: Option<usize>) { self.u32(value.map_or(u32::MAX, |n| n as u32)); }
    pub fn text(&mut self, text: &str) { self.count(text.len()); self.0.extend(text.as_bytes()); }
}

pub(crate) struct Reader<'a> { bytes: &'a [u8], offset: usize }
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self { Self { bytes, offset: 0 } }
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self.offset.checked_add(count).ok_or("Invalid simulation save: length overflow.")?;
        ensure(end <= self.bytes.len(), "truncated data")?;
        let result = &self.bytes[self.offset..end]; self.offset = end; Ok(result)
    }
    pub fn u8(&mut self) -> Result<u8> { Ok(self.take(1)?[0]) }
    pub fn u16(&mut self) -> Result<u16> { Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap())) }
    pub fn u32(&mut self) -> Result<u32> { Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap())) }
    pub fn u64(&mut self) -> Result<u64> { Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap())) }
    pub fn i32(&mut self) -> Result<i32> { Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap())) }
    pub fn f64(&mut self) -> Result<f64> { let n = f64::from_bits(self.u64()?); ensure(n.is_finite(), "non-finite number")?; Ok(n) }
    pub fn range(&mut self, low: f64, high: f64) -> Result<f64> { let n = self.f64()?; ensure(n >= low && n <= high, "number outside its allowed range")?; Ok(n) }
    pub fn count(&mut self, max: usize) -> Result<usize> { let n = self.u32()? as usize; ensure(n <= max, "collection exceeds its limit")?; Ok(n) }
    pub fn index(&mut self, length: usize) -> Result<usize> { let n = self.u32()? as usize; ensure(n < length, "invalid expression reference")?; Ok(n) }
    pub fn option(&mut self, length: usize) -> Result<Option<usize>> { let n = self.u32()?; if n == u32::MAX { Ok(None) } else { ensure((n as usize) < length, "invalid expression reference")?; Ok(Some(n as usize)) } }
    pub fn text(&mut self, max: usize) -> Result<String> { let len = self.count(max)?; String::from_utf8(self.take(len)?.to_vec()).map_err(|_| "Invalid simulation save: text is not UTF-8.".into()) }
    pub fn finished(&self) -> bool { self.offset == self.bytes.len() }
}

fn checksum(bytes: &[u8]) -> u32 {
    // FNV-1a detects accidental corruption; validation does not rely on it for trust.
    bytes.iter().fold(0x811c9dc5_u32, |hash, byte| (hash ^ *byte as u32).wrapping_mul(0x01000193))
}

pub(crate) fn save(engine: &Engine) -> Result<Vec<u8>> {
    let mut w = Writer(Vec::new());
    w.0.extend(MAGIC); w.u32(VERSION); w.u32(0); w.u32(0);
    w.count(engine.genomes.len());
    for genome in &engine.genomes {
        w.u32(genome.parent_id); w.u32(genome.mutation_kind); w.count(genome.bytes);
        w.text(&genome.source);
        genome.program.write_checkpoint(&mut w);
    }
    engine.world.write_checkpoint(&mut w);
    ensure(w.0.len() <= MAX_STATE_BYTES, "save exceeds 128 MiB")?;
    let len = w.0.len() as u32;
    w.0[12..16].copy_from_slice(&len.to_le_bytes());
    let hash = checksum(&w.0[20..]);
    w.0[16..20].copy_from_slice(&hash.to_le_bytes());
    Ok(w.0)
}

pub(crate) fn restore(bytes: &[u8]) -> Result<Engine> {
    ensure(bytes.len() >= 20 && bytes.len() <= MAX_STATE_BYTES, "save size is outside its limit")?;
    let mut r = Reader::new(bytes);
    ensure(r.take(8)? == MAGIC, "unknown save format")?;
    ensure(r.u32()? == VERSION, "unsupported save version")?;
    ensure(r.u32()? as usize == bytes.len(), "incorrect save length")?;
    ensure(r.u32()? == checksum(&bytes[20..]), "checksum mismatch")?;
    let count = r.count(MAX_GENOMES)?;
    let mut genomes = Vec::with_capacity(count);
    for index in 0..count {
        let parent = r.u32()?; let kind = r.u32()?;
        ensure(parent as usize <= index && kind <= 3 && ((parent == 0) == (kind == 0)), "invalid genome lineage")?;
        let saved_bytes = r.count(MAX_GENOME_BYTES)?;
        let source = r.text(MAX_SOURCE_BYTES)?;
        // Both the retained exact expression graph and its editable source are validated.
        let compiled = Program::compile(&source).map_err(|error| format!("Invalid saved genome: {error}"))?;
        let program = Program::read_checkpoint(&mut r)?;
        ensure(program.checkpoint_source_matches(&compiled, &source, parent == 0), "genome source does not match its compiled program")?;
        let mut genome = Genome::new(source, program, parent, kind);
        // Preserve the original budget accounting, including allocation capacities.
        ensure(saved_bytes >= genome.bytes && saved_bytes <= MAX_GENOME_BYTES, "invalid genome budget")?;
        genome.bytes = saved_bytes;
        genomes.push(genome);
        ensure(registry_bytes(&genomes) <= MAX_GENOME_BYTES, "genome registry exceeds 64 MiB")?;
    }
    let world = World::read_checkpoint(&mut r, genomes.len())?;
    ensure(r.finished(), "trailing data")?;
    Ok(Engine { genomes, world, ..Engine::new() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::MutationKind;
    use crate::simulation::mutate_registered;

    const PROGRAM: &str = "ri001 = ri001 + 1; rf001 = food.angle; rb001 = food.visible; rs001 = if food.visible then \"food\" else \"search\"; rv001 = vec(flow.x, flow.y); clear rb001 when ri001 == 9; thrust = 0.6; turn = if food.visible then food.angle else sin(age); grow = if size < 2 then 1 else 0; divide = size >= 1.5 && energy > 80 * size; photosynthesize = 1; absorb = 1;";

    fn live_engine() -> Engine {
        let mut e = Engine::new();
        let id = e.compile(PROGRAM); assert_eq!(id, 1);
        e.world.environment.configure(0.5, 0.8, 7.0);
        e.world.environment.set_initial_nutrients(3.0);
        for (kind, x, y, strength, radius) in [(0, 700.0, 400.0, 0.6, 210.0), (1, 900.0, 300.0, -0.3, 110.0), (2, 200.0, 300.0, 0.5, 150.0), (3, 600.0, 250.0, 0.8, 130.0)] {
            assert!(e.world.environment.add_source(kind, x, y, strength, radius) > 0);
        }
        e.world.reset_empty(56311, 600); e.world.set_food_rate(27.5); e.world.set_mutation_rate(1.0);
        e.world.seed_cells(id, 12, -1.0, -1.0);
        for c in &mut e.world.cells { c.size = 2.0; c.energy = 350.0; c.nutrient_stored = 4.0; c.cooldown = 0.0; }
        for _ in 0..37 { e.world.step(&mut e.genomes); }
        e
    }

    #[test]
    fn checkpoint_resumes_all_dynamics_and_future_mutations_exactly() {
        let mut original = live_engine();
        assert!(original.world.births > 0 && original.world.mutations > 0 && original.genomes.len() > 1);
        assert!(original.world.cells.iter().any(|c| c.memory.populated_count() == 5));
        // Include nested AST mutations: their printed source has a different
        // structural graph, so simply recompiling source would lose continuity.
        let mut parent = 1;
        for seed in 1..15 {
            parent = mutate_registered(&mut original.genomes, parent, seed, MutationKind::Expression).genome_id;
        }
        original.world.seed_cells(parent, 2, -1.0, -1.0);
        let bytes = save(&original).unwrap();
        let mut restored = restore(&bytes).unwrap();
        assert_eq!(save(&restored).unwrap(), bytes, "checkpoint is a lossless roundtrip");
        for (a, b) in original.genomes.iter().zip(&restored.genomes) { assert_eq!(a.program.graph_json(), b.program.graph_json()); }
        for _ in 0..240 { original.world.step(&mut original.genomes); restored.world.step(&mut restored.genomes); }
        assert_eq!(save(&original).unwrap(), save(&restored).unwrap(), "future physics, fields, memory, RNG, and mutated genomes stay identical");
        let original_next = original.world.environment.add_source(1, 1.0, 1.0, 0.5, 40.0);
        assert_eq!(restored.world.environment.add_source(1, 1.0, 1.0, 0.5, 40.0), original_next, "source sequence is retained");
    }

    #[test]
    fn checkpoint_preserves_every_register_type_and_populated_zero() {
        let mut e = Engine::new(); let mut program = String::from("thrust = 0; turn = 0; divide = false;");
        for index in 1..=999 {
            program.push_str(&format!("ri{index:03}=0;rf{index:03}=0.0;rb{index:03}=false;rs{index:03}=\"\";rv{index:03}=vec(0,0);"));
        }
        // Vector constructors add three nodes, keeping all 4,995 addresses below the 8,192-node ceiling.
        let id = e.compile(&program); assert_ne!(id, 0, "{}", e.error);
        e.world.seed_cells(id, 1, -1.0, -1.0); e.world.step(&mut e.genomes);
        assert_eq!(e.world.cells[0].memory.populated_count(), 4995);
        let copy = restore(&save(&e).unwrap()).unwrap();
        assert_eq!(copy.world.cells[0].memory, e.world.cells[0].memory);
        assert_eq!(copy.world.cells[0].memory.populated_count(), 4995);
    }

    #[test]
    fn invalid_checkpoints_are_atomic_including_semantically_invalid_data() {
        let mut e = live_engine(); let before = save(&e).unwrap();
        let mut cases = vec![Vec::new(), before[..before.len()-1].to_vec()];
        for index in [0, 8, 12, 16, before.len()-1] { let mut bad = before.clone(); bad[index] ^= 0xff; cases.push(bad); }
        for invalid in 0..6 {
            let mut bad = restore(&before).unwrap();
            match invalid {
                0 => bad.world.cells[0].vx = f64::NAN,
                1 => bad.world.cells[0].genome_id = u32::MAX,
                2 => bad.world.cells[0].body = [1.0; 5],
                3 => bad.genomes[0].source = "thrust = true; turn = 0; divide = false;".into(),
                4 => bad.world.ticks = u64::MAX,
                _ => bad.genomes[0].source = "thrust = 0; turn = 0; divide = false;".into(),
            }
            cases.push(save(&bad).unwrap());
        }
        let mut trailing = before.clone(); trailing.push(0);
        let len = trailing.len() as u32; trailing[12..16].copy_from_slice(&len.to_le_bytes());
        let hash = checksum(&trailing[20..]); trailing[16..20].copy_from_slice(&hash.to_le_bytes()); cases.push(trailing);
        for bad in cases {
            assert!(!e.restore_state(&bad), "invalid save was accepted");
            assert!(!e.error.is_empty());
            assert_eq!(save(&e).unwrap(), before, "rejection must preserve the live engine");
        }
    }

    #[test]
    fn empty_reset_keeps_recipe_and_rates_but_clears_dynamic_state() {
        let mut e = live_engine(); let genome_count = e.genomes.len();
        e.world.reset_empty(72, 13);
        assert_eq!(e.world.cells.len(), 0); assert_eq!(e.world.food.len(), 13); assert_eq!(e.world.ticks, 0);
        assert_eq!(e.world.environment.sources.len(), 4); assert_eq!(e.world.food_rate, 27.5); assert_eq!(e.world.mutation_rate, 1.0);
        assert_eq!(e.genomes.len(), genome_count);
        assert_eq!(e.world.seed_cells(1, 1, -1.0, -1.0), 1);
        assert_eq!(e.world.cells.len(), 1);
        let bytes = save(&e).unwrap(); assert_eq!(save(&restore(&bytes).unwrap()).unwrap(), bytes);
        let empty = Engine::new(); let bytes = save(&empty).unwrap(); assert_eq!(save(&restore(&bytes).unwrap()).unwrap(), bytes);
    }
}
