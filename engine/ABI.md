# agar engine

## Whole-count chemistry in ABI 21

`set_count_chemistry(1)` selects whole-count arithmetic in a fresh world before introducing cells or advancing. `uses_count_chemistry()` reports the active mode after load. The player selects this mode by default on new simulations. Checkpoint 17 persists it, the raw inventories, integer world totals and the remainders used by slow processes. Checkpoints 3–16 restore using continuous arithmetic.

A free ATP unit, mineral unit or carbon/gas unit in the older APIs represents 10,000 counts. Inventories are unsigned 32-bit. Existing physical snapshots expose them as count / 10,000 for backward compatibility. In a `cellular counts;` program resource reads and product pools are typed as `count`, and ingredient declarations take whole counts. Comparisons are exact, count addition/subtraction/multiplication saturate, and division returns a scalar. Old `cellular;` and action-language sources retain their units. Body size, construction progress, ratios, concentrations and physical control signals retain their scalar meaning.

`count_audit_ptr()` returns a pointer to twelve unsigned 64-bit integers in counted mode, or zero in continuous mode. Copy the values before requesting another mutable engine operation.

| Offsets in u64 values | Held | Supplied | Removed or consumed |
| --- | --- | --- | --- |
| 0–2 | All minerals | Founder, source and player inputs | Sinks and player withdrawals |
| 3–5 | All inorganic and organic carbon | Founder, source and reservoir inputs | Sinks and reservoir exports |
| 6–8 | Oxygen | External input plus photosynthetic production | External exports, respiration and decay |
| 9–11 | Stored ATP plus chemical and construction energy | Introduced energy plus light used for fixation | Explicit energy withdrawals |

For the first three rows, held plus removed equals supplied. In the energy row, supplied less held and withdrawn is dissipated energy.

## Cellular extension

The engine accepts an explicit `cellular;` source mode. It includes typed product declarations, gene expression, physical recipes, pool/concentration/built reads, named receptor observations, pure functions, assembly initiation, activity and paid recycling. Existing unmarked programs retain the action/register language documented below. See [cellular language and accounting](../design/cellular.md) for the implemented rules.

`language_schema_ptr` retains schema version 3. Its additive `cellular.mechanisms` array lists each canonical constructor, typed target, required material roles, effect description and typed `receptorFields` with `requiresValid` flags. `cellular.recipeControls` describes activity, start_when and recycle types/defaults; `cellular.userFunctions` gives numeric-parameter and inlining limits. Top-level `functions` supplies builtin names and arities. Graph/evaluation JSON identifies its mode; cellular roots describe genes, activity, boolean initiation and recycling. `cell_body_ptr` adds material pools, ingredient-level assemblies, lifecycle progress and sampled construction/recovery controls. Compact rendering remains unchanged; ABI 12 extends full cell records and aggregate metrics with gas/carbon state. Render summaries do not grant legacy family capabilities to mechanisms.

Mutation kinds additionally accept gene silence (4), gene duplication (5), and future placement changes (6). Checkpoint 16 stores named precursor inputs, bond endpoints, recruitment progress, contact declarations, independent signal channels, junction diagnostics, the cellular graph, mechanism identities, construction controls and physical state; readers also accept versions 3 through 15. Bare family kinds retain their combined capabilities; constructor kinds such as `store(food)` and `digest(tissue)` each grant one explicit operation. The same minimal paid founder core bootstraps both modes. A derived adhesin product names its scaffold precursor in the program and in the current product entries of `cell_body_ptr`. Its finishing reaction consumes existing stock, ATP and shared work. The number of occupied bonds cannot exceed either endpoint’s capacity of one site per 0.0015 mass of exposed, active adhesin.


The dependency-free Rust crate owns simulation, genome compilation, mutation, and cell memory. TypeScript loads WASM, renders snapshots, and stores browser-local catalogs/checkpoints. There is no simulation server or JavaScript simulation fallback.

## Physical bodies and lifecycle

The world defaults to 1,200 × 760, supports dimensions up to 4,800 × 3,040, is toroidal and advances at a fixed 1/30 second. Programs sample body state, hardware measurements, and old memory before action resolution. Construction changes capabilities on the following tick. Newly captured organic material enters digestive storage before later assimilation.

Envelope area, mechanical mass, ATP-like usable energy, carbon, dissolved gases, digestive contents, and embodied structural energy are distinct ledgers. Radius is `7 × sqrt(body.area)`. Growing the enclosing frame costs minerals, organic carbon and usable energy; it creates space without creating organelles. Organelle material must come from cell reserves. Internal footprints are limited by body area, and membrane attachment capacity scales with perimeter. Damaged or incomplete structures still occupy space and retain their material. Separate carbon reserves and gas pools do not add to the legacy mechanical `body.mass`.

The former area-4 ceiling is removed. The remaining geometric bound is `(760 / 8 / 7)²`, approximately 184.18 area units, giving a maximum radius of 95 world units regardless of world resize. This bounds toroidal contact geometry, separately from physical growth costs. There is no population cap or maximum cell age. Energy exhaustion, damage and loss of essential machinery still cause death. Organic particles have no count cap; the genome registry remains bounded to 2,048 entries and 64 MiB of estimated allocations. Allocations and identifier exhaustion fail explicitly.

Every organelle has a stable local identifier, family, built mass, integrity, and recorded embodied energy. Family targets do not grant capabilities. Family-specific expression spends minerals, organic carbon and usable energy to produce persistent assembly proteins. Count/size targets assemble organs only from matching protein already present before the tick. Translation and assembly share core work and physical budgets; leaving an irrelevant gene on still produces costly protein. Competing requests share finite budgets; source statement order does not confer priority. Lowering targets leaves structures intact until explicit recycling. Recycling and repair are paid operations. Core machinery cannot be dismantled below the viable minimum. Compartment loss returns mineral/carbon/gut overflow to the environment; excess usable energy dissipates.

Every introduced founder has area 1, one core of mass 0.04, 68 usable energy, 3.4 reserve carbon and 0.6 free minerals. Its introduction ledgers record 0.99 minerals (0.39 structure plus 0.6 reserve), 3.79 organic carbon (0.39 structural plus 3.4 reserve), and 175 energy (68 usable, 28.08 embodied, 3.12 fabrication heat and 75.8 baseline chemical potential). Internal dissolved gas pools start empty and fill through finite passive exchange. No optional organelles are present. The same constructor is used for every genome; the program must build its own capabilities through ordinary paid ticks. Mutations change construction requests, not existing physical organs.

Division requires doubled grown frame, sufficient reserves, core machinery for two viable bodies, and a feasible partition. Actual discrete organs are assigned to daughters; they are not copied. Protein pools and their embodied energy split proportionally with cytoplasm. Producers must construct sufficient photosynthetic hardware for both descendants if both are to remain producers. The parent retains memory. The daughter starts with fresh registers and applies its program’s optional publication initializer. Death releases internal gases once and returns the complete remaining food potential. Tissue carries structural carbon, organic reserves, minerals, actual stored fabrication energy and remaining ATP. Existing gut substrates retain their separate compositions. Whole-cell engulfment transfers the same contents into available gut space and costs 8 ATP per unit of prey area. Damage, dismantling, division, and death cannot create minerals or recover unrecorded construction energy.

### Protein expression and assembly

Both language modes retain 90% of ATP paid to fabricate new material and dissipate 10%. The recorded investment follows each built item through fission. Maintenance, movement and the act of division consume energy separately.

`protein.<family>` reports conserved construction material awaiting assembly. It counts toward body mass and internal space at one space unit per material unit. Its upkeep is 0.6 energy per material per second and its half-life is 45 seconds. Turnover returns its mineral and organic-carbon mass and dissipates its embodied energy; released room/material can fund work on later ticks.

Translation uses 60% of the family's construction energy and 0.6 units of shared core work per material; assembly uses the remaining 40% and 0.4 work. Shared capacity is `0.55 × functional core mass` per second. Thus `0.6 × synthesized mass + 0.4 × assembled mass`, plus other paid work, cannot exceed that tick's common work budget. There is no per-family protein-stock quota; energy, minerals and actual packing limit synthesis. Expression can continue after targets are complete, so programs should regulate it using protein stock and built mass.

### Organic substrates

New habitats begin with zero organic particles. Producer secretion, physical digestive residues, compartment overflow and dead biomass supply particulate food. Dissolved minerals, oxygen and inorganic carbon can be supplied or withdrawn by their respective water sources and sinks.

Organic kinds retain separate energy/mineral gut pools and are not interchangeable:

| Kind | ID | Relative processing speed | Organic-carbon assimilation fraction | Aerobic decay / second |
| --- | ---: | ---: | ---: | ---: |
| Exudate | 0 | 1.2 | 0.80 | 0.040 |
| Tissue | 1 | 0.7 | 0.65 | 0.012 |
| Waste | 2 | 0.35 | 0.32 | 0.006 |

The three `digest.*` efforts share one digestive work budget. Digestion transfers chemical substrate from the gut into organic-carbon reserves and costs usable energy; it does not generate usable energy. Exudate/tissue/waste carbon assimilation fractions are 0.80/0.65/0.32. Every unassimilated carbon atom remains in a physical waste residue. Mineral assimilation fractions are 1.0/0.8/0.5, with returned or retained minerals fully accounted. Newly fixed carbon carries 20 energy units per carbon. Recovered tissue can carry additional energy previously held by the prey. Digestion sends the corresponding fraction of this energy with its carrier carbon into reserves or waste. Oxidizing one organic-carbon unit consumes one oxygen, produces one inorganic carbon and recovers 80% of that material’s actual chemical energy as ATP. Ordinary substrate therefore yields 16 ATP per carbon. Finite digestive work, respiration machinery and ATP headroom bound the realized rate. The primitive `respire(oxygen)` requires enzyme and membrane ingredients. Physical core/legacy metabolic machinery supplies its existing respiratory capacity; neither genotype names nor organism labels control gas reactions.

Background particle decay is an explicit oxygen-limited reaction using the rates above. It consumes actual available O2 and organic carbon together, returns matching inorganic carbon and mineral content, and increments the decay-respiration ledger. Without oxygen, particles retain their carbon. Exudate comes from carbon fixation and supplied organic particles. Death and extracted cell reserves contribute tissue. Engulfed gut contents retain their substrate identity. All transfers preserve carbon and mineral mass and cannot create energy.

Same-kind nearby emissions may merge by adding their exact contents. Counts never cause particle deletion or conversion. Generic senses report appearance and contact properties, not a semantic prey/edibility classification.

### Organelle families

Family order is part of the ABI. The minimum is material per functional unit; incomplete units occupy space but supply no capability below it.

| Index | Family | Minimum mass | Capability |
| --- | --- | --- | --- |
| 0 | `core` | 0.04 | Essential metabolism, baseline construction and repair |
| 1 | `photosynthetic` | 0.04 | Capture shared local light |
| 2 | `metabolic` | 0.03 | Energy-conversion throughput |
| 3 | `digestive` | 0.04 | Digestive storage and processing |
| 4 | `reserve` | 0.04 | Additional energy and mineral storage |
| 5 | `transporter` | 0.015 | Dissolved mineral uptake |
| 6 | `phagocyte` | 0.025 | Physical contact capture |
| 7 | `flagellum` | 0.025 | Thrust and steering |
| 8 | `piercer` | 0.025 | Contact damage and extraction |
| 9 | `cortex` | 0.02 | Surface protection |
| 10 | `repair` | 0.025 | Additional repair throughput |
| 11 | `photoreceptor` | 0.01 | Light measurements |
| 12 | `thermoreceptor` | 0.01 | Heat measurements |
| 13 | `chemoreceptor` | 0.015 | Dissolved mineral and organic cues |
| 14 | `mechanoreceptor` | 0.01 | Flow and contact measurements |
| 15 | `optical` | 0.02 | Generic object observations |

## Contact signaling and exchange

Cellular programs share the existing five sparse register banks and their synchronous write semantics. A program can publish a recognition marker and a second scalar payload using separate registers. Avoidance and adhesion selectors match the recognition marker. A literal on a publication initializes it at birth. Named contact receptors report the selected neighbor’s `message` and `message_valid`. The same observation has independent `signal` and `signal_valid` fields. Its `bonded` flag identifies whether this exact contact partner shares an established bond. The optional adhesion declaration suffix `daughters` retains a directly divided daughter if both sides qualify. This is the default. `recruit` also admits a free partner when both genomes opt in, both working surfaces recognize the other’s published marker, and contact continues for 15 ticks. Weapon filtering and reciprocal adhesion use start-of-tick values for their actual candidate. The Rust compiler's `cellular.registers` and `cellular.contactMessages` schema entries describe this surface. Contact declarations appear in program inspection, and `cell_memory_ptr` exposes occupied slots for cellular and legacy programs. The full cell and environment projections retain their layouts. `cell_body_ptr` adds `colony` with `neighbors`, the sorted `neighborIds` of actual partners, and three signed net amounts for the last exchange (`energyNet`, `mineralNet`, `carbonNet`). [Declaration and physical interaction rules](../design/cellular.md#register-memory-contact-messages-and-colonies)

## Language and registers

An immutable typed graph computes `(current measurements, old memory) → (requested actions, next memory)`. `let` names a sampled expression that re-evaluates each tick. Expressions are pure, with no recursion or user-controlled loops. Registers retain state through synchronous write/clear rules.

```text
# Counts are integers; unit sizes are material mass.
target.core.count = 2;
express.core = if organelle.core.mass + protein.core < 0.08 then 1 else 0;
target.flagellum.count = 2;
target.flagellum.size = 0.08;
express.flagellum = if energy.fraction > 0.65 && organelle.flagellum.mass + protein.flagellum < 0.16 then 0.4 else 0;
target.photoreceptor.count = 2;
express.photoreceptor = if organelle.photoreceptor.mass + protein.photoreceptor < 0.02 then 0.3 else 0;

rs001 = object.token when object.valid;
clear rs001 when !object.available;
scan.target = if populated(rs001) then rs001 else "";
scan.bearing = 0;
scan.width = 3.14;

turn = if light.valid then clamp(light.bearing * 2, -1, 1) else 0;
thrust = if energy.fraction > 0.3 then 0.4 else 0;
grow = if body.space.free < 0.05 && energy.fraction > 0.6 then 0.5 else 0;
divide = body.division_ready;
```

### Outputs

There are **81 outputs**: 11 base actions, four controls for each of 16 families, three scan controls, then three digestive enzyme controls. `thrust`, `turn`, and `divide` are required exactly once. All other outputs are optional. Scalar destinations accept integer expressions; integer, boolean, and string destinations require their declared type.

| Output | Type | Omitted value / range |
| --- | --- | --- |
| `thrust` | scalar | Required; 0–1 |
| `turn` | scalar | Required; −1–1 |
| `divide` | boolean | Required |
| `grow`, `photosynthesize`, `bite`, `defend`, `repair` | scalar | 0; clamp 0–1 |
| `engulf` | boolean | false |
| `absorb`, `feed` | scalar | 1; clamp 0–1 |
| `target.<family>.count` | integer | 0; negative results clamp to 0 |
| `target.<family>.size` | scalar | Family minimum; clamp from that minimum to 10¹², with physical construction further limited by geometry and resources |
| `express.<family>`, `recycle.<family>` | scalar | 0; clamp 0–1 |
| `scan.bearing` | scalar | 0; wrap body-relative radians to [−π, π) |
| `scan.width` | scalar | 2π; full angular width clamped to 0–2π |
| `scan.target` | string | Empty token |
| `digest.exudate`, `digest.tissue`, `digest.waste` | scalar | 1, 1, 0.35 respectively; clamp 0–1 |

The old `build.<family>` spelling aliases the same `express.<family>` destination and protein mechanics. Defining both spellings for one family is a duplicate-output error. Canonical schema/graph names use `express.*`; source text is preserved.

Counts remain integer-typed: `2` is accepted, `2.0` and `2 / 1` are scalar expressions and rejected. Huge target counts do not allocate huge arrays; the physical construction allocator admits only affordable progress. Omitted body controls leave existing organs intact. Scan controls take effect next tick; a remembered token can request reacquisition only through ordinary current visibility. Each cell retains at most eight tracks; a track expires after a gap longer than 30 ticks (one simulated second). Tokens are never reused for replacement objects.

Locals can reference only earlier locals and cannot shadow sensors, functions, outputs, keywords, or register addresses. Registers can be read regardless of rule order. Statements end in semicolons; `#` begins a line comment.

| Addresses | Type | Empty read |
| --- | --- | --- |
| `ri001`–`ri999` | Signed 32-bit integer | `0` |
| `rf001`–`rf999` | Finite scalar | `0.0` |
| `rb001`–`rb999` | Boolean | `false` |
| `rs001`–`rs999` | String, up to 128 UTF-8 bytes | `""` |
| `rv001`–`rv999` | 2D vector of finite scalars | `vec(0, 0)` |

Exactly three address digits are required: `000`, `1000`, and abbreviated addresses are invalid. There are 999 slots per bank, 4,995 total. Storage is sparse. Empty reads do not populate slots; explicitly storing a default value does. `populated(register)` reports occupancy.

`ri001 = expression;` writes each tick. Append `when boolean_expression` for a guarded write. `clear ri001;` removes a value, also optionally guarded by `when`. Each address permits one write and one clear rule. All RHSs, guards, and outputs read old memory; writes commit together. **Clear wins** if both rules fire. Otherwise, no firing rule preserves the old value.

Each populated slot costs **0.0025 energy per simulated second**, charged from start-of-tick occupancy. New writes start costing energy next tick; clears stop that cost next tick. Stored zero, false, empty text, and zero vectors still cost energy. Introduced cells and daughters start with independent empty memory.

### Primitive operations

Integer-form literals within i32 range are integers; decimal/exponent literals and larger finite integer-form literals are scalars. Integer arithmetic saturates at i32 bounds. Mixed numeric operations widen to scalar; `/` always returns scalar. `int(x)` truncates and saturates; `float(x)` widens. Integer writes require integer expressions; scalar writes also accept integers. Conditional branches need matching types or compatible numeric types.

Scalars/vector components saturate at ±10¹². Division by zero and NaN yield zero; infinite arithmetic saturates. Negative square roots, zero-vector normalization, and zero-vector angle return zero. Both conditional branches evaluate as safe, pure graph nodes.

| Operations | Semantics |
| --- | --- |
| `+`, `-`, `*`, `/`, unary `-` | Numeric; vectors also support addition/subtraction, scalar multiplication/division, and negation |
| `<`, `<=`, `>`, `>=` | Numeric comparison |
| `==`, `!=` | Matching primitive types or compatible numeric types |
| `!`, `&&`, `||` | Boolean |
| String `+` | Concatenation, truncated at a UTF-8 character boundary to 128 bytes |
| `sin`, `cos`, `abs`, `min`, `max`, `clamp`, `sqrt`, `sign` | Numeric helpers; clamp accepts reversed bounds |
| `atan2(y, x)`, `lerp(a, b, t)`, `noise(seed, phase)` | Scalar helpers; noise is smooth deterministic interpolation in −1 to 1 |
| `activate(c, k)`, `repress(c, k)` | Saturating scalar responses; clamp arguments nonnegative, use c/(c+k) with zero-denominator result 0, and its complement |
| `vec(x, y)`, `x(v)`, `y(v)`, `length(v)`, `dot(a, b)`, `normalize(v)`, `angle(v)` | Vector construction/components/geometry |
| `length(text)` | Integer Unicode-character count |
| `int(x)`, `float(x)`, `identity(value)` | Numeric conversion and type-preserving identity |

String literals use double quotes and accept `\"`, `\\`, `\n`, `\r`, `\t`, and four-digit `\u` escapes for individual valid Unicode scalar values. Oversized literals are rejected; concatenation and mutations remain bounded. There is no implicit string/numeric conversion.

### Read signals

There are **174 read signals**. Scalars are finite numbers; `organelle.<family>.count` is integer; availability, validity, and division-ready flags are boolean; tokens are strings; vectors are `vec2`.

| Group | Signals |
| --- | --- |
| Core state | `energy`, `energy.fraction`, `energy.capacity`, `age`, `time`, `speed`, `id`, `integrity`, `memory.used`, `memory.cost` |
| Body | `body.area`, `body.mass`, `body.birth_area`, `body.maturity`, `body.division_ready` |
| Packing | `body.space.free`, `.used`, `.capacity`; `body.surface.free`, `.used`, `.capacity` |
| Reserves and digestion | `nutrient.stored`, `.capacity`, `.fraction`; `gut.energy`, `.capacity`, `.fraction`; `digestion.rate`, `growth.rate`, `lifespan`, `exudation.rate` |
| Gut composition | `gut.exudate.energy`, `.nutrients`; `gut.tissue.energy`, `.nutrients`; `gut.waste.energy`, `.nutrients` |
| Internal chemistry | `carbon.organic`, `carbon.inorganic`, `oxygen` (amounts); `respiration.rate`, `carbon.fixation.rate` (carbon/s) |
| Protein and construction | `protein.<family>`; `expression.rate`, `assembly.rate`, `assembly.capacity` |
| Actual organelles | `organelle.<family>.count`, `.mass`, `.capacity`, `.damaged`, `.building`, for each family above |
| Light | `light.available`, `.valid`, `.level`, `.bearing` |
| Heat | `heat.available`, `.valid`, `.value`, `.bearing` |
| Chemistry | `chemical.available`, `.valid`, `.mineral`, `.organic`, `.mineral_bearing`, `.organic_bearing` |
| Flow | `flow.available`, `.valid`, `.vector` (vec2), `.speed`, `.bearing` |
| Contact | `contact.available`, `.valid`, `.token` (string), `.offset` (vec2), `.pressure`, `.resistance` |
| Optical return | `object.available`, `.valid`, `.token` (string), `.offset` (vec2), `.area`, `.contrast`, `.pigment`, `.motion` (vec2) |

Self-state is available through the core. Environmental groups require their corresponding functional receptor. `available=false` means absent/broken hardware; `valid=false` means no usable return. Invalid values are zero, zero vectors, or empty tokens. Programs must guard measurements; zero alone is not evidence of an object at zero distance or cold water. Photosynthesis does not grant light sensing, and light sensing does not grant photosynthesis.

Optical range falls with the square root of surface illumination; surfaces without sufficient measured contrast are invisible even when hardware is available. A center ray is occluded by intervening cell circles, including across wrapped edges; particles inside a cell are hidden by that surface. This is a bounded visibility approximation, not an optical ray tracer. Flow bearing derives from the measured vector, so it cannot bypass receptor resolution.

Particles and cells share generic optical/contact formats. The genome cannot read another organism's energy, genome identity, integrity, attack allocation, exact mass, or feeding strategy. Visible pigment and contrast describe surfaces rather than assigned ecological classes. Organic cues are transported field values, not directions to a chosen food object. Contact feeding and attacks recheck physical reach and do not require optical detection.

The previous `size`, `invest.*`, `food.*`, `prey.*`, `threat.*`, `neighbor.*`, free temperature/flow/mineral readings, and body allocation fractions are removed. They have no compatibility aliases. Old genome files remain exportable but require an explicit rewrite for language version 3.

### Authoritative language metadata

`language_schema_ptr()` returns JSON with `version: 3`, `signals: {name,type}[]`, `outputs: {name,type,required,default}[]`, `maxNodes`, `maxSourceBytes`, and `registersPerType`. Output defaults retain primitive types, including integer counts and the empty string token. Ordering is the same as graph roots and evaluation outputs. Frontends can compare catalogs against this schema without parsing Rust source or duplicating type inference.

### Bounds and mutation

Limits are **8,192 expression nodes**, **65,536 tokens**, **256 KiB UTF-8 source**, and **64 nested expression levels**, for both authored and generated programs. References, constants, operators, calls, and guards contribute nodes; comments and whitespace do not. Invalid compilation leaves the world unchanged.

Mutation kinds are any (0), constant (1), expression (2), reference (3), cellular gene silence (4), cellular gene duplication (5), and cellular placement (6). Regulatory mutations parse the exact inherited source, select one reachable node, and patch only its source location. Constants retain their primitive types and bounds. Expression mutations replace one compatible operator/function token or insert/remove one typed unary operation. Binary operator replacement preserves precedence, so unparenthesized neighboring operands do not regroup. References use compatible sensors, same-type register addresses, or earlier visible bindings. Mutations cannot introduce forward references, cycles, ill-typed guards, or illegal addresses. Register write destinations and omitted outputs remain unchanged, including omitted `grow`.

Placement changes patch or add a body-relative `at` or `direction` constant for future machinery. Inherited instances keep their original geometry and ingredients; an unmatched active instance has no current activity control, while a same-named recycling selector can recover it.

The complete patched source is typechecked again and becomes the child’s executable program. Every byte outside the edited node is retained, including comments, whitespace, declaration names, and unused definitions. Descendants inherit that source and accumulate one change per mutation; mutation never flattens the program into generated bindings. Checkpoint versions 3 and 4 require the compiled graph to match the exact stored source. Source maps are transient and reconstructed when inspecting or mutating a genome. Numeric compatibility is by type, not physical units. Validity does not guarantee survival or useful behavior.

No applicable change, oversized generated source, or exhausted registry capacity safely retains the parent. Default daughter mutation probability is 0.10. Mutation uses a separate deterministic stream derived from seed, tick, and cell IDs. Existing variants are reused before capacity checks, allowing reset to replay existing lineages. Division always pays its ordinary cost, including fallback inheritance. Successful automatic variants increment `mutation_count`; skipped attempts increment `mutation_skipped_count`. Draft mutations affect neither counter.

## Environment and ecology

Light, heat, flow, dissolved minerals, and organic cues are local fields. Sources supply light, heat, currents, or minerals. Photosynthetic organisms compete for the same finite local photons and minerals. Consumers require actual capture/digestive machinery and accessible organic material. Hunters additionally pay for movement, piercing/capture, and any receptors they use. A mixed organism must fit and maintain both sets of machinery.

Usable reserves, raw gut energy, mineral reserves, frame material, organelle material, and embodied structural energy remain distinct. Digestive assimilation and residue depend on the substrate, as listed above; inorganic minerals do not turn into free usable energy. Loss of storage capacity follows the same explicit overflow path as recycling or damage. Ecological outcomes emerge from resource and physical constraints; there are no guild quotas or guaranteed predator persistence.

## WASM API, version 18

The module exports `memory`. Pointers/lengths use bytes, except snapshot lengths, which count elements: float64 for the world and float32 for the environment. Returned buffers are engine-owned: read the matching length and copy immediately before other allocating calls. Refresh JavaScript views after memory growth. Never free engine-owned buffers. Caller-allocated source buffers must be freed once with their exact pointer/length.

| Exports | Behavior |
| --- | --- |
| `abi_version() -> u32` | Returns 20 |
| `alloc(len) -> ptr`, `dealloc(ptr, len)` | Allocate/free a zeroed source buffer; above 256 KiB returns null |
| `compile_genome(ptr, len) -> u32` | UTF-8 source → ID ≥ 1; zero plus readable error on failure |
| `analyze_genome(ptr, len) -> ptr`, `genome_analysis_len()` | Authoritative compiler analysis as engine-owned UTF-8 JSON; no genome registration or simulation mutation |
| `mutate_genome(id, seed, kind) -> u32` | Variant ID; parent ID plus explanation on safe no-op; zero for invalid ID/kind |
| `genome_source_ptr(id)`, `genome_source_len()` | UTF-8 source copy; null for invalid ID |
| `cell_evaluation_ptr(cell_id)`, `cell_evaluation_len()` | Read-only expression preview for one living cell as engine-owned UTF-8 JSON; null plus readable error for a missing/dead cell |
| `genome_graph_ptr(id)`, `genome_graph_len()` | UTF-8 JSON of the compiled expression graph; null pointer and zero length for invalid ID |
| `genome_parent(id)`, `genome_mutation_kind(id)` | Provenance; zero for authored/unknown genomes |
| `genome_count()`, `genome_limit()` | Current count / 2,048 |
| `genome_bytes()`, `genome_byte_limit()` | Estimated allocations / 64 MiB |
| `set_mutation_rate(rate: f32)`, `mutation_rate()` | Set/get probability, clamped 0–1; nonfinite input becomes zero |
| `mutation_count()`, `mutation_skipped_count()` | Successful/skipped automatic variants since reset |
| `error_ptr()`, `error_len()` | Latest UTF-8 error |
| `reset(seed, genome_id) -> u32` | Recreate world; 1 on success, 0 plus readable error on invalid genome or allocation failure |
| `seed_cells(genome_id, count: f64, x: f32, y: f32) -> u32` | Introduce the requested minimal cells atomically; return count added. Count must be a finite nonnegative integer within the addressable ID range. Invalid counts, ID exhaustion, and allocation failure return zero with a readable error; count zero is a successful no-op. Negative/nonfinite coordinates scatter globally; otherwise within 38 units |
| `place_cell(genome_id: u32, x: f64, y: f64) -> u32` | Introduces one ordinary founder at the exact wrapped coordinate and returns its cell ID. A blocked destination returns zero without advancing RNG or introductions. |
| `move_cell(id: u32, x: f64, y: f64, colony: u32) -> u32` | Relocates a living cell. Mode 1 translates its entire current, physically attached component by the same offset. Checks destination bodies, barriers and attached links before moving any member. Returns the number moved. A successful drop retains velocity, orientation and intracellular state. |
| `set_cell_stores(id: u32, mask: u32, energy: f64, minerals: f64) -> u32` | Bit 0 sets available ATP, bit 1 sets free minerals. Each selected value must lie within the current compartment capacity. Zero ATP triggers ordinary death, releases gases and returns remaining material. Returns 1 on success. |
| `energy_withdrawn_total()`, `mineral_withdrawn_total()` | Cumulative external withdrawals from cell compartments. Increasing either store increments the normal introduction ledger. Mineral absorbers have a separate removal ledger. |
| `tick(steps) -> u32` | Run at most 120 steps; 1 on success, 0 plus readable error if a step cannot allocate. Earlier completed steps in the batch remain valid |
| `snapshot_ptr()`, `snapshot_len()` | Build/read float64 world snapshot; length in elements |
| `render_snapshot_ptr()` | Compact geometry/appearance snapshot; read `presentation_len()` immediately afterwards |
| `render_links_snapshot_ptr()` | Compact render snapshot with a relationship kind for each established bond. Read `presentation_len()` immediately afterwards. |
| `metrics_snapshot_ptr()` | Aggregate metrics and per-genome population groups; read `presentation_len()` |
| `cell_snapshot_ptr(cell_id)` | Exactly one living cell in the 163-value layout; missing cell returns null with zero presentation length |
| `summary_snapshot_ptr()` | Seven values describing the current checkpoint summary and exact tick; read `presentation_len()` |
| `presentation_len()` | Element count for the most recent render, metrics, selected-cell or summary projection |
| `language_schema_ptr()`, `language_schema_len()` | Authoritative language-version-3 read/write signal names, types, defaults, and limits as UTF-8 JSON |
| `cell_body_ptr(cell_id)`, `cell_body_len()` | Actual organelles, body packing, construction requests, and achieved work as UTF-8 JSON |
| `cell_memory_ptr(cell_id)`, `cell_memory_len()` | UTF-8 JSON of populated registers; unknown cell returns `[]` and an error |
| `set_environment(temperature: f32, light: f32, current: f32)` | Clamp parameters; nonfinite values use 0.5/0/0 |
| `set_initial_gases(oxygen: f32, carbon: f32) -> u32` | Set next-reset amounts per unit-volume patch, each 0–100; 1 success, 0 invalid, no live reset |
| `set_gas_exchange(mode: u32, rate: f32, oxygen_equilibrium: f32, carbon_equilibrium: f32) -> u32` | Mode 0 sealed / 1 globally open; rate 0–1/s; equilibrium concentrations 0–100; edits preserve live inventories |
| `oxygen_dissolved_total() -> f64`, `inorganic_carbon_total() -> f64` | Current dissolved oxygen and effective inorganic-carbon amounts |
| `oxygen_input_total() -> f64`, `oxygen_removed_total() -> f64` | External oxygen introductions/removals, including initial water and signed reservoir exchange |
| `carbon_input_total() -> f64`, `carbon_removed_total() -> f64` | External dissolved inorganic-carbon introductions/removals; world metrics additionally count organic introductions |
| `set_initial_nutrients(level: f32)` | Set the next reset's dissolved mass per patch, clamped 0–5; nonfinite input uses 0.3. Live material is unchanged |
| `add_nutrients(x: f32, y: f32, amount: f32) -> f64` | Add fertilizer, clamped to 1,000; nonpositive/nonfinite amounts return zero. Negative/nonfinite coordinates distribute uniformly; otherwise deposit in the wrapped local patch. Returns actual added mass |
| `nutrient_total() -> f64`, `nutrient_input_total() -> f64` | Current total material / cumulative external input since reset, including initial water, cells, and food |
| `nutrient_dissolved_total() -> f64`, `nutrient_food_total() -> f64`, `nutrient_cell_total() -> f64` | Current water, food, and cell biomass-plus-reserve components |
| `add_source(kind, x: f32, y: f32, strength: f32, radius: f32) -> u32` | New ID, or zero for invalid input/capacity; clamp strength/radius |
| `add_source_configured(kind, x, y, strength, radius, angle, length) -> u32` | Add a complete source with one derived-cache rebuild; real parameters are f32 |
| `replace_sources(ptr, count) -> u32` | Atomically replace at most 64 sources with one rebuild. Each record is seven little-endian f64 values: kind, x, y, strength, radius, angle, length; real values retain scalar-ABI f32 precision. Buffer may be unaligned. Invalid lists preserve state; count zero accepts null and clears sources |
| `update_source(id, kind, x, y, strength, radius, angle, length) -> u32` | Update one source without replacing its ID or resetting live fields; all real arguments are f32; 1 success, 0 invalid/missing |
| `set_world_size(width: f32, height: f32) -> u32` | Resize to multiples of 20; width 400–4800, height 400–3040; preserve material and proportionally remap positions; reject incompatible barriers atomically |
| `nutrient_removed_total() -> f64` | Cumulative dissolved mineral mass exported by absorbers since reset |
| `add_barrier(x, y, length, width, angle: f32, floating: u32) -> u32` | Create an oriented rectangular fixed/floating obstacle; ID or 0 on invalid input/capacity |
| `update_barrier(id, x, y, length, width, angle: f32, floating: u32) -> u32` | Edit one obstacle; 1 success, 0 missing/invalid; float velocities reset on edits |
| `remove_barrier(id) -> u32`, `clear_barriers()` | Remove one/all barriers without changing material inventories |
| `remove_source(id) -> u32`, `clear_sources()` | Remove by ID (1 success, 0 missing) / all sources; existing heat persists |
| `environment_snapshot_ptr()`, `environment_snapshot_len()` | Build/read fields, sources and barriers; length in floats |
| `environment_metadata_ptr()`, `environment_metadata_len()` | Same 21-value header followed immediately by source and barrier records, omitting patch samples. Shares the environment projection buffer; length in floats |
| `predation_count()`, `attack_count()` | Predation deaths / contact bite hits since reset |
| `photosynthesis_total() -> f64` | Chemical potential stored by actual carbon fixation since reset: 20 × fixed carbon |
| `reset_empty(seed: u32) -> u32` | Start an empty habitat with zero organisms and zero organic particles. Retains the environmental recipe, mutation probability and genome registry. Returns 1. |
| `alloc_state(len)` | Allocate a checkpoint input buffer, 20 bytes–128 MiB; release using `dealloc(ptr, len)` |
| `save_state_ptr()`, `save_state_len()` | Build/read an owned versioned checkpoint; length in bytes; copy immediately |
| `restore_state(ptr, len) -> u32` | Validate and atomically replace world and registry; 1 success, 0 rejection with `error_ptr/len` feedback |

Unannotated integer arguments/counters are `u32`. Mutation probability returns `f32`.

`seed_cells` takes only genome, count and position. It never evaluates the genome during introduction. The program begins expressing construction requests on the next normal tick. Division partitions the parent's actual organs and does not call this founder constructor.

### Genome analysis

`Engine.analyze(source)` runs the same Rust lexer, parser, and type checker used by compilation. It returns:

```ts
interface GenomeAnalysis {
  valid: boolean;
  diagnostics: { severity: 'error'; message: string; line: number; column: number; from: number; to: number }[];
  bindings: { name: string; type: 'integer' | 'scalar' | 'boolean' | 'string' | 'vec2'; from: number; to: number }[];
  nodeCount: number;
  sourceBytes: number;
}
```

Ranges use zero-based UTF-16 offsets with exclusive ends, suitable for JavaScript strings and editor selections. Diagnostic line/column values are one-based; columns also count UTF-16 code units. Binding ranges cover the declared `let` identifier. Errors identify the real failing token or literal; missing input at EOF uses a zero-width range at the end of the document. Messages omit the legacy `Line ..., column ...:` prefix because coordinates are separate fields.

Valid source has no diagnostics. Invalid source returns the first compiler error; completed earlier declarations and the partial node count remain available after parser/type errors. Lexical errors return no bindings and a zero node count because lexing precedes parsing. `sourceBytes` counts the input's UTF-8 bytes, independently of its UTF-16 length. Oversized input is rejected before parsing, with an empty range at the document start.

Analysis changes only its transient response buffer. It does not consume genome IDs or registry space, change the latest engine error, advance RNGs or cells, or alter checkpoint bytes. It remains available when the genome registry is full. The native ABI accepts a null pointer for empty or oversized input; other nonempty inputs must provide a readable UTF-8 source buffer. Copy the returned JSON before another allocating engine call, and free only the caller's source buffer. The analysis JSON schema is unchanged in ABI 12.

### World snapshot

Layout: `12 + cells × 163 + food × 5` floats. The first 39 floats of each cell are physical/rendering state; the following 16 groups of six floats summarize organelle families in the order above. The world snapshot uses float64 values and a JavaScript `Float64Array`, preserving the entire u32 cell-ID range. Persistent state uses complete checkpoints.

```text
Header (12): tick, time, cell_count, food_count, total_births,
             total_deaths, total_eaten, width, height, reserved_zero,
             reserved_zero, reserved_zero
Each food (5): x, y, energy, nutrients, kind (0 exudate, 1 tissue, 2 waste)
```

| Cell offset | Field |
| --- | --- |
| 0 | `id` |
| 1 | `genome_id` |
| 2 | `x` |
| 3 | `y` |
| 4 | `vx` |
| 5 | `vy` |
| 6 | `angle` |
| 7 | `energy` |
| 8 | `age` |
| 9 | `generation` |
| 10 | `parent_id` |
| 11 | `radius` |
| 12 | `body_area` |
| 13 | `body_mass` |
| 14 | `birth_area` |
| 15 | `energy_capacity` |
| 16 | `basal_metabolism` |
| 17 | `memory_used` |
| 18 | `memory_cost` |
| 19 | `integrity` |
| 20 | `photosynthesis_rate` |
| 21 | `nutrient_stored` |
| 22 | `nutrient_capacity` |
| 23 | `nutrient_uptake_rate` |
| 24 | `gut_energy` |
| 25 | `gut_capacity` |
| 26 | `gut_nutrients` |
| 27 | `digestion_rate` |
| 28 | `growth_rate` |
| 29 | `maturity` |
| 30 | `lifespan` (0: no fixed maximum) |
| 31 | `exudation_rate` |
| 32 | `body_upkeep` |
| 33 | `space_used` |
| 34 | `space_capacity` |
| 35 | `surface_used` |
| 36 | `surface_capacity` |
| 37 | `division_ready` |
| 38 | `optical_range` |

For family index `k`, offsets `39 + 6k` through `44 + 6k` are **count, mass, capacity, damaged mass, building mass, functional mass**. All summaries describe actual structures. `division_ready` is 0/1. Basal metabolism includes body upkeep and current thermal scaling, excluding action/register/construction costs. Digestive rate reports assimilated substrate chemical potential per second; usable energy is generated by respiration. Photosynthesis reports the light energy stored by actual carbon fixation. `optical_range` is built hardware reach, not a free genome signal.

Cell offsets 135–137 contain gut energies for exudate/tissue/waste; 138–140 contain their mineral masses; 141–156 contain protein stock mass in family order; 157 is actual synthesis mass per second. The old aggregate gut fields equal the sums of their three compartments. Cell offsets 158–162 append organic-carbon reserve, internal O2, internal DIC, aerobic respiration rate (carbon/s), and carbon-fixation rate (carbon/s).

The reserved header field and `max_food` are zero; neither is a population or particle limit. Float64 world identifiers preserve all u32 cell IDs exactly; counters are exact through 2⁵³, with u64 counters retained internally. The environment rendering buffer remains float32.

### Presentation projections

The presentation exports retain their ownership rules. ABI 12 extended full cell and aggregate-metric layouts; ABI 13 appends the exact tick to the summary and adds an environment metadata projection. Render, metrics, selected-cell and summary projections share one engine-owned `Float64Array` buffer. Read `presentation_len()` and copy before calling another projection or any allocating engine operation. These exports do not advance time, change observations or memory, or modify checkpoint state.

`render_snapshot_ptr()` begins with `12 + cell_count × 21 + food_count × 5` values. ABI 19 appends the public signals, each encoded as a zero-based cell row followed by its value. The remaining pairs are established bond endpoints encoded by cell ID, in ascending order within each pair. Pending recruitment is private simulation state. A frame with neither signals nor bonds ends after the particle records. It omits organelle inspection arrays, resource totals and environmental grid samples. Particle layout remains unchanged.

`render_links_snapshot_ptr()` uses the same header, cell, particle and signal records. Its bond records are triples of first cell ID, second cell ID and kind. Kind `0` indicates recruited partners with compatible resource junctions, `1` indicates a bond retained at division, and `2` indicates another recruited contact. An inherited external feeding partner keeps its recruited relationship. Current transfer rates are available separately in the cell inspector. The inspector’s `colony` object partitions its `neighborIds` into `feedingNeighborIds`, `colonyNeighborIds` and `contactNeighborIds`.

| Header offsets | Fields in order |
| --- | --- |
| 0–3 | tick, time, cell count, food count |
| 4–8 | births, deaths, eaten, world width, world height |
| 9–11 | public signal count, mutation rate, maximum living generation |

| Compact cell offsets | Fields in order |
| --- | --- |
| 0–6 | id, genome id, x, y, vx, vy, angle |
| 7–13 | radius, energy, energy capacity, gut energy, gut capacity, integrity, photosynthesis rate |
| 14–17 | flagellum count, flagellum capacity, cortex mass, cortex capacity |
| 18–20 | photosynthetic capacity, piercer capacity, generation |

The visual family capacities retain their existing meaning: functional mass divided by the family's minimum mass. They are not action throughput values. `cell_snapshot_ptr(id)` supplies the full 163-value cell record when inspection needs it, without packing the rest of the world. A missing cell clears the presentation buffer; it does not replace the latest simulation error.

`metrics_snapshot_ptr()` returns a 54-value header followed by three values for each living genome group. Groups are sorted by genome ID; each is `[genome_id, population, maximum_generation]`. Registry counts include all registered genomes, whereas groups contain only genomes with living cells.

| Metric offsets | Fields in order |
| --- | --- |
| 0–7 | tick, time, population, food count, births, deaths, eaten, maximum generation |
| 8–14 | mutation rate, mutated births, skipped mutations, genome registry count, genome limit, registry bytes, byte limit |
| 15–19 | predation deaths, attacks, harvested light energy, current energy, introduced energy |
| 20–24 | total minerals, introduced minerals, dissolved minerals, particle minerals, cell minerals |
| 25–28 | autotrophs, heterotrophs, mixotrophs, attack-capable cells |
| 29–31 | total current photosynthesis, digestion and exudation rates |
| 32–35 | exudate count, tissue count, waste count, living genome group count |
| 36–39 | total carbon, total introduced carbon, organic carbon in cells, organic carbon in particles |
| 40–43 | cumulative fixed carbon, cellular respired carbon, background-decay respired carbon, legacy organic-carbon baseline |
| 44–46 | internal cellular O2, dissolved O2, dissolved inorganic carbon |
| 47–50 | external O2 introductions, external O2 removals, external inorganic-carbon removals, internal cellular inorganic carbon |
| 51–52 | cumulative ATP and free minerals withdrawn by player edits |
| 53 | total recoverable energy in external food packets, including enrichment |

Metric 18 sums usable energy, embodied structural energy and chemical potential in structural/reserve/gut/particle carbon. It is a total energy inventory; the cell's `energy` field reports its ATP-like usable reserve. Feeding categories follow the same functional photosynthetic/digestive organ checks as cell inspection; attack-capable count is independent. Rates and ledgers preserve the full snapshot's meanings. `summary_snapshot_ptr()` returns `[time, population, food_count, births, deaths, maximum_generation, tick]` and performs no particle, organelle or field export.

The TypeScript `Engine` exposes `renderSnapshot()`, `metricsSnapshot()`, `cellSnapshot(id)` and `summary()` alongside its existing API. `renderSnapshotData()` and `metricsSnapshotData()` return owned packed buffers for transfer.

### Simulation worker ownership

`WorkerEngine` owns one `Engine` instance in a dedicated module worker. Its asynchronous methods preserve command order, including edits, ticks, inspection and checkpoint capture. Render and metric buffers cross the worker boundary using transferable `ArrayBuffer`s. The UI obtains full environmental fields only when requested and detailed cell data only for inspection.

`advance(steps, {frame?, metrics?, environment?})` executes a bounded batch of 0–120 fixed ticks and returns `{tick, summary, frame?, metrics?, environment?, error?}`. `frame: false` omits drawing data; `environment: "metadata"` omits patch samples. If a tick fails after earlier ticks completed, the response retains the exact resulting tick, summary and original error. The session updates revision, history and save metadata even when the last drawing frame is older. `presentation(options)` returns the same projections without advancing time. A rejected ordinary command does not block subsequent commands.

`captureCheckpoint()` synchronously captures checkpoint bytes and summary inside a single queued worker command, so both describe the same state even when later advances are already queued. Restore transfers a copy of the caller's bytes, preserving the original on either success or failure. `dispose()` terminates the worker and rejects outstanding/future requests. These ownership changes do not change tick duration, physical phase order or the checkpoint model. Gas fields, internal carbon/oxygen inventories and reaction ledgers extend checkpoints to version 9.

### Body inspection

`cell_body_ptr(cell_id)` returns actual structure and packing, separately from the compact rendering snapshot:

```ts
{
  cellId: number;
  area: number; mass: number;
  foodPotential: {body: number; reserve: number; gut: number; atp: number; total: number};
  spaceUsed: number; spaceCapacity: number;
  surfaceUsed: number; surfaceCapacity: number;
  organs: {id: number; kind: string; mass: number; integrity: number;
    functional: boolean; minimumMass: number; embodiedEnergy: number}[];
  requested: {kind: string; count: number; size: number; build: number; recycle: number; proteinMass: number}[];
  constructionRate: number; recyclingRate: number; repairRate: number;
  synthesisRate: number; assemblyCapacity: number;
}
```

The five food-potential values give the gross recovered energy in the built body, organic reserve, gut, available ATP and their sum. The body includes its structural carbon and actual retained fabrication energy. Carbon and minerals have independent inventories. External packets hold at most 24 total energy units regardless of enrichment.

The JSON `build` field retains its compatibility spelling and reports expression effort. `proteinMass` is each family's actual stock. Requested outputs describe the genome's commands. Organs, packing, and achieved rates describe the actual body. The body buffer, language schema, program source, analysis, graph, memory, and expression-preview buffers are independent. Copy returned buffers before another allocating WASM call.

### Environment snapshot

ABIs 12 through 14 use `21 + columns × rows × 7 + source_count × 8 + barrier_count × 10` floats:

```text
Header: columns, rows, ambient_temperature, ambient_light,
        current_strength, source_count, initial_nutrients,
        world_width, world_height, barrier_count, nutrient_removed_total,
        initial_oxygen, initial_carbon, atmosphere_open, exchange_rate,
        oxygen_equilibrium, carbon_equilibrium,
        oxygen_input_total, oxygen_removed_total, carbon_input_total, carbon_removed_total
Each row-major patch: light, temperature, flow_x, flow_y, nutrients, oxygen, inorganic_carbon
Each source: id, kind, x, y, strength, radius, angle, length
Each barrier: id, x, y, length, width, angle, floating, vx, vy, reserved_zero
```

Fields retain physical 20 × 20 patches when the world changes size. Patch nutrients are dissolved mass. `initial_nutrients` configures the next reset and does not modify live stock. Resize splits existing patch amounts by normalized area overlap, preserving total material and the external-input ledger. Temperature and organic-cue intensities are area averages. Cell, particle, emitter and obstacle centers are remapped proportionally; biological dimensions, velocities and individual inventories do not scale. Per-patch incident-light power remains constant, so larger lit habitats have proportionally more total available sunlight. Spatial buckets keep approximately 60-unit physical spacing.

| Source kind | Effect |
| --- | --- |
| 0 | Instantaneous local illumination |
| 1 | Persistent heating/cooling |
| 2 | Clockwise/counterclockwise vortex |
| 3 | Mineral spring, total input 1.5 × strength mass/s |
| 4 | Mineral absorber, requested export 1.5 × strength mass/s, limited to available mass in each affected patch |
| 5 | Directed Gaussian jet; angle selects direction, length controls longitudinal extent |
| 6 | Parallel current band; flat longitudinal center with rounded end falloff |
| 7 | Radial outward/inward current |
| 8 | Dissolved oxygen emitter, total input 1.5 × strength amount/s |
| 9 | Oxygen absorber, requested export 1.5 × strength amount/s, limited by available oxygen |
| 10 | Effective inorganic-carbon emitter, total input 1.5 × strength amount/s |
| 11 | Inorganic-carbon absorber, requested export 1.5 × strength amount/s, limited by available carbon |
| 12 | Local reservoir aerator, exchanging 5 × strength water-volume units/s over a normalized footprint |

At most 64 sources and 64 barriers are supported. Source strength is clamped to 0–1 for light, springs, absorbers and gas objects (kinds 0,3,4,8–12), and −1–1 for heat and flow. Source radius is 20–1000; length is 40–4800; angles are radians normalized to [0, 2π). Zero strength disables a source without changing its identity. Flow superposition is capped at speed 60. Mineral, O₂ and inorganic-carbon sources and sinks operate within the displayed disk. Patch centers inside it receive normalized weights proportional to `(1 − distance²/radius²)²` using wrapped distance. Covered solid patches receive no dose and sinks take only local available stock. The input and output ledgers count actual transfers. Transport can carry material beyond the source after injection. The material invariant is `current_total + cumulative_removed = cumulative_introduced`. Grid samples of time-independent emitter flow are cached and recomputed after source edits or world resize; moving barrier masks remain independent. The legacy nonzero time-varying global current retains direct evaluation. These derived caches are rebuilt on restore and are not checkpoint data.

Barriers are oriented rectangles with length 20–2400 and width 10–400. Their rotated bounding boxes must fit within the world dimensions (1e-3 unit tolerance accommodates the f32 ABI); invalid create/update and incompatible world shrink return zero without altering state. Fixed barriers stay anchored. Moving or resizing an already floating barrier preserves its velocity. Fixing it clears that velocity. Floating barriers follow local prescribed flow with finite response time and separate from other obstacles using periodic rectangle overlap resolution. Cells and organic particles use swept collision detection, preventing thin-wall tunneling; colliding motion slides along obstacle faces. Placing a barrier over an existing organism or particle projects it clear without destroying its inventory. Barriers occlude optical sensing, feeding/predation paths, and field transport across intersected grid edges. Heat and organic interpolation also excludes samples across walls.

Currents remain prescribed velocity fields, not a pressure-solving fluid simulation. Obstacles stop crossing flux and suppress flow inside their solid regions; they do not solve a new incompressible flow field around their shapes. Obstacle transport is resolved at the 20-unit field grid; physical cells and particles use continuous rectangle geometry. Overlapping fixed barriers are allowed as compound walls. Barrier/organism displacement does not attempt fluid-volume displacement.

The organic-cue field is internal simulation state and is persisted as an additional checkpoint field; it is not an additional rendering channel. Genomes read it through built chemoreceptors using `chemical.organic` and `chemical.organic_bearing`.

### Dissolved gases and reservoir exchange

Oxygen and effective dissolved inorganic carbon are distinct, nonnegative amount arrays beside the mineral field. Each 20 × 20 patch contains one abstract water-volume unit, so amount and displayed concentration have equal numeric values. The engine defaults to a sealed boundary and reset inventory of one oxygen and one inorganic-carbon unit per patch; built-in UI habitats explicitly select open water. These are simulation units, not calibrated mg/L; DIC represents an effective available inorganic-carbon pool, not a claim that all carbon is free CO2.

Minerals, O2 and DIC share the same conservative finite-volume solver, obstacle masks and once-per-step face velocities. Diffusion coefficients are respectively 12, 80 and 60 world-unit²/s. Outgoing mass is jointly limited by each donor patch's old inventory; same-step arrivals cannot be sent again. Resizing distributes all three conserved fields by area overlap and leaves their external ledgers unchanged. Field temperatures and the organic scent cue remain scalar intensities.

The sealed boundary contributes no background air–water exchange. Open mode exposes every uncovered water patch to the configured reservoir. An explicitly placed aerator is a local air connection even when the rest of the scene is sealed; removing it restores the closed background. Solid full-depth obstacle patches have no exposed air–water surface. For each exposed patch, the exact bounded reservoir relaxation is `Δamount = (equilibrium × volume − amount) × (1 − exp(−rate × dt))`. Global exchange and local aerator conductance add. Positive flux increments external introductions; negative flux increments removals and never overdraws the field. Gas emitters/absorbers likewise charge only actual external amounts. No boundary operation resets an entire field to equilibrium during a tick.

The oxygen target uses the explicitly abstract temperature law `oxygen_equilibrium × (1.25 − 0.5 × temperature)`; its nominal value occurs at temperature 0.5. DIC equilibrium is constant until carbonate/pH chemistry is introduced. Reservoir exchange uses the field temperature at the start of that fixed tick.

Environment processing first moves barriers and computes shared face velocities, then transports dissolved amounts, applies emitters/absorbers and signed reservoir exchange. Organism membrane exchange and reactions run afterward in the world biology phase. `take_oxygen`, `take_inorganic_carbon`, `release_oxygen` and `release_inorganic_carbon` are internal transfers and do not change external ledgers; the world owns reaction production/consumption and organic-carbon accounting. Primitive physical flow sources do not create gases.

### Memory snapshots

Memory includes only populated slots, ordered by primitive type/address. Primitive JSON types are retained; vectors use arrays:

```json
[{"name":"ri001","type":"integer","value":12},
 {"name":"rb002","type":"boolean","value":false},
 {"name":"rv999","type":"vec2","value":[0.5,-1]}]
```

### Organism expression evaluation

`cell_evaluation_ptr(cell_id)` evaluates the organism's exact stored source against freshly sampled **current world state and current committed memory**, using the ordinary Rust node evaluator and the same sensing code as simulation ticks. It neither commits writes/clears nor advances time, fields, cells, food, RNGs, lineage, or counters. It is a preview of the displayed snapshot, not a recording of the last tick: the next tick first advances environmental fields and may therefore read slightly different inputs. All pure branches and unused bindings have values because the graph evaluator is eager.

The version-1 JSON response has this shape (`Value` means number, boolean, string, or a two-number vector):

```ts
{
  version: 1; cellId: number; genomeId: number; time: number; tick: number;
  semantics: 'current-snapshot'; source: string;
  expressions: {id: number; from: number; to: number; type: string; value: Value}[];
  bindings: {name: string; from: number; to: number; type: string; value: Value}[];
  signals: {name: string; type: string; value: Value}[];
  outputs: {name: string; type: string; value: Value; from: number | null; to: number | null}[];
  writes: {name: string; kind: 'write' | 'clear'; type: string; value: Value | null;
    guard: boolean; applies: boolean; from: number; to: number}[];
}
```

All offsets are **UTF-16** positions in `source`, with exclusive `to`. Expression ranges identify the original expression; a reference has its own range instead of borrowing the declaration's range. Binding ranges cover declaration names. Write ranges cover destination register names. Omitted outputs have null ranges. Type labels are `integer`, `scalar`, `boolean`, `string`, and `vec2`.

Expression IDs are strictly increasing graph-node IDs, bounded below 8,192; they
need not be contiguous. Pure-function inlining creates internal graph nodes
without a unique authored-source value, so those nodes are omitted from the
evaluation array. Argument expressions and the full invocation result remain
inspectable at the call site. Graph JSON retains every node and its original ID.

`expressions` and `bindings` show raw typed values. `outputs` report commands after ordinary clamping/defaults, before physical resource constraints. `signals` includes every readable builtin. Write `value` is the converted proposed register value; clears use null. `guard` is true when no `when` exists. A clear applies when its guard is true; a write applies only when its guard is true and no simultaneous clear wins for that address. The preview never changes register occupancy or charges energy.

The evaluation buffer is independent of source, analysis, graph, memory, and field buffers. Copy it immediately before another allocating WASM call. A missing/dead organism clears its buffer and returns null with an error, so the UI must discard previous live values. Newborns can be inspected immediately with their current empty memory. Stored source and compiled graph must agree exactly; inspection derives source locations without changing either.

### Genome graph

Graph JSON remains version 1 in ABI 12: `{version: 1, nodes: [...], roots: [...]}`. Inspection does not evaluate programs, advance the world, change memory, or register genomes. Invalid IDs clear the graph buffer and report an error.

Nodes contain `id`, `kind`, `type`, `label`, `inputs`, `bindings`, and `reachable`. IDs are compiler-array indexes. Inputs preserve operand order and duplicates and precede their consumer's index. Conditional operands are condition, then branch, else branch. Types are `integer`, `count`, `scalar`, `boolean`, `string`, and `vec2`. Kinds are `constant`, `sensor`, `register`, `populated`, `reference`, `unary`, `binary`, `conditional`, and `call`. Unused nodes remain present, marked unreachable, so graph counts match the 8,192-node budget.

Roots contain `id`, `kind`, `name`, `type`, `value`, `guard`, and `default`. All **81 output roots** are included in metadata order: 11 base actions, family-major count/size/expression/recycle controls, bearing/width/target scan controls, then exudate/tissue/waste enzyme controls. An authored output references its expression and has a null default; an omitted output has null value and a JSON-literal string default, such as `"0"`, `"false"`, or `"\"\""` for an empty token. Defaults do not create expression nodes or mutate source.

Register roots are `write:<address>` or `clear:<address>` with the destination type. Clear values are null. A null guard means unconditional. There are at most **10,071 roots**: 81 outputs plus one write and one clear for 4,995 registers. Reads and occupancy checks refer to old memory; writes/clears describe next memory. No same-tick dependency runs from a write back into a read.

#### Frontend network projection

`src/genome-network.ts` derives the visual network from this unchanged compiler graph. It combines repeated sensor reads by name and register reads/writes/clears by address. Each operator, call, and conditional remains a distinct instance. Constant leaves become inline port values, and reference nodes collapse to their underlying dependencies while preserving binding names. `populated(register)` remains an explicit occupancy operation connected to the register; it is not a value read.

Edges retain operand ports and separate `when` guards. The visualizer uses cyan for reads from inputs/registers, amber for action/register writes, rose dashed lines for clears, and neutral lines for computation. Direct input/register-to-root edges take the destination's write/clear role. Register write and clear edges are explicitly temporal and do not participate in same-tick layout ordering. Thus a unified register node may have a visible cycle without changing the synchronous memory semantics.

Constant writes, constant guards, unconditional clears, and omitted output defaults remain inspectable even when they need no incoming edge. The overview initially hides implicit defaults and isolated constant action outputs; controls reveal them, and authored constants remain in the searchable list. Unused expressions are excluded from the network but counted in compiler statistics. Iterative views contain at most 112 nodes, with focused UI neighborhoods limited to three hops. They show actual dependencies and consumers, expose omitted nodes/frontiers, and never manufacture shortcut edges across hidden nodes. Projection and navigation do not execute programs or change the world.

### Persistent checkpoints

Current checkpoint format is **17**, using the `CELLAR01` magic, a separate version, exact byte length, FNV-1a corruption checksum, and bounded little-endian values. The checksum is not authentication; every decoded value is independently checked. Checkpoints preserve actual frame and organelles, integrity, resource/embodied-energy ledgers, construction progress, protein stocks and their embodied energy, separate gut substrates and particle kinds, memory occupancy and values, sensor tracks and scan controls, sources/fields, deterministic RNG state, counters, and the genome registry with lineage.

Version-1 and version-2 body schemas are explicitly unsupported. They cannot be silently interpreted as paid structures or hardware-gated observations. Existing exports are preserved; create a new simulation with rewritten genomes. Future unknown versions are also rejected. Version-3 checkpoints remain supported. Existing organs, energy, minerals, memory and program source are preserved; formerly unclassified particles and gut contents import as tissue, and protein pools initialize empty. New digestion/expression rules govern subsequent evolution, so continuation is deterministic within the new engine but is not an exact replay of the old engine. Versions 4 through 16 remain supported. Saving writes version 17, including cellular pools, recipe snapshots, ingredient ledgers, sampled construction controls, activity, genetic progress, mechanism kinds, world dimensions, emitter directions, floating obstacle state, gas fields and reservoir configuration, cumulative external species fluxes, internal organism gas/carbon pools, reaction ledgers and cumulative mineral removals. Primitive mechanism tags are rejected before version 6, recycling/local-control additions before version 7, respire(oxygen) before version 9, and contact declarations, cellular register operations and adhere() before version 10. Dynamic signal fields and resource junctions require version 11. Signal junctions and the two player withdrawal ledgers require version 12. Pre-version-12 saves initialize withdrawals to zero. Version 15 records the endpoints and origin of each accepted bond and the elapsed contact ticks of prospective pairs. During fission, an existing bond may pass to the daughter that retains working contact with the same peer. Each original interface can pass to at most one daughter. Earlier formats only supplied endpoint counts. Version 16 clears earlier contacts on hardware without installed adhesin. Historical surface mass, its original ingredients, and their embodied ATP remain available for material accounting. When an old storage recipe had been counted as complete with a missing trace ingredient, the reader validates the saved contents against the historical capacity before applying the corrected capacity. Excess ATP is retained as structural enrichment. Version 14 omits the former food generation rate and fractional credit. Loading an earlier save keeps every existing organic particle and its ledgers, and discards those two settings. Version 13 and later preserve additional chemical energy in individual packets, gut pools, reserves and structural material. Earlier saves retain their historical paid fabrication energy and initialize the new carrier-energy fields to zero. Old recipe sources default to allowing new starts and requesting no recycling. Optical coverage corrections and directional primitive interactions change affected trajectories; deterministic continuation is verified within the current engine.

On pre-version-9 restore, water receives the documented default O2/DIC inventories and matching external baseline ledgers. Existing organic material receives an explicit legacy carbon baseline; old snapshots are preserved but subsequent trajectories follow the gas-aware physiology.

Stored programs retain exact expression trees and source. Restore recompiles source and requires its typed graph to match the stored graph. It validates backward references, scopes, register types/addresses, output types, finite physical state, IDs, resource bounds, and complete input consumption before atomically replacing the world. A failed restore leaves the current world unchanged. Save size remains bounded at 128 MiB; writing stops before exceeding the limit and reports allocation failures. Cell and particle counts are bounded by encoded data and available memory, not biological ceilings. Age is validated as finite nonnegative state rather than against a lifespan.

`resetEmpty(seed)` prepares a world with the configured dissolved chemical inventories, zero cells and zero organic particles. The UI then introduces the selected founders. Adding one organism introduces exactly one minimal cell, whose program grows its specialized body.

## Verification

The native suite covers typing, synchronous memory, source-preserving mutations, physical accounting, capacity limits, hardware-dependent sensing, division, and deterministic restore. WASM checks exercise the real frontend boundary, all five register banks, expression budgets, schema/catalog agreement, and exported buffer layouts. The static Pages build typechecks the frontend and uses relative paths. Unsupported legacy fixtures must be rejected explicitly, not migrated implicitly.

`npm run test:worker` exercises the actual TypeScript facade and dispatcher in Node worker threads against the compiled WASM. It checks ordered commands, compact/full projection parity, selected-cell inspection, transferred buffer ownership, atomic checkpoint summaries, restore ownership, recoverable errors, partial tick results, main-loop responsiveness and disposal. The native presentation tests also compare packed fields and checkpoint bytes directly.
