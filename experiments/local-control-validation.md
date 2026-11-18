# Local control implementation: validation

## Artifact and compatibility

- Engine: `engine/pkg/agar_engine.wasm`, **778,448 bytes**.
- SHA-256: `bdce9ddb422fc51721bbf692c5a15fa527d3b240de08986ea0012286cef1ff42`.
- The production asset `docs/assets/agar_engine-BRJVugcb.wasm` is byte-identical.
- ABI **10**; language schema **3**, with additive cellular metadata.
- New checkpoints use format **7**. The decoder accepts formats **3–7**.

## Tests checked

The final native test log records **246 passed, 0 failed, 0 ignored**, finishing
in 17.61 seconds. Separate final logs report passing worker, session, storage
and live program-inspection checks.

The final production build, cellular UI checks, actual-WASM cellular scenarios,
and WASM/loader/artifact checks also passed. All eight current presets completed
their funded first division. The local-control example passed bright-to-shade
motor teardown, removal of the old motor records, brighter-light regrowth and
exact save continuation. These controlled scenarios establish a funded lifecycle,
not long-term ecosystem stability.

The browser compiled and saved `examples/local-control.cell` as a valid program
with seven products, seven genes, six mechanisms and two functions. A fresh
one-cell run built all six machines. Reducing ambient light to 10% removed both
motors; the harvester, two receptors and recycler remained installed, while the
motor recipes showed paused starts and an 80% recycle request. The inspector
showed no installed components in the removed motor recipe. Screenshots of the
checked source and resulting body are in `screenshots/`.

The native suite includes the following focused coverage:

- Named receptors sample their physical attachment points, rotate and wrap with
  the body, respect optical occlusion and integrity, and use the previous
  committed activity snapshot. Unreachable receptor expressions are not sampled.
- Initiation gates prevent new commitments while existing partial assemblies
  continue. Activity, initiation and recycling read the same observation snapshot.
- Pure functions enforce typing, lexical capture, declaration order, recursion
  rejection and bounded expansion. Concentrations dilute with cell area; response
  helpers have defined bounded endpoints.
- Recycling requires complete, active machinery and paid resources. Tests check
  material and energy accounting, original ingredient identities, full inherited
  stock tables and next-tick availability of recovered ingredients. Fractional
  effort completely removes the selected physical record without discarding an
  unaccounted residue.
- Surface weapons, shields and damage obey local geometry. Mixed legacy/new
  bodies preserve independent legacy cortex regulation and weapon reach.
- Cached bindings and phase summaries match reference values, including mixed
  bodies and different cell areas. All seven nested input/control vectors retain
  capacity, clear stale values after program changes and preserve exact replay.
- Geometry mutations change one future placement or direction without retyping
  existing material. New control graphs and physical state round-trip through
  checkpoint 7.

Source tests: `engine/src/genome/control_tests.rs`,
`engine/src/cellular_control_tests.rs`, and
`engine/src/simulation/local_control_tests.rs`.

## Actual-WASM migration evidence

[`local-control-migration.json`](local-control-migration.json) records the current
artifact hash above and the previous checkpoint-6 artifact hash
`cbb776a40980d54d6324c16063ab14f4ab68226275ccf8dca1b6db35666cb0f5`.

All **eight** preserved mechanism presets were developed for **1,200 ticks
(40 seconds)** in the old engine. Every source remained byte-identical. Each
checkpoint imported with exact world/environment bytes and physical body state;
only newly added control-report diagnostic fields were omitted from cross-version
body comparison. Every migrated run then passed byte-exact checkpoint-7 restore
and **60 ticks** of continuation.

The unchanged Phototroph also matched the old engine across **91 sampled states
over 2,700 ticks (90 seconds)**, including **one division**, no deaths and a final
population of two. Comparisons covered world/environment bytes, physical bodies
and conserved totals.

Old/new future equality is intentionally not asserted for optical programs: the
previous zero-width optical bug has been corrected. Directional surface
interactions are also an intentional behavior change. Save migration preserves
the imported state; it does not promise the old erroneous future trajectory.

## Scoped performance result

[`cellular-bindings-benchmark.json`](cellular-bindings-benchmark.json) measures
**native cellular input projection only**, with warmed caches and output buffers.
Each result is the median of seven alternating samples of 20,000 calls.

| Products / recipes / instances | Previous | Cached | Speedup | Allocations per projection |
| --- | ---: | ---: | ---: | ---: |
| 16 / 16 / 64 | 4.758878 µs | 0.574690 µs | 8.280778× | 2 → 0 |
| 64 / 64 / 256 | 90.479047 µs | 2.656854 µs | 34.054958× | 2 → 0 |

The benchmark checks exact projection values before timing. These results do
**not** establish a whole-simulation, WASM or browser speedup. They exclude cache
rebuilds and allocations required to create physical records. The checks above
establish sampled replay and targeted invariants, not long-term ecosystem
stability or equivalence for every possible world.
