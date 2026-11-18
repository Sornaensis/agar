# Simulation performance

## Current cellular execution improvements

The checkpoint-7 implementation retains all **seven cellular input/control
vectors** across ticks: product amounts, built amounts and receptor readings on
input; gene expression, activity, initiation gates and recycling effort on
output. Reusing the outer tick workspace no longer discards these nested
allocations. Every vector is resized and reset when a cell or its program changes,
while all expressions continue to read the same immutable observation snapshot.

Derived identity tables bind program products to physical pools, recipe instances
to current definitions, and installed ingredients to stock. Identity includes the
complete product or recipe definition; a shared name cannot retype existing
material or take control of an inherited machine. Recycling deliberately uses a
separate name selector and still recovers each instance's original ingredients.
The tables rebuild when definitions or physical record structure change and are
excluded from checkpoints and biological equality.

A work-phase snapshot computes each machine's completion and mass once, then
reuses them for active capacities, storage, space, support, construction, repair
and recycling. Action preparation similarly replaces one body scan per legacy
family with one ordered traversal. These snapshots are local to a phase and are
never reused across construction, damage or division. The original floating-point
accumulation order is retained. Exact comparisons cover mixed legacy/cellular
bodies and non-unit cell areas.

### Measured input-projection cost

| Products / recipes / physical instances | Previous identity scans | Cached bindings and reused outputs | Speedup | Heap allocations per projection |
| --- | ---: | ---: | ---: | ---: |
| 16 / 16 / 64 | 4.759 µs | 0.575 µs | 8.28× | 2 → 0 |
| 64 / 64 / 256 | 90.479 µs | 2.657 µs | 34.05× | 2 → 0 |

These are **native release microbenchmarks of cellular input projection only**.
They compare the previous product/recipe scans with `cellular_inputs_into` on
identical stocks and complete/partial machinery. Results are medians of seven
20,000-call samples, alternating measurement order. The benchmark asserts
bit-exact projected values before timing. Allocation counts are measured
separately with warmed identity caches and output buffers.

The numbers do **not** establish a whole-simulation, gene-evaluation, WASM or
browser speedup. Creating physical records, rebuilding identity tables and other
engine paths can still allocate. New local sensing and directional interactions
also change the work performed by current programs, so comparing their new
trajectories with older presets would mix performance and behavior changes.
Raw measurements are in
[cellular-bindings-benchmark.json](experiments/cellular-bindings-benchmark.json).

### Current migration and unchanged-behavior evidence

The actual compiled WASM imports checkpoint-6 worlds for all eight preserved
mechanism programs after 40 seconds of paid development. Their source text,
immediate world/environment bytes and physical body state match exactly; only
new control-report diagnostic fields are omitted from body comparison. Each
upgraded checkpoint restores and continues for 60 ticks with byte-exact
checkpoint-7 replay.

A separate, unchanged Phototroph program matches the old engine in **91 sampled
world/environment/body states over 90 seconds, including one division**. Its
source has no optical receptors or directional surface interactions. Old/new
future equality is deliberately not asserted for optical programs: the previous
engine's zero-width optical bug is now fixed. Results and both WASM hashes are in
[local-control-migration.json](experiments/local-control-migration.json).

Reproduce with:

```sh
cargo run --release --manifest-path engine/Cargo.toml --example cellular-bindings -- 20000
node scripts/check-local-control-migration.mjs --baseline /path/to/cellular-v6/engine/pkg/agar_engine.wasm --output experiments/local-control-migration.json
```

The migration script also checks the old checkout's `src/presets.ts` to verify
that every preserved source is byte-identical. Implementations are in
[cellular.rs](engine/src/cellular.rs), [genome/cellular.rs](engine/src/genome/cellular.rs)
and [simulation.rs](engine/src/simulation.rs).

## Earlier material allocation improvement (checkpoint 6)

The preceding pass introduced **five common costs and one optional product stock**
per cellular work request. Previously every synthesis, ingredient installation, repair or lifecycle
request allocated a vector containing all five common resources and every product
pool, even when almost all entries were zero. The allocator now visits just the
common costs and the one stock consumed by an installation operation. It preserves
the order of demands and floating-point additions used by the previous solver.

The world's tick workspace reuses the demand list, product-to-pool mapping, stock
and embodied-energy snapshots, and allocation result/active/resource buffers.
The same workspace serves cells sequentially; it is excluded from checkpoints.
The solver clears its requirement buffer between progressive-filling rounds
instead of allocating a new one each round. It still shares finite resources by
requested completion fraction, and blocks only requests that consume an exhausted
resource. Newly synthesized stock and newly freed room cannot fund construction
in the same tick.

### Measured material-planning cost

| Workload | Previous dense allocation | Sparse allocation | Speedup | Heap allocations per call |
| --- | ---: | ---: | ---: | ---: |
| 16 product pools, 66 requests | 14.301 µs | 2.958 µs | 4.835× | 74 → 0 |
| 64 product pools, 258 requests | 110.230 µs | 11.168 µs | 9.870× | 265 → 0 |

These are native release microbenchmarks of **request construction and material
allocation**, using the same requests and resource limits. Each workload has
synthesis, competing ingredient installations, growth and replication requests,
including unavailable and limited ingredients. Values are medians of seven
20,000-call samples, with measurement order alternating. Allocation counters are
read in a separate call and disabled during timing. Sparse buffers are warmed;
zero allocations applies to this allocator path after capacity is sufficient.
Creating actual products or machinery can still allocate physical-state records.

This does **not** establish an overall simulation, WASM or browser speedup. Current
presets also changed their physical machinery, so comparing their full runs with
the old presets would mix performance and biological changes. The benchmark
asserts bit-exact results before timing. Raw measurements are in
[material-allocation-benchmark.json](experiments/material-allocation-benchmark.json).

Four targeted native tests check missing-ingredient independence, fair completion
fractions, resource bounds, scratch reuse across different cells, and bit-exact
agreement with the prior dense solver across 192 workloads with 0–1,024 pools.
That pass's actual-WASM compatibility check used the eight preserved cellular
family sources, comparing complete world/environment bytes and body inspection,
then imports a v5 save into v6 and checks its future and current-format replay.
Save files themselves are not compared across differing format versions. The
actual-WASM run passed **91 sampled old/new states for each of all eight old
presets** over 90 simulation seconds, including one birth per preset. The
30-second v5 checkpoints migrated successfully, matched the next 60 seconds of
the previous engine's trajectory, and replayed as byte-exact v6 checkpoints.
Binary hashes and scenario results are recorded in
[cellular-compatibility.json](experiments/cellular-compatibility.json).

Reproduce with:

```sh
cargo run --release --manifest-path engine/Cargo.toml --example material-allocation -- 20000
node scripts/check-cellular-compatibility.mjs --baseline /path/to/cellular-v5/engine/pkg/agar_engine.wasm --output experiments/cellular-compatibility.json
```

The new solver is in [material_allocation.rs](engine/src/material_allocation.rs),
its cellular integration in [cellular.rs](engine/src/cellular.rs), and its
reproducible benchmark in
[material-allocation.rs](engine/examples/material-allocation.rs).
The v5→v6 comparison above is historical and requires the corresponding v6 source
checkout; the current migration script and v7 evidence are documented above.

## Historical architecture measurements

The remaining measurements describe the previous optimization pass and its
legacy programs/checkpoint-4 biology, not the current mechanism presets.
Reproducing those cross-version byte comparisons requires matching source
versions: later checkpoints add cellular state and mechanism definitions.
The current harness without `--baseline` can measure current work. The first
cellular implementation's development results remain recorded in
[documented validation](experiments/cellular-validation.json).

## What changed

The engine keeps its fixed 1/30-second timestep, numerical operation order,
resource arbitration, and existing physical rules. This second pass implements
the four architectural directions identified in the first pass.

1. **Reusable tick storage and explicit phases.** Inputs, outputs, capabilities,
   tracking, resource requests, photon footprints, predation events, emissions,
   spatial buckets and candidate lists retain their allocations. Observation
   data is immutable while genomes evaluate. Geometry rebuilds after movement,
   upkeep and digestion; collision pairs update cached and canonical positions
   together while preserving the original frozen collision grid and pair order.
   Later interaction passes index the settled positions. Construction, division
   and compaction end that geometry's validity. Scratch is absent from saves,
   starts empty on restore/reset, and survives rejected tick preflights.
   Organic and thermal advection share exact same-time patch-center flow samples;
   mineral transport retains its distinct face samples.
2. **Separate presentation from inspection.** Drawing exports 21 doubles per cell
   instead of the legacy 158. Particles still use five doubles each. Metrics are
   a separate aggregate projection; a selected cell retains the complete legacy
   row. Organs, memory and program inspection are requested separately. The UI
   requests metrics and environmental overlays on their 250 ms cadence when
   relevant, instead of reconstructing them with every frame.
3. **One worker owns each simulation.** Engine commands, inspection and checkpoint
   capture execute in order inside a dedicated module worker. Typed arrays and
   checkpoints transfer to the UI. The animation loop allows one advancing batch
   at a time, with batches capped at eight ticks; painting uses the last completed
   frame. Checkpoint bytes and summary describe the same tick, and save revisions
   cannot mark newer changes saved. This separates simulation work from UI work;
   it does not parallelize the numerical engine.
4. **Derived execution and compact spatial data.** A compiled genome plan keeps
   dependencies of outputs, register writes and guards, skips unused bindings,
   aliases references and folds constant-only subgraphs using the same typed
   operations. The authored graph remains intact for inspection, mutation and
   checkpoints. All register reads still see old memory; clears still win.
   Contiguous geometry/capability arrays feed sensing, occlusion, collisions,
   capture and contact targeting. Bodies and registers remain canonical cell
   data; there is no wholesale entity-model replacement.

The earlier body summaries, cheaper division feasibility checks, gated sensing,
shared evaluator buffers and construction-buffer reuse remain in place.

Sources: [simulation](engine/src/simulation.rs), [fields](engine/src/environment.rs),
[execution plan](engine/src/genome/execution.rs),
[presentation projections](engine/src/simulation/presentation.rs),
[worker](src/simulation-worker-runtime.ts), [session](src/session.ts).

## Measurements against the first optimized version

These comparisons are **phase one → this architecture pass**, not the original
upload → final. The original comparison is preserved in
[phase1-performance.json](experiments/phase1-performance.json).

| Workload | Phase one (ms/tick) | Current (ms/tick) | Observed tick-time change |
| --- | ---: | ---: | ---: |
| Mixed ecosystem: 84 founders | 5.124 | 4.430 | 13.5% less |
| 400 producers | 11.010 | 10.965 | 0.4% less |
| 80 foragers; 10,000 initial particles | 29.297 | 30.094 | 2.7% more |
| Fields only | 2.010 | 1.917 | 4.6% less |

The mixed workload improves clearly in these samples. Producer throughput is
essentially unchanged. The particle-heavy median is slightly slower, with strongly
overlapping samples: baseline 28.56–32.72 ms and current 28.01–30.33 ms. This run
establishes no additional particle-throughput improvement. The field-only case has
no organic material, so its small difference does not isolate the shared-flow
optimization.

### Presentation cost and size

The following compares **current legacy world export → current compact drawing
export**. Environmental data has a separate cadence and is excluded from this
particular comparison.

| Workload | Rust export (ms) | Payload (bytes) |
| --- | ---: | ---: |
| Mixed ecosystem | 0.0460 → 0.0150 | 111,776 → 18,616 |
| 400 producers | 0.1775 → 0.0581 | 517,536 → 79,136 |
| Particle-heavy | 0.0950 → 0.0674 | 483,176 → 395,496 |

The cell-dense frame is about 85% smaller including particles. Particle-dense
frames shrink less because particles retain their full representation. Metrics
use 312–432 bytes in these populated worlds. A selected cell uses 1,264 bytes;
its measured export is below 0.001 ms when selecting the first living row.
Selecting another ID additionally incurs the existing linear lookup. Complete
per-export samples and payloads are in
[simulation-performance.json](experiments/simulation-performance.json).

An optional synthetic workload gives 80 cells a program with **512 unused dynamic
bindings**. Its tick time falls from **13.383 to 0.371 ms (36.1×)** over three
100-tick samples. This demonstrates eliminating dormant graph work, not an
expected gain for ordinary presets. Its independent results are in
[simulation-dormant-graph.json](experiments/simulation-dormant-graph.json).

### Method and limits

The harness uses release WASM, Node 24.22.1/V8 with `--no-liftoff`, seed 42, the same
initial worlds and five repeated 300-tick windows. Engine order alternates.
Setup, compilation/restoration and validation are outside tick timers. Export
medians use 30 calls per sample. Raw samples, binary hashes, final populations,
checkpoint hashes and payload sizes are recorded in the reports.

These timers cover WASM computation and export calls. They exclude JavaScript
object construction/copying, worker scheduling/transfers, browser painting and
input latency. They therefore do not measure the worker's responsiveness benefit
or establish a browser FPS improvement. Small timing differences need caution.

Derived plans consume memory. The live `genomeBytes` diagnostic now includes that
storage; for the mixed run it changes from 199,058 to 293,010 bytes. The live
registry budget counts this cost. Reusable buffers retain capacity until reset
or disposal, exchanging retained storage for fewer allocations.
The checkpoint format remains version 4, but a previous save or registry already
near the existing 64 MiB live genome limit can now be rejected or reach the
mutation limit earlier. Exact checkpoint equivalence in these measurements is
for worlds below that limit.

## Correctness and validation

- **181 native tests pass**, including execution-plan equivalence, synchronous
  register semantics, accounting, packing, sensing and checkpoint replay.
- All four primary workloads and the dormant-graph workload match **complete
  canonical checkpoint bytes** at sampled warm-up boundaries and after every
  measured trajectory. The mixed run includes nine births, eight deaths and two
  mutations.
- Legacy world/environment buffers match exactly. Compact rendering, selected
  detail and summary exports match their legacy projections. Metrics compare
  every field; only header index 13, `genomeBytes`, is normalized for the known
  runtime-storage accounting change. No checkpoint or physical state is
  normalized. Export calls are verified to leave checkpoint bytes unchanged.
- Reused workspace matches fresh checkpoint restores through population changes,
  death and reset. Buffer-retention and failed-preflight tests pass.
- Actual worker integration tests and asynchronous session tests pass, including
  ordered commands, same-tick inspection/capture, partial failures and retries.
- WASM/loader/artifact, inspection, language, graph, storage, food-chain and
  navigation checks pass. TypeScript and the final Pages production build pass.
- Browser smoke execution was blocked by this environment's browser/socket
  restrictions. The included browser test was not counted as passed.

This is sampled trajectory equivalence plus targeted invariant tests, not proof
for every possible world or cross-platform floating-point identity.

## Reproduce

Build both versions with the same Rust toolchain, then run:

```sh
npm run build:wasm
npm run bench:simulation -- --baseline /path/to/phase1/engine/pkg/agar_engine.wasm --output experiments/comparison.json --samples 5 --ticks 300
npm run bench:simulation -- --baseline /path/to/phase1/engine/pkg/agar_engine.wasm --scenario dormant-graph --samples 3 --ticks 100 --output experiments/dormant-comparison.json
```

`--scenario` also accepts `mixed-ecosystem`, `dense-producers`, `particle-heavy`
and `dynamic-fields`. Without it, the four ordinary workloads run. Without
`--baseline`, the harness measures the current engine and checks its projections.

## Next performance work

Profile current phase time and candidate/allocation counts before another broad
rewrite. Concrete remaining candidates are arbitrary-position particle flow
sampling and optical candidate/occlusion work as local density rises. Measure
named receptor workloads explicitly, since these now sample actual attachment
locations. Keep accumulation, arbitration and torus semantics exact; microbenchmarks
of cached inputs or allocation do not establish whole-world tick performance.
