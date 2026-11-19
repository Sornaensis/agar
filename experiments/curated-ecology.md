# Predator survival and the curated presets

## Historical trials with supplied food

In the earlier engine, Still water was a dark 1,200 × 760 world with an open atmosphere. A historical comparison introduced one Predator and supplied 360 organic particles at startup and six per second afterward. Mutation was 0.1. These measurements describe that retired setup.

| Seed | Original predators at 300 s | Particles captured by 300 s | Particles still in the water |
| --- | ---: | ---: | ---: |
| 42 | 0 | 37 | 2,153 |
| 7 | 0 | 14 | 2,164 |
| 2026 | 0 | 19 | 2,159 |

At seed 42 the mean chemical energy in a particle fell from 21.04 at launch to 1.47 at 300 seconds. At that point 78.9% held less than one energy unit. The canvas now draws particles and their halos according to their remaining chemical energy. The Metrics particle count also shows average organic carbon per particle. Only particles that enter capture range can fill a gut. The old genome also counted slow-to-digest waste toward its feeding-rest threshold. In the 400 × 400 comparison pool described below, the former genome survived five minutes at seed 42 and captured 104 particles, yet never grew beyond its starting area. It had to fund search, digestion, repair and daughter machinery out of the same meals.

The former mixed ecosystem behaved differently. At seed 42 with ordinary browser settings, its two predator founders were down to one living hunter at five minutes, then recovered to two at ten minutes. The run had one observed predator descendant. All six founding Foragers died without an observed descendant. A successful bite or a full-looking gut does not establish lineage survival.

## Historical genome comparison

The revised genome doubles initial exudate digestion capacity and keeps the larger tissue enzyme. It delays the burst of construction for a daughter until 1.75 times birth area and keeps single copies of several adult machines. It remembers a prey cue for 18 seconds. Paired chemical receptors steer the search when vision has no return. A gut containing waste alone leaves the hunter searching. Contact recognition still protects peers carrying its marker.

These controlled trials were recorded before the tissue-energy update. They isolate the genome from the habitat change. Each starts with one unmodified hunter in a 400 × 400 world and runs for 300 simulated seconds. The dark trial scatters organic food. The prey trial scatters 20 non-attacking cells and supplies no additional food.

| Seed | Dark trial, living hunters | Dark trial, births | Prey trial, living hunters | Prey trial, births | Prey killed |
| --- | ---: | ---: | ---: | ---: | ---: |
| 42 | 3 | 2 | 1 | 1 | 14 |
| 7 | 3 | 2 | 2 | 1 | 20 |
| 2026 | 2 | 1 | 1 | 1 | 13 |

Under these same conditions the former genome produced zero offspring in all three dark trials. With prey, it reproduced in one of three trials and went extinct on seed 7.

## Historical feeding-density comparison

Shrinking the stock fed pool from 1,200 × 760 to 640 × 480 gives each founder more nearby food while keeping the same total input, mutation rate and oxygen exchange. The original and revised genomes were each run in both sizes. Each column below covers seeds 42, 7 and 2026.

| Genome and habitat | Lineages alive at 300 s | Lineages alive at 600 s | Lineages with births by 600 s |
| --- | ---: | ---: | ---: |
| Former genome, original size | 0/3 | 0/3 | 0/3 |
| Revised genome, original size | 2/3 | 0/3 | 0/3 |
| Former genome, compact size | 3/3 | 3/3 | 0/3 |
| Revised genome, compact size | 3/3 | 3/3 | 3/3 |

All three revised lineages ended with three living hunters in the compact pool. The same result held on three additional seeds (19, 71 and 314). World size alone did not prevent survival. Restoring the original dimensions and raising organic input from 6 to 18 particles per second gave nearly the same input per unit area as the compact pool. Starting with the same 360 particles, the revised founder produced two births and finished with three living hunters at 600 seconds on seeds 42, 7 and 2026.

## Still water and Light and shade

Still water is a quiet, lit 640 × 480 habitat. In the ten-minute test, one Predator shares the habitat with 48 Phototroph founders. On seeds 42, 7 and 2026, their hunts produced 35, 17 and 33 kills. The predator lineage ended with 5, 2 and 6 living cells respectively. Matched lone hunters in empty water exhausted their starting stores by 102 seconds on all three seeds.

Light and shade starts 32 Phototrophs, 12 Mixotrophs and two Rememberers in a 400 × 400 pool with a lamp and a current. It starts with zero particles. Across the same three seeds, 7, 7 and 8 individual Mixotrophs both fixed carbon in light and digested real food in shade. Both Rememberers in every run steered using a recently lost chemical cue.

## The food web

Foragers could double their area yet lose a daughter on the division tick. In the failing case the original core held all the ATP storage. The revised grazer pays for storage that can pass to a daughter and allows offspring with smaller compartments to build on what they inherit.

A Forager also recognizes a Phototroph by contact, then can build the matching adhesion, carbon and mineral machinery. Both partners pay to make these structures. Resource transfer follows the difference in their stored fractions. Foragers still scavenge loose particles and tissue when detached. In a controlled 70-second encounter, an attached grazer fed on real donor tissue, received 4.89 units of organic carbon and returned 0.531 units of minerals. Closing just the grazer’s two junctions reduced both transfers to zero.

The **Sunlit food web** starts 48 Phototrophs, 12 Foragers and one Predator together in 400 × 400 cool water. Light powers primary production, and the water exchanges gases with the atmosphere. Each founder starts with ordinary body reserves. No organic particles are supplied at launch or afterward.

| Seed | Phototrophs at 600 s | Foragers at 600 s | Predators at 600 s | Observed Forager descendants | Predation deaths |
| --- | ---: | ---: | ---: | ---: | ---: |
| 42 | 267 | 21 | 8 | 32 | 47 |
| 7 | 226 | 41 | 9 | 62 | 71 |
| 2026 | 229 | 26 | 10 | 43 | 56 |

All three lineages reproduced on each seed under ABI 19 with bilateral recruitment. The previous ABI 16 runs ended with 5, 4 and 7 predators on these seeds. A matched feeding trial isolates the energetic change from differences in population trajectories. The same developed Phototroph yielded 128.29 energy units of carcass tissue under ABI 16 and 352.25 in the current engine. [Full preset results](scenarios.json) · [Matched feeding trial](predation-yield.json)

## Colony formation

In Colony pool (seed 42), two independently placed Nurse rosette founders built working adhesion surfaces before they were moved into contact. The previous engine counted them as partners on the first tick and throughout all 60 ticks observed. Under ABI 19 the same encounter produced zero bonds. Enabling recruitment on just one founder also produced zero. With bilateral recruitment, the bond appeared on tick 15. Interrupting the encounter reset its accumulated contact time. [Reproduce the encounter](../scripts/check-colony-formation.mjs)

A Crown rosette founded by one cell reached three connected cells at 162 simulated seconds. The middle cell signaled to two partners and an outer cell activated 0.06 mass of additional harvesting machinery. Closing its signal junctions kept the structure intact and eliminated the edge response. [Reproduce the open and closed controls](../scripts/check-crown.mjs)

## Repeating the comparisons

```sh
npm run test:predator -- --compare
npm run test:predator-foraging -- --compare
npm run test:scenarios
npm run test:ecosystem
```

The optional comparison also runs the former genome under current conditions. The stock trial compares a lone hunter against a hunter with living producers. In the 400 × 400 foraging trials, the hunter encounters either lit live prey or a scattered bloom that died from heat stress before the water was cooled and darkened. [Former genome](fixtures/predator-before-curation.cell)
