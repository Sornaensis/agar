# Cellular programs

Start a source file with `cellular;` to use products, genes and assembly recipes.
The primary built-in genomes use this mode. Existing programs without the marker
keep the previous action/register language and physical bodies.

## Three declarations

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
| `built.Name` | Functional mass of completed machines matching that recipe, reduced by damage. Unfinished machines contribute zero. |
| `activity` | Fast 0–1 regulation of existing complete machinery; omission means 1. |
| `at`, `direction` | Body-relative attachment angle and force/viewing direction in radians. |
| `let` | A pure expression recomputed from the current snapshot. |

Products can be `scaffold`, `anchor`, `motor`, `filament`, `pigment`, `enzyme`,
`membrane`, `transporter`, `receptor`, `regulator`, `replicase` or `constrictor`.
The compiler validates recipe roles, positive quantities, size and finite angles.
Each required role must supply at least 10% of recipe mass. A propulsion mechanism therefore
cannot be manufactured from scaffold alone. Products of the same role can have
different names, allowing independent expression and allocation.

Recipes automatically consume available compatible stock. There is no desired
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

## Physical memory and sensing

```text
product Brake = regulator(8);
gene conserve: Brake =
  if energy.fraction < 0.45 && pool.Brake < 0.02 then 0.2 else 0;
```

`regulator(8)` has an eight-second half-life. Synthesis costs real material and
energy; decay returns its material and dissipates retained energy. Use its stock
in an activity expression, for example `clamp(1 - pool.Brake / 0.02, 0, 1)`.
The stock persists through ticks and is partitioned at division. For a
concentration-like signal, divide stock by `body.area`; `pool` itself reports mass.
Other declared products are stable while uninstalled, and still occupy space and
incur upkeep.

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
programs additionally support silencing one gene (kind 4) and duplicating one gene
(kind 5). Normal mutation may select these operations. Product identities and
physical recipe definitions are fixed by these mutation operations; inherited
machinery survives a regulatory mutation unchanged.

Checkpoint format 6 retains mechanism identities as well as pools, unfinished
ingredients, assembly identities, activities and genetic progress. Formats 3, 4
and 5 remain readable. Old cellular family sources retain their existing semantics;
v5 saves continue with the same physics. The WASM ABI remains 10 and the language
schema remains 3, with an additive `cellular.mechanisms` catalog.

This is a coarse cellular language: named aggregate material pools and discrete
machines implementing a fixed set of small physical operations. There is no
molecule simulation, spatial intracellular transport, user-defined reaction
chemistry or multicellular adhesion yet.

## Material allocation

Synthesis, installation, repair and cell-cycle work share five common resources:
energy, minerals, internal space, surface space and translation work. An ingredient
installation additionally consumes one product pool. The allocator stores just
these sparse costs and reuses work buffers between cells and ticks. It preserves
the prior weighted completion-fraction sharing, demand order, and starting-stock
rule. See [performance measurements](../PERFORMANCE.md) for the focused benchmark
and exact comparison against the earlier dense allocator.
