//! cargo run --release --manifest-path engine/Cargo.toml --example cellular-bindings -- 20000
//! Only the cellular input projection is timed. This measures identity lookup
//! and buffer reuse, not world stepping, compilation, sensing or rendering.
use cellarium_engine::body::Body;
use cellarium_engine::cellular::{AssemblyDef, AssemblyInstance, AssemblyKind, CellularProgram,
    CellularState, Ingredient, MechanismKind, ProductDef, ProductKind, ProductPool, Recipe, RecipePart};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

struct CountingAllocator;
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static COUNTING: AtomicBool = AtomicBool::new(false);
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) { ALLOCATIONS.fetch_add(1, Ordering::Relaxed); }
        System.alloc(layout)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) { ALLOCATIONS.fetch_add(1, Ordering::Relaxed); }
        System.alloc_zeroed(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { System.dealloc(ptr, layout) }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) { ALLOCATIONS.fetch_add(1, Ordering::Relaxed); }
        System.realloc(ptr, layout, size)
    }
}
#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn fixture(definitions: usize, copies: usize) -> (Body, CellularProgram) {
    let products: Vec<_> = (0..definitions).map(|i| ProductDef {
        name: format!("Product{i}"), kind: ProductKind::Scaffold, half_life: 0.0,
    }).collect();
    let assemblies = (0..definitions).map(|i| AssemblyDef {
        name: format!("Assembly{i}"), kind: AssemblyKind::Mechanism(MechanismKind::Support),
        parts: (0..3).map(|j| ((i + j) % definitions, 0.04 / 3.0)).collect(),
        at: 0.0, direction: 0.0,
    }).collect();
    let program = CellularProgram { products, assemblies };
    program.validate().unwrap();
    let (mut body, _) = Body::founder();
    body.area = definitions as f64;
    let mut state = CellularState::default();
    state.pools = program.products.iter().enumerate().map(|(i, product)| ProductPool {
        product: product.clone(), mass: (i + 1) as f64 * 0.0001,
        embodied_energy: (i + 1) as f64 * 0.001,
        expression_requested: 0.0, expression_rate: 0.0,
    }).collect();
    for copy in 0..copies {
        for def in &program.assemblies {
            let parts: Vec<_> = def.parts.iter().map(|&(p, mass)| RecipePart {
                product: program.products[p].clone(), required_mass: mass,
            }).collect();
            state.assemblies.push(AssemblyInstance {
                id: state.next_assembly_id,
                recipe: Recipe { name: def.name.clone(), kind: def.kind,
                    parts: parts.clone(), at: def.at, direction: def.direction },
                ingredients: parts.iter().map(|p| Ingredient {
                    mass: p.required_mass * if copy == copies - 1 { 0.75 } else { 1.0 },
                    embodied_energy: 0.0,
                }).collect(),
                integrity: 1.0 - copy as f64 * 0.02, activity: 1.0,
            });
            state.next_assembly_id += 1;
        }
    }
    body.cellular = Some(state);
    (body, program)
}

// Exact previous input preparation, including full physical-recipe matching.
fn previous(body: &Body, program: &CellularProgram) -> (Vec<f64>, Vec<f64>) {
    let products = program.products.iter().map(|def| body.cellular.as_ref()
        .and_then(|s| s.pools.iter().find(|pool| &pool.product == def))
        .map_or(0.0, |pool| pool.mass)).collect();
    let assemblies = program.assemblies.iter().map(|def| body.cellular.as_ref().map_or(0.0, |state| {
        state.assemblies.iter().filter(|a| a.recipe.name == def.name && a.recipe.kind == def.kind
            && a.recipe.at == def.at && a.recipe.direction == def.direction
            && a.recipe.parts.len() == def.parts.len()
            && a.recipe.parts.iter().zip(&def.parts).all(|(part, &(i, mass))|
                part.required_mass == mass && program.products.get(i) == Some(&part.product)))
            .map(AssemblyInstance::functional_mass).sum()
    })).collect();
    (products, assemblies)
}
fn elapsed(mut run: impl FnMut(), iterations: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iterations { run(); }
    start.elapsed().as_secs_f64() * 1e6 / iterations as f64
}
fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}
fn count(mut run: impl FnMut()) -> u64 {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
    run();
    COUNTING.store(false, Ordering::Relaxed);
    ALLOCATIONS.load(Ordering::Relaxed)
}
fn main() {
    let iterations = std::env::args().nth(1).and_then(|n| n.parse().ok()).unwrap_or(20000);
    assert!(iterations > 0);
    println!("{{\"scope\":\"native cellular input projection; warmed identity caches and output buffers\",\"samples\":7,\"iterations\":{iterations},\"workloads\":[");
    for (workload, definitions) in [16, 64].into_iter().enumerate() {
        let (body, program) = fixture(definitions, 4);
        let mut products = Vec::new();
        let mut assemblies = Vec::new();
        body.cellular_inputs_into(&program, &mut products, &mut assemblies);
        let (old_products, old_assemblies) = previous(&body, &program);
        assert_eq!(products.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), old_products.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
        assert_eq!(assemblies.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), old_assemblies.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
        let old_allocations = count(|| { black_box(previous(black_box(&body), black_box(&program))); });
        let allocations = count(|| { body.cellular_inputs_into(black_box(&program), &mut products, &mut assemblies); });
        let mut old_samples = Vec::new();
        let mut new_samples = Vec::new();
        for sample in 0..7 {
            let old = || elapsed(|| { black_box(previous(black_box(&body), black_box(&program))); }, iterations);
            let mut current = || elapsed(|| {
                black_box(&body).cellular_inputs_into(black_box(&program), &mut products, &mut assemblies);
                black_box((&products, &assemblies));
            }, iterations);
            if sample % 2 == 0 { old_samples.push(old()); new_samples.push(current()); }
            else { new_samples.push(current()); old_samples.push(old()); }
        }
        let old_us = median(old_samples);
        let new_us = median(new_samples);
        println!("{}{{\"products\":{definitions},\"recipes\":{definitions},\"instances\":{},\"previousMedianUs\":{old_us:.6},\"cachedMedianUs\":{new_us:.6},\"speedup\":{:.6},\"previousAllocations\":{old_allocations},\"cachedAllocations\":{allocations}}}",
            if workload > 0 { "," } else { "" }, definitions * 4, old_us / new_us);
    }
    println!("]}}");
}
