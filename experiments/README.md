# Food-chain measurements

These are deterministic simulation experiments, not a biological validation or a promise of indefinite coexistence. Mutations are disabled so each founding strategy remains identifiable.

- `food-chain-before.json` records the original ABI 4 mixed ecosystem, with its original populations, programs, and external food input.
- `food-chain-after.json` records the revised ABI 5 mixed ecosystem, including the producer-heavy starting recipe. Its lit habitat has no initial or continuing externally supplied particles.

Each file records the actual engine hash, environment, programs, and fixed seeds. The original run samples 0, 30, 120, and 300 simulated seconds. The revised run also samples 299 seconds so the 300-second founder lifespan transition is visible. Counts include descendants of the same program; energy-flow and growth readings are instantaneous rates.

The comparison covers the complete rework: both biological rules and the starting recipe changed. It is not a controlled estimate of one coefficient's effect. Extinction is allowed, and total organic energy does not guarantee that a particular cell can find enough food locally.

Run the short correctness checks:

```sh
npm run test:food-chain
```

Repeat the current three-seed study after installing dependencies:

```sh
npm run test:food-chain -- --measure --output experiments/food-chain-rerun.json
```

The script compiles the actual built-in programs, checks distinct light/organic feeding strategies, and reports populations without asserting a desired survival outcome. The Rust tests separately verify direct transfer from a living stationary producer to a stationary consumer, finite guts, assimilation losses, contact/wound restrictions, growth and division costs, mineral conservation, and persistence.

## Recorded result at 300 seconds

| Seed | Phototroph | Forager | Mixotroph | Predator |
| --- | ---: | ---: | ---: | ---: |
| 42 | 229 | 0 | 0 | 3 |
| 7 | 218 | 0 | 1 | 2 |
| 2026 | 203 | 0 | 6 | 0 |

The original setup lost all producers and foragers by 120 seconds in each recorded seed. The revised setup retains producers; predator descendants survive in two seeds. It does not produce stable coexistence of every strategy: Foragers disappear before 299 seconds in all three final runs. The founder lifespan transition at 300 seconds also removes non-reproducing founding cells.
