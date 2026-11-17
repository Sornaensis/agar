# Organelle growth and physical sensing

**Status: proposed design; not implemented.** This replaces the six `invest.*` targets with structures a cell must build, maintain, and inherit. The genome remains a pure reactive program with synchronous registers. These are simulation rules, not a claim of biological fidelity.

## 1. Starting point and intended change

[The current simulation](../engine/src/simulation.rs) uses one `size` for biomass, collision area, reserve capacity, and growth. Six body fractions determine capabilities. Founders receive them immediately; descendants remodel them. One sensing allocation unlocks exact environmental and target properties. [Built-in programs](../src/presets.ts) rely on `food.*`, `prey.*`, and `threat.*`.

Replace this with construction constrained by space, materials, energy, and assembly throughput. Growing the enclosing body creates room without automatically creating capabilities. Capabilities require functional structures. A changed genome changes construction requests; it never swaps the body instantly.

## 2. Separate space, material, and energy

Use a uniform-depth 2D abstraction. Store these quantities separately:

- **Envelope area** determines collision radius, internal space, and light interception geometry. Radius is proportional to the square root of area. Supported area requires paid structural frame and membrane material.
- **Material mass** equals scaffold plus organelles plus unbound reserve/gut minerals, each counted once. Retain the existing mineral currency as the conserved material proxy; this is not a carbon chemistry model. Energy does not create inertial mass. Mineral-free organic energy still occupies digestive/storage capacity; inertia uses material plus displaced water.
- **Usable energy**, **undigested energy**, and **embodied structural energy** have distinct ledgers. Construction spends usable energy; only an explicitly retained portion becomes recoverable structural energy. The rest dissipates.

Internal occupancy is the essential core, organelle footprints, and storage compartments. Compartment contents occupy their reserved compartment capacity, rather than being counted twice. External structures consume membrane attachment space and internal support machinery. Membrane capacity scales with perimeter, so doubling area does not double attachment capacity.

Required invariants are:

1. Internal occupancy never exceeds supported area; attachment footprints never exceed usable perimeter.
2. All structural material comes from cell reserves. Growth transfers material into frame or organelles; it cannot create minerals.
3. Every joule of recovered structural energy came from a recorded construction input. Build–dismantle and death–consume cycles cannot generate energy.
4. Area, mass, and capabilities are independent derived quantities. Adding an organelle inside an existing envelope must not create more room for itself.
5. Structural damage cannot erase material or compartment contents. Material is retained as damaged structure, released as debris, or returned to water according to explicit rules.

Movement uses actual material mass plus a water-displacement term for inertia, with drag derived from envelope and external structures. Basal maintenance depends on supported body and machinery, so empty space is not free. Replace the current overloaded `size` with explicit `body.area` and `body.mass` in the new language version. Remove the current fixed size-4 growth ceiling only alongside verified collision and wrapping bounds; those geometric limits must be documented separately from biological constraints.

## 3. A practical organelle catalogue

All optional modules require construction energy and minerals, impose maintenance, and have finite operating throughput. A module's definition specifies minimum functional size, footprint, growth rate, damage response, and action costs. Exact coefficients are balancing work after the accounting model passes its tests.

| Structure or module | Space requirement | Capability and tradeoff |
| --- | --- | --- |
| Essential core and assembly machinery | Mandatory internal space, scaling with supported body | Genome execution, baseline metabolism, bounded construction and basic repair. Enough remains to bootstrap an otherwise bare cell; additional assembly machinery increases throughput. |
| Photosynthetic unit | Internal area and light-exposed membrane coupling | Converts shared local light into usable energy and organic release. More machinery helps only while photons and processing capacity remain available. |
| Metabolic complex | Internal area | Raises energy-conversion throughput above the small core baseline. Does not create substrate, photons, or energy independently. |
| Digestive vacuole | Internal area including its cavity | Holds and processes ingested material. More/larger vacuoles increase capacity and processing; a full compartment prevents additional capture. |
| Reserve vacuole | Internal area including its cavity | Stores additional energy substrate and minerals. Larger reserves displace productive machinery and add carried mass. |
| Mineral transporters | Membrane footprint and internal support | Absorb dissolved minerals with a paid, supply-limited throughput. Independent of digestive machinery. |
| Phagocytic cup | Membrane footprint and internal support | Captures contacting particles or sufficiently manageable objects. Aperture, force, handling time, and digestive space constrain capture. |
| Flagellum | Membrane anchor, internal motor, external filament | Produces thrust and steering. More motors/longer filaments cost material, actuation energy, drag, and attachment space; length cannot increase force without adequate motor capacity. |
| Piercing apparatus | Membrane anchor and internal machinery | Damages contacted surfaces and transfers accessible material into digestive storage. Requires a paid action; it cannot identify profitable prey. |
| Protective cortex | Distributed membrane coverage and internal support | Resists physical damage and environmental stress. Thickness adds mass and reduces available surface access for other modules. |
| Repair complex | Internal area | Speeds repair and replacement of damaged modules using resources. Core repair remains slow but sufficient to avoid a construction dependency deadlock. |
| Photoreceptor | Membrane footprint and internal support | Measures local light and a body-relative brightness gradient. Separate from photosynthesis, so a producer can be blind and a heterotroph can follow light. |
| Thermoreceptor | Membrane footprint and internal support | Measures local temperature and a thermal gradient. Temperature still affects unsensing cells physically. |
| Chemoreceptor | Membrane footprint and internal support | Measures named dissolved compounds and their local gradients, including minerals and organic cues. Receptor channels are physical chemical classes, not “food” or “enemy.” |
| Mechanoreceptor | Membrane footprint and internal support | Reports contact pressure, local relative flow, and mechanical disturbance. Adds awareness, without being required for collisions to exist. |
| Optical detector | Internal optics and membrane aperture | Detects nearby contrasting objects: direction, apparent size, motion, and surface appearance. Performance depends on light, aperture, distance, and occlusion. |

Use individual records for discrete modules, including built mass, integrity, and a stable local identifier. Distributed modules such as cortex and transporters may use aggregated material with equivalent footprint accounting. Each family has a common requested count and unit size initially; damage and partial construction can make actual instances unequal. Arbitrary per-instance scripting is unnecessary for the first implementation.

Minimum unit size prevents infinitely many free sensors. Larger sensors improve reach or resolution with diminishing returns, without revealing hidden properties. More units improve angular coverage; larger units improve sensitivity.

## 4. Construction and body growth

Construction requests compete for one finite assembly budget. Compute all demands from the same snapshot, then proportionally allocate the available energy, minerals, internal area, membrane space, and assembly time. Resolve deterministic ties by module family and stable local identifier, never statement order. Large requested counts must not allocate large arrays; instantiate only affordable progress.

Build an envelope before filling its new space. Frame growth consumes energy and minerals, changes collision geometry, and raises upkeep and drag. Organelle construction fills existing capacity. Incomplete structures occupy space and hold material but provide no capability below their functional threshold. Above it, capability follows built size and integrity. Completion or growth affects actions on the following tick.

Dismantling is an explicit, rate-limited request. It costs processing energy and releases recoverable material; it never refunds the full construction energy. Recovered minerals beyond reserve capacity go to water. Digestive or reserve compartments cannot be removed while silently discarding contents: first transfer contents into available compartments, then release overflow as accounted particles/minerals. Automatic destruction follows the same accounting.

Lower targets do not instantly remove organs. Build/recycle requests for one family resolve to one net request before resources move. Essential core cannot be recycled below its viable minimum. Optional construction cannot spend energy committed to maintenance.

## 5. Proposed language interface

Keep outputs declarative and statically typed. Names below are proposals for a new language version, not currently valid code:

```text
# Targets describe a body; build controls the effort spent approaching it.
target.flagellum.count = 2;          # integer, nonnegative
target.flagellum.size = 0.08;        # scalar, module material units
build.flagellum = if energy.fraction > 0.65 then 1 else 0;
target.photoreceptor.count = 1;
target.photoreceptor.size = 0.02;
build.photoreceptor = 0.3;
recycle.flagellum = 0;

grow = if body.space.free < 0.05 then 0.5 else 0;
turn = if light.available then clamp(light.bearing * 2, -1, 1) else 0;
thrust = if energy.fraction > 0.3 then 0.4 else 0;
divide = body.maturity >= 1 && body.division_ready;
```

`build.*` and `recycle.*` are scalars clamped to 0–1; counts are nonnegative integers and sizes are bounded finite scalars. Targets are recomputed each tick. Omitted count targets are zero, size targets use the module's minimum, and construction/recycling requests are zero. Existing organs therefore remain intact without outputs. Lower targets remove organs only with explicit recycling effort. No organelle address bank or new register type is needed.

Expose actual `organelle.<family>.count`, `.mass`, `.capacity`, `.damaged`, and `.building`, alongside free space and surface. Program requests cannot claim capacity that has not been built. `body.division_ready` reports physical eligibility, not whether the program requested division.

Sensor groups have separate `.available` and `.valid` flags. An absent/broken sensor has `available=false`; a working detector with no return has `valid=false`. Invalid numeric values are zero, vectors are zero, tokens are empty strings. Values must always be guarded; zero is not evidence of cold water, contact, or zero distance. Internal energy, memory, body state, and owned construction progress remain core self-signals.

## 6. Replace semantic target sensors

Remove `food.*`, `prey.*`, `threat.*`, and the broad `neighbor.*` family in the new language version. Also remove free external `light`, temperature, flow, and nutrient readings; their replacements require the corresponding sensor. Do not replace these with a renamed omniscient “edible” flag.

An optical return can expose `object.valid`, `object.token`, estimated body-relative `object.offset`, apparent area, contrast, observable pigment, and estimated relative motion. These measurements have hardware-dependent quantization/error; they are not exact hidden-state reads. They cannot expose another cell's energy, genome, integrity, attack allocation, exact mass, or feeding strategy. Particles and cells share this format. Appearance follows visible surfaces/contents, not assigned roles.

Keep the initial language small: expose one selected optical return and one contact return per tick. A scan direction/width and optional remembered object token select among physically detectable returns. Selection is deterministic and based on angle, contrast and distance; energy content must not influence ranking. Optical range is capped by built hardware, environmental visibility, and toroidal geometry. Local spatial queries must continue to scale with nearby objects.

Tokens identify observed objects without granting remote access. Use bounded opaque strings compatible with existing registers and a fixed-size track cache. Tokens remain stable during a continuous track, never alias a different object, and expire after a documented observation gap. Identifier exhaustion fails explicitly instead of wrapping. Persist tracks for checkpoint replay. A remembered token can request reacquisition but cannot reveal position outside detection. Scan controls take effect on the next tick.

Chemoreception samples local concentration and gradient. Add an explicitly tracked organic-cue field sourced by exudation, decay, and leakage; do not query the nearest consumer or inject a direction to “food.” Cues can lead to barren patches, competitors, or dangerous cells. Contact sensation reports surface pressure and physical resistance, while successful ingestion reveals material through the cell's own gut changes.

Attacks and capture act on actual contact within the apparatus's reach and facing. A selected token may guide aiming, but it cannot extend reach or make unseen remote damage possible. A cell can attempt contact feeding without an optical detector; it simply cannot inspect the result at a distance. Built-in programs infer potential meals and risks from measurements and remembered outcomes.

## 7. Survival, inheritance, and ecology

Founders use an explicit starter body recipe and resource budget, selected alongside the genome. Starter organelles are charged against that introduction budget. Record the introduced energy/material in metrics; do not evaluate `build.*` once to create a free specialist. Include viable producer, scavenger, hunter, and mixed starter recipes. A bare-cell recipe needs sufficient reserves and baseline construction to reach its first energy source.

Division requires sufficient grown frame, energy, core machinery and a feasible partition of actual organelles. Distributed material and reserves split conservatively. Discrete organs are assigned to daughters deterministically; they cannot be cloned. Organelles that support fission must first accumulate enough material for two functional units and pay their splitting cost. A daughter may inherit no flagellum or chloroplast; the genome must anticipate this, or the lineage may fail.

Preflight both daughters' packing and essential core requirements. If an organelle cannot fit either partition, division waits; no silent resizing or deletion is allowed. Construction records follow their physical structures. The parent resamples tracks from its new position; the daughter starts with no tracks. Preserve current memory inheritance: the parent keeps registers; the daughter starts empty. Mutations change the inherited program; new organs still require construction. Age, lifespan, and unrelated ecosystem rules need not change.

Damage reduces effective machinery without automatically freeing its occupied space. Repair consumes resources; destroyed material becomes debris. Capacity loss uses the same overflow path as dismantling. Death releases frame, organelles, reserves, and gut contents once.

Producers compete for light and minerals, consumers for accessible organic material, and hunters need capture/damage, digestion, movement, and appropriate sensing. Mixed strategies occupy both sets of machinery. Retain finite local light sharing and digestive assimilation losses. Physical tradeoffs should produce scarcity without guild quotas, free reseeding, or protected predators.

## 8. Implementation order and acceptance

1. **Accounting and geometry:** split area/mass/energy ledgers, add module definitions and packing invariants, then construction, recycling and capacity overflow. Keep existing behavior as a temporary adapter only while this step is incomplete.
2. **Capabilities and lifecycle:** route actions through built structures; add starter bodies, paid organelle duplication, deterministic division and checkpoint state. Remove instant founder specialization and fractional remodeling.
3. **Physical sensing:** implement separate light/heat/chemical/mechanical/optical hardware and generic returns; add cue transport and bounded tracking. Rewrite all built-in programs before removing old sensor names and investment outputs together.
4. **Inspection and ecosystem tuning:** show space/surface usage, actual organs, progress, damage, requested versus achieved actions, and unavailable measurements. Tune resource costs only after correctness checks pass.

Acceptance must demonstrate: no net energy/material gain over build/recycle/divide/death cycles; bounded space at every intermediate step; no capabilities from unbuilt modules; fair simultaneous resource allocation independent of source order; safe storage loss; no organelle duplication at division; absent sensors reveal no hidden state; generic observations do not encode organism class; stale tokens cannot target replacements; deterministic checkpoint continuation; and viable complete producer–consumer growth/reproduction cycles from paid founder budgets. Multi-seed ecosystem runs should report extinctions as well as persistence, without claiming guaranteed equilibrium.

This is a body/language schema change. Give older saves and genomes an explicit unsupported-version explanation unless a separately designed, tested conversion is added; do not reinterpret old body fractions or omniscient readings as paid organelles silently. Preserve old exported files. Implementation, compatibility conversion, and numerical balancing remain future work.
