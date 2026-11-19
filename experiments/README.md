# Ecosystem measurements

[Curated ecology](curated-ecology.md) records the current predator investigation and preset outcomes. Older runs below used the catalogs and engines identified in each section.

## Production and adhesion

[Production measurements](production-chains.json) summarize the tick-by-tick material and ATP checks in a closed, dark founder assay, the outcomes of four single-gene knockouts, and the first functional machinery in each playable founder. The source check covers all 227 assembly recipes in the 16 shipped examples. [Costs and methods](../README.md#what-it-costs-to-build-a-capability)

## Performance update — ABI 13, checkpoint 9

[performance-abi13.json](performance-abi13.json) records ten before/after engine
benchmarks with identical checkpoint and physical projection bytes.
[presentation-performance.json](presentation-performance.json) measures the actual
periodic-image helper with ordered-image equivalence checks.
[optimization-equivalence.json](optimization-equivalence.json) covers division,
mutation, runtime edits, resize and checkpoint continuation across ABI 12 and 13.
[optimization-validation.json](optimization-validation.json) records the 17
integration checks; all 306 native release tests also pass. Measurements and
limits are explained in [PERFORMANCE.md](../PERFORMANCE.md).

The ecological rules and the mixed-ecosystem limitations recorded below are
unchanged by this optimization pass.

## Gas exchange — ABI 12, checkpoint 9

`gas-exchange-seed42.json` records 600 simulated seconds with the ABI 12 gas
model and its authored genomes, using the original mixed population counts and
the built-in open Sunlit pond. No organic food is supplied. The report includes
carbon and oxygen inventories, external exchange, fixation, respiration and
decay alongside the existing mineral and energy checks. All four accounting
checks pass at every sample.

This run ends with 285 Phototrophs, no Foragers, two Mixotrophs and one Predator.
A Predator descendant is observed during the run; no Forager descendant is
observed. The consumer-persistence gate therefore remains unsatisfied, as it
was in the earlier records below. This is a measured ecosystem limitation,
not evidence of stable coexistence. Separate supplied-habitat lifecycle checks
verify that all eight then-current genomes can build, feed and reproduce under the
new chemistry.

## Earlier cellular food chain — ABI 10, checkpoint 7

The ABI 10 mixed-ecosystem regression used the sources and Sunlit pond environment shipped at the time, with 72 Phototrophs, 6 Foragers,
4 Mixotrophs and 2 Predators, with no initial or continuing external organic
food. Organisms grow their machinery from identical minimal founders.

Those fixtures tested staged consumer construction, hunting and tissue
digestion, recovery from a captured meal at low energy, and navigation around
producer surfaces. Real thermal-stress fixtures check paid repair, recovery of
the original machinery without redundant copies, and repair initiation before
navigation is complete. Native coverage also verifies tissue recovery from a
newborn that dies during division, whole-recipe inheritance, strictly external
waste release, and cellular repair/fission affordability independent of empty
storage. Current Forager fixtures cover tissue-funded reproduction, continued
search with a waste-filled gut, and paid escape and repair after injury;
Predator fixtures cover paid paired heat sensing, fed heat escape, repair and
tissue-funded reproduction. The ten-minute ecosystem gate requires both consumer
lineages to reproduce and remain alive at the endpoint. It counts distinct
descendants observed every simulated second; this is not an exact lifetime
birth counter. Recorded comparisons and their limits are in
[predation-validation.md](predation-validation.md). The final four runs retain
reproducing Predators, but each still fails at least one Forager survival or
reproduction requirement.

## Earlier protein/substrate model — ABI 10

Introduced organisms still share one minimal core. Programs now express paid family-specific proteins, regulate the stock, and assemble organs from it on later ticks. Organic matter distinguishes exudate, tissue and waste with separate gut pools and shared enzyme allocation. There is no five-minute age timer or fixed organic-particle count limit.

The current correctness gates exercise development, resource acquisition, reproduction, subtype conservation, uncapped particle saves and survival beyond the former age limit. Their results are reported by the scripts below. Historical timings and mixed-ecosystem counts are not expectations for this changed model.

## Historical genome-only introduction — ABI 9

Every introduced organism now starts with the same one-core cell and finite energy and mineral reserves. Its program constructs its optional organs during normal simulation ticks. There is no external body selection or genome-to-body mapping. Division partitions existing physical organs.

The focused food-chain checks verify identical initial bodies across all eight built-in genomes, paid development of functioning organs, real light/organic/mixed acquisition, and conserved material. With their unmodified programs, the Phototroph produces a daughter after 47.3 simulated seconds with light and no organic food; the Forager does so after 51.9 seconds with supplied nearby organic particles. These controlled lifecycle checks establish that both programs can develop, acquire resources, grow and reproduce. They do not establish persistence in the mixed ecosystem.

The navigation check grows the actual Forager's sensors and feeding organs from the same minimal cell, then verifies escape from a resistant surface and capture and digestion of nearby particles. No new long-running mixed-ecosystem measurement accompanies this initialization change. The ABI 8 records below remain historical results.

## Historical organelle model — ABI 8

`organelle-ecosystem-final-42.json`, `organelle-ecosystem-final-7.json` and `organelle-ecosystem-final-2026.json` record **300 simulated seconds**, with mutation disabled. Samples are at 0, 30, 120, 299 and 300 seconds. This covers one founder lifespan and visible descendant generations; it does not establish long-term persistence.

Each historical run starts with 72 Phototrophs, 6 Foragers, 4 Mixotrophs and 2 Predators in Sunlit pond. There is no initial or continuing external organic-food input. Those runs used selected starter-body recipes, recorded as explicit resource introductions. Later capabilities required paid construction; sensors exposed local physical measurements. Their scavenger programs distinguished tentative small-object returns from resistant contact and steered away from surfaces they could not capture. The initialization, programs and physical rules differ from ABI 10, so these counts are not predictions for the current build.

### Living lineage counts at 300 seconds

| Seed | Phototroph | Forager | Mixotroph | Predator | Total births | Total deaths |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 42 | 253 | 0 | 5 | 0 | 265 | 91 |
| 7 | 245 | 0 | 6 | 1 | 266 | 98 |
| 2026 | 202 | 0 | 11 | 1 | 234 | 104 |

### Highest generation visible at a sample

| Seed | Forager | Mixotroph | Predator |
| --- | ---: | ---: | ---: |
| 42 | 0 | 2 | 0 |
| 7 | 0 | 2 | 1 |
| 2026 | 0 | 4 | 1 |

Generation 0 is a founder; generation 1 or higher directly demonstrates reproduction. These are sampled living descendants, not per-lineage lifetime birth counters. A zero does not prove an unsampled short-lived descendant never existed.

- **Forager:** is extinct by 300 seconds in seeds 42, 7, 2026.
- **Mixotroph:** remains present in seeds 42, 7, 2026.
- **Predator:** remains present in seeds 7, 2026; is extinct by 300 seconds in seed 42.

The engine checks material and energy budgets throughout its correctness tests, and this script checks both at each recorded ecological sample. The largest absolute sampled mineral discrepancy in these runs is 4.06e-10 material units. No protected lineage, population quota or automatic reseeding is used. Consumer survival remains sensitive to navigation, access to organic material and the cost of rebuilding inherited organs.

## Earlier organelle diagnostic

`organelle-ecosystem-baseline-42.json` is a separate **900-second** ABI 8 diagnostic using earlier programs and a different engine hash. Its final lineage counts are 246 Phototrophs, 0 Foragers, 4 Mixotrophs and 0 Predators. A Predator daughter was visible at 120 seconds, but that lineage was extinct by 600 seconds. No Forager descendants appeared at its sample points. The old scavenger program stopped for all contacts, including resistant living surfaces.

The baseline and later ABI 8 measurements differ in program behavior and engine build. They are not a controlled estimate of the isolated navigation change. The final 300-second files must not be presented as 900-second reruns or evidence of permanent coexistence.

## Run current checks and measurements

```sh
npm run test:food-chain
npm run test:cellular
npm run test:ecosystem
node --no-liftoff scripts/check-ecosystem.mjs --assert --seed 42,7,2026 --duration 600 --output experiments/ecosystem-rerun.json
node --no-liftoff scripts/check-ecosystem.mjs --assert --seed 42 --duration 600 --mutation 0.1 --output experiments/ecosystem-mutation-rerun.json
```

These commands use the current bundled WASM and programs. They do not reproduce historical runs unless that exact engine and source are restored. Omit `--assert` to record an experiment without requiring consumer persistence. The regression gate checks a finite observation window, not a desired equilibrium.

A separate native lifecycle test supplies a stationary scavenger with accounted organic particles in darkness. It captures and digests meals, grows a paid body and produces a daughter; the identical unfed control has no births. This verifies the heterotrophic growth pathway independently of the mixed preset's navigation and food availability.

`--duration` accepts positive whole simulated seconds and defaults to 600. Standard sample times are 0, 15, 30, 60, 120, 180, 300, 600 and 900 seconds, filtered to the requested interval; the requested endpoint is always included. Reports retain complete sources, WASM hash, configuration, lineage diagnostics and conservation checks. `--assert` requires at least 300 simulated seconds and writes its report before reporting failures.

## Historical allocation-model experiments — ABI 4–6

- `food-chain-before.json` and `food-chain-after.json` retain the earlier 300-second ABI 4-to-5 comparison.
- `population-before.json` and `population-after.json` retain the ABI 5-to-6 population-cap and feeding comparison, including a separate mutation-enabled run.

Those files use the former fractional investment bodies and semantic food/prey/threat sensors. Their balance, coefficients and checkpoint compatibility assumptions are obsolete for the organelle model. They remain historical records with their original hashes and source; they are not current regression expectations.
