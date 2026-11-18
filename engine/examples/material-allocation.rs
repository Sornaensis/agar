//! Run with: cargo run --release --example material-allocation -- 20000
//! Measures only request construction and material allocation, using exactly the
//! same inputs for the previous dense algorithm and the new sparse allocator.
//! This is deliberately not an end-to-end simulation speed claim.

#[allow(dead_code)]
#[path = "../src/material_allocation.rs"]
mod material_allocation;

use material_allocation::{AllocationScratch, MaterialDemand};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

struct CountingAllocator;
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static COUNT_ALLOCATIONS: AtomicBool = AtomicBool::new(false);
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        System.alloc(layout)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if COUNT_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        System.alloc_zeroed(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNT_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        System.realloc(ptr, layout, size)
    }
}
#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[derive(Clone)]
struct DenseDemand {
    amount: f64,
    costs: Vec<f64>,
}

fn old_dense(templates: &[MaterialDemand<usize>], common: [f64; 5], stocks: &[f64]) -> Vec<f64> {
    let mut resources = vec![0.0; 5 + stocks.len()];
    resources[..5].copy_from_slice(&common);
    resources[5..].copy_from_slice(stocks);
    let demands: Vec<_> = templates
        .iter()
        .map(|demand| {
            let mut costs = vec![0.0; resources.len()];
            costs[..5].copy_from_slice(&demand.common);
            if let Some(stock) = demand.stock {
                costs[5 + stock] = 1.0;
            }
            DenseDemand {
                amount: demand.amount,
                costs,
            }
        })
        .collect();
    let mut result = vec![0.0; demands.len()];
    let mut active = vec![true; demands.len()];
    for _ in 0..=resources.len() {
        let mut needed = vec![0.0; resources.len()];
        for (index, demand) in demands.iter().enumerate() {
            if active[index] {
                for (r, value) in needed.iter_mut().enumerate() {
                    *value += (demand.amount - result[index]) * demand.costs[r];
                }
            }
        }
        if needed.iter().all(|&value| value <= 0.0) {
            break;
        }
        let scale = needed
            .iter()
            .enumerate()
            .filter(|(_, n)| **n > 0.0)
            .map(|(r, &n)| resources[r] / n)
            .fold(1.0, f64::min)
            .clamp(0.0, 1.0);
        for (index, demand) in demands.iter().enumerate() {
            if active[index] {
                let added = (demand.amount - result[index]) * scale;
                result[index] += added;
                for (r, resource) in resources.iter_mut().enumerate() {
                    *resource = (*resource - added * demand.costs[r]).max(0.0);
                }
            }
        }
        if scale >= 1.0 - f64::EPSILON {
            break;
        }
        let mut froze = false;
        for (index, demand) in demands.iter().enumerate() {
            if active[index]
                && (0..resources.len())
                    .any(|r| needed[r] > 0.0 && resources[r] <= 1e-12 && demand.costs[r] > 0.0)
            {
                active[index] = false;
                froze = true;
            }
        }
        if !froze {
            break;
        }
    }
    result
}

fn workload(pools: usize) -> (Vec<MaterialDemand<usize>>, [f64; 5], Vec<f64>) {
    let mut demands = Vec::new();
    for pool in 0..pools {
        demands.push(MaterialDemand::new(
            pool,
            0.002,
            [40.0 + (pool % 5) as f64 * 10.0, 1.0, 1.0, 0.0, 0.6],
        ));
        for part in 0..3 {
            let mut install = MaterialDemand::new(
                pool,
                0.01 + part as f64 * 0.005,
                [
                    30.0,
                    0.0,
                    if pool % 3 == 0 { 0.5 } else { 0.0 },
                    (pool % 4) as f64 * 0.3,
                    0.4,
                ],
            );
            install.stock = Some(pool);
            demands.push(install);
        }
    }
    demands.push(MaterialDemand::new(0, 0.006, [80.0, 1.0, 0.0, 0.0, 1.0]));
    demands.push(MaterialDemand::new(0, 0.002, [80.0, 1.0, 1.0, 0.0, 1.0]));
    let stocks = (0..pools)
        .map(|pool| [0.0, 0.003, 0.012, 0.03][pool % 4])
        .collect();
    (demands, [10.0, 0.02, 0.2, 0.08, 0.04], stocks)
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn time(iterations: usize, mut run: impl FnMut() -> f64) -> f64 {
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(run());
    }
    start.elapsed().as_secs_f64() * 1e9 / iterations as f64
}

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .map(|s| s.parse::<usize>().expect("iterations must be a number"))
        .unwrap_or(20_000)
        .max(1);
    let samples = 7;
    println!("{{\"scope\":\"native request construction and material allocation; excludes the rest of the simulation\",\"samples\":{samples},\"iterationsPerSample\":{iterations},\"workloads\":[");
    for (case, pools) in [16, 64].into_iter().enumerate() {
        let (templates, common, stocks) = workload(pools);
        let mut scratch = AllocationScratch::default();
        let mut requests = Vec::with_capacity(templates.len());
        requests.extend_from_slice(&templates);
        let expected = old_dense(&templates, common, &stocks);
        let actual = scratch.allocate(&requests, common, &stocks);
        assert_eq!(
            expected.iter().map(|n| n.to_bits()).collect::<Vec<_>>(),
            actual.iter().map(|n| n.to_bits()).collect::<Vec<_>>()
        );
        ALLOCATIONS.store(0, Ordering::Relaxed);
        COUNT_ALLOCATIONS.store(true, Ordering::Relaxed);
        black_box(old_dense(&templates, common, &stocks));
        let dense_allocations = ALLOCATIONS.load(Ordering::Relaxed);
        ALLOCATIONS.store(0, Ordering::Relaxed);
        requests.clear();
        requests.extend_from_slice(&templates);
        black_box(scratch.allocate(&requests, common, &stocks));
        let sparse_allocations = ALLOCATIONS.load(Ordering::Relaxed);
        COUNT_ALLOCATIONS.store(false, Ordering::Relaxed);
        assert_eq!(
            sparse_allocations, 0,
            "warmed material allocation must not allocate"
        );
        let mut dense_ns = Vec::new();
        let mut sparse_ns = Vec::new();
        for sample in 0..samples {
            // Alternate measurement order to reduce persistent warmup bias.
            for dense in if sample % 2 == 0 {
                [true, false]
            } else {
                [false, true]
            } {
                if dense {
                    dense_ns.push(time(iterations, || {
                        old_dense(black_box(&templates), black_box(common), black_box(&stocks))
                            .iter()
                            .sum()
                    }));
                } else {
                    sparse_ns.push(time(iterations, || {
                        requests.clear();
                        requests.extend_from_slice(black_box(&templates));
                        scratch
                            .allocate(&requests, black_box(common), black_box(&stocks))
                            .iter()
                            .sum()
                    }));
                }
            }
        }
        let dense_median = median(&mut dense_ns);
        let sparse_median = median(&mut sparse_ns);
        println!("{}{{\"productPools\":{pools},\"requests\":{},\"oldDenseNsMedian\":{dense_median:.1},\"sparseNsMedian\":{sparse_median:.1},\"speedup\":{:.3},\"oldHeapAllocationsPerCall\":{dense_allocations},\"sparseHeapAllocationsAfterWarmup\":{sparse_allocations},\"bitExact\":true}}", if case == 0 { "" } else { "," }, templates.len(), dense_median / sparse_median);
    }
    println!("]}}");
}
