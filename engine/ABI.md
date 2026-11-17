# agar engine

This dependency-free Rust crate owns simulation, compilation, mutation, and cell memory. TypeScript loads the WASM and renders copied snapshots. There is no simulation server or JavaScript simulation fallback.

## Simulation model

The 1,200 × 760 world wraps at both edges. Sensing, fields, contact, and collisions use the same toroidal geometry. Steps are fixed at 1/30 second, in this order:

1. Advance dissolved nutrients, sources, and heat; decay food; freeze sensors and old memory; evaluate programs and commit register changes.
2. Advance age, apply senescence, and resolve movement/upkeep, thermal damage, then repair.
3. Digest material already in each gut, share dissolved nutrients, drift food, resolve collisions, ingest food, then photosynthesize and release exudates.
4. Resolve predation, grow cells, pay for gradual body remodeling, and release gut contents above any reduced capacity.
5. Divide cells, emit remains and remove deaths, add newborns, then replenish external food. Newborns first act next tick.

All capabilities during a tick use the body allocation from before remodeling. Introduced founders adopt their initial programmed body targets on their first tick; subsequent target changes require paid remodeling. Digestion, nutrient uptake, and photosynthesis can fund growth in the same tick, while expressions observe the earlier frozen state. Newly ingested food and engulfed prey cannot be digested until a later tick.

- Exponential drag of 1.8/s acts relative to local water velocity. Collision separation/impulses are weighted by inverse biomass. Movement rates depend on propulsion investment, as specified below.
- `size` is biomass and relative 2D area. Founders start at 1; live sizes range from 0.5 to 4. Radius is `7 × sqrt(size)`; energy capacity is `200 × size`. Free nutrient capacity is `2 × size`; founders carry 0.5 free nutrients.
- Basal metabolism is `(0.35 + 0.30 × sum(first five body fractions) + 0.80 × body.attack) × size × (0.5 + temperature)` energy/s. The original balanced body retains the rate `0.65 × size` before temperature. Upkeep uses pre-growth size; registers and optional actions add the costs below.
- Growth requests `0.05 × grow × sqrt(size)` biomass/s and consumes 80 energy plus one free nutrient per actual biomass added. It preserves at least 20% of the resulting, larger energy capacity. Stored minerals and maximum size also limit growth. Energy or integrity at zero causes death; later feeding or photosynthesis cannot revive that cell. Cells also die at age 300 seconds.
- Food contact reach is radius plus 2.5. The nearest eligible center wins each particle, with lower ID breaking ties. `feed` and digestion capacity limit ingestion into a finite gut; digestion later assimilates usable energy at 65% efficiency.
- Division requires `divide = true`, at least twice `birth.size`, size ≥ 1, energy ≥ `90 × size`, and an expired twelve-second cooldown. It costs `12 × size`, then splits remaining energy, biomass, free nutrients, gut contents, and invested mass equally. Both halves update `birth.size` to their new size and retain body fractions, integrity, and remaining engulf cooldown. The parent keeps its age and memory; the daughter starts with empty memory, age zero, and a new ID. Daughters inherit the genome unless mutation succeeds. Division grants no free repair.
- Reset creates 22 size-1 cells with 88–100 energy and 360 food particles. Food replenishment defaults to six particles/s and accepts 0–60. User introductions explicitly add resources.
- There is no fixed population cap. Introducing cells and advancing births reserve memory before changing the world; allocation failure reports an error. Introductions also reject requests beyond the remaining addressable cell IDs. Food remains bounded to 1,200 particles, and the genome registry to 2,048 entries and 64 MiB of estimated allocations. The registry byte budget covers source and program graphs, not the entire WASM heap. Identical authored text reuses an authored ID; mutant identity includes parent ID, category, and source.
- Reset clears cell memory, restores heat to ambient and dissolved nutrients to the configured starting level, and restarts counters/input accounting. It preserves compiled genomes, mutation probability, environment settings, and source IDs. Food replenishment returns to its default; the UI reapplies its selected rate. The same seed, programs, configuration, and action sequence replay within the same runtime. Cross-platform bit-identical floating-point replay is not guaranteed.

## Language and registers

An immutable graph computes `(sensors, old memory) → (actions, next memory)`. `let` names a sampled expression and re-evaluates each tick. Expressions have no side effects, recursion, or user-controlled loops. Only register rules retain state.

```text
let hungry = energy.fraction < 0.4 || (rb001 && energy.fraction < 0.8);
rb001 = hungry;
ri001 = ri001 + 1 when !food.visible;
clear ri001 when food.visible;
rs001 = if hungry then "forage" else "rest";
rf001 = if populated(rf001) then rf001 * 0.95 + light.level * 0.05
        else light.level;
rv001 = vec(flow.x, flow.y);
thrust = if hungry then 0.8 else 0.2;
turn = if food.visible then clamp(food.angle * 1.8, -1, 1)
       else sin(age * 0.7 + id) * 0.6;
grow = if size < 2 && energy.fraction > 0.6 then 1 else 0;
divide = size >= 2 && energy.fraction > 0.8;
```

There are 17 outputs. Programs define `thrust`, `turn`, and `divide` exactly once. Optional `grow`, `photosynthesize`, `engulf`, `bite`, `defend`, and `repair` default to zero/false. Other numeric outputs have these defaults:

| Output | Omitted value | Meaning |
| --- | --- | --- |
| `absorb` | `1` | Request dissolved nutrient uptake |
| `invest.propulsion` | `0.2` | Target propulsion fraction |
| `invest.photosynthesis` | `0.2` | Target photosynthesis fraction |
| `invest.digestion` | `0.2` | Target digestion fraction |
| `invest.armor` | `0.2` | Target armor fraction |
| `invest.sensing` | `0.2` | Target sensing fraction |
| `feed` | `1` | Ingest food into the gut for later digestion |
| `invest.attack` | `0` | Target attack fraction |

Actions are numeric 0–1 except `turn` (−1 to 1), `divide` (boolean), and `engulf` (boolean). Numeric actions accept integers or scalars and clamp at evaluation. The host normalizes investment targets against their shared body budget; the language preserves each independent signal. `Outputs::default()` uses the same omitted-action defaults.

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
| `vec(x, y)`, `x(v)`, `y(v)`, `length(v)`, `dot(a, b)`, `normalize(v)`, `angle(v)` | Vector construction/components/geometry |
| `length(text)` | Integer Unicode-character count |
| `int(x)`, `float(x)`, `identity(value)` | Numeric conversion and type-preserving identity |

String literals use double quotes and accept `\"`, `\\`, `\n`, `\r`, `\t`, and four-digit `\u` escapes for individual valid Unicode scalar values. Oversized literals are rejected; concatenation and mutations remain bounded. There is no implicit string/numeric conversion.

### Sensors

Numeric sensors are scalars. Angles are relative to heading, in radians. Food/neighbor/prey/threat sensing extends `sense.range` units center-to-center: `40 + 400 × body.sensing` (balanced 120, maximum 440). Absent targets yield false visibility, zero angle, distance `sense.range`, and zero ID/size where available. Zero gradients and zero flow produce angle zero. Wrapped searches visit each bucket at most once.

| Signals | Meaning |
| --- | --- |
| `energy`, `energy.fraction`, `size`, `age`, `speed`, `time`, `id` | Energy/capacity fraction, biomass, age/time in seconds, world speed, stable ID |
| `memory.used`, `memory.cost` | Pre-tick populated slots and upkeep in energy/s |
| `food.visible`, `food.angle`, `food.distance` | Selected food target, maximizing raw energy / (distance + 20); particles buried inside another cell are excluded |
| `food.energy`, `food.nutrients` | Raw energy and mineral content of that same target; zero without a visible target |
| `neighbor.visible`, `neighbor.angle`, `neighbor.distance` | Nearest other cell |
| `light.level`, `light.angle` | Light 0–1 and direction toward increasing light |
| `temperature.value`, `temperature.angle` | Normalized temperature 0–1 and direction toward increasing temperature |
| `flow.x`, `flow.y`, `flow.speed`, `flow.angle` | Water velocity in world units/s; components use world axes |
| `prey.visible`, `prey.id`, `prey.angle`, `prey.distance`, `prey.size`, `prey.same_genome` | Cell of any size within sensing range; prefer a different genome, then nearest within that group; exact genome-ID equality. Programs choose hunting risk; engulfing still requires its physical size advantage. |
| `threat.visible`, `threat.angle`, `threat.distance`, `threat.size` | Nearest cell with positive attack allocation, including smaller hunters capable of biting |
| `integrity` | Structural health 0–1 |
| `nutrient.stored`, `nutrient.capacity`, `nutrient.fraction` | Free reserve, capacity `2 × size`, and stored/capacity fraction |
| `nutrient.level`, `nutrient.angle` | Bilinearly sampled dissolved mass per patch and direction toward increasing mass |
| `body.propulsion`, `body.photosynthesis`, `body.digestion`, `body.armor`, `body.sensing`, `body.attack` | Actual invested fractions, each 0–1, with sum ≤ 1 |
| `body.unassigned` | Remaining unspecialized core fraction, `1 − sum(body fractions)` |
| `sense.range` | Current target-sensing radius in world units |
| `gut.energy`, `gut.capacity`, `gut.fraction` | Raw food energy awaiting digestion, raw-energy capacity, and occupancy fraction |
| `digestion.rate`, `growth.rate`, `exudation.rate` | Last completed tick's assimilated energy/s, added biomass/s, and released raw food energy/s |
| `birth.size`, `maturity`, `lifespan` | Biomass at last birth/division; `clamp(size / birth.size − 1, 0, 1)`; maximum age in seconds |
| `prey.integrity`, `prey.attack`, `prey.photosynthesis` | Sensed prey health and actual attack/photosynthesis allocations; zero without visible prey |

### Bounds and mutation

Limits are **8,192 expression nodes**, **65,536 tokens**, **256 KiB UTF-8 source**, and **64 nested expression levels**, for both authored and generated programs. References, constants, operators, calls, and guards contribute nodes; comments and whitespace do not. Invalid compilation leaves the world unchanged.

Mutation kinds are any (0), constant (1), expression (2), and reference (3). Constants retain their primitive types and bounds. Expression replacements preserve actual operand/result signatures. References use compatible sensors, same-type register addresses, or earlier visible bindings. Mutations cannot introduce forward references, cycles, ill-typed guards, or illegal addresses. They edit existing expressions/reads and preserve register write destinations; absent action outputs are not introduced, except the existing compatibility treatment of omitted `grow` as zero.

Serialization preserves simultaneous register rules, prunes unreachable expressions, and emits shallow topological bindings. Export/recompile must preserve future memory states as well as immediate outputs. Numeric compatibility is by type, not physical units. Validity does not guarantee survival or useful behavior.

No applicable change, oversized generated source, or exhausted registry capacity safely retains the parent. Default daughter mutation probability is 0.10. Mutation uses a separate deterministic stream derived from seed, tick, and cell IDs. Existing variants are reused before capacity checks, allowing reset to replay existing lineages. Division always pays its ordinary cost, including fallback inheritance. Successful automatic variants increment `mutation_count`; skipped attempts increment `mutation_skipped_count`. Draft mutations affect neither counter.

## Environment, nutrients, and capabilities

### Fields and mineral ledger

A 60 × 38 grid represents 20 × 20 world-unit patches. Defaults are temperature 0.5, light zero, no currents/sources, and 0.3 dissolved nutrients per patch (684 total). Light updates immediately. Persistent heat advects, diffuses, and relaxes toward ambient temperature. Currents are prescribed periodic velocity fields plus vortices; food follows flow and cells experience relative drag. Cells do not displace water or generate wakes.

Up to 16 sources are supported: kind 0 light, kind 1 heat/cooling, kind 2 signed vortex, and kind 3 nutrient spring. Light/spring strength is 0–1; heat/vortex strength is −1 to 1. Radius is 40–400 and coordinates wrap. Ambient temperature/light are 0–1; background current strength is 0–30. Combined local flow is capped at 60 units/s. Each spring adds `1.5 × strength` nutrients/s, distributed with normalized Gaussian weights; total supply is independent of radius.

Dissolved nutrients use conservative, periodic finite-volume advection and diffusion at 12 world units²/s. Paired transfers and old-balance donor budgets preserve material and nonnegative patches. Heat uses its separate interpolation-based transport. Changing the starting nutrient level (0–5 per patch) affects the next reset only; reset restores that recipe and retains source configuration/IDs.

There is one generic mineral resource. One biomass unit binds one mineral unit; cells also hold free reserves. The ledger is:

```text
nutrient_total = dissolved water + food nutrients + sum(cell size + free reserve + gut nutrients)
```

Initial water, introduced cells, external food, fertilizer, and spring supply increment `nutrient_input_total`. Reset restarts it with the restored water plus reset founders/food. Transfers, uptake, growth, division, remodeling, predation, decay, and death never count as new input. Total material matches cumulative input within floating-point rounding; metabolic energy costs do not consume minerals.

Ordinary introduced food contains 18–24 raw energy and 0.25 nutrients. Both decay exponentially at 0.02/s: energy dissipates, minerals return to the local dissolved patch. `feed` defaults to one for source compatibility; setting it to zero stops ingestion and digestion. Intake is limited to `60 × feed × size × k_digestion` raw energy/s and remaining gut capacity, `100 × size × k_digestion`. Partial ingestion transfers the same fraction of a particle's nutrients into the gut.

Digestion processes at most `6 × feed × size × k_digestion` raw energy/s from the gut carried into the tick. Only 65% becomes usable energy; the remainder dissipates. Gut minerals enter the free reserve proportionally, with overflow recycled to water. Full energy reserves can still digest for minerals; when both reserves are full the meal remains unprocessed. Reducing digestive allocation releases excess gut contents as food rather than deleting their minerals. `total_eaten` counts particles removed by ingestion, not decay.

### Invested body

Propulsion, photosynthesis, digestion, armor, sensing, and attack share one body budget. Initial storage uses `[0.2, 0.2, 0.2, 0.2, 0.2, 0]`; a founder adopts the targets from its program on its first tick. The first five targets default to 0.2, while attack defaults to zero; set unwanted targets explicitly to zero. Targets whose sum exceeds one are normalized down proportionally. A smaller sum leaves an unspecialized core. Sensors report actual fractions, which approach later target changes gradually.

For an ability, `k_ability = body.ability / 0.2`, ranging from 0 to 5. Remodeling occurs after interactions and before division. Required reassigned mass is `size × 0.5 × sum(abs(old fraction − target fraction))`, including the core as a seventh fraction. Each tick moves at most `0.08 × size × dt` biomass and costs 12 energy per actual biomass reassigned, limited by remaining energy. Fractions move proportionally toward the target; there is no mineral cost or refund for removing investment. The new allocation affects abilities next tick.

Body upkeep is `(0.30 × sum(first five fractions) + 0.80 × body.attack) × size` energy/s before temperature. Attack tissue therefore has a higher maintenance cost even while not biting. Body upkeep is already included in basal metabolism, never charged twice. Fully unassigned cells have basal rate `0.35 × size` before temperature.

| Effect/action | Resolution and cost |
| --- | --- |
| Propulsion | Turn rate `3.2 × turn × sqrt(k_propulsion)` radians/s; acceleration `100 × thrust × k_propulsion` world units/s²; upkeep `1.45 × thrust × size × k_propulsion` energy/s. |
| Sensing | Range `40 + 400 × body.sensing`; applies to food, neighbors, prey, and threats. |
| Dissolved uptake | Request `0.25 × absorb × sqrt(size)` nutrients/s, independently of digestion allocation. Limited by reserve room and energy at 1 energy/nutrient. Eligible requests share scarce local patch stock proportionally, in stable-ID order for deterministic summation. |
| Photosynthesis | Intrinsic capacity `3 × size × photosynthesize × k_photosynthesis` gross energy/s, distributed over a normalized capture footprint with radius `60 × sqrt(size)` and scaled by each patch's light. The entire habitat supplies at most 2,400 gross energy/s at full light; overlapping footprints share each patch's finite input proportionally. At least 12%, plus surplus above energy capacity, becomes organic food when particle space is available; energy that cannot fit dissipates. Upkeep `0.18 × size × photosynthesize × k_photosynthesis` energy/s applies even in darkness. |
| Thermal stress | Outside [0.15, 0.85], damage is `0.3 × distance outside interval × (1 − 0.5 × body.armor)` integrity/s. |
| Repair | Up to `0.15 × repair` integrity/s, costing `20 × size` energy per integrity actually restored, limited by available energy. |
| Armor/defense | Bite reduction `R = clamp(0.4 × body.armor + 0.65 × defend × k_armor, 0, 0.95)`. Active defense costs `0.6 × size × defend × k_armor` energy/s. |
| Bite | Upkeep `0.9 × size × bite × k_attack` energy/s. Contact damage `0.12 × bite × k_attack × min(attacker size / prey size, 2) × (1 − prey R)` integrity/s. Feeding attackers request `80 × actual integrity damage × prey size × feed` raw energy from prey reserves into available gut space, with proportional free minerals. No immediate usable energy is awarded. |
| Engulf | Requires contact, ≥1.6× prey size, prey integrity ≤0.35, positive attack/digestion allocation and feed effort, expired engulf cooldown, and energy strictly above cost `8 × prey size`. The entire prey's reserve energy, `80 × prey size`, and gut energy must fit in the attacker's gut. All prey minerals move into the gut too. Sets cooldown to `clamp(2 / k_digestion, 0.4, 8)` seconds. No immediate usable energy is awarded. |
| Other deaths | Recover `0.6 × (remaining nonnegative energy + 80 × size)` plus remaining raw gut energy into food; all biomass, free-reserve, and gut minerals are retained. |

Photon capture weights are `(1 − (distance / radius)^4)^2` inside `radius = 60 × sqrt(size)`, normalized across covered 20 × 20 field patches. Collection area scales with biomass, so growing cells can collect more light to pay their greater upkeep. The areal supply allows an isolated cell to realize its intrinsic capacity; overlapping cells still compete for the same finite input. Each patch supplies `(2400 / 2280) × patch light` gross energy/s. A cell's request on a patch is its intrinsic capacity times normalized capture weight times that patch's light, so capture never exceeds either cellular capacity or the shared areal light supply. Crowded producers compete even when their centers occupy different field patches.

Remains use particles of at most 24 energy, dividing their minerals proportionally. At the particle cap, leftover minerals enter water and excess energy dissipates. Consumed prey cannot emit a second set of remains. Photosynthetic exudates carry organic energy without manufacturing minerals. They are deposited outside the producer's membrane and combined into nearby particles when possible, excluding particles inside that producer's body; `exudation.rate` counts only energy actually deposited within the bounded particle budget.

Engulf/bite use the frozen prey ID and recheck contact after movement; they do not retarget. Reach is summed radii plus 0.75. Engulf claims resolve by descending current biomass, then ascending ID: one consumer per victim. Consumed cells cannot harvest or attack later that tick. Surviving bites aggregate damage before applying it, allowing reciprocal kills. Damage is capped to remaining integrity; attackers' extraction requests use their proportion of that actual damage. Requests share the prey's available energy and proportional free-mineral reserves in stable-ID order, with no structural or gut material counted twice. `feed = 0` stops extraction while still permitting bite damage. Any remaining resources from a bite death become food. Recovery does not increase the consumer's biomass directly; growth converts free nutrients separately.

`predation_count` counts engulf/bite deaths; `attack_count` counts contact bite hits, including hits on later ticks. `photosynthesis_total` is actual gross absorbed energy; upkeep is separate. Cell photosynthesis, nutrient uptake, and remodeling rates report actual activity during the last completed tick.

## WASM API, version 6

The module exports `memory`. Pointers/lengths use bytes, except world/environment snapshot lengths, which count `f32` values. Returned buffers are engine-owned: read the matching length and copy immediately before other allocating calls. Refresh JavaScript views after memory growth. Never free engine-owned buffers. Caller-allocated source buffers must be freed once with their exact pointer/length.

| Exports | Behavior |
| --- | --- |
| `abi_version() -> u32` | Returns 5 |
| `alloc(len) -> ptr`, `dealloc(ptr, len)` | Allocate/free a zeroed source buffer; above 256 KiB returns null |
| `compile_genome(ptr, len) -> u32` | UTF-8 source → ID ≥ 1; zero plus readable error on failure |
| `analyze_genome(ptr, len) -> ptr`, `genome_analysis_len()` | Authoritative compiler analysis as engine-owned UTF-8 JSON; no genome registration or simulation mutation |
| `mutate_genome(id, seed, kind) -> u32` | Variant ID; parent ID plus explanation on safe no-op; zero for invalid ID/kind |
| `genome_source_ptr(id)`, `genome_source_len()` | UTF-8 source copy; null for invalid ID |
| `genome_graph_ptr(id)`, `genome_graph_len()` | UTF-8 JSON of the compiled expression graph; null pointer and zero length for invalid ID |
| `genome_parent(id)`, `genome_mutation_kind(id)` | Provenance; zero for authored/unknown genomes |
| `genome_count()`, `genome_limit()` | Current count / 2,048 |
| `genome_bytes()`, `genome_byte_limit()` | Estimated allocations / 64 MiB |
| `set_mutation_rate(rate: f32)`, `mutation_rate()` | Set/get probability, clamped 0–1; nonfinite input becomes zero |
| `mutation_count()`, `mutation_skipped_count()` | Successful/skipped automatic variants since reset |
| `error_ptr()`, `error_len()` | Latest UTF-8 error |
| `reset(seed, genome_id) -> u32` | Recreate world; 1 on success, 0 plus readable error on invalid genome or allocation failure |
| `seed_cells(genome_id, count: f64, x: f32, y: f32) -> u32` | Add the requested cells atomically; return count added. Count must be a finite nonnegative integer within the addressable ID range. Invalid counts, ID exhaustion, and allocation failure return zero with a readable error; count zero is a successful no-op. Negative/nonfinite coordinates scatter globally; otherwise within 38 units |
| `add_food(x: f32, y: f32, count) -> u32` | Add food; return count added. Negative/nonfinite coordinates scatter globally; otherwise within 45 units |
| `set_food_rate(rate: f32)` | Set 0–60 particles/s; nonfinite input becomes zero |
| `tick(steps) -> u32` | Run at most 120 steps; 1 on success, 0 plus readable error if a step cannot allocate. Earlier completed steps in the batch remain valid |
| `snapshot_ptr()`, `snapshot_len()` | Build/read world snapshot; length in floats |
| `cell_memory_ptr(cell_id)`, `cell_memory_len()` | UTF-8 JSON of populated registers; unknown cell returns `[]` and an error |
| `set_environment(temperature: f32, light: f32, current: f32)` | Clamp parameters; nonfinite values use 0.5/0/0 |
| `set_initial_nutrients(level: f32)` | Set the next reset's dissolved mass per patch, clamped 0–5; nonfinite input uses 0.3. Live material is unchanged |
| `add_nutrients(x: f32, y: f32, amount: f32) -> f64` | Add fertilizer, clamped to 1,000; nonpositive/nonfinite amounts return zero. Negative/nonfinite coordinates distribute uniformly; otherwise deposit in the wrapped local patch. Returns actual added mass |
| `nutrient_total() -> f64`, `nutrient_input_total() -> f64` | Current total material / cumulative external input since reset, including initial water, cells, and food |
| `nutrient_dissolved_total() -> f64`, `nutrient_food_total() -> f64`, `nutrient_cell_total() -> f64` | Current water, food, and cell biomass-plus-reserve components |
| `add_source(kind, x: f32, y: f32, strength: f32, radius: f32) -> u32` | New ID, or zero for invalid input/capacity; clamp strength/radius |
| `remove_source(id) -> u32`, `clear_sources()` | Remove by ID (1 success, 0 missing) / all sources; existing heat persists |
| `environment_snapshot_ptr()`, `environment_snapshot_len()` | Build/read fields and sources; length in floats |
| `predation_count()`, `attack_count()` | Predation deaths / contact bite hits since reset |
| `photosynthesis_total() -> f64` | Gross absorbed light energy since reset |
| `reset_empty(seed, initial_food)` | Reset dynamics and counters, retain the environment recipe, food/mutation rates, and genome registry; create requested food (up to 1,200) and no cells |
| `alloc_state(len)` | Allocate a checkpoint input buffer, 20 bytes–128 MiB; release using `dealloc(ptr, len)` |
| `save_state_ptr()`, `save_state_len()` | Build/read an owned versioned checkpoint; length in bytes; copy immediately |
| `restore_state(ptr, len) -> u32` | Validate and atomically replace world and registry; 1 success, 0 rejection with `error_ptr/len` feedback |

Unannotated integer arguments/counters are `u32`. Mutation probability returns `f32`.

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

Analysis changes only its transient response buffer. It does not consume genome IDs or registry space, change the latest engine error, advance RNGs or cells, or alter checkpoint bytes. It remains available when the genome registry is full. The native ABI accepts a null pointer for empty or oversized input; other nonempty inputs must provide a readable UTF-8 source buffer. Copy the returned JSON before another allocating engine call, and free only the caller's source buffer. The analysis JSON schema is unchanged in ABI 6.

### World snapshot

Layout: `12 + cells × 63 + food × 4` floats. Cell offsets 0–52 retain their ABI 4 field meanings. Header offset 9 is now reserved and always zero; it no longer encodes a population limit.

The float32 snapshot is only for rendering. Persistent saves use the checkpoint API below.

```text
Header (12): tick, time, cell_count, food_count, total_births,
             total_deaths, total_eaten, width, height, reserved_zero,
             max_food, food_rate
```

| Cell offset | Field | Cell offset | Field |
| --- | --- | --- | --- |
| 0 | id | 20 | basal_metabolism |
| 1 | genome_id | 21 | memory_used |
| 2 | x | 22 | memory_cost |
| 3 | y | 23 | light_level |
| 4 | vx | 24 | light_angle |
| 5 | vy | 25 | temperature |
| 6 | angle | 26 | temperature_angle |
| 7 | energy | 27 | flow_x |
| 8 | age | 28 | flow_y |
| 9 | generation | 29 | prey_id |
| 10 | parent_id | 30 | prey_angle |
| 11 | radius | 31 | prey_distance |
| 12 | food_angle | 32 | prey_size |
| 13 | food_distance | 33 | prey_visible |
| 14 | food_visible | 34 | threat_angle |
| 15 | neighbor_angle | 35 | threat_distance |
| 16 | neighbor_distance | 36 | threat_size |
| 17 | neighbor_visible | 37 | threat_visible |
| 18 | size | 38 | integrity |
| 19 | energy_capacity | 39 | photosynthesis_rate |

| Cell offset | Added field |
| --- | --- |
| 40 | nutrient_stored |
| 41 | nutrient_capacity |
| 42 | nutrient_level |
| 43 | nutrient_angle |
| 44 | nutrient_uptake_rate |
| 45 | body_propulsion |
| 46 | body_photosynthesis |
| 47 | body_digestion |
| 48 | body_armor |
| 49 | body_sensing |
| 50 | sense_range |
| 51 | remodeling_rate |
| 52 | body_upkeep |
| 53 | body_attack |
| 54 | gut_energy |
| 55 | gut_capacity |
| 56 | gut_nutrients |
| 57 | digestion_rate |
| 58 | growth_rate |
| 59 | birth_size |
| 60 | maturity |
| 61 | lifespan |
| 62 | exudation_rate |

Each food is four floats: **x, y, energy, nutrients**. Visibility is 0/1. Uptake is nutrients/s; remodeling is reassigned biomass/s. `body_upkeep` is the body component before temperature, already included in offset 20: `basal_metabolism = (0.35 × size + body_upkeep) × (0.5 + temperature)`. Basal excludes propulsion, memory, growth, and optional action costs. Gut energy is unassimilated organic energy, and gut nutrients remain distinct from the free nutrient reserve. Digestion rate is usable energy/s after assimilation loss; photosynthesis rate remains gross harvested energy/s; exudation rate is organic energy actually released as food/s. Nutrient fraction, gut fraction, and unassigned body fraction can be derived from the packed stores/fractions.

Inspection sensors are recomputed against the current world. Memory usage therefore shows committed occupancy, while the completed tick charged previous occupancy. Float IDs/counters are exact only through 2²⁴; internally cell IDs are `u32` and tick/statistic counters are `u64`.

### Environment and memory snapshots

Environment layout: `7 + columns × rows × 5 + source_count × 6` floats.

```text
Header: columns, rows, ambient_temperature, ambient_light,
        current_strength, source_count, initial_nutrients
Each row-major patch: light, temperature, flow_x, flow_y, nutrients
Each source: id, kind, x, y, strength, radius
```

Patch nutrients are dissolved mass, not concentration per world-unit area. `initial_nutrients` is the next-reset recipe; the live field can differ. Source stride remains six, with kind 3 denoting nutrient springs.

Memory includes populated slots, ordered by primitive type/address. JSON retains primitive types; vectors use arrays:

```json
[{"name":"ri001","type":"integer","value":12},
 {"name":"rb002","type":"boolean","value":false},
 {"name":"rv999","type":"vec2","value":[0.5,-1]}]
```

### Genome graph

The graph backend retains its existing JSON version-1 format in ABI 6. It inspects a registered program without evaluating it, advancing the world, changing cell memory, or registering a new genome. The UTF-8 buffer is separate from source, memory, and field buffers; copy it before further allocating WASM calls. An invalid ID clears the graph length and returns null with a readable error.

The JSON shape is `{version: 1, nodes: [...], roots: [...]}`. Each node has `id`, `kind`, `type`, `label`, `inputs`, `bindings`, and `reachable`. IDs are actual compiler-array indexes; input IDs preserve operand order and duplicates and are always lower than their consumer's ID. A conditional's inputs are condition, then branch, else branch. Types use `integer`, `scalar`, `boolean`, `string`, or `vec2`. Kinds are `constant`, `sensor`, `register`, `populated`, `reference`, `unary`, `binary`, `conditional`, and `call`. Labels are concise source literals, names, or operators; bindings list aliases attached to that node. Unused expressions remain present with `reachable: false`, so graph counts match the compiler's 8,192-node budget.

Roots have `id`, `kind`, `name`, `type`, `value`, `guard`, and `default`. All **17 output roots** are included (`output:thrust`, etc.), in language order: thrust, turn, divide, grow, photosynthesize, engulf, bite, defend, repair, absorb, invest.propulsion/photosynthesis/digestion/armor/sensing, then feed and invest.attack. An omitted optional output has null `value` and a string default: `"1"` for absorb and feed, `"0.2"` for the first five investments, `"0"` for invest.attack, and `"0"`/`"false"` for older optional actions. Authored outputs have a node ID and null default. Appended defaults do not add expression nodes or appear in printed mutation source unless explicitly authored.

Register rules use IDs such as `write:ri001` and `clear:ri001`, with the register's destination type. Clear roots have null value. A null guard means unconditional; otherwise it is a boolean node ID. There are at most **10,007 roots**: 17 actions plus one write and one clear for each of 4,995 registers. Registers and occupancy checks are **old-memory leaves**, while write/clear roots describe **next memory**. There is no same-tick edge from a write back to a read; simultaneous commit and clear priority remain the language's semantics. Existing graph consumers retain the same schema and use the expanded root count/defaults.

#### Frontend network projection

`src/genome-network.ts` derives the visual network from this unchanged compiler graph. It combines repeated sensor reads by name and register reads/writes/clears by address. Each operator, call, and conditional remains a distinct instance. Constant leaves become inline port values, and reference nodes collapse to their underlying dependencies while preserving binding names. `populated(register)` remains an explicit occupancy operation connected to the register; it is not a value read.

Edges retain operand ports and separate `when` guards. The visualizer uses cyan for reads from inputs/registers, amber for action/register writes, rose dashed lines for clears, and neutral lines for computation. Direct input/register-to-root edges take the destination's write/clear role. Register write and clear edges are explicitly temporal and do not participate in same-tick layout ordering. Thus a unified register node may have a visible cycle without changing the synchronous memory semantics.

Constant writes, constant guards, unconditional clears, and omitted output defaults remain inspectable even when they need no incoming edge. The overview initially hides implicit defaults and isolated constant action outputs; controls reveal them, and authored constants remain in the searchable list. Unused expressions are excluded from the network but counted in compiler statistics. Iterative views contain at most 112 nodes, with focused UI neighborhoods limited to three hops. They show actual dependencies and consumers, expose omitted nodes/frontiers, and never manufacture shortcut edges across hidden nodes. Projection and navigation do not execute programs or change the world.

### Persistent checkpoints

`Engine.saveState()` returns an owned `Uint8Array`; `Engine.restoreState(bytes)` rejects invalid data without changing the live world. Current checkpoint format **2** uses the existing `CELLAR01` magic, a separate format version, exact byte length, and an FNV-1a corruption checksum, followed by bounded little-endian values. The checksum is not an authenticity mechanism; every decoded value is independently checked.

Checkpoints retain float64 positions, velocities, resources, all five register banks and their occupancy, cell age/cooldowns, six body allocations, gut contents, birth size, recent digestion/growth/exudation rates, food, every environment grid/source and source ID sequence, food credit, both world seed and RNG state, tick/event counters, and the full genome registry with lineage. Compiled expression trees, binding scopes, and current-format budget accounting are retained exactly: recompiling a mutated program's source alone could change future mutation selection. Restore checks syntax, expression types and backward references, register types/addresses, finite physical state, resource limits, IDs, and complete input consumption before replacing anything. Cell counts are bounded by the available encoded bytes, rather than a fixed population ceiling. Saves remain limited to 128 MiB; the writer stops before exceeding that buffer budget and reports allocation failure explicitly. Unsupported future format versions are rejected explicitly.

Version **1** is decoded explicitly. Its 15 output roots gain omitted `feed = 1` and `invest.attack = 0`; the original 46 sensor indices keep their meanings. Cells retain their five existing allocations and receive zero attack allocation, empty guts, zero new rates, and `birth.size = min(size, 1)`. The old exact AST and source are both validated and retained, including mutated graphs. Allocation charges are recomputed from the bounded decoded objects because the Program structure grew; subsequent version-2 saves retain those charges exactly. Existing age and resources are retained, and resumed cells adopt the new biological rules. The IndexedDB catalog schema and source export format do not need a reset. The real ABI-4 fixture in `engine/fixtures/trophic-v1.bin` exercises this path, including an expression mutant and populated registers of every primitive type.

Existing ABI-5 version-2 saves require no format conversion. `engine/fixtures/trophic-v2.bin` was captured from the frozen ABI-5 engine and contains an authored program, an expression mutant, all five occupied register types, attack tissue, nonempty guts, and growth activity. Its JSON companion records the reproducible recipe and binary hashes. The loader check requires byte-identical restoration, exact program graphs and register values, and deterministic continuation across independent current-engine instances.

`resetEmpty(seed, initialFood)` initializes a fresh world with no organisms while retaining its configured environment and rates. The UI then compiles chosen catalog genomes and calls `seed(id, count)`; `seed(id, 1)` creates exactly one organism.

## Verification

`cargo test --manifest-path engine/Cargo.toml` covers parsing, mutation, synchronous memory, material conservation, proportional uptake, partial feeding, body remodeling/capabilities, predation, replay, checkpoint migration/continuation/corruption, and ABI behavior. `npm run test:wasm` exercises the compiled module, including all 4,995 slots, the exact 8,192-node bound, multi-tick mutation round-trips, nutrient/body behavior, packed ABI 6 layouts, and lossless checkpoints across separate WASM instances. `node scripts/check-engine-loader.mjs` also verifies populations beyond the former limits, rejected seed counts, and the real version-1 fixture's exact source, mutated graph, cell state, typed registers, and deterministic version-2 continuation. `npm run build:pages` checks TypeScript and emits a static relative-path build.
