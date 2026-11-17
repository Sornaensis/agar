# Food-chain measurements

These deterministic experiments test the simulator's behavior. They do not guarantee indefinite coexistence or validate a biological model.

## Population and feeding correction

- `population-before.json` records the shipped ABI 5 mixed ecosystem for seeds 42, 7, and 2026, with mutations disabled, through 900 simulated seconds.
- `population-after.json` records the ABI 6 comparison for those seeds and a separate default-mutation run (seed 42, 10%). Engine hashes, starting programs, environment settings, and population ancestry are retained with the results.

Both builds use 72 Phototrophs, 6 Foragers, 4 Mixotrophs, and 2 Predators in Sunlit pond, with no initial or continuing externally supplied organic food. The new build removes the organism cap, adds finite photon sharing over biomass-scaled capture areas, allows bites to transfer existing prey reserves into a gut, and improves food targeting and consumer programs. Smaller hunters now trigger threat sensing. The Predator remembers a hunger cycle and escapes hot water. This compares the complete correction, not the isolated effect of one coefficient.

The earlier build reaches exactly 384 organisms in every recorded seed. At 900 seconds its lineage counts are:

| Seed | Phototroph | Forager | Mixotroph | Predator |
| --- | ---: | ---: | ---: | ---: |
| 42 | 380 | 0 | 0 | 4 |
| 7 | 362 | 0 | 9 | 13 |
| 2026 | 367 | 0 | 17 | 0 |

This baseline reproduces producer dominance and population-cap saturation. It does not reproduce universal predator extinction with mutations disabled: two seeds retain predators, including mature cells with enough reserves to divide that are blocked by the cap.

The corrected build's lineage counts at 900 seconds are:

| Seed | Mutation rate | Phototroph | Forager | Mixotroph | Predator |
| --- | ---: | ---: | ---: | ---: | ---: |
| 42 | 0% | 202 | 0 | 2 | 15 |
| 7 | 0% | 200 | 0 | 0 | 17 |
| 2026 | 0% | 222 | 2 | 4 | 4 |
| 42 | 10% | 192 | 0 | 0 | 18 |

Predator descendants persist in all four measured runs, reaching generations 4–7. The default-mutation case records 73 mutation events; at the final checkpoint, actual body roles are 192 photosynthetic-only cells and 18 digestion-only cells, with 21 attack-capable cells overlapping those groups. Some producer descendants acquired attack tissue.

This corrects the observed cap/consumer failure for these runs, but does not guarantee every founding lineage persists. Pure Foragers disappear in both seed-42 runs and seed 7; Mixotrophs also disappear in seed 7 and the default-mutation case. These are 15-minute measurements across three 300-second lifespans, not a claim of perpetual stability.

Lineage labels follow descendants of each founding program. With mutations enabled, a descendant can change its diet or capabilities; lineage and current feeding strategy are distinct. The current measurement script also reports actual body roles, using the same at-least-5% tissue thresholds as the UI. Extinction remains possible; no species has a protected population or a guaranteed survivor.

## Reproduce the current checks

```sh
npm run test:food-chain
npm run test:food-chain -- --measure --output experiments/population-rerun.json
npm run test:food-chain -- --measure --seed 42 --mutation 0.1 --output experiments/population-default-rerun.json
```

The first command runs short acquisition and feeding checks. The second runs seeds 42, 7, and 2026 through 900 seconds with mutation disabled. The third uses the app's default mutation rate and seed. Counts and energy-flow readings are measured, rather than asserted to match a desired ecological outcome.

Native and WASM tests separately verify resource conservation, shared photon limits, food-value sensing, simultaneous bites, delayed digestion, growth costs, lifespan, and exact checkpoint continuation. Population regressions include division from 384 to 768 cells and save/resume with 2,049 cells. Actual ABI 4/v1 and ABI 5/v2 checkpoint fixtures test compatibility with previous builds.

## Earlier food-chain rework

`food-chain-before.json` and `food-chain-after.json` preserve the earlier 300-second ABI 4-to-5 comparison. Those builds also changed the starting recipe. The original setup lost all producers and foragers by 120 seconds in all three recorded seeds. The ABI 5 result at 300 seconds was:

| Seed | Phototroph | Forager | Mixotroph | Predator |
| --- | ---: | ---: | ---: | ---: |
| 42 | 229 | 0 | 0 | 3 |
| 7 | 218 | 0 | 1 | 2 |
| 2026 | 203 | 0 | 6 | 0 |

Those historical results motivated the longer population-and-feeding checks above. Each historical file retains its original engine hash, environment, programs, and sampling times.
