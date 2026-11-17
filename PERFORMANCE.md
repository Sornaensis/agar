# Simulation performance

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

Profile phase time and candidate/allocation counts before another broad rewrite.
The remaining concrete candidates are organ construction's repeated family scans
and resource-allocation loop, arbitrary-position particle flow sampling, and
optical candidate/occlusion work as local density rises. Keep their accumulation,
arbitration and torus semantics explicit. The producer and particle results show
why these workloads need their own measurements: cheaper presentation, dormant
graph execution and a worker do not automatically make every physical tick faster.
