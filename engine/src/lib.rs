//! Dependency-free Rust simulation exported through a small, versioned WASM ABI.
//! All mutable engine state belongs to this module. Genomes compute synchronous
//! register transitions and commands from an immutable snapshot.
pub mod body;
mod checkpoint;
pub mod environment;
pub mod genome;
pub mod simulation;

use genome::{GenomeAnalysis, MutationKind, Program, MAX_SOURCE_BYTES};
use simulation::{mutate_registered, registry_bytes, Genome, World, MAX_GENOMES, MAX_GENOME_BYTES};
use std::cell::RefCell;

struct Engine {
    genomes: Vec<Genome>,
    world: World,
    error: String,
    snapshot: Vec<f64>,
    environment_snapshot: Vec<f32>,
    source_buffer: Vec<u8>,
    memory_buffer: Vec<u8>,
    graph_buffer: Vec<u8>,
    analysis_buffer: Vec<u8>,
    evaluation_buffer: Vec<u8>,
    state_buffer: Vec<u8>,
    body_buffer: Vec<u8>,
    schema_buffer: Vec<u8>,
}

impl Engine {
    fn new() -> Self {
        Self {
            genomes: Vec::new(),
            world: World::empty(1),
            error: String::new(),
            snapshot: Vec::new(),
            environment_snapshot: Vec::new(),
            source_buffer: Vec::new(),
            memory_buffer: Vec::new(),
            graph_buffer: Vec::new(),
            analysis_buffer: Vec::new(),
            evaluation_buffer: Vec::new(),
            state_buffer: Vec::new(),
            body_buffer: Vec::new(),
            schema_buffer: Vec::new(),
        }
    }
    fn compile(&mut self, source: &str) -> u32 {
        self.error.clear();
        if let Some(index) = self
            .genomes
            .iter()
            .position(|g| g.parent_id == 0 && g.source == source)
        {
            return index as u32 + 1;
        }
        let program = match Program::compile(source) {
            Ok(program) => program,
            Err(error) => {
                self.error = error;
                return 0;
            }
        };
        if self.genomes.len() >= MAX_GENOMES {
            self.error = format!("This session has reached {MAX_GENOMES} genomes. Existing genomes remain available; reload to start a fresh session.");
            return 0;
        }
        let genome = Genome::new(source.to_owned(), program, 0, 0);
        if registry_bytes(&self.genomes).saturating_add(genome.bytes) > MAX_GENOME_BYTES {
            self.error = "This session has reached its 64 MiB genome storage limit. Existing genomes remain available; reload to start a fresh session.".into();
            return 0;
        }
        self.genomes.push(genome);
        self.genomes.len() as u32
    }
    fn mutate(&mut self, parent_id: u32, seed: u32, kind: u32) -> u32 {
        if !self.valid_genome(parent_id) {
            return 0;
        }
        let kind = match kind {
            0 => MutationKind::Any,
            1 => MutationKind::Constant,
            2 => MutationKind::Expression,
            3 => MutationKind::Reference,
            _ => {
                self.error = "Unknown mutation kind. Use 0 (any), 1 (constant), 2 (expression), or 3 (reference).".into();
                return 0;
            }
        };
        let result = mutate_registered(&mut self.genomes, parent_id, seed as u64, kind);
        if let Some(reason) = result.skipped {
            self.error = reason.into();
        }
        result.genome_id
    }
    fn valid_genome(&mut self, id: u32) -> bool {
        if id == 0 || id as usize > self.genomes.len() {
            self.error = "Unknown genome: compile a valid program first.".into();
            false
        } else {
            self.error.clear();
            true
        }
    }
    fn restore_state(&mut self, bytes: &[u8]) -> bool {
        match checkpoint::restore(bytes) {
            Ok(restored) => {
                *self = restored;
                true
            }
            Err(error) => {
                self.error = error;
                false
            }
        }
    }
}

thread_local! { static ENGINE: RefCell<Engine> = RefCell::new(Engine::new()); }

#[no_mangle]
pub extern "C" fn abi_version() -> u32 {
    9
}

/// Allocate a writable, zero-initialized byte buffer. The caller must deallocate
/// it exactly once with the same pointer and length, after compile_genome.
#[no_mangle]
pub extern "C" fn alloc(len: u32) -> *mut u8 {
    if len as usize > MAX_SOURCE_BYTES {
        return std::ptr::null_mut();
    }
    let data = vec![0u8; len as usize].into_boxed_slice();
    Box::into_raw(data) as *mut u8
}

/// Separate bounded allocation for checkpoints; deallocate using dealloc.
#[no_mangle]
pub extern "C" fn alloc_state(len: u32) -> *mut u8 {
    if len as usize > checkpoint::MAX_STATE_BYTES || len < 20 {
        return std::ptr::null_mut();
    }
    let mut bytes = Vec::new();
    if bytes.try_reserve_exact(len as usize).is_err() {
        return std::ptr::null_mut();
    }
    bytes.resize(len as usize, 0u8);
    Box::into_raw(bytes.into_boxed_slice()) as *mut u8
}

/// Owned checkpoint buffer. Copy before any other engine operation.
#[no_mangle]
pub extern "C" fn save_state_ptr() -> *const u8 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.error.clear();
        match checkpoint::save(&e) {
            Ok(bytes) => {
                e.state_buffer = bytes;
                e.state_buffer.as_ptr()
            }
            Err(error) => {
                e.error = error;
                e.state_buffer.clear();
                std::ptr::null()
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn save_state_len() -> u32 {
    ENGINE.with(|e| e.borrow().state_buffer.len() as u32)
}

/// # Safety
/// ptr..ptr+len must be a readable allocation returned by alloc_state.
/// Returns 1 on complete success, 0 on rejection; rejection preserves live state.
#[no_mangle]
pub unsafe extern "C" fn restore_state(ptr: *const u8, len: u32) -> u32 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        if ptr.is_null() || len < 20 || len as usize > checkpoint::MAX_STATE_BYTES {
            e.error = "Simulation save must fit in 128 MiB.".into();
            return 0;
        }
        u32::from(e.restore_state(std::slice::from_raw_parts(ptr, len as usize)))
    })
}

/// # Safety
/// `ptr` and `len` must refer to an allocation returned by alloc, not yet freed.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: u32) {
    if ptr.is_null() {
        return;
    }
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
        ptr,
        len as usize,
    )));
}

/// # Safety
/// `ptr..ptr+len` must be a readable allocation in this module's memory.
/// Invalid UTF-8 or syntax returns 0, preserving all existing genomes/world state.
#[no_mangle]
pub unsafe extern "C" fn compile_genome(ptr: *const u8, len: u32) -> u32 {
    ENGINE.with(|engine| {
        let mut engine = engine.borrow_mut();
        if ptr.is_null() || len as usize > MAX_SOURCE_BYTES {
            engine.error = "Program must be valid UTF-8 and no larger than 256 KiB.".into();
            return 0;
        }
        let bytes = std::slice::from_raw_parts(ptr, len as usize);
        match std::str::from_utf8(bytes) {
            Ok(source) => engine.compile(source),
            Err(_) => {
                engine.error = "Program must contain valid UTF-8 text.".into();
                0
            }
        }
    })
}

/// Analyze source with the authoritative compiler, without registering a genome
/// or changing world state, RNGs, registry budgets, or the latest engine error.
/// Returns engine-owned UTF-8 JSON; copy immediately using genome_analysis_len.
/// # Safety
/// For 0 < len <= MAX_SOURCE_BYTES, a non-null ptr must reference readable memory.
/// Empty and oversized inputs need no allocation and may use a null pointer.
#[no_mangle]
pub unsafe extern "C" fn analyze_genome(ptr: *const u8, len: u32) -> *const u8 {
    let analysis = if len as usize > MAX_SOURCE_BYTES {
        GenomeAnalysis::invalid_input(
            len as usize,
            format!("program exceeds {MAX_SOURCE_BYTES} bytes."),
        )
    } else if len == 0 {
        Program::analyze("")
    } else if ptr.is_null() {
        GenomeAnalysis::invalid_input(len as usize, "Program must contain valid UTF-8 text.")
    } else {
        match std::str::from_utf8(std::slice::from_raw_parts(ptr, len as usize)) {
            Ok(source) => Program::analyze(source),
            Err(_) => GenomeAnalysis::invalid_input(
                len as usize,
                "Program must contain valid UTF-8 text.",
            ),
        }
    };
    ENGINE.with(|engine| {
        let mut engine = engine.borrow_mut();
        engine.analysis_buffer = analysis.json().into_bytes();
        engine.analysis_buffer.as_ptr()
    })
}

/// Authoritative signal/output schema for editor catalogue consistency.
#[no_mangle]
pub extern "C" fn language_schema_ptr() -> *const u8 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.schema_buffer = genome::language_schema_json().into_bytes();
        e.schema_buffer.as_ptr()
    })
}
#[no_mangle]
pub extern "C" fn language_schema_len() -> u32 {
    ENGINE.with(|e| e.borrow().schema_buffer.len() as u32)
}
#[no_mangle]
pub extern "C" fn cell_body_ptr(id: u32) -> *const u8 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.error.clear();
        e.body_buffer.clear();
        match e.world.body_json(id) {
            Some(json) => {
                e.body_buffer = json.into_bytes();
                e.body_buffer.as_ptr()
            }
            None => {
                e.error = "This organism is no longer alive.".into();
                std::ptr::null()
            }
        }
    })
}
#[no_mangle]
pub extern "C" fn cell_body_len() -> u32 {
    ENGINE.with(|e| e.borrow().body_buffer.len() as u32)
}
#[no_mangle]
pub extern "C" fn energy_total() -> f64 {
    ENGINE.with(|e| e.borrow().world.energy_total())
}
#[no_mangle]
pub extern "C" fn energy_input_total() -> f64 {
    ENGINE.with(|e| e.borrow().world.energy_input_total())
}

#[no_mangle]
pub extern "C" fn genome_analysis_len() -> u32 {
    ENGINE.with(|engine| engine.borrow().analysis_buffer.len() as u32)
}

#[no_mangle]
pub extern "C" fn error_ptr() -> *const u8 {
    ENGINE.with(|e| e.borrow().error.as_ptr())
}

#[no_mangle]
pub extern "C" fn error_len() -> u32 {
    ENGINE.with(|e| e.borrow().error.len() as u32)
}

/// Register a derived program without changing any existing cell or genome.
/// Returns the parent ID for a safe no-op; error bytes explain why. Preview
/// mutations never increment the world's mutated/skipped-birth counters.
#[no_mangle]
pub extern "C" fn mutate_genome(genome_id: u32, seed: u32, kind: u32) -> u32 {
    ENGINE.with(|e| e.borrow_mut().mutate(genome_id, seed, kind))
}

/// Copy source into an engine-owned UTF-8 buffer. Read source_len next and copy
/// the view before another engine call can grow WASM memory or replace it.
#[no_mangle]
pub extern "C" fn genome_source_ptr(genome_id: u32) -> *const u8 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.source_buffer.clear();
        if !e.valid_genome(genome_id) {
            return std::ptr::null();
        }
        let Engine {
            source_buffer,
            genomes,
            ..
        } = &mut *e;
        source_buffer.extend_from_slice(genomes[(genome_id - 1) as usize].source.as_bytes());
        source_buffer.as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn genome_source_len() -> u32 {
    ENGINE.with(|e| e.borrow().source_buffer.len() as u32)
}

/// Describe the immutable compiled graph as UTF-8 JSON in an engine-owned
/// buffer. Read graph_len and copy immediately; never deallocate this pointer.
#[no_mangle]
pub extern "C" fn genome_graph_ptr(genome_id: u32) -> *const u8 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.graph_buffer.clear();
        if !e.valid_genome(genome_id) {
            return std::ptr::null();
        }
        e.graph_buffer = e.genomes[(genome_id - 1) as usize]
            .program
            .graph_json()
            .into_bytes();
        e.graph_buffer.as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn genome_graph_len() -> u32 {
    ENGINE.with(|e| e.borrow().graph_buffer.len() as u32)
}

/// Evaluate an organism against the current world snapshot without committing
/// writes or stepping time, environment, RNG, or any organism. Copy immediately.
#[no_mangle]
pub extern "C" fn cell_evaluation_ptr(cell_id: u32) -> *const u8 {
    ENGINE.with(|engine| {
        let mut engine = engine.borrow_mut();
        engine.evaluation_buffer.clear();
        engine.error.clear();
        let Some(inputs) = engine.world.sense_cell(cell_id) else {
            engine.error = "This organism is no longer alive.".into();
            return std::ptr::null();
        };
        let cell = engine
            .world
            .cells
            .iter()
            .find(|cell| cell.id == cell_id)
            .unwrap();
        let genome = &engine.genomes[(cell.genome_id - 1) as usize];
        match Program::evaluation_json(
            &genome.source,
            &inputs,
            &cell.memory,
            cell.id,
            cell.genome_id,
            engine.world.ticks,
        ) {
            Ok(json) => {
                engine.evaluation_buffer = json.into_bytes();
                engine.evaluation_buffer.as_ptr()
            }
            Err(error) => {
                engine.error = error;
                std::ptr::null()
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn cell_evaluation_len() -> u32 {
    ENGINE.with(|engine| engine.borrow().evaluation_buffer.len() as u32)
}

#[no_mangle]
pub extern "C" fn genome_parent(genome_id: u32) -> u32 {
    ENGINE.with(|e| {
        e.borrow()
            .genomes
            .get(genome_id.wrapping_sub(1) as usize)
            .map_or(0, |g| g.parent_id)
    })
}

#[no_mangle]
pub extern "C" fn genome_mutation_kind(genome_id: u32) -> u32 {
    ENGINE.with(|e| {
        e.borrow()
            .genomes
            .get(genome_id.wrapping_sub(1) as usize)
            .map_or(0, |g| g.mutation_kind)
    })
}

#[no_mangle]
pub extern "C" fn set_mutation_rate(rate: f32) {
    ENGINE.with(|e| e.borrow_mut().world.set_mutation_rate(rate as f64));
}

#[no_mangle]
pub extern "C" fn mutation_rate() -> f32 {
    ENGINE.with(|e| e.borrow().world.mutation_rate as f32)
}

#[no_mangle]
pub extern "C" fn mutation_count() -> u32 {
    ENGINE.with(|e| e.borrow().world.mutations.min(u32::MAX as u64) as u32)
}

#[no_mangle]
pub extern "C" fn mutation_skipped_count() -> u32 {
    ENGINE.with(|e| e.borrow().world.mutations_skipped.min(u32::MAX as u64) as u32)
}

#[no_mangle]
pub extern "C" fn genome_count() -> u32 {
    ENGINE.with(|e| e.borrow().genomes.len() as u32)
}

#[no_mangle]
pub extern "C" fn genome_limit() -> u32 {
    MAX_GENOMES as u32
}

#[no_mangle]
pub extern "C" fn genome_bytes() -> u32 {
    ENGINE.with(|e| registry_bytes(&e.borrow().genomes).min(u32::MAX as usize) as u32)
}

#[no_mangle]
pub extern "C" fn genome_byte_limit() -> u32 {
    MAX_GENOME_BYTES as u32
}

#[no_mangle]
pub extern "C" fn predation_count() -> u32 {
    ENGINE.with(|e| e.borrow().world.predations.min(u32::MAX as u64) as u32)
}

#[no_mangle]
pub extern "C" fn attack_count() -> u32 {
    ENGINE.with(|e| e.borrow().world.attacks.min(u32::MAX as u64) as u32)
}

#[no_mangle]
pub extern "C" fn photosynthesis_total() -> f64 {
    ENGINE.with(|e| e.borrow().world.photosynthesis_total)
}

#[no_mangle]
pub extern "C" fn nutrient_total() -> f64 {
    ENGINE.with(|e| e.borrow().world.nutrient_total())
}

#[no_mangle]
pub extern "C" fn nutrient_input_total() -> f64 {
    ENGINE.with(|e| e.borrow().world.nutrient_input_total())
}

#[no_mangle]
pub extern "C" fn nutrient_dissolved_total() -> f64 {
    ENGINE.with(|e| e.borrow().world.environment.nutrient_total())
}

#[no_mangle]
pub extern "C" fn nutrient_food_total() -> f64 {
    ENGINE.with(|e| e.borrow().world.nutrient_food_total())
}

#[no_mangle]
pub extern "C" fn nutrient_cell_total() -> f64 {
    ENGINE.with(|e| e.borrow().world.nutrient_cell_total())
}

/// Change only the initial dissolved mass per patch used on the next reset.
#[no_mangle]
pub extern "C" fn set_initial_nutrients(level: f32) {
    ENGINE.with(|e| {
        e.borrow_mut()
            .world
            .environment
            .set_initial_nutrients(level as f64)
    });
}

/// Introduce minerals, recording the actual addition on the input ledger.
#[no_mangle]
pub extern "C" fn add_nutrients(x: f32, y: f32, amount: f32) -> f64 {
    ENGINE.with(|e| {
        e.borrow_mut()
            .world
            .environment
            .add_nutrients(x as f64, y as f64, amount as f64)
    })
}

/// Return an engine-owned UTF-8 JSON array of occupied registers, ordered by
/// type and address. Copy before another engine call replaces it or grows memory.
#[no_mangle]
pub extern "C" fn cell_memory_ptr(cell_id: u32) -> *const u8 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.error.clear();
        e.memory_buffer = if let Some(cell) = e.world.cells.iter().find(|cell| cell.id == cell_id) {
            cell.memory.to_json().into_bytes()
        } else {
            e.error = "Unknown cell: it may have died or the world may have been reset.".into();
            b"[]".to_vec()
        };
        e.memory_buffer.as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn cell_memory_len() -> u32 {
    ENGINE.with(|e| e.borrow().memory_buffer.len() as u32)
}

#[no_mangle]
pub extern "C" fn set_environment(temperature: f32, light: f32, current: f32) {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.error.clear();
        e.world
            .environment
            .configure(temperature as f64, light as f64, current as f64);
    });
}

/// Kinds: 0 light, 1 heat, 2 vortex, 3 nutrient spring. Position wraps.
/// Returns zero without changing the environment if invalid or already full.
#[no_mangle]
pub extern "C" fn add_source(kind: u32, x: f32, y: f32, strength: f32, radius: f32) -> u32 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.error.clear();
        let id = e.world.environment.add_source(kind, x as f64, y as f64, strength as f64, radius as f64);
        if id == 0 {
            e.error = "Could not add source: use kind 0 (light), 1 (heat), 2 (vortex), or 3 (nutrient spring), finite values, and at most 16 sources.".into();
        }
        id
    })
}

#[no_mangle]
pub extern "C" fn remove_source(id: u32) -> u32 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.error.clear();
        if e.world.environment.remove_source(id) {
            1
        } else {
            e.error = "Unknown environment source.".into();
            0
        }
    })
}

#[no_mangle]
pub extern "C" fn clear_sources() {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.error.clear();
        e.world.environment.clear_sources();
    });
}

/// Packed grid/source snapshot; copy immediately using environment_snapshot_len.
#[no_mangle]
pub extern "C" fn environment_snapshot_ptr() -> *const f32 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        let Engine {
            world,
            environment_snapshot,
            ..
        } = &mut *e;
        world
            .environment
            .snapshot(world.time(), environment_snapshot);
        environment_snapshot.as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn environment_snapshot_len() -> u32 {
    ENGINE.with(|e| e.borrow().environment_snapshot.len() as u32)
}

#[no_mangle]
pub extern "C" fn reset(seed: u32, genome_id: u32) -> u32 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        if !e.valid_genome(genome_id) {
            return 0;
        }
        match e.world.try_reset(seed, genome_id) {
            Ok(()) => 1,
            Err(error) => {
                e.error = error;
                0
            }
        }
    })
}

/// Reset dynamic state, retain the environment recipe and rates, and seed no cells.
#[no_mangle]
pub extern "C" fn reset_empty(seed: u32, initial_food: u32) {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.error.clear();
        e.world.reset_empty(seed, initial_food as usize);
    });
}

#[no_mangle]
pub extern "C" fn seed_cells(genome_id: u32, count: f64, x: f32, y: f32) -> u32 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.error.clear();
        if !count.is_finite() || count < 0.0 || count.fract() != 0.0 || count > u32::MAX as f64 {
            e.error = "Organism count must be a nonnegative whole number within the addressable identifier range.".into();
            return 0;
        }
        if !e.valid_genome(genome_id) { return 0; }
        match e.world.try_seed_cells(genome_id, count as usize, x as f64, y as f64) {
            Ok(added) => added as u32,
            Err(error) => { e.error = error; 0 }
        }
    })
}

#[no_mangle]
pub extern "C" fn add_food(x: f32, y: f32, count: u32) -> u32 {
    ENGINE.with(|e| {
        e.borrow_mut()
            .world
            .add_food(x as f64, y as f64, count as usize) as u32
    })
}

#[no_mangle]
pub extern "C" fn set_food_rate(rate: f32) {
    ENGINE.with(|e| e.borrow_mut().world.set_food_rate(rate as f64));
}

#[no_mangle]
pub extern "C" fn tick(steps: u32) -> u32 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.error.clear();
        let Engine {
            world,
            genomes,
            error,
            ..
        } = &mut *e;
        for _ in 0..steps.min(120) {
            if let Err(message) = world.try_step(genomes) {
                *error = message;
                return 0;
            }
        }
        1
    })
}

/// Rebuild the snapshot and return its pointer. Read snapshot_len afterwards,
/// then copy/use the Float64Array before another engine call or memory growth.
#[no_mangle]
pub extern "C" fn snapshot_ptr() -> *const f64 {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        let Engine {
            world, snapshot, ..
        } = &mut *e;
        world.snapshot(snapshot);
        snapshot.as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn snapshot_len() -> u32 {
    ENGINE.with(|e| e.borrow().snapshot.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_abi_is_non_mutating_even_when_registry_is_full() {
        ENGINE.with(|engine| {
            let mut engine = engine.borrow_mut();
            *engine = Engine::new();
            let id = engine.compile("ri001=ri001+1;thrust=0.5;turn=0.1;divide=false;");
            engine.world.reset(84, id);
            engine.world.environment.configure(0.4, 0.8, 6.0);
            let Engine { world, genomes, .. } = &mut *engine;
            for _ in 0..12 {
                world.step(genomes);
            }
            let template = engine.genomes[0].clone();
            engine.genomes.resize(MAX_GENOMES, template);
            engine.error = "Retain this prior error".into();
        });
        let before = ENGINE.with(|engine| checkpoint::save(&engine.borrow()).unwrap());
        let budget = genome_bytes();
        for source in [
            "let note=\"é🌱\";thrust=0;turn=0;divide=false;",
            "let unfinished =",
            "",
        ] {
            let pointer = unsafe { analyze_genome(source.as_ptr(), source.len() as u32) };
            let json = unsafe {
                std::str::from_utf8(std::slice::from_raw_parts(
                    pointer,
                    genome_analysis_len() as usize,
                ))
                .unwrap()
            };
            assert_eq!(json, Program::analyze(source).json());
            assert_eq!(genome_count(), MAX_GENOMES as u32);
            assert_eq!(genome_bytes(), budget);
            assert_eq!(
                ENGINE.with(|engine| engine.borrow().error.clone()),
                "Retain this prior error"
            );
            assert_eq!(
                ENGINE.with(|engine| checkpoint::save(&engine.borrow()).unwrap()),
                before
            );
        }
        let invalid_utf8 = [0xff];
        for (pointer, length) in [
            (invalid_utf8.as_ptr(), 1),
            (std::ptr::null(), 0),
            (std::ptr::null(), MAX_SOURCE_BYTES as u32 + 1),
        ] {
            let result = unsafe { analyze_genome(pointer, length) };
            let json = unsafe {
                std::str::from_utf8(std::slice::from_raw_parts(
                    result,
                    genome_analysis_len() as usize,
                ))
                .unwrap()
            };
            assert!(json.contains("\"valid\":false"));
            assert_eq!(
                ENGINE.with(|engine| checkpoint::save(&engine.borrow()).unwrap()),
                before
            );
        }
    }

    #[test]
    fn bad_program_preserves_world_and_existing_genomes() {
        let mut engine = Engine::new();
        let id = engine.compile(genome::DEFAULT_PROGRAM);
        assert_eq!(id, 1);
        engine.world.reset(81, id);
        let mut before = Vec::new();
        engine.world.snapshot(&mut before);
        assert_eq!(
            engine.compile("thrust = imaginary_sensor; turn = 0; divide = false;"),
            0
        );
        assert!(!engine.error.is_empty());
        assert_eq!(engine.genomes.len(), 1);
        let mut after = Vec::new();
        engine.world.snapshot(&mut after);
        assert_eq!(before, after);
        assert_eq!(engine.compile(genome::DEFAULT_PROGRAM), 1);
        assert_eq!(engine.genomes.len(), 1, "same source is deduplicated");
        assert!(engine.error.is_empty());
    }

    #[test]
    fn unknown_genome_is_rejected() {
        let mut engine = Engine::new();
        assert!(!engine.valid_genome(0));
        assert!(!engine.valid_genome(1));
        engine.compile(genome::DEFAULT_PROGRAM);
        assert!(engine.valid_genome(1));
        assert!(!engine.valid_genome(u32::MAX));
    }

    #[test]
    fn snapshot_layout_has_exact_documented_length() {
        let mut engine = Engine::new();
        engine.compile(genome::DEFAULT_PROGRAM);
        engine.world.reset(11, 1);
        engine.world.cells[0].energy = 40.0;
        engine.world.cells[0].age = 12.0;
        engine.world.cells[0].vx = 2.5;
        engine.world.cells[0].vy = -1.25;
        engine.world.cells[0].integrity = 0.8;
        engine.world.snapshot(&mut engine.snapshot);
        assert_eq!(abi_version(), 9);
        assert_eq!(simulation::CELL_STRIDE, 135);
        assert_eq!(engine.snapshot.len(), 12 + 22 * 135 + 360 * 4);
        assert_eq!(engine.snapshot[7], simulation::WIDTH as f64);
        assert_eq!(engine.snapshot[8], simulation::HEIGHT as f64);
        assert_eq!(
            engine.snapshot[9], 0.0,
            "the reserved field is not a population limit"
        );
        let first = &engine.snapshot[12..12 + simulation::CELL_STRIDE];
        assert_eq!(&first[4..6], &[2.5, -1.25]);
        assert_eq!(first[7], 40.0);
        assert_eq!(first[8], 12.0);
        assert_eq!(first[11], simulation::CELL_RADIUS as f64);
        assert_eq!(first[12], 1.0, "body area");
        assert_eq!(first[13], engine.world.cells[0].mass() as f64);
        assert_eq!(first[14], 1.0, "birth area");
        assert_eq!(first[15], engine.world.cells[0].energy_capacity() as f64);
        assert_eq!(first[16], engine.world.cells[0].basal_metabolism() as f64);
        assert_eq!(&first[17..19], &[0.0, 0.0], "memory occupancy and upkeep");
        assert_eq!(first[19], 0.8);
        assert_eq!(first[20], 0.0, "photosynthesis rate starts empty");
        assert_eq!(first[21], engine.world.cells[0].nutrient_stored as f64);
        assert_eq!(first[22], engine.world.cells[0].nutrient_capacity() as f64);
        assert_eq!(first[24], 0.0, "gut raw energy");
        assert_eq!(first[25], engine.world.cells[0].gut_capacity() as f64);
        assert_eq!(&first[26..30], &[0.0, 0.0, 0.0, 0.0]);
        assert_eq!(first[30], 300.0);
        assert!(first[33] <= first[34], "internal packing");
        assert!(first[35] <= first[36], "surface packing");
        assert_eq!(first[37], 0.0, "founder cannot divide immediately");
        for (index, kind) in body::OrganelleKind::ALL.iter().copied().enumerate() {
            let offset = 39 + index * 6;
            let cell = &engine.world.cells[0];
            assert_eq!(first[offset], cell.body.count(kind) as f64);
            assert_eq!(first[offset + 1], cell.body.mass(kind) as f64);
            assert_eq!(first[offset + 2], cell.body.capacity(kind) as f64);
            assert_eq!(first[offset + 3], cell.body.damaged(kind) as f64);
            assert_eq!(first[offset + 4], cell.body.building(kind) as f64);
            assert_eq!(first[offset + 5], cell.body.functional_mass(kind) as f64);
        }
        assert_eq!(engine.snapshot[12 + 22 * simulation::CELL_STRIDE + 3], 0.25);
        assert!(engine.snapshot.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn manual_mutation_is_a_repeatable_preview_with_distinct_provenance() {
        let mut engine = Engine::new();
        let parent = engine.compile(genome::DEFAULT_PROGRAM);
        engine.world.reset(72, parent);
        let mut before = Vec::new();
        engine.world.snapshot(&mut before);
        let mutant = engine.mutate(parent, 57, 1);
        assert_ne!(mutant, 0);
        assert_ne!(mutant, parent);
        assert!(engine.error.is_empty());
        assert_eq!(engine.genomes[(mutant - 1) as usize].parent_id, parent);
        assert_eq!(engine.genomes[(mutant - 1) as usize].mutation_kind, 1);
        assert_eq!(engine.mutate(parent, 57, 1), mutant);
        assert_eq!(engine.genomes.len(), 2);
        assert_eq!(engine.world.mutations, 0);
        assert_eq!(engine.world.mutations_skipped, 0);
        let mut after = Vec::new();
        engine.world.snapshot(&mut after);
        assert_eq!(before, after);

        // Explicitly authoring the same text must not silently claim it arose
        // from mutation. The frontend retains the actual mutant ID when seeding.
        let source = engine.genomes[(mutant - 1) as usize].source.clone();
        let authored = engine.compile(&source);
        assert_ne!(authored, mutant);
        assert_eq!(engine.genomes[(authored - 1) as usize].parent_id, 0);
        assert_eq!(engine.compile(&source), authored);
    }

    #[test]
    fn invalid_mutation_request_and_capacity_are_non_destructive() {
        let mut engine = Engine::new();
        let parent = engine.compile(genome::DEFAULT_PROGRAM);
        assert_eq!(engine.mutate(parent, 57, 99), 0);
        assert!(!engine.error.is_empty());
        assert_eq!(engine.mutate(0, 57, 1), 0);
        assert_eq!(engine.genomes.len(), 1);
        let fill = engine.genomes[0].clone();
        engine.genomes.resize(MAX_GENOMES, fill);
        assert_eq!(engine.mutate(parent, 57, 1), parent);
        assert!(engine.error.contains("limit"));
        assert_eq!(engine.genomes.len(), MAX_GENOMES);
        assert_eq!(engine.world.mutations, 0);
        assert_eq!(engine.world.mutations_skipped, 0);
    }

    #[test]
    fn compiled_genome_byte_capacity_is_atomic_and_cached() {
        let mut engine = Engine::new();
        let id = engine.compile(genome::DEFAULT_PROGRAM);
        assert!(engine.genomes[0].bytes > engine.genomes[0].source.len());
        engine.world.reset(81, id);
        let mut before = Vec::new();
        engine.world.snapshot(&mut before);
        engine.genomes[0].bytes = MAX_GENOME_BYTES;
        assert_eq!(engine.compile("thrust = 0; turn = 0; divide = false;"), 0);
        assert!(engine.error.contains("64 MiB"));
        assert_eq!(engine.genomes.len(), 1);
        let mut after = Vec::new();
        engine.world.snapshot(&mut after);
        assert_eq!(before, after);
        assert_eq!(
            engine.compile(genome::DEFAULT_PROGRAM),
            id,
            "existing authored source is still usable at the byte cap"
        );
    }

    #[test]
    fn seed_export_rejects_unrepresentable_counts_without_mutating_the_world() {
        ENGINE.with(|e| *e.borrow_mut() = Engine::new());
        let id = ENGINE.with(|e| e.borrow_mut().compile("thrust=0;turn=0;divide=false;"));
        let before = ENGINE.with(|e| checkpoint::save(&e.borrow()).unwrap());
        for count in [
            -1.0,
            0.5,
            f64::NAN,
            f64::INFINITY,
            u32::MAX as f64 + 1.0,
            u32::MAX as f64,
        ] {
            assert_eq!(seed_cells(id, count, -1.0, -1.0), 0);
            assert!(
                error_len() > 0,
                "invalid or ID-exhausting requests must report why they failed"
            );
            assert_eq!(
                ENGINE.with(|e| checkpoint::save(&e.borrow()).unwrap()),
                before
            );
        }
        assert_eq!(seed_cells(id, 0.0, -1.0, -1.0), 0);
        assert_eq!(error_len(), 0, "zero is an intentional no-op");
        assert_eq!(seed_cells(id, 513.0, -1.0, -1.0), 513);
        assert_eq!(error_len(), 0);
    }

    #[test]
    fn language_and_body_exports_are_typed_independent_and_read_only() {
        ENGINE.with(|engine| *engine.borrow_mut() = Engine::new());
        let source = "thrust=0;turn=0;divide=false;target.flagellum.count=999;target.flagellum.size=.2;build.flagellum=0;";
        let id = ENGINE.with(|engine| engine.borrow_mut().compile(source));
        reset_empty(91, 0);
        assert_eq!(seed_cells(id, 1.0, 200.0, 100.0), 1);
        assert_eq!(tick(1), 1);
        let before = ENGINE.with(|engine| checkpoint::save(&engine.borrow()).unwrap());
        let schema_pointer = language_schema_ptr();
        let schema_length = language_schema_len() as usize;
        let schema = unsafe { std::slice::from_raw_parts(schema_pointer, schema_length) }.to_vec();
        assert_eq!(
            std::str::from_utf8(&schema).unwrap(),
            genome::language_schema_json()
        );
        let body_pointer = cell_body_ptr(1);
        assert!(!body_pointer.is_null());
        let body_length = cell_body_len() as usize;
        let body = unsafe { std::slice::from_raw_parts(body_pointer, body_length) }.to_vec();
        let json = std::str::from_utf8(&body).unwrap();
        assert!(json.contains("\"spaceUsed\":"));
        assert!(json.contains("\"kind\":\"flagellum\",\"count\":999"));
        let actual_organs = json.split("\"requested\"").next().unwrap();
        assert!(
            !actual_organs.contains("\"kind\":\"flagellum\""),
            "targets do not grant free organs"
        );
        genome_source_ptr(id);
        genome_graph_ptr(id);
        cell_memory_ptr(1);
        cell_evaluation_ptr(1);
        environment_snapshot_ptr();
        snapshot_ptr();
        assert_eq!(
            unsafe { std::slice::from_raw_parts(schema_pointer, schema_length) },
            schema
        );
        assert_eq!(
            unsafe { std::slice::from_raw_parts(body_pointer, body_length) },
            body
        );
        assert_eq!(
            before,
            ENGINE.with(|engine| checkpoint::save(&engine.borrow()).unwrap())
        );
        assert!(cell_body_ptr(u32::MAX).is_null());
        assert_eq!(cell_body_len(), 0);
        assert!(error_len() > 0);
        assert_eq!(
            before,
            ENGINE.with(|engine| checkpoint::save(&engine.borrow()).unwrap())
        );
    }

    #[test]
    fn environment_and_memory_exports_have_independent_owned_buffers() {
        ENGINE.with(|e| *e.borrow_mut() = Engine::new());
        let id = ENGINE.with(|e| e.borrow_mut().compile("ri001 = 7; rb999 = true; rs002 = \"hello\"; rv003 = vec(1, 2); thrust = 0; turn = 0; divide = false;"));
        assert!(id > 0);
        assert_eq!(seed_cells(id, 1.0, -1.0, -1.0), 1);
        tick(1);
        let pointer = cell_memory_ptr(1);
        let memory = unsafe { std::slice::from_raw_parts(pointer, cell_memory_len() as usize) };
        let json = std::str::from_utf8(memory).unwrap().to_owned();
        for name in ["ri001", "rb999", "rs002", "rv003"] {
            assert!(json.contains(name));
        }
        assert!(json.contains("hello"));
        assert_eq!(error_len(), 0);
        let source = add_source(0, -1.0, 1.0, 2.0, 2.0);
        assert!(source > 0);
        let env_pointer = environment_snapshot_ptr();
        let env =
            unsafe { std::slice::from_raw_parts(env_pointer, environment_snapshot_len() as usize) };
        assert_eq!(env.len(), 7 + environment::PATCHES * 5 + 6);
        assert_eq!(&env[..7], &[60.0, 38.0, 0.5, 0.0, 0.0, 1.0, 0.3]);
        assert_eq!(
            &env[7 + environment::PATCHES * 5..],
            &[source as f32, 0.0, 1199.0, 1.0, 1.0, 40.0]
        );
        assert_eq!(
            unsafe { std::slice::from_raw_parts(pointer, cell_memory_len() as usize) },
            json.as_bytes()
        );
        let unknown = cell_memory_ptr(u32::MAX);
        assert_eq!(
            unsafe { std::slice::from_raw_parts(unknown, cell_memory_len() as usize) },
            b"[]"
        );
        assert!(error_len() > 0);
        assert_eq!(remove_source(source), 1);
        assert_eq!(error_len(), 0);
        assert_eq!(remove_source(source), 0);
        assert!(error_len() > 0);
        set_environment(f32::NAN, f32::INFINITY, f32::NEG_INFINITY);
        assert_eq!(error_len(), 0);
        assert_eq!(genome_byte_limit(), 64 * 1024 * 1024);
        assert!(genome_bytes() > 0);
    }

    #[test]
    fn source_allocation_limit_matches_the_compiler_byte_limit() {
        assert_eq!(MAX_SOURCE_BYTES, 256 * 1024);
        assert!(alloc(MAX_SOURCE_BYTES as u32 + 1).is_null());
        let pointer = alloc(MAX_SOURCE_BYTES as u32);
        assert!(!pointer.is_null());
        unsafe {
            dealloc(pointer, MAX_SOURCE_BYTES as u32);
        }
    }

    #[test]
    fn nutrient_exports_report_material_and_explicit_inputs_without_counting_recycling() {
        ENGINE.with(|e| *e.borrow_mut() = Engine::new());
        let id = ENGINE.with(|e| {
            e.borrow_mut()
                .compile("thrust = 0; turn = 0; divide = false; grow = 1; absorb = 1;")
        });
        set_initial_nutrients(0.0);
        reset(7, id);
        set_food_rate(0.0);
        assert_eq!(nutrient_dissolved_total(), 0.0);
        assert_eq!(nutrient_food_total(), 90.0);
        let introduced = nutrient_food_total() + nutrient_cell_total();
        assert!(
            nutrient_cell_total() > 0.0,
            "founder structures and reserves are charged"
        );
        assert!((nutrient_total() - introduced).abs() < 1e-8);
        assert!((nutrient_input_total() - introduced).abs() < 1e-8);
        assert_eq!(add_nutrients(100.0, 100.0, 4.0), 4.0);
        assert_eq!(add_nutrients(0.0, 0.0, f32::NAN), 0.0);
        assert!(add_source(3, 100.0, 100.0, 1.0, 80.0) > 0);
        tick(60);
        assert!((nutrient_total() - nutrient_input_total()).abs() < 1e-8);
        assert!(
            (nutrient_total()
                - (nutrient_dissolved_total() + nutrient_food_total() + nutrient_cell_total()))
            .abs()
                < 1e-8
        );
        assert!((nutrient_input_total() - (introduced + 7.0)).abs() < 1e-8);
        let before = nutrient_total();
        set_initial_nutrients(0.5);
        assert_eq!(
            nutrient_total(),
            before,
            "the reset recipe does not edit the running field"
        );
        reset(7, id);
        assert!((nutrient_total() - (0.5 * environment::PATCHES as f64 + introduced)).abs() < 1e-8);
        assert!((nutrient_total() - nutrient_input_total()).abs() < 1e-8);
    }

    #[test]
    fn graph_export_has_an_independent_buffer_and_does_not_advance_the_world() {
        ENGINE.with(|e| *e.borrow_mut() = Engine::new());
        let id = ENGINE.with(|e| {
            e.borrow_mut()
                .compile("ri001 = ri001 + 1; thrust = 0; turn = 0; divide = false;")
        });
        reset(7, id);
        tick(2);
        let before = ENGINE.with(|e| {
            let e = e.borrow();
            let mut snapshot = Vec::new();
            e.world.snapshot(&mut snapshot);
            (snapshot, e.world.cells[0].memory.to_json(), e.genomes.len())
        });
        let pointer = genome_graph_ptr(id);
        assert!(!pointer.is_null());
        assert_eq!(error_len(), 0);
        let length = genome_graph_len() as usize;
        let json = unsafe { std::slice::from_raw_parts(pointer, length) }.to_vec();
        let graph = std::str::from_utf8(&json).unwrap();
        assert!(graph.contains("\"version\":1"));
        assert!(graph.contains("\"id\":\"write:ri001\""));
        genome_source_ptr(id);
        cell_memory_ptr(1);
        environment_snapshot_ptr();
        snapshot_ptr();
        assert_eq!(unsafe { std::slice::from_raw_parts(pointer, length) }, json);
        let after = ENGINE.with(|e| {
            let e = e.borrow();
            let mut snapshot = Vec::new();
            e.world.snapshot(&mut snapshot);
            (snapshot, e.world.cells[0].memory.to_json(), e.genomes.len())
        });
        assert_eq!(before, after);
        assert!(genome_graph_ptr(0).is_null());
        assert_eq!(genome_graph_len(), 0);
        assert!(error_len() > 0);
        assert!(genome_graph_ptr(u32::MAX).is_null());
        assert_eq!(genome_graph_len(), 0);
        assert!(!genome_graph_ptr(id).is_null());
        assert_eq!(error_len(), 0);
        assert_eq!(
            unsafe {
                std::slice::from_raw_parts(genome_graph_ptr(id), genome_graph_len() as usize)
            },
            json
        );
    }

    #[test]
    fn expression_preview_is_repeatable_side_effect_free_and_rejects_missing_cells() {
        ENGINE.with(|engine| *engine.borrow_mut() = Engine::new());
        let id = ENGINE.with(|engine| {
            engine
                .borrow_mut()
                .compile("# 🌿\nlet count=ri001+1;ri001=count;thrust=0;turn=0;divide=false;")
        });
        reset_empty(71, 0);
        assert_eq!(seed_cells(id, 2.0, 100.0, 100.0), 2);
        tick(3);
        let before = ENGINE.with(|engine| checkpoint::save(&engine.borrow()).unwrap());
        let pointer = cell_evaluation_ptr(1);
        assert!(!pointer.is_null());
        let length = cell_evaluation_len() as usize;
        let copied = unsafe { std::slice::from_raw_parts(pointer, length) }.to_vec();
        let json = std::str::from_utf8(&copied).unwrap();
        assert!(json.contains("\"tick\":3"));
        assert!(json.contains("\"type\":\"integer\",\"value\":4"));
        assert!(json.contains("\"name\":\"energy\",\"type\":\"scalar\""));
        cell_memory_ptr(1);
        genome_source_ptr(id);
        genome_graph_ptr(id);
        snapshot_ptr();
        environment_snapshot_ptr();
        assert_eq!(
            unsafe { std::slice::from_raw_parts(pointer, length) },
            copied
        );
        assert_eq!(
            before,
            ENGINE.with(|engine| checkpoint::save(&engine.borrow()).unwrap())
        );
        assert_eq!(
            unsafe {
                std::slice::from_raw_parts(cell_evaluation_ptr(1), cell_evaluation_len() as usize)
            },
            copied
        );
        assert!(cell_evaluation_ptr(0).is_null());
        assert_eq!(cell_evaluation_len(), 0);
        assert!(error_len() > 0);
        assert!(cell_evaluation_ptr(u32::MAX).is_null());
        assert_eq!(cell_evaluation_len(), 0);
        assert_eq!(
            before,
            ENGINE.with(|engine| checkpoint::save(&engine.borrow()).unwrap())
        );
    }

    #[test]
    fn legacy_checkpoints_are_rejected_atomically_with_an_explanation() {
        ENGINE.with(|engine| {
            let mut engine = engine.borrow_mut();
            *engine = Engine::new();
            let id = engine.compile("ri001=ri001+1;thrust=0;turn=0;divide=false;");
            engine.world.reset(71, id);
        });
        assert_eq!(tick(3), 1);
        let before = ENGINE.with(|engine| checkpoint::save(&engine.borrow()).unwrap());
        for legacy in [
            include_bytes!("../fixtures/trophic-v1.bin").as_slice(),
            include_bytes!("../fixtures/trophic-v2.bin").as_slice(),
        ] {
            ENGINE.with(|engine| {
                let mut engine = engine.borrow_mut();
                assert!(!engine.restore_state(legacy));
                assert!(
                    engine.error.to_lowercase().contains("unsupported"),
                    "{}",
                    engine.error
                );
            });
            assert_eq!(
                before,
                ENGINE.with(|engine| checkpoint::save(&engine.borrow()).unwrap())
            );
        }
        assert!(!cell_evaluation_ptr(1).is_null());
        let mut copy = checkpoint::restore(&before).unwrap();
        assert_eq!(tick(30), 1);
        for _ in 0..30 {
            copy.world.step(&mut copy.genomes);
        }
        assert_eq!(
            checkpoint::save(&copy).unwrap(),
            ENGINE.with(|engine| checkpoint::save(&engine.borrow()).unwrap())
        );
    }
}
