# Predation and heterotroph validation

## Outcome

The corrected engine and programs support actual hunting, tissue digestion and
reproducing pure-consumer Predator lineages in all four ten-minute mixed runs.
**Forager persistence remains unresolved.** Two runs retain one Forager founder
each; the other two lose the lineage. Only seed 7 observes a Forager descendant,
and that lineage is extinct by the endpoint. Consequently, all four runs fail
at least one unchanged Forager survival or reproduction assertion. These
results establish working predation, not stable ecosystem coexistence.

These measurements use the actual catalog **Mixed ecosystem / Sunlit pond**:
72 Phototroph, 6 Forager, 4 Mixotroph and 2 Predator founders, identical minimal
initial bodies, no initial organic food and no continuing food input. Startup,
environment, decay, population rules and initial energy have not been changed.

## Ten-minute mixed-world results

| Seed | Mutation rate | Foragers alive | Predators alive | Observed Forager descendants | Observed Predator descendants | Predations | Ecological gate |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 42 | 0 | 0 | 7 | 0 | 7 | 56 | Forager survival + reproduction fail |
| 42 | 0.1 | 1 | 8 | 0 | 9 | 75 | Forager reproduction fails |
| 7 | 0 | 0 | 5 | 1 | 6 | 59 | Forager survival fails |
| 2026 | 0 | 1 | 2 | 0 | 4 | 35 | Forager reproduction fails |

Descendants are distinct generation-positive IDs observed once per simulated
second, not an exact lifetime birth counter. Living generation and individual
consumer details are included in the JSON reports. All four runs pass material
and energy accounting with unchanged tolerances. The largest endpoint material
discrepancy is below 8.4e-10.

The surviving Foragers are founders at area 1 with 36.36 ATP (seed 42 with
mutation) and 49.14 ATP (seed 2026), respectively. The injury reflex fixes the
controlled attack failure but does not establish a positive reproduction budget
in these mixed worlds. Final reports are
[seed 42](predation-current-seed42.json),
[seed 42 with mutation](predation-current-mutation.json),
[seed 7](predation-current-seed7.json) and
[seed 2026](predation-current-seed2026.json).

Before this correction, seed 42 lost both Predators by 60 seconds and all
Foragers by 120 seconds. At 600 seconds it contained 563 Phototrophs, no other
lineages, and zero attacks or predations. This is a combined engine/program
comparison, not an estimate of one isolated change. The original programs,
environment and engine hash remain in [predation-baseline.json](predation-baseline.json).

## Defects corrected

- **Development spending:** consumers establish feeding and navigation before
  optional storage and reproductive support. Predators build a piercer before
  growth; captured substrate can trigger its missing enzyme at low ATP.
- **Inaccessible energy:** cellular construction, envelope repair and fission
  spend actual available ATP. Empty storage no longer imposes a hidden reserve.
  Operations still pay their complete costs; legacy reserve rules are retained.
- **Broken inheritance:** division balances functioning duplicates by complete
  recipe identity, including ingredients and placement. Opposite motors and
  different digestive enzymes remain distinct; material and machinery are not
  cloned. Core support and physical packing remain mandatory.
- **Wear and maintenance:** small injuries no longer trigger redundant copies
  immediately. A paid repair mechanism can be built during development, even
  when injury disables a later developmental prerequisite.
- **Contact and secretion geometry:** Predators keep approaching soft particles
  instead of stopping just outside capture reach. New and merged secretion
  packets clear the emitter's inclusive capture surface, preventing immediate
  reingestion of digestive waste while allowing later physical recapture.
- **Substrate budgets:** Forager induces a .04 Tissue enzyme after capture.
  Waste alone no longer suppresses its food search, while total gut occupancy
  still limits capture. Predator uses .08 Tissue and .025 Exudate enzymes. The
  final Exudate adjustment resolves a measured growth/space stall: the former
  1.248 ATP/s flux barely covered 1.243 ATP/s basal maintenance at area 1.7605.
- **Injury response:** Forager escape takes priority over meal rest and optical
  attraction until paid repair restores integrity. It can respond to its own
  injury when an attacker is outside contact-receptor coverage. The controlled
  paid-piercer encounter records 51 attack ticks instead of 434; healthy fed
  control traces remain identical. Escape and repair have real energy costs.
- **Thermal behavior:** two paid flank heat receptors let Predator turn toward
  cooler water. Escape overrides full-gut rest and contact stopping. The
  receptors cost 1.92 ATP and .02 material, and provide no heat protection.
- **Newborn accounting:** a daughter that dies immediately after division now
  returns its body and gut through the normal death path. A previously failing
  full-world tick lost .7813 mineral units; the corrected replay conserves them.

## Correctness coverage

The final native suite passes **262 tests**. Actual-WASM focused food-chain and
navigation checks, all eight funded lifecycle checks, threaded-worker checks,
cellular UI/schema checks, TypeScript and the production build also pass.
The production page loads the same fingerprinted WASM tested here; an obsolete,
unreferenced public engine copy was removed.

Controlled cultures are deliberately separate from mixed-world persistence:

| Controlled pathway | Measured result |
| --- | --- |
| Forager, supplied Exudate | First division 81.07 s |
| Forager, accounted Tissue | First division 281.63 s; both daughters retain eight working feeding/navigation mechanisms |
| Forager, gut over 45% Waste with visible Exudate ahead | Exudate captured in 1.97 s; capture still pauses above the total-occupancy limit |
| Predator, supplied Exudate fallback | First division 269.40 s within the unchanged 900 s bound |
| Predator, accounted Tissue plus real thermal injury and repair | First division 133.77 s; both daughters retain eleven working essentials, including both heat receptors |
| Fed Forager under an actual paid piercing attack | Survives and repairs; disabled-reflex control dies with food and ATP remaining; healthy fed behavior is unchanged |
| Fed Predator approaching heat from four directions | Escapes without injury; disabling the paid receptors removes the response |

See [predation-validation-results.json](predation-validation-results.json) for
all eight funded cultures and source hashes, and the accompanying navigation,
lifecycle, native and integration logs. Test inputs introduce food through the
ordinary accounted path; the mixed-world runs introduce none.

## Remaining Forager limitation

Global food abundance does not imply affordable local access. A separate
producer-establishment experiment advanced the same 72 producers for 90 paid
seconds before introducing consumers. It still produced no Forager offspring
within 300 consumer-seconds, despite thousands of food-energy units in the
world. Foragers assimilated approximately .548 ATP per living-cell-second
against .493 basal maintenance, leaving only .055 before movement and
construction. That startup change was not adopted.

Controlled natural-secretion scenes also rejected a center-corrected tangent
and simple forward-traversal policy: neither consistently improved intake over
current grazing. These results support further work on local acquisition and
movement budgets; they do not justify a claim that the mixed ecosystem is
balanced. No survival quota, automatic reseeding or additional organic food
was added to make a test pass.

## Reproduce and inspect

```sh
npm run test:engine
npm run test:food-chain
npm run test:cellular
npm run test:ecosystem
node --no-liftoff scripts/check-ecosystem.mjs --assert --seed 42,7,2026 --duration 600 --output experiments/ecosystem-rerun.json
node --no-liftoff scripts/check-ecosystem.mjs --assert --seed 42 --duration 600 --mutation 0.1 --output experiments/ecosystem-mutation-rerun.json
```

**The strict ecosystem command currently fails for the Forager outcomes shown
above.** Reports are saved before assertion failure. Failure tolerances and
survival/reproduction requirements have not been weakened. Conservation failures
also preserve the exact checkpoint and snapshot before throwing.

Start a fresh **Mixed ecosystem** to use updated built-in programs. Existing
saved simulations retain their original genomes. Sources are in
[Forager](../examples/forager.cell), [Predator](../examples/predator.cell) and the
[preset generator](../src/presets.ts).

Build provenance:

- WASM SHA256: `8cff1c20df7a0a805bfe01f2e75bfbbb86996e26baf38cfc6419cb0da6b65c6b`
- Current preset aggregate SHA256: `6dab30b54d1c66e07bd5e7386a0b6b8f9e3d940cbdadbf3e1a044961d23903fc`
- ABI 10, cellular schema 3, checkpoint 7.

The `predation-pre-*`, maintenance and older engine-hash reports are historical
intermediate candidates. Incomplete runs are marked as such; they are not final
600-second results. Native and WASM trajectories can diverge through floating
point differences, so the table above uses actual WASM throughout.
