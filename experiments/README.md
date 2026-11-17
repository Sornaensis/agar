# Ecosystem measurements

These are deterministic tests of the simulator, not a biological validation or a guarantee of stable coexistence. Each JSON preserves its engine hash, environment, complete authored genomes, starter-body recipes, mutation rate and sampled results.

## Current organelle model — ABI 8

`organelle-ecosystem-final-42.json`, `organelle-ecosystem-final-7.json` and `organelle-ecosystem-final-2026.json` record **300 simulated seconds**, with mutation disabled. Samples are at 0, 30, 120, 299 and 300 seconds. This covers one founder lifespan and visible descendant generations; it does not establish long-term persistence.

Each run starts with 72 Phototrophs, 6 Foragers, 4 Mixotrophs and 2 Predators in Sunlit pond. There is no initial or continuing external organic-food input. Starter organs and reserves are explicit resource introductions. Later capabilities require paid construction; sensors expose local physical measurements. The updated scavenger programs distinguish tentative small-object returns from resistant contact and steer away from surfaces they cannot capture.

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

The baseline and current measurements differ in program behavior and engine build. They are not a controlled estimate of the isolated navigation change. The final 300-second files must not be presented as 900-second reruns or evidence of permanent coexistence.

## Reproduce

```sh
npm run test:food-chain
node scripts/check-preset-navigation.mjs
npm run test:food-chain -- --measure --duration 300 --output experiments/organelle-rerun.json
npm run test:food-chain -- --measure --seed 42 --duration 300 --mutation 0.1 --output experiments/organelle-mutation-rerun.json
```

The short food-chain gate verifies real light, organic and mixed acquisition, physical starter bodies and conserved material. The navigation fixture uses the actual Forager program and public WASM APIs: it starts against a resistant cell, escapes, then captures and digests particles. It does not assert a desired ecosystem equilibrium.

A separate native lifecycle test supplies a stationary scavenger with accounted organic particles in darkness. It captures and digests meals, grows a paid body and produces a daughter; the identical unfed control has no births. This verifies the heterotrophic growth pathway independently of the mixed preset's navigation and food availability.

`--duration` accepts positive whole simulated seconds and defaults to 900. Standard sample times are filtered to the requested interval, and the requested endpoint is always included. Measurements report observed outcomes rather than asserting particular lineage counts.

## Historical allocation-model experiments — ABI 4–6

- `food-chain-before.json` and `food-chain-after.json` retain the earlier 300-second ABI 4-to-5 comparison.
- `population-before.json` and `population-after.json` retain the ABI 5-to-6 population-cap and feeding comparison, including a separate mutation-enabled run.

Those files use the former fractional investment bodies and semantic food/prey/threat sensors. Their balance, coefficients and checkpoint compatibility assumptions are obsolete for the organelle model. They remain historical records with their original hashes and source; they are not current regression expectations. Current checkpoint 3 explicitly rejects checkpoint 1/2, preserving old saved data rather than silently migrating it.
