# Predator survival and the curated presets

## Reproducing the original failure

The comparison uses one randomly placed Predator in the stock **Still water** environment. The world is 1,200 × 760, dark, with the usual open-atmosphere exchange. The browser starts this habitat with 360 organic particles and supplies six more per simulated second. Mutation is 0.1. Every meal comes from the ordinary scattered supply.

| Seed | Original predators at 300 s | Particles captured by 300 s | Particles still in the water |
| --- | ---: | ---: | ---: |
| 42 | 0 | 37 | 2,153 |
| 7 | 0 | 14 | 2,164 |
| 2026 | 0 | 19 | 2,159 |

At seed 42 the mean chemical energy in a particle fell from 21.04 at launch to 1.47 at 300 seconds. At that point 78.9% held less than one energy unit. The canvas now draws particles and their halos according to their remaining chemical energy. The Metrics particle count also shows average organic carbon per particle. Only particles that enter capture range can fill a gut. The old genome also counted slow-to-digest waste toward its feeding-rest threshold. In the 400 × 400 comparison pool described below, the former genome survived five minutes at seed 42 and captured 104 particles, yet never grew beyond its starting area. It had to fund search, digestion, repair and daughter machinery out of the same meals.

The former mixed ecosystem behaved differently. At seed 42 with ordinary browser settings, its two predator founders were down to one living hunter at five minutes, then recovered to two at ten minutes. The run had one observed predator descendant. All six founding Foragers died without an observed descendant. A successful bite or a full-looking gut does not establish lineage survival.

## A hunter from an ordinary founder

The revised genome doubles initial exudate digestion capacity and keeps the larger tissue enzyme. It delays the burst of construction for a daughter until 1.75 times birth area and keeps single copies of several adult machines. It remembers a prey cue for 18 seconds. Paired chemical receptors steer the search when vision has no return. A gut containing waste alone leaves the hunter searching. Contact recognition still protects peers carrying its marker.

These checks isolate the genome from the habitat change. Each starts with one unmodified hunter in a 400 × 400 world and runs for 300 simulated seconds. The dark trial scatters organic food. The prey trial scatters 20 non-attacking cells and supplies no additional food.

| Seed | Dark trial, living hunters | Dark trial, births | Prey trial, living hunters | Prey trial, births | Prey killed |
| --- | ---: | ---: | ---: | ---: | ---: |
| 42 | 3 | 2 | 1 | 1 | 14 |
| 7 | 3 | 2 | 2 | 1 | 20 |
| 2026 | 2 | 1 | 1 | 1 | 13 |

Under these same conditions the former genome produced zero offspring in all three dark trials. With prey, it reproduced in one of three trials and went extinct on seed 7.

## Feeding density and world size

Shrinking the stock fed pool from 1,200 × 760 to 640 × 480 gives each founder more nearby food while keeping the same total input, mutation rate and oxygen exchange. The original and revised genomes were each run in both sizes. Each column below covers seeds 42, 7 and 2026.

| Genome and habitat | Lineages alive at 300 s | Lineages alive at 600 s | Lineages with births by 600 s |
| --- | ---: | ---: | ---: |
| Former genome, original size | 0/3 | 0/3 | 0/3 |
| Revised genome, original size | 2/3 | 0/3 | 0/3 |
| Former genome, compact size | 3/3 | 3/3 | 0/3 |
| Revised genome, compact size | 3/3 | 3/3 | 3/3 |

All three revised lineages ended with three living hunters in the compact pool. The same result held on three additional seeds (19, 71 and 314). World size alone did not prevent survival. Restoring the original dimensions and raising organic input from 6 to 18 particles per second gave nearly the same input per unit area as the compact pool. Starting with the same 360 particles, the revised founder produced two births and finished with three living hunters at 600 seconds on seeds 42, 7 and 2026. In the environment editor, organic input is a total for the entire world.

## The food web

Foragers could double their area yet lose a daughter on the division tick. In the failing case the original core held all the ATP storage. The revised grazer pays for storage that can pass to a daughter and allows offspring with smaller compartments to build on what they inherit.

A Forager also recognizes a Phototroph by contact, then can build the matching adhesion, carbon and mineral machinery. Both partners pay to make these structures. Resource transfer follows the difference in their stored fractions. Foragers still scavenge loose particles and tissue when detached. In a controlled 70-second encounter, an attached grazer received 5.43 units of organic carbon and returned 0.285 units of minerals. Closing just the grazer’s two junctions reduced both transfers to zero.

The **Sunlit food web** starts 48 Phototrophs, 12 Foragers and one Predator together in 400 × 400 cool water. Light powers primary production, and the water exchanges gases with the atmosphere. Each founder starts with ordinary body reserves. No organic particles are supplied at launch or afterward.

| Seed | Phototrophs at 600 s | Foragers at 600 s | Predators at 600 s | Observed Forager descendants | Predation deaths |
| --- | ---: | ---: | ---: | ---: | ---: |
| 42 | 268 | 12 | 5 | 25 | 50 |
| 7 | 269 | 1 | 4 | 11 | 46 |
| 2026 | 221 | 26 | 7 | 34 | 63 |

All three lineages reproduced on each seed. Seed 7 ended with 1 surviving Forager. The [full preset results](scenarios.json) include founder counts, source hashes, exchange measurements and the observation interval for each scenario.

## Repeating the comparisons

```sh
npm run test:predator -- --compare
npm run test:predator-foraging -- --compare
npm run test:scenarios
npm run test:ecosystem
```

The comparisons load the [former genome](fixtures/predator-before-curation.cell) and the live catalog. The stock-habitat comparison measures the former and revised dimensions separately. The isolated food and prey comparisons keep the world dimensions fixed.
