# Cellular programs

Start a source file with `cellular;` to use products, genes and assembly recipes.
The primary built-in genomes use this mode. Existing programs without the marker
keep the previous action/register language and physical bodies.

## Declarations and controls

```text
cellular;
product Mount = anchor();
product Rotor = motor();
product Tail = filament();

gene mounts: Mount = if built.Swimmer + 0.000001 < 0.04 && pool.Mount < 0.002 then 1 else 0;
gene rotors: Rotor = if built.Swimmer + 0.000001 < 0.04 && pool.Rotor < 0.002 then 1 else 0;
gene tails: Tail = if built.Swimmer + 0.000001 < 0.04 && pool.Tail < 0.002 then 1 else 0;

assembly Swimmer: propel() {
  Mount: 0.01;
  Rotor: 0.01;
  Tail: 0.02;
  at: 3.141592653589793;
  direction: 0;
  activity: 0.25;
}
```

| Construct | Meaning |
| --- | --- |
| `product Name = kind();` | A named material type; declaration creates no stock. |
| `gene name: Product = expression;` | Requested synthesis effort. Genes share paid translation capacity, energy, minerals and space. |
| `assembly Name: operation(target) { ... }` | A physical recipe for one mechanism. Each ingredient specifies actual mass per complete machine. |
| `pool.Name` | Uninstalled mass of the named product in the cell. |
| `concentration.Name` | Uninstalled product mass divided by current envelope area. Growth dilutes this value. |
| `built.Name` | Functional mass of completed machines matching that recipe, reduced by damage. Unfinished machines contribute zero. |
| `sensed.Name.field` | A measurement from the named, physically built receptor. Available fields depend on its mechanism. |
| `start_when` | Boolean permission to start another instance; defaults to true. Existing incomplete instances continue paid installation. |
| `activity` | Fast 0–1 regulation of existing complete machinery; omission means 1. |
| `recycle` | Requested 0–1 disassembly effort for physical instances with this recipe name; defaults to zero and needs active recycling machinery. |
| `at`, `direction` | Body-relative attachment angle and force/viewing direction in radians. |
| `let` | A pure expression recomputed from the current snapshot. |
| `fn name(args) = expression;` | A pure function with up to eight numeric parameters. Calls expand into the bounded expression graph. |

Products can be `scaffold`, `anchor`, `motor`, `filament`, `pigment`, `enzyme`,
`membrane`, `transporter`, `receptor`, `regulator`, `replicase` or `constrictor`.
The compiler validates recipe roles, positive quantities, size and finite angles.
Each required role must supply at least 10% of recipe mass. A propulsion mechanism therefore
cannot be manufactured from scaffold alone. Products of the same role can have
different names, allowing independent expression and allocation.

Recipes consume compatible stock after their initiation condition permits a new instance. There is no desired
organelle count, invisible inventory or instantaneous body replacement. Regulate
gene expression to limit inventory and construction. Small stock buffers reduce overshoot: material already made can keep assembling after a gene turns off. Multiple recipes using the
same named product compete for that stock. Partly installed machines occupy space
and retain their exact ingredient composition; they provide no function until all
ingredients are installed. Damage then reduces their effective capacity.

The small example above demonstrates machinery, not a self-sustaining organism.
[solar.cell](../examples/solar.cell) adds a complete light-harvesting example;
[regulated-motor.cell](../examples/regulated-motor.cell) demonstrates physical
regulatory memory. The full built-in sources also assemble energy processing,
storage, mineral uptake, growth and reproduction.

## Small mechanisms

An assembly names one operation. Several independently constructed assemblies
make a working pathway: `import(particle)` captures food, `store(food)` holds it,
and `digest(tissue)` processes the tissue already in that compartment. Removing
one step removes that capability. Activity regulates the existing machine;
building another enzyme does not grant storage or capture.

| Constructor | Effect |
| --- | --- |
| `support()` | Structural support for viability and division. |
| `translate()` | Shared paid synthesis and installation work. |
| `store(energy)` | Energy capacity. |
| `store(mineral)` | Free-mineral capacity. |
| `store(food)` | Undigested-food capacity. |
| `harvest(light)` | Uses the local shared photon supply to produce energy. |
| `import(mineral)` | Paid uptake of dissolved minerals into free storage. |
| `import(particle)` | Captures nearby organic particles into food storage. |
| `engulf()` | Transfers sufficiently small, damaged organisms into food storage. |
| `digest(exudate)`, `digest(tissue)`, `digest(waste)` | Processes only the named stored substrate. |
| `propel()` | Applies a force at its declared attachment point. |
| `pierce()` | Contact damage; collecting food still requires the relevant machinery. |
| `shield()` | Protection from damage. |
| `repair()` | Paid repair of existing structure. |
| `sense(light)`, `sense(heat)`, `sense(chemical)` | Local measurements of the named physical field. |
| `sense(contact)` | Relative flow and physical contact measurements. |
| `sense(optical)` | Directional visible-object measurements with occlusion. |
| `grow()` | Paid envelope growth, limited by existing structural support. |
| `replicate()` | Paid genome copying. |
| `constrict()` | Paid constriction after replication and feasible geometry. |
| `recycle()` | Paid disassembly work for instances selected by recipe recycling expressions. |

Constructor targets are typed identifiers, not strings or expressions. For example,
`digest(light)` and `store(tissue)` are compile errors. Recipe material roles and
minimum physical size are checked for every mechanism.

Storage and support are passive physical structures: setting activity to zero
does not erase their capacity. Active enzymes, importers and motors obey activity.
Receptors use the stored activity from the preceding tick when observations are
sampled, so regulation cannot rewrite the current observation.

Substrate enzymes have separate paid processing capacities; they do not need a
generic metabolic organelle. Light harvesters likewise have their own reaction
capacity, while competing for the same finite habitat light. Digestion retains
the existing substrate-specific conversion losses and material recovery rules.
Two recipes may consume the same named enzyme stock and compete for it, as shown
in [separate-digestion.cell](../examples/separate-digestion.cell).

Bare family names such as `digestive`, `core` and `reserve` retain the previous
combined capabilities for saved sources. The eight default programs now use only
mechanism constructors. All introduced cells still receive the same paid minimal
founder core and finite reserves to bootstrap construction; subsequent support,
translation and storage are explicitly assembled. Daughters can survive entirely
on the new support machinery and inherit actual storage and translation machines.

## Physical memory and pure functions

```text
product Brake = regulator(8);
gene conserve: Brake =
  if energy.fraction < 0.45 && pool.Brake < 0.02 then 0.2 else 0;
```

`regulator(8)` has an eight-second half-life. Synthesis costs real material and
energy; decay returns its material and dissipates retained energy. Use its stock
in an activity expression, for example `clamp(1 - pool.Brake / 0.02, 0, 1)`.
The stock persists through ticks and is partitioned at division. Read
`concentration.Brake` for mass per envelope area; `pool` itself reports mass.
Other declared products are stable while uninstalled, and still occupy space and
incur upkeep.

`concentration.Brake` directly reads `pool.Brake / body.area`. For a smooth
response, `activate(c, k)` computes `max(c, 0) / (max(c, 0) + max(k, 0))`, with a
zero result when the denominator is zero. `repress(c, k)` is `1 - activate(c, k)`.
These helpers have no state or implicit synthesis. For example:

```text
fn brake_response(amount, threshold) = repress(amount, threshold);
let motor_activity = brake_response(concentration.Brake, 0.02);
```

Functions accept zero to eight numeric arguments, promoted to scalars. Their
result type is inferred; a function can return a boolean, scalar or another
supported expression type. Define a function before calling it. Its references
to preceding `let` bindings are lexical; calls cannot change those references.
Functions are nonrecursive, at most 64 may be declared, and inlined bodies count
toward the ordinary expression-node and nesting limits.

## Receptors with local readings

Each `sensed.Recipe.field` reads the named recipe's physical receptors at their
body-relative attachment point. Light, heat and chemical receptors report local
field values, without a supplied gradient direction. Optical receptors use their
declared viewing direction, light, range and occlusion. Contact receptors report
nearby surfaces and relative flow at their attachment point.

| Mechanism | Named fields in addition to `available` and `valid` |
| --- | --- |
| `sense(light)` | `level` (`light` is an alias) |
| `sense(heat)` | `value` |
| `sense(chemical)` | `mineral`, `organic` |
| `sense(contact)` | `offset`, `pressure`, `resistance`, `flow`, `speed`, `bearing` |
| `sense(optical)` | `offset`, `area`, `contrast`, `pigment`, `motion` |

Offsets, flow and motion are vectors in body coordinates; observation offsets
start at the receptor's position. Absent or inactive receptors report
`available = false`. A missing optical/contact return reports `valid = false`
and zero observation values. Contact receptors still measure flow when no
surface is touching: guard `flow`, `speed` and `bearing` with `available`.
Other measurements require `valid`.

Matching instances of a recipe share its exact attachment and direction. Their
functional mass, multiplied by stored activity, is summed to determine sampling
quality and range; they do not duplicate a reading or select separate targets.
Different named recipes are sampled independently. Optical selection ranks
visible contrast, distance and angular offset; contact selects the closest
surface gap to the receptor. Only channels used by the program are sampled.

```text
let difference = sensed.LeftEye.level - sensed.RightEye.level;
```

Two motors can react differently to this difference. The full
[local-control.cell](../examples/local-control.cell) example compares two light
receptors, builds a decaying brake in shade, dismantles motors and permits their
regrowth when the brake concentration falls. Existing whole-cell sensor names
remain available for earlier sources.

## Construction control and material recovery

`start_when` controls initiation separately from expression and activity. It is
sampled with the other program expressions. If false, no additional instance
starts; previously started instances still consume paid installation work.
Several recipes can therefore share a named product while choosing independently
when to begin construction. The default is true for compatibility.

An active, complete `recycle()` mechanism supplies finite disassembly throughput.
A recipe's `recycle` expression requests dismantling of instances with that
recipe name, including inherited instances of earlier definitions. Recovery
always uses each instance's actual ingredient identities. Positive disassembly
requests suppress installation into the same instance during that tick.

Recycling costs energy and shared work. It preserves accounted material and
retained energy without refunding usable energy. Recovered stocks and freed room
cannot fund another operation in the same tick. Incomplete machinery can also
be dismantled. The Body inspector shows new-start permission, requested recycling
and achieved material recovery, separately from activity.

Cellular expressions can read internal energy, body geometry, stored minerals,
gut contents, age, speed and integrity. External light, heat, chemical, flow,
contact and optical observations require actual functioning receptors. Cellular
mode excludes global time/IDs, the legacy division-ready command signal,
organelle diagnostics and sparse register reads/writes. Player inspection can
show richer state than a genome can sense.

## Movement and lifecycle

Each completed motor contributes a force in its declared direction and torque
from its attachment point. Forces add; torques use the cross product of the lever
arm and force. Opposing motors can cancel movement while still spending energy.
Changing activity preserves the built motor; changing future expression does not
erase it.

Primitive piercers act from their attachment point along their declared
direction. Primitive shields protect their local surface sector, and contact
damage is localized around the struck surface. Whole-cell environmental stress
continues to affect the body. Existing family organs retain their compatibility
interaction rules.

Growth and reproduction use three explicit mechanisms:

| Assembly | Physical effect |
| --- | --- |
| `grow()` | Pays for new envelope area using membrane and enzyme machinery. |
| `replicate()` | Pays for a material-bearing genome copy over multiple ticks. |
| `constrict()` | Performs paid constriction after replication and feasible division geometry. |

Fission still requires sufficient energy, doubled birth area, elapsed cooldown
and a feasible partition with sufficient structural support in each half. Constriction
completion becomes available to the next tick. Product pools split, machinery
instances are assigned whole, and copied genetic material becomes inherited
material. Surplus contents caused by reduced inherited capacity spill into the
environment. A changed genome cannot reinterpret existing ingredient identities.

Cellular fission requires the actual division cost, without an extra reserve
proportional to empty energy-storage capacity. Cellular envelope repair follows
the same affordability rule. Authored activity conditions can require a larger
reserve. Legacy programs retain their existing reserve rules.

Cellular partitioning prefers to distribute complete functional duplicates of
each exact recipe between the two cells. Identity includes ingredient definitions,
amounts and attachment geometry, so opposite motors or altered products remain
distinct. Core support and physical space take priority; the allocator can fall
back to the existing load-based packing when necessary. It never copies a lone
instance or guarantees that a cell which failed to build duplicates will produce
two independently functioning daughters.

Overflow after partition can exhaust a daughter with no functioning energy
compartment. If it dies in that same tick, its body and gut material still return
to the environment through the ordinary death path.

Secreted organic particles are placed just outside the emitter's inclusive
capture surface, including when a secretion merges with an existing packet.
This geometric clearance prevents immediate reingestion. It does not prevent
later capture after the cell and particle move back into contact.

## Tick and accounting rules

1. Snapshot existing bodies, stocks, capacities and sensed surroundings.
2. Evaluate genes and activities from that same snapshot.
3. Existing machinery acts: movement, uptake, feeding, photosynthesis and damage.
4. Perform paid growth, repair, installation, synthesis and replication. Only
   previously available product stock can be installed this tick.
5. Resolve eligible division and resource spills.

Material is counted in free minerals, product pools, each installed ingredient,
envelope, genetic material, guts, particles and dissolved fields. Transfers do not
create material. Fabrication spends energy, retains a fraction as embodied energy
and dissipates the remainder. Gene duplication changes demand, not the shared
budget. Recipe definitions are retained with each physical instance.

## Inspection, mutation and saves

The Body inspector shows product stocks, requested and achieved synthesis,
recipe composition, progress, shortages, activity, placement and replication.
The Program inspector and graph expose gene/activity expressions and pool/built
dependencies. Inspection does not commit a tick.

Constant, expression and reference mutations remain source preserving. Cellular
programs additionally support silencing one gene (kind 4), duplicating one gene
(kind 5), and changing a future recipe's attachment or direction (kind 6).
Normal mutation may select these operations. Placement mutations change future
construction; inherited machinery retains its actual recipe and ingredient
identities. A same-named recycling request can dismantle that earlier machinery
without reinterpreting its components.

Checkpoint format 7 retains mechanism identities as well as pools, unfinished
ingredients, assembly identities, activities and genetic progress. Formats 3, 4,
5 and 6 remain readable. Existing sources default to unrestricted initiation and
zero recycling. Corrected optical coverage and directional primitive interactions
can change affected trajectories. Save/restore replay is deterministic within the
current engine. The WASM ABI remains 10 and language schema remains 3, with
additive receptor-field, recipe-control and pure-function metadata.

This is a coarse cellular language: named aggregate material pools and discrete
machines implementing a fixed set of small physical operations. There is no
molecule simulation, spatial intracellular transport, user-defined reaction
chemistry or multicellular adhesion yet.

## Material allocation

Synthesis, installation, disassembly, repair and cell-cycle work share five common resources:
energy, minerals, internal space, surface space and translation work. An ingredient
installation additionally consumes one product pool. The allocator stores just
these sparse costs and reuses work buffers between cells and ticks. It preserves
the prior weighted completion-fraction sharing, demand order, and starting-stock
rule. See [performance measurements](../PERFORMANCE.md) for the focused benchmark
and exact comparison against the earlier dense allocator.

Cellular work can spend the cell's actual usable energy. There is no hidden
reserve proportional to storage capacity: building an empty energy compartment
does not make existing energy inaccessible. Programs regulate their own reserve
through gene expression and initiation conditions, while every accepted operation
still pays its full energy, mineral and work costs. This matters when a hungry
cell must synthesize an enzyme for food already in its gut.
