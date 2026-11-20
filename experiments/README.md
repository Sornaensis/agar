# Ecosystem measurements

## Springwater basin

Nine stock genomes start as 116 founders in the 800 × 520 Springwater basin. Two mineral springs supply 8,700 counts per simulated second at their configured rates. A downstream absorber can remove up to 5,400 counts per second when minerals are present. Two current bands and a vortex move released food around two fixed ledges and one drifting barrier. All organic food comes from cells.

On native seed 7, a docked Forager received carbon from a Phototroph at 40 seconds. Juvenile Nurse cells received carbon, ATP and minerals between 96 and 116 seconds. A Crown rim cell responding to a neighboring signal released carbon at 128 seconds, and a bonded Pulse swimmer powered its motor at 148 seconds. These are the first observations in four-second samples.

In the basin study preceding the Predator feeding revision, the WebAssembly engine recorded these outcomes at ten simulated minutes. Late Forager births occurred between minutes five and ten.

| Seed | Cells alive | Lineages alive | Foragers alive | Late Forager births | Extinct lineages |
| --- | ---: | ---: | ---: | ---: | --- |
| 7 (default) | 432 | 9 of 9 | 1 | 1 | None |
| 17 | 418 | 9 of 9 | 1 | 4 | None |
| 42 | 426 | 9 of 9 | 1 | 2 | None |
| 2026 | 449 | 8 of 9 | 0 | 3 | Forager |

Native seed 7 reached 900 seconds with 547 cells across all nine lineages, including 1 Forager. Native seed 42 reached 900 seconds with 550 cells and 8 surviving lineages. The Forager lineage disappeared between the 780- and 840-second censuses. On WASM seed 7, doubling the number of Forager founders from 12 to 24 left 0 at 600 seconds. The same study includes the 108-founder genome-only comparison and the 116-founder community before the feeding change. [Censuses, source programs and measured interactions](springwater-food-web.json) · [Previous basin study](springwater-basin.json)

In a finite carcass bloom the revised Forager divided at 61 seconds. The two halves inherited 72 and 36 units of gut capacity, and the daughter divided at 102 seconds. Physical inheritance also produced daughters with zero gut capacity in a free-running control. In a separate paired encounter, a Forager gained 6.4613 carbon and returned 0.3156 mineral through its producer dock. Both transfers were zero with exchange ports closed.

Replay this earlier ten-minute observation with `npm run test:scenarios -- --seeds 7 --genome "Predator=experiments/fixtures/predator-before-particle-foraging.cell" --output basin.json`. The current stock uses `npm run test:scenarios -- --seeds 7 --output current-basin.json`.

## Predator particle feeding

The preceding Predator had a forward eye and generally pursued optical returns only while hungry. The current Predator can grow a pair of flank eyes in weak organic plumes. Once a flank eye is built, it can pay for a sideways motor that brings its forward inlet toward a nearby small object. A well-fuelled cell with room in its gut can use the same response. The particle must still reach a working inlet.

In 18 six-second approaches to a real Phototroph secretion, the old genome consumed and digested 8 packets and the revised genome consumed and digested 17. Each approach used one isolated 0.570-energy packet, with two starting gaps and nine bearings. Closing all inlets prevented feeding. In a separate trial both adults had more than 70% of their ATP capacity, an empty gut, and an identical fresh 0.582-energy packet. The old adult missed both flank placements for 16 seconds. The revised adult collected the left and right packets in 0.73 and 0.70 seconds. The remote control packet persisted for the full 16 seconds.

In Springwater, five-minute measurements sum the energy assimilated by every living Predator descendant after each tick. The rates divide those amounts by cumulative live Predator-seconds. The totals include scavenged material and tissue from kills.

| Seed | Previous energy | Revised energy | Previous rate | Revised rate | Previous kills | Revised kills |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 7 | 1,719.3 | 1,805.3 | 1.90 | 1.99 | 18 | 18 |
| 42 | 3,264.4 | 2,072.0 | 2.44 | 1.98 | 37 | 23 |

The revised default browser simulation had 427 cells from 9 founding lineages at ten minutes, including 3 Predators and 1 Forager. The older finite-prey stress test still misses its granddaughter target. All three dark-carrion trials ended without a surviving Predator for either genome. [Exact observations and replay commands](predator-particle-feeding.json)

## Predator recognition

[The paired pond study](predator-recognition-pond.json) compares the earlier ring-of-displays Predator with its [pre-recognition genome](fixtures/predator-before-recognition.cell). The habitat, mutation rate, Phototrophs, Foragers, and founder counts are identical on both sides. Every kill was credited to the Predator lineage. Each victim’s founding lineage therefore gives the exact number of kin and prey kills, including mutated descendants. The study also records births, deaths from ATP depletion, and whether each fission product has an active display carrying marker 17 immediately after the split and one tick later.

At ten simulated minutes, the previous Predator had killed two kin and 44 other cells on seed 42, and zero kin and 32 other cells on seed 7. The revised Predator killed zero kin on either seed. It killed 29 Phototrophs on seed 42 and 15 on seed 7. Two revised Predators were alive on seed 42 and one on seed 7. Both lineages reproduced.

Every observable revised daughter carried marker 17 after one tick (1/30 second). On each seed the first division left eight active display patches in each half. The next observable division left twelve in the retaining parent and eleven in the daughter. One further daughter was absent from the live snapshot on the split tick. The previous genome also wrote 17 within one tick in all six observable births in its 300-second timing control. Each of those halves had one active patch.

The continuously fed division regression tests marker timing independently of the finite-prey stress test.

Run either command from the project folder to produce the detailed censuses.

```sh
node --no-liftoff scripts/check-ecosystem.mjs --scenario mixed-ecosystem --seed 42,7 --duration 600 --sample-at 450 --trace-predator-births --predator-source experiments/fixtures/predator-before-particle-foraging.cell --output revised-recognition.json
node --no-liftoff scripts/check-ecosystem.mjs --scenario mixed-ecosystem --seed 42,7 --duration 600 --sample-at 450 --trace-predator-births --predator-source experiments/fixtures/predator-before-recognition.cell --output previous-recognition.json
```

## Earlier pond measurements

[Local surfaces and food composition](local-surfaces-pond.json) compares the supplied archive and revised Sunlit food web on seeds 42 and 7. Separate seed-42 treatments close the producer’s carbon exporters and assign a uniform composition to every packet of a given origin. A further comparison rotates only the Forager’s constrictor by a quarter turn. Use `--disable-stock-export` or `--flank-forager-division` with `scripts/check-ecosystem.mjs` to repeat those two treatments. The report identifies each engine and founder program by hash and records births, mortality, predation, and all four resource balances.

[The 45-minute Sunlit food web runs](extended-local-ecology.json) follow 48 Phototrophs, 12 Foragers, and one Predator per seed with the stock mutation rate of 0.1. At 45 minutes seed 42 had 71 Phototrophs, zero Foragers and two Predators. Seed 7 had 124, zero and two. Of 39 Forager deaths, 38 came from ATP depletion and one from predation. Phototrophs reproduced into the final minute on both seeds. The last Predator births occurred at 21:10 on seed 42 and 7:00 on seed 7. The integer energy balance closed at every census. The report identifies the engine used for the long runs and the editor build that reproduced every recorded measurement through 120 seconds on seed 42.

To repeat the study, run `node --no-liftoff scripts/check-ecosystem.mjs --scenario mixed-ecosystem --seed 42,7 --duration 2700 --sample-at 450,750,1200,1500,1800,2100,2400 --predator-source experiments/fixtures/predator-before-recognition.cell --output pond-45min.json`.

[Curated ecology](curated-ecology.md) records earlier predator and preset investigations. Older runs below used the catalogs and engines identified in each section.

In the revised stock pond, all three lineages were alive at 300 seconds on both seeds. Seed 42 ended at 900 seconds with 100 Phototrophs, six Predators and no Foragers. Eighteen Foragers depleted their energy and one was killed. Rotating only the Forager constrictor by 90 degrees left 116 Phototrophs, five Foragers and four Predators alive at 900 seconds. The forward mount supported reproduction by the first daughter in the separate finite-food assay. The stock long-run ecosystem assertion and the isolated Predator survival assertions remain unmet.

## Predator reproduction and ecological accounting

In a 75-minute save of the earlier Sunlit food web, seven hunters remained. Their last birth was at 6:26. All seven were at least 95% full of ATP and together held 1,586,466 counts of gut energy. Their mean progress toward doubling their area was 7.1%. Inspection of six hunters found too little interior space for the growth machinery they needed after division.

The current hunter builds duplicate growth machinery before constricting. A fed mother and its daughter can each divide again on accounted prey. The grazer provisions both halves with feeding reserves and mineral uptake. The producer duplicates its light harvesting, respiration, mineral uptake and protein synthesis equipment. On the same pond seed, earlier producers recorded 46 deaths with depleted ATP by minute twenty. With the duplicate equipment, there were zero producer deaths from depleted ATP at that point.

In whole-count runs with the stock founders and habitat, the first 30 minutes gave the following results. All four balances closed at every 30-second observation.

| Seed | Producers alive | Grazers alive | Hunters alive | Producer deaths from predation | Producer deaths with depleted ATP |
| --- | ---: | ---: | ---: | ---: | ---: |
| 42 | 171 | 43 | 39 | 197 | 1 |
| 7 | 185 | 56 | 30 | 204 | 0 |

At minute 75 on seed 42, 68 producers, 107 grazers and 32 hunters were alive. The hunter lineage had 123 births and 92 deaths; its latest birth was at 74:46. Producer deaths comprised 658 kills and two deaths with depleted ATP. The counts for minerals, carbon, oxygen and energy reconciled at all 151 observations. The pond had received 13,701,939,580 energy counts. It retained 584,870,144 and recorded 13,117,069,436 in process losses. Seed 7 ended its separate 45-minute run with 80 producers, 97 grazers and 37 hunters and zero discrepancies at all 91 observations.

To measure the same habitat over 75 simulated minutes, run `node --no-liftoff scripts/check-ecosystem.mjs --scenario mixed-ecosystem --seed 42 --duration 4500 --output experiments/pond-75min.json`. The report includes the engine hash, founder programs, feeding, births, deaths and energy balance.

The small-bite accounting test feeds tissue containing ten carbon counts and ten mineral counts, once as ten equal bites and once as a single portion. Both cost four ATP counts and retain eight mineral counts. In the older model, the ten single bites cost ten ATP counts and retained zero minerals.

## Whole-count chemistry

Run `npm run test:integer-chemistry` for a one-count-per-second emitter, staged adhesin production, conservation through death, migration of a genuine v16 save and exact continuation of a hunting run. The comparison uses the same seed, habitat and genome sources with each resource model. Timings cover engine stepping only and the two populations can diverge. [Whole-count comparison](integer-chemistry.json) · [Seeded preset trials](scenarios.json)

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
node --no-liftoff scripts/check-ecosystem.mjs --scenario mixed-ecosystem --assert --seed 42,7,2026 --duration 600 --output experiments/ecosystem-rerun.json
node --no-liftoff scripts/check-ecosystem.mjs --scenario mixed-ecosystem --assert --seed 42 --duration 600 --mutation 0.1 --output experiments/ecosystem-mutation-rerun.json
```

These commands use the current bundled WASM and programs. They do not reproduce historical runs unless that exact engine and source are restored. Omit `--assert` to record an experiment without requiring consumer persistence. The regression gate checks a finite observation window, not a desired equilibrium.

A separate native lifecycle test supplies a stationary scavenger with accounted organic particles in darkness. It captures and digests meals, grows a paid body and produces a daughter; the identical unfed control has no births. This verifies the heterotrophic growth pathway independently of the mixed preset's navigation and food availability.

`--duration` accepts positive whole simulated seconds and defaults to 600. Standard sample times are 0, 15, 30, 60, 120, 180, 300, 600 and 900 seconds, filtered to the requested interval; the requested endpoint is always included. Add census times with `--sample-at` followed by a comma-separated list of whole simulated seconds. Reports retain complete sources, WASM hash, configuration, lineage diagnostics and conservation checks. `--assert` requires at least 300 simulated seconds and writes its report before reporting failures. With `--assert --births-after 600`, every founding lineage must record a birth after minute ten. The report includes exact lineage events and an independent check of the energy lost to each process in whole-count worlds.

## Historical allocation-model experiments — ABI 4–6

- `food-chain-before.json` and `food-chain-after.json` retain the earlier 300-second ABI 4-to-5 comparison.
- `population-before.json` and `population-after.json` retain the ABI 5-to-6 population-cap and feeding comparison, including a separate mutation-enabled run.

Those files use the former fractional investment bodies and semantic food/prey/threat sensors. Their balance, coefficients and checkpoint compatibility assumptions are obsolete for the organelle model. They remain historical records with their original hashes and source; they are not current regression expectations.
