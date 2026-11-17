# Simulation performance

## Changes made

This pass removes repeated work while preserving the fixed 1/30-second timestep,
phase order, physical rules, mutation behavior, and checkpoint/ABI formats.

- **One organ scan for family signals.** `Body::family_summaries()` replaces the
  80 separate family scans previously needed for each cell's genome inputs. The
  render snapshot also uses these summaries and derives capacities from them.
  Per-family accumulation order and empty-family signed zeros are preserved.
- **Reuse immutable capabilities and appearance.** A tick's initial capacities
  serve allocation preflight, sensing and the existing action phases. Neighbor
  appearance is calculated once per observed cell, rather than for every observer.
  These values are local to the phase, so damage or construction cannot leave a
  persistent cache stale.
- **Avoid unused sensing work.** Light and heat gradients require their matching
  sensors. Spatial sensing indexes are unnecessary when no relevant observer has
  optical or mechanical hardware. Particle velocity is sampled only for the
  returned contact/optical observations, since motion does not affect selection.
- **Retain temporary allocations.** Field solves and genome evaluation reuse
  storage across ticks. Construction reuses index/build/recycle vectors across
  families; collision and secretion queries reuse candidate vectors across their
  loops. Three organic decay factors are evaluated once per tick.
- **Cheaper readiness checks.** Division feasibility shares the actual packing
  algorithm without constructing daughter bodies or cloning organ records. The
  packing index vector still allocates when packing is needed.
- **Keep integer operations integer.** The evaluator uses direct conversions and
  saturating arithmetic instead of redundant float roundtrips; scratch reservation
  now uses the correct `Vec::reserve` semantics.

Implementation: [body](engine/src/body.rs), [simulation](engine/src/simulation.rs),
[fields](engine/src/environment.rs), and [genomes](engine/src/genome.rs).

## Measurements

| Workload | Before (ms/tick) | After (ms/tick) | Tick time change | Snapshot export before → after (ms) |
| --- | ---: | ---: | ---: | ---: |
| Mixed ecosystem (84 founders) | 5.50 | 5.22 | -5.1% | 0.765 → 0.681 |
| 400 producers | 11.68 | 11.22 | -3.9% | 0.753 → 0.432 |
| 80 foragers; 10,000 initial particles | 38.47 | 28.31 | -26.4% | 0.437 → 0.347 |
| Fields only (heat, flow, mineral sources) | 1.99 | 2.01 | +1.2% | 0.600 → 0.616 |

Negative changes mean less time per tick. The particle-heavy improvement is the
largest result. The approximately 1% field-only difference is smaller than the
run-to-run spread; this pass does not establish a field-solver throughput gain.
The 4–5% population-workload gains are modest. Dense-world snapshot export
uses about 43% less time.

The included [comparison harness](scripts/benchmark-simulation.mjs) measures release
WASM engines compiled with the same Rust toolchain. It uses seed 42, naturally
grown cells, five repeated 300-tick windows, alternating engine order and a warm-up
trajectory. Node runs with `--no-liftoff` to use the optimizing WASM compiler from
the start rather than timing transitions between compilation tiers. Raw samples,
engine hashes, final populations and runtime details are in
[simulation-performance.json](experiments/simulation-performance.json).

The timer covers WASM tick batches, excluding setup, checkpoint serialization,
JavaScript snapshot-object construction and rendering. Rust world/environment
snapshot exports have a separate timer. These results describe these workloads
on this machine, not a browser FPS guarantee or a prediction for arbitrarily
large ecosystems. Small percentage differences should be treated cautiously.

## Correctness checks

- All 170 original native tests passed before editing; all 172 native tests pass
  after the changes, including new summary-bit-equivalence and field-scratch
  restore/reset regressions.
- Actual WASM integration and loader/artifact checks pass, including checkpoint
  replay, mutations, resource accounting, legacy-save handling and 2,049-cell
  restoration.
- Live program inspection, food-chain development/division and resistant-contact
  navigation tests pass against the rebuilt WASM.
- All four benchmark scenarios match sampled complete checkpoints and render
  buffers exactly. The mixed run includes nine births, eight deaths and two
  mutations, rather than only static populations.
- TypeScript checking and the existing frontend production build pass. The source
  package includes the updated engine and matching generated site assets.

The comparison checks complete checkpoint bytes every 120 warm-up ticks and at
the end of the trial/measured intervals, plus exact world/environment render
buffers. This tests substantially more state than population totals, but is
sampled trajectory equivalence rather than a proof for every possible world.
Scratch storage is omitted from checkpoints and starts empty when restoring.

To reproduce:

```sh
npm run build:wasm
npm run bench:simulation -- --baseline /path/to/original/engine/pkg/agar_engine.wasm --output experiments/comparison.json
```

Rebuild the original engine with the same compiler before comparing. Optional
arguments are `--scenario mixed-ecosystem|dense-producers|particle-heavy|dynamic-fields`,
`--samples N` and `--ticks N`. Without `--baseline`, the harness measures the current
engine alone.

## Architecture and recommended direction

The existing structure is a reasonable foundation. Rust owns a `World` containing
cell and particle vectors, dense environmental fields, per-cell organelle vectors
and sparse typed registers. Genomes are immutable, topologically ordered
expression graphs. A wrapped 20×13 spatial grid narrows interaction queries.

Each tick updates fields and particle decay, freezes observations and capabilities,
evaluates genomes and commits memory, then performs movement/upkeep, digestion,
mineral uptake, collisions, feeding, photosynthesis, predation, construction,
division and death. Several actions intentionally use capabilities from the start
of the tick. New organs cannot retroactively act earlier in that tick.

### 1. Extend reusable storage and explicit phase summaries

Give the tick a reusable workspace for remaining inputs, outputs, request lists
and spatial buckets. Define when each summary/index is valid. Rebuild after
movement, collision displacement or population compaction where necessary; retain
bucket capacity even when contents must change. Keep stable arbitration and
floating-point accumulation order. Measure phase times, allocation counts,
spatial candidate counts and occlusion tests to choose the next target.

For environmental fields, organic and thermal advection evaluate flow at the same
patch centers and time. Sharing those exact evaluations is a possible next step.
Mineral fluxes use edge positions, and rendering uses a later simulation time, so
those flow samples cannot all share one indiscriminate cache.

### 2. Separate presentation data from detailed inspection

`SimulationSession.advance()` currently refreshes the entire world after every
batch. `Engine.snapshot()` reads 158 doubles per cell, builds 16 organelle objects
per cell, reconstructs every particle and environmental sample, and recomputes
resource totals. Most of that is unnecessary for drawing a frame.

Use a compact rendering snapshot for positions and visible appearance; refresh
metrics on their existing 250 ms UI cadence, and request organs, memory and
program values only for the selected cell. Keep complete checkpoints for saving.
The relevant code is [engine.ts](src/engine.ts), [session.ts](src/session.ts) and
[app.ts](src/app.ts). No frontend behavior was changed in this pass.

### 3. Move engine ownership to a Web Worker

The main animation callback synchronously runs up to 30 ticks before rendering.
Moving simulation and checkpoint serialization to a dedicated worker would let
controls and painting respond while the engine works. Keep commands ordered,
transfer bounded snapshots, and apply backpressure. This improves responsiveness;
it does not inherently reduce the computation required by a tick. Preserve the
30 Hz numerical timestep independently of display frequency.

### 4. Optimize execution and layout where measurements justify it

The genome evaluator already walks a flat graph. A later derived execution plan
could skip unused bindings and precompute constant-only expressions, while keeping
the original graph intact for mutation, source spans and inspection. Output roots,
register values and guards must retain their synchronous semantics.

For large populations, separate frequently accessed geometry/capabilities into
contiguous arrays before considering a wholesale data-model rewrite. Profile
optical candidates and occlusion searches as density rises. Independent sensing,
genome evaluation and field preparation are possible parallel stages; resource
arbitration, collisions and predation require deliberate deterministic resolution.
These are follow-up directions, not implemented changes or promised speedups.
