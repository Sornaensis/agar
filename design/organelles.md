# Organelle growth and physical sensing

**Status: implemented in engine ABI 10 / checkpoint 4 / language 3.** This replaces the six `invest.*` targets with structures a cell must build, maintain, and inherit. The genome determines the body's development and remains a pure reactive program with synchronous registers. These are simulation rules, not a claim of biological fidelity.

## 1. Starting point and intended change

The previous model used one `size` for biomass, collision area, reserve capacity, and growth, with six fractional investments and exact target sensors. The implementation in [body.rs](../engine/src/body.rs), [simulation.rs](../engine/src/simulation.rs), and [the built-in programs](../src/presets.ts) replaces that model with physical structures and generic observations.

Construction is constrained by space, materials, energy, and assembly throughput. Growing the enclosing body creates room without automatically creating capabilities. Capabilities require functional structures. A changed genome changes construction requests; it never swaps the body instantly.

## 2. Separate space, material, and energy

Use a uniform-depth 2D abstraction. Store these quantities separately:

- **Envelope area** determines collision radius, internal space, and light interception geometry. Radius is proportional to the square root of area. Supported area requires paid structural frame and membrane material.
- **Material mass** equals scaffold plus organelles plus unassembled proteins plus unbound reserve/gut minerals, each counted once. Retain the existing mineral currency as the conserved material proxy; this is not a carbon chemistry model. Energy does not create inertial mass. Mineral-free organic energy still occupies digestive/storage capacity; inertia uses material plus displaced water.
- **Usable energy**, **undigested energy**, and **embodied structural energy** have distinct ledgers. Construction spends usable energy; only an explicitly retained portion becomes recoverable structural energy. The rest dissipates.

Internal occupancy includes the essential core, organelle footprints, unassembled protein stocks, and storage compartments. Compartment contents occupy their reserved compartment capacity, rather than being counted twice. External structures consume membrane attachment space and internal support machinery. Membrane capacity scales with perimeter, so doubling area does not double attachment capacity.

Required invariants are:

1. Internal occupancy never exceeds supported area; attachment footprints never exceed usable perimeter.
2. All structural material comes from cell reserves. Growth transfers material into frame or organelles; it cannot create minerals.
3. Every joule of recovered structural energy came from a recorded construction input. Build–dismantle and death–consume cycles cannot generate energy.
4. Area, mass, and capabilities are independent derived quantities. Adding an organelle inside an existing envelope must not create more room for itself.
5. Structural damage cannot erase material or compartment contents. Material is retained as damaged structure, released as debris, or returned to water according to explicit rules.

Movement uses actual material mass plus a water-displacement term for inertia, with drag derived from envelope and external structures. Basal maintenance depends on supported body and machinery, so empty space is not free. Language version 3 replaces the overloaded `size` with explicit `body.area` and `body.mass`. The former size-4 growth ceiling is removed; the separate collision and wrapping safety bound is documented in section 9.

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

Family-specific `express.*` controls synthesize paid assembly proteins. Proteins occupy internal space, cost upkeep and turn over; they are not functional organs. Count/size targets assemble organs from matching protein present before the current tick. Expression and assembly compete for one finite core-work budget. Compute all demands from the same snapshot, then proportionally allocate the available energy, minerals, internal area, membrane space, and assembly time. Resolve deterministic ties by module family and stable local identifier, never statement order. Large requested counts must not allocate large arrays; instantiate only affordable progress.

Build an envelope before filling its new space. Frame growth consumes energy and minerals, changes collision geometry, and raises upkeep and drag. Organelle construction fills existing capacity. Incomplete structures occupy space and hold material but provide no capability below their functional threshold. Above it, capability follows built size and integrity. Completion or growth affects actions on the following tick.

Dismantling is an explicit, rate-limited request. It costs processing energy and releases recoverable material; it never refunds the full construction energy. Recovered minerals beyond reserve capacity go to water. Digestive or reserve compartments cannot be removed while silently discarding contents: first transfer contents into available compartments, then release overflow as accounted particles/minerals. Automatic destruction follows the same accounting.

Lower targets do not instantly remove organs. Build/recycle requests for one family resolve to one net request before resources move. Essential core cannot be recycled below its viable minimum. Optional construction cannot spend energy committed to maintenance.

## 5. Language interface

Outputs remain declarative and statically typed. The following is valid language-version-3 code:

```text
# Targets describe organs; expression supplies matching construction proteins.
target.flagellum.count = 2;          # integer, nonnegative
target.flagellum.size = 0.08;        # scalar, module material units
express.flagellum = if energy.fraction > 0.65 && organelle.flagellum.mass + protein.flagellum < 0.16 then 1 else 0;
target.photoreceptor.count = 1;
target.photoreceptor.size = 0.02;
express.photoreceptor = if organelle.photoreceptor.mass + protein.photoreceptor < 0.02 then 0.3 else 0;
recycle.flagellum = 0;

grow = if body.space.free < 0.05 then 0.5 else 0;
turn = if light.available then clamp(light.bearing * 2, -1, 1) else 0;
thrust = if energy.fraction > 0.3 then 0.4 else 0;
divide = body.maturity >= 1 && body.division_ready;
```

`express.*` and `recycle.*` are scalars clamped to 0–1; counts are nonnegative integers and sizes are bounded finite scalars. Targets are recomputed each tick. Omitted count targets are zero, size targets use the module's minimum, and expression/recycling requests are zero. Existing organs remain intact without outputs; existing protein may finish previously requested assembly after expression is switched off. `build.*` is retained as a source compatibility alias for `express.*`. Lower targets remove organs only with explicit recycling effort. No organelle address bank or new register type is needed.

Read `protein.<family>` for matching construction stock and `expression.rate`, `assembly.rate`, `assembly.capacity` for actual translation/assembly and common work capacity. Indiscriminate expression continues after organs meet their targets, occupying room and spending resources on unused protein. The built-ins regulate synthesis with actual mass, protein supply, energy and developmental phase.

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

Every introduced founder has the same minimal physical structure: area 1, one core of mass 0.04, 68 usable energy and 0.6 free minerals in the core's bounded compartments. Its frame, core, reserves and fabrication costs count as resource introductions. The genome is the complete developmental plan: all optional organs and additional cores must be built through normal resource-limited ticks. There is no separate body selection, implicit lookup by genome name, or instantaneous construction at introduction.

Built-in programs prioritize an initial energy-acquisition path, use actual functional organelle signals to regulate subsequent construction, and prepare duplicates for division. A program that fails to build a working energy path can exhaust its initial reserves. Those developmental choices belong to editable, mutable source code; the engine applies the same physical construction rules to every genome.

Division requires sufficient grown frame, energy, core machinery and a feasible partition of actual organelles. Distributed material and reserves split conservatively. Discrete organs are assigned to daughters deterministically; they cannot be cloned. Organelles that support fission must first accumulate enough material for two functional units and pay their splitting cost. A daughter may inherit no flagellum or chloroplast; the genome must anticipate this, or the lineage may fail.

Preflight both daughters' packing and essential core requirements. If an organelle cannot fit either partition, division waits; no silent resizing or deletion is allowed. Construction records follow their physical structures. The parent resamples tracks from its new position; the daughter starts with no tracks. Preserve current memory inheritance: the parent keeps registers; the daughter starts empty. Mutations change the inherited program; new organs still require construction. Age is recorded without a maximum lifespan. Cell death follows energy exhaustion, damage or loss of essential machinery; no age timer kills a healthy cell.

Damage reduces effective machinery without automatically freeing its occupied space. Repair consumes resources; destroyed material becomes debris. Capacity loss uses the same overflow path as dismantling. Death releases frame, organelles, assembly proteins, reserves, and gut contents once. Structural material and accessible cell reserves are tissue. Existing gut exudate, tissue and waste retain their identities through engulfing, spills and death; predation never upgrades waste into fresh tissue.

Producers compete for light and minerals, consumers for accessible organic material, and hunters need capture/damage, digestion, movement, and appropriate sensing. Mixed strategies occupy both sets of machinery. Retain finite local light sharing and digestive assimilation losses. Physical tradeoffs should produce scarcity without guild quotas, free reseeding, or protected predators.

## 8. Implementation order and acceptance

1. **Accounting and geometry:** split area/mass/energy ledgers, add module definitions and packing invariants, then construction, recycling and capacity overflow. Keep existing behavior as a temporary adapter only while this step is incomplete.
2. **Capabilities and lifecycle:** route actions through built structures; start introduced cells from one common minimal structure, then use genome-directed growth, paid organelle duplication, deterministic division and checkpoint state. Remove instant founder specialization and fractional remodeling.
3. **Physical sensing:** implement separate light/heat/chemical/mechanical/optical hardware and generic returns; add cue transport and bounded tracking. Rewrite all built-in programs before removing old sensor names and investment outputs together.
4. **Inspection and ecosystem tuning:** show space/surface usage, actual organs, progress, damage, requested versus achieved actions, and unavailable measurements. Tune resource costs only after correctness checks pass.

Acceptance must demonstrate: no net energy/material gain over build/recycle/divide/death cycles; bounded space at every intermediate step; no capabilities from unbuilt modules; fair simultaneous resource allocation independent of source order; safe storage loss; no organelle duplication at division; absent sensors reveal no hidden state; generic observations do not encode organism class; stale tokens cannot target replacements; deterministic checkpoint continuation; and viable complete producer–consumer growth/reproduction cycles from paid founder budgets. Multi-seed ecosystem runs should report extinctions as well as persistence, without claiming guaranteed equilibrium.

This is a body/language schema change. Give older saves and genomes an explicit unsupported-version explanation unless a separately designed, tested conversion is added; do not reinterpret old body fractions or omniscient readings as paid organelles silently. Preserve old exported files. This implementation rejects pre-organelle checkpoint versions 1/2 and preserves their saved records. A tested checkpoint-3 import keeps actual organs/source, classifies legacy untyped organic matter as tissue, and starts new protein pools empty. New saves use checkpoint 4; future trajectories use the updated rules. Numerical coefficients are exposed in the source and remain tunable; ecological persistence is measured rather than guaranteed.


## 9. Implemented limits and inspection

- There are 16 organelle families. Each record has a local identity, material mass, integrity, and embodied energy. `.count` includes incomplete units; `.building` is the material still missing before unfinished units become functional. `.capacity` is effective mass divided by that family’s minimum functional mass.
- Supported area is 0.25 to `(760 / (8 * 7))²` (about 184.18). The upper bound keeps a cell’s diameter within one quarter of the shorter torus dimension. Local collision and photon queries account for actual body radius. It is independent of population; there is no fixed organism or organic-particle count ceiling.
- The scaffold costs 0.35 material per unit of area. Internal capacity is `0.9 * area`; membrane attachment capacity is `0.9 * sqrt(area)`. Contents use the capacity of their physical compartments and do not occupy the same internal space twice.
- Growth uses a finite shared assembly budget. New space, machinery, and recovered material cannot fund another construction request in the same tick. Continuous actions use the machinery present at the beginning of the tick. Energy and mineral overflow after compartment damage, recycling, or division follows one explicit spill path.
- Optical sensing requires illumination, checks line of sight against local cell silhouettes, and quantizes appearance and motion according to hardware. Additional optical units broaden angular coverage; more effective sensor mass improves reach and resolution. Direction and width requests apply to the next tick.
- Each cell retains at most eight observation tracks. A track expires after 30 unobserved ticks (one second). Opaque strings are stable for continuous observations and are never reused for a different object. Remembered tokens cannot bypass range, light, occlusion, or field of view. The daughter starts with no tracks; the parent must reacquire from its new position.
- Organic chemical cues are dimensionless, transported, diffused, and decaying local fields. They are emitted by organic decay, exudation, and damage. They are neither food energy nor mineral material and cannot be consumed.
- The Body inspector exposes actual space, membrane usage, individual organelles, functional thresholds, damage, the latest construction requests, and protein stocks and achieved expression/assembly/recycling/repair rates. The program viewer retains native typed expression values and all 169 read signals / 81 write signals.
- Checkpoint version 4 stores individual organs, family protein stocks and embodied energy, typed particles and gut pools, resource ledgers, scan settings, observation identity sequences, and the organic cue field. The world snapshot uses 64-bit values to preserve exact 32-bit organism IDs. Source packages include the current WASM artifact and static GitHub Pages build.

The acceptance checks live in the Rust body/simulation/sensing suites and the WASM, language, inspection, storage, and food-chain scripts. Measurements in `experiments/` identify their engine ABI and binary hash; older studies describe the earlier model.

## 10. Organic matter and expression tradeoffs

Organic matter has three physical substrate classes: soluble exudate, cell tissue and digestive waste. Each keeps its energy and mineral contents through storage and transfers. `digest.exudate`, `digest.tissue` and `digest.waste` allocate finite digestive work; `gut.<kind>.energy` and `.nutrients` expose owned gut contents. Tissue is slower than exudate and leaves waste; waste is slower again and yields less usable energy. Waste processing and decay return the unrecovered mineral portion to water and dissipate energy. No recycling or predation loop increases food quality or creates resources.

Protein synthesis consumes minerals and 60% of the organ family's construction energy; final assembly consumes the remaining 40%. The matching work costs share `0.55 × functional core mass` per second. Protein occupies one unit of internal space per material, adds 0.6 energy upkeep per mass per second, and turns over with a 45-second half-life. No per-family stock limit exists. Targets alone provide no organs; expression alone can fill a cell with useless stock. Feedback and developmental ordering determine whether the same starting resources become productive machinery or wasteful upkeep.

Correctness checks cover healthy cells past 300 seconds, particle creation/restoration above 1,200, subtype preservation, distinct digestion yields, shared enzyme capacity, nonproductive-expression cost, delayed assembly, exact material conservation, no net energy creation, and pool inheritance/checkpoint continuation.
