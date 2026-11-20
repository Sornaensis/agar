# agar

A playable, client-only artificial-life simulator: a TypeScript interface and canvas renderer backed by a Rust WebAssembly engine. Single cells carry reactive cellular programs, material-bearing products and individually assembled machinery. The earlier action/register language remains supported for existing worlds. Cells compete for light, dissolved minerals, inorganic carbon, oxygen and organic material in a wrapping 2D environment with heat and currents.

## Run

Use Node.js 22.12 or newer. The source includes a compiled WASM module, so Rust is not needed for frontend work.

```sh
npm ci
npm run dev
```

To use the included build, run `python3 -m http.server 8000 --directory docs` from the project folder and open `http://localhost:8000`. For development, open the local URL printed by Vite. In **Simulation**, start **Springwater basin** to follow producers, grazers, hunters and colonies in an 800 × 520 habitat. Two mineral springs, a mineral sink, paired currents and three barriers shape the habitat. You can edit the founders, environment and seed before starting.

## Whole-count resources

New simulations store ATP, minerals, oxygen, inorganic carbon and organic carbon in unsigned 32-bit counts. One former resource unit equals **10,000 counts**. A founder receives 680,000 ATP, 6,000 free minerals and 34,000 reserve organic carbon. Light, heat, movement, sensor concentrations, activity fractions and developmental progress remain continuous. Each chemical reaction transfers whole molecules. World totals use 64-bit integers.

Chemical emitters and absorbers accept **0–15,000 counts per second** in both the builder and the player. At a setting of one, a source delivers one count per simulated second. Resource-limited uptake, diffusion and slow production carry fractional work between ticks until a whole transfer is due. The carbon and mineral entering one product count come out of the cell’s free reserves. Photosynthesis fixes one inorganic carbon into one organic carbon and releases one oxygen for 20 light-energy counts. Aerobic respiration of ordinary organic carbon consumes one oxygen and returns one inorganic carbon, 16 ATP and four counts of dissipated energy.

Use **Resource arithmetic** when starting a simulation to compare whole counts with continuous chemistry on the same seed. Saved runs retain their arithmetic mode. Existing genome sources with the `cellular;` header still read reserves in their original units. [Genome units and arithmetic](design/cellular.md#quantities-in-a-counted-program) · [Measured comparison](experiments/integer-chemistry.json)

## Genome editor

The **Editor tools** menu searches declarations, opens references, jumps to a line, and renames scoped symbols. F12 follows a definition, Shift+F12 lists its references, F2 renames it, and Ctrl/Cmd+Shift+O opens the outline. Renaming edits linked identifiers together and can be undone in one step. The outline, definition jumps and references also work in a living cell’s read-only program. Use the gutter or the command menu to fold a long assembly.

Ctrl/Cmd+Space suggests legal names for the current expression. Call hints show the active argument. For an unambiguous misspelling, the underlined diagnostic offers a one-click correction. The Rust checker reports independent errors together and warns about ineffective recipe settings such as trying to close a storage compartment with `activity`. Warnings allow saving.

## Cellular programming

Built-in genomes use `cellular counts;` programs: declare typed **products**, express them through **genes**, then consume their stored material in explicit **assembly recipes**. Each recipe names one mechanism, such as `store(food)`, `digest(structural)`, `import(mineral)` or `propel()`. Storage, capture, digestion, structural support and translation are independently built. A motor needs anchor, motor and filament material; a missing ingredient prevents it from functioning. Gene expression, assembly initiation and machine activity are separate. Replication and constriction require built machinery and paid progress.

Named receptors expose `sensed.Eye.level` or `sensed.Chemistry.organic` at their actual attachment points. Regulatory products provide physical memory, read as whole counts with `pool.Name` or mass per area with `concentration.Name`. Pure `fn` definitions and `activate`/`repress` helpers express reusable feedback. A recipe's `start_when` permits new instances without stopping ones already underway. Its `recycle` expression selects physical machinery for paid recovery by a built `recycle()` mechanism. Current defaults share products between independently regulated recipes.

A free product can turn over at a rate specified by its genome. For example, `product Catalyst = enzyme(90);` gives uninstalled enzyme a 90-second half-life. Omit the half-life to keep ordinary stock stable. A mutation can change a stock’s lifetime, an ingredient amount in a recipe, or the substrate preference of a machine. An already installed machine keeps its chemistry.

The basin includes a producer that converts reserve carbon into an inhibitory ligand and releases it through membrane transporters. A consumer that captures ligand alongside food digests the food more slowly. A recovery enzyme can convert the ligand back into reserve carbon at one ATP per carbon. Both synthesis and recovery depend on installed enzyme, available substrate and room for the product. Select a particle to see its ligand fraction, or inspect a cell’s **Physiology** to see the amount it holds.

Registers hold values between ticks. A built membrane display exposes a marker to a local contact receptor. Paired signal junctions carry a separate scalar between bonded cells. **Rememberer** compares organic concentrations at its left and right chemical receptors and remembers the resulting turn for up to 1.5 seconds. **Mixotroph** remembers its feeding mode between bright and dark water. **Predator** builds eight display patches before arming its front piercer. Before division it fills the gaps with eight more and positions touch receptors by the cleavage plane, so each half inherits a ring of displays. A newborn writes marker 17 and payload −2.5 on its first tick (1/30 second). A matching value at the attacking surface protects the other cell. An unmarked cell remains eligible prey.

Select a cell and open **Memory** to see its registers or **Program** to inspect a contact reading. Each cell in a colony still builds its own machinery and pays its own energy costs. The basic adhesion program is also available as an [editable example](examples/colony.cell).

### What it costs to build a capability

A new founder receives a paid core and starting reserves. Its specialized machines start unbuilt. The core supplies a finite amount of work each tick, shared by all competing production stages. A product becomes available to the next stage on the following tick.

| Stage | Inputs actually consumed | Where the investment goes |
| --- | --- | --- |
| Make a primary product | One free organic carbon and one free mineral per unit of product, its role-specific ATP cost, and 0.6 core work | The product pool acquires that mass and retains 90% of the ATP spent. |
| Finish adhesin | One unit of the **named scaffold precursor**, 25 ATP and 0.6 core work per unit | The precursor’s mass and stored energy move into finished adhesin, which also retains 90% of the finishing cost. |
| Assemble a machine | Its specified ingredients from existing pools, installation ATP and 0.4 core work per unit installed | The installed parts carry their previous investment plus 90% of installation ATP. The machine only works after it receives every ingredient. |
| Grow and replicate | Working growth or replication machinery, one carbon and one mineral per unit of new frame or replica, 80 ATP and one core work per unit | New frame and genetic material retain their mass and fabrication energy. Constriction then consumes time and ATP. Division charges 12 ATP per unit of parent area and partitions the existing body. |
| Repair or recycle cellular machinery | Repair uses 60 ATP and one core work per unit restored. Recycling uses 20 ATP and 0.4 core work per unit dismantled. Both require the corresponding machinery. | Repair restores integrity. Recycling returns the actual ingredients, up to their recorded synthesis and finishing investment. Installation energy dissipates. Death releases the remaining investment in tissue. |

An adhesive surface must contain anchor, membrane and finished adhesin, each at least 10% of its recipe mass. One binding site requires 0.0015 units of exposed, active adhesin at each end of a bond. Damage or deactivation can reduce this capacity. In the isolated production test, the cell made scaffold on tick 1, finished adhesin on tick 2 and completed its first working surface on tick 41. Removing any of the four producer genes prevented the surface from working. Run `npm run test:production-chains` to repeat the measurements. [Production measurements](experiments/production-chains.json)

Light harvesting draws on local light and inorganic carbon to produce organic carbon and oxygen. Digestion moves the ingested substrate into reserves and waste. Aerobic respiration consumes organic carbon and oxygen, releases inorganic carbon and yields ATP. These reactions supply the reserves charged by the construction steps above. [Gas and carbon accounting](design/gas-exchange.md)

**Food web.** Phototrophs supply soluble carbon in two ways. Paid membrane outlets shed particles into the current. A Forager that repeatedly senses organic carbon while its gut is nearly empty can build an adhesive patch before it meets a producer. A matching display at the patch permits docking. The Forager can then receive carbon and return minerals through paired ports. Its parent builds three smaller food stores and four translation assemblies as it prepares to divide.

Rememberers follow organic plumes and catch detritus on either flank. In moderately lit water, a Mixotroph can keep fixing carbon while digesting a passing meal. Ligand producers increase synthesis after membrane damage. The Forager can build a recovery enzyme after ingesting ligand. Predators hunt unmarked cells and spare a matching display at the attacking surface.

Start a new basin to use these genomes. A saved basin keeps its own programs and descendants. [Springwater food-web measurements](experiments/README.md#springwater-basin)

Open **Metrics → Food web** and select a founder lineage to see its population history, recent births, deaths and kills, time since its last birth, ATP saturation and progress toward doubling its area. The reserve readout also counts living cells whose ATP is at or below 10% of capacity while reserve organic carbon alone contains gross energy worth at least 25% of that capacity. Mutants count toward their founder's totals. The player records a census every 15 simulated seconds while the run advances and keeps the latest 720 samples. Older saves start their event records when first opened with this version.

**Resource balance** compares the current inventories against external inputs and withdrawals. In whole-count runs it also checks energy against measured losses from maintenance, respiration, decomposition, fabrication, feeding and division, and storage overflow. Older saves show their opening energy difference separately. Digestion carries fractional ATP charges and mineral recovery forward between bites, so dividing one meal into smaller pieces preserves its eventual cost and yield.

**How surfaces recognize a partner.** In `import(particle)`, `pierce()`, `engulf()` and `adhere()`, `engage_when` is a boolean expression evaluated for each contact candidate. The Predator’s touch hardware checks the candidate for its marker or payload before a pierce or engulf. Immediately after division each half holds its weapons for the tick that initializes the daughter’s registers. Nurse rosettes also require `peer.division_interface` on their adhesive surfaces. A different genome can use a range of signal values, combine the marker and payload from a facing display, or bind without a marker. [Language reference and examples](design/cellular.md#register-memory-contact-messages-and-colonies)

**How colonies form.** Nurse rosettes, Crown rosettes and Pulse swimmers grow by retaining daughter cells at division. When a bonded cell divides again, each existing attachment can stay with the half that still touches its partner. Separately placed rosette or swimmer founders stay independent when they touch. Phototrophs and Foragers can recruit each other for exchange. Each builds a marker display and recognizes the other’s display through touch hardware at the dock. Their active adhesive patches must overlap. Recruitment takes 15 consecutive ticks of contact (half a simulation second). Only an established bond permits junction transport or a group move.

**Crown rosette** grows colonies through division. Cells with multiple signaling neighbors emit a beacon. An edge cell receiving that beacon can use a second light harvester. With sufficient carbon and ATP in reserve, the rim cell releases food through an outward-facing carbon exporter. Losing the beacon closes the outlet. The [editable genome](examples/crown-rosette.cell) checks the occupancy and payload of individual ports linked by active signal junctions. [See the three-cell colony in the player](screenshots/cellular-body.jpg).

**Nurse rosettes** and **Pulse swimmers** grow alongside the solitary feeders in the basin. The rosette’s young cells request support. Attached adults increase their light-harvesting effort, and compatible junctions share usable energy, free minerals and organic carbon according to reserve levels. Pulse swimmers relay short signals through paired membrane junctions on physically bonded partners. A received pulse activates the young cell’s motor and starts a recovery period before it can fire again.

In the player, a teal bar marks a colony bond retained at division. An amber ring marks a feeding link between cells that docked on contact and have matching functional resource ports. A thin gray line marks any other contact bond. On a bonded cell, a violet or amber dot shows a positive or negative outgoing signal. A gray dot marks a published zero. You can also inspect a solitary cell’s outgoing signal in its Overview. Select a cell and open **Body → Resource exchange** for its bonded partners and signed transfers in the last tick. **Settings → Cell connections** toggles the highlights. [Nurse rosette](examples/nurse-rosette.cell) · [Pulse swimmer](examples/pulse-swimmer.cell)

See [the cellular language and physical rules](design/cellular.md), [the solar example](examples/solar.cell), [a regulated motor](examples/regulated-motor.cell), [a feeding pathway](examples/separate-digestion.cell), and [local sensing with motor recycling and regrowth](examples/local-control.cell). The Body inspector shows stocks, recipe composition, bottlenecks, placement, new-start permission, recycling and replication. Named observations are available through the Program inspector's input reference and live values. The following organelle/action-language details also document compatibility behavior for saved legacy programs.

### What a predator gets from a kill

New body material retains 90% of the fabrication energy spent on it. When a cell dies, its tissue carries the energy stored in the parts still present, its organic reserves, and its remaining free ATP. Gut contents keep their original food types. Whole-cell engulfment transfers the same meal to the predator, subject to gut capacity.

A hunter must capture, digest and respire the meal to raise its own ATP. Food energy and carbon are tracked separately, so two tissue fragments with the same carbon content can have different food values. With room in its reserves, a predator assimilates 65% of the tissue it processes and leaves 35% in waste. Oxygen-supported respiration converts 80% of the assimilated potential to ATP. A 100-energy tissue meal can therefore supply 52 ATP on the first pass, before the predator’s own costs. Recycled parts contribute only what the cell still holds.

In a matched seed-42 encounter, the same grown Phototroph released 127.69 energy units under ABI 16 and 350.84 under ABI 20. The predator ended the 60-second encounter with 67.57 and 132.04 ATP respectively. A fasting predator of the same age ended with 18.05 ATP in both versions. Both engines used the same prey with docking switched off to isolate feeding. Run `node scripts/check-predation-yield.mjs` to measure the current catalog genomes, or provide the ABI 16 module with `--baseline` to repeat the paired comparison. [Measured outcomes](experiments/predation-yield.json)

## Workspace

| View | Use |
| --- | --- |
| Simulation | Open saved runs or start a new one. Saved runs reopen paused. |
| Environments | Create, inspect, edit a new copy, rename, import, export or delete a habitat recipe. |
| Genomes | Create or copy programs, check types, mutate a draft, inspect its graph and explicitly save. |

In **Player**, choose **Add organism**, select a genome, then click or tap the water to place a founder. Choose **Place several** to keep placing that genome or press Escape to finish. With the canvas focused, Enter uses the center of the view.

Select a cell to see its energy, minerals, integrity, and cell connections. Drag the selected cell or choose **Move cell** and click a destination. **Move linked group** translates every cell connected by a physical bond. **Edit reserves** changes energy and minerals up to the cell’s current capacity. Setting energy to zero kills the cell and releases its remains. Placement, movement and reserve edits work while running or paused, including in the floating player.

Open **Body** to inspect built machinery, resource exchange and incoming colony signals. Unbuilt recipes, physical budgets and production rates are expandable. **Memory** shows registers and **Program** shows the evaluator. **Metrics** tracks population and feeding capabilities, with food, gas, mineral and energy accounting in expandable sections.

Open **Genomes** inside a simulation to follow its ancestry tree. Each root is a genome registered independently in this run. When a daughter inherits a changed genome, its variant branches from the parent genome. A division that copies the genome stays on the same branch. The same change arising again from the same parent reuses the existing variant.

Each row shows how many living cells carry that exact genome and their highest cell generation. The hierarchy initially shows lineages with living carriers, including extinct ancestors on their paths. Turn on **Show Extinct** to see branches that have died out. The player remembers this preference across runs. Expand or collapse a branch, select a genome to inspect its program or graph, or choose **Newest variant** to reveal its place in the tree. Saved runs retain the parent links and mutation types, including runs saved before this view was added. [See a branching Crown rosette culture](screenshots/genome-lineages.png).

Built-ins stay unchanged. Editing or importing creates independent catalog items. Running simulations own their programs and environment state, so catalog renames and deletions do not alter existing worlds.

## Floating player

**Pop out** moves the current player into a browser picture-in-picture window that stays on top. It keeps the same simulation, controls, camera and inspector, and continues while the main app shows Metrics, Settings, Environments or Genomes, or while the main tab is in the background. The simulation fills the pop-out. Hover anywhere inside to show the controls and open panels. Moving the pointer outside hides them while the simulation keeps running. Tab reveals controls for keyboard use, and a tap reveals them on a touchscreen. The browser manages its own outer window frame. **Return to tab** or closing the floating window restores that same player. Explicit Pause still pauses.

If Document Picture-in-Picture is unavailable, **Window** opens an ordinary popup. The main tab identifies this fallback and provides a return control. The originating tab must remain open. An open player continues stepping even when both windows are hidden, using a bounded timer instead of drawing frames. Browser throttling may slow it; suspension, system sleep and closing the main tab can suspend or end playback. There is no offline catch-up. The existing dedicated worker remains the sole simulation owner. Without an open player window, hiding the main tab still pauses advancement.

## Editing the environment

Click or tap an emitter, absorber, flow generator or barrier to open its controls. Drag it to a new position while the world runs or is paused. With the canvas focused, the arrow keys nudge the selected object by one world unit, or ten with **Shift**. Its strength, radius and other settings stay with it. Open **Environment** to select an object from the list, remove it, or choose a placement tool. Click to place an object, or press **Enter** on the canvas to place it at the center of the view.

The environment builder has the same positioning controls, plus starting mineral and gas concentrations. Floating barriers hold their positions in the preview and start drifting when the simulation runs. The player and builder each remember their chosen field overlay across edits and reloads. The floating player shares the main player's selection.

| Object | Controls and effect |
| --- | --- |
| Light | Strength and radius set local illumination. |
| Heat / cooling | Positive strength heats and negative strength cools. |
| Vortex | Radius sets its extent; strength controls intensity and rotation direction. |
| Mineral emitter / absorber | Adds or removes dissolved minerals used to build cells. |
| Oxygen emitter / absorber | Adds or removes dissolved O₂ used for respiration and aerobic decay. |
| Inorganic carbon emitter / absorber | Adds or removes dissolved inorganic carbon used in photosynthesis. |
| Aerator | Exchanges both gases with the configured reservoir, including in sealed water. |
| Directional jet | Direction, length and half-width describe a local stream. Negative strength reverses it. |
| Current band | Direction, length and half-width describe a long current; combine bands with opposite strengths for neighboring currents. |
| Radial flow | Positive strength drives outward flow; negative strength draws inward. |
| Fixed / floating barrier | Length, thickness and angle define the obstacle. Fixed barriers stay in place; floating barriers drift with the local current. Their motion mode can be changed after placement. |

**World size** is configurable in the builder and live drawer: width **400–4,800**, height **400–3,040**, in increments of **20** world units. This changes the simulated domain and field grid. A resize preserves existing material; newly added space does not create nutrients. Every rotated barrier must fit within the world dimensions. The **Current bands** preset demonstrates a larger world with opposing currents, a vortex, fixed and floating barriers, and a mineral absorber.

**Navigation:** scroll to zoom around the pointer; Shift-drag, middle-drag or right-drag to pan. Touch supports dragging and pinch zoom. The **−**, **+** and **Fit** buttons work in both views. With the canvas focused, **+ / −** zoom and **Home** fits the world.

Currents are prescribed flow fields, not a pressure-solving fluid simulation. Barriers represent full-depth obstacles in this wrapping 2D domain: they obstruct organisms, particles and transport across the wall; they are not boats that water can pass underneath. Their field effects are resolved on the 20-unit grid. Floating barriers follow the current without a full rigid-body buoyancy or angular-momentum model. 

## Gas exchange

New worlds begin with dissolved minerals, oxygen and inorganic carbon. The builder sets their initial concentrations. Each chemical has its own local emitter and absorber. Strength 1 requests 1.5 units per simulated second across the entire footprint. Radius spreads that throughput over nearby water patches. Absorbers draw only from available stock. **Gases & atmosphere** controls surface exchange. An aerator exchanges both gases with the configured reservoir, including in sealed water.

Light powers built photosynthetic machinery. Fixing one unit of dissolved inorganic carbon produces one organic carbon and one O₂. Fixed carbon enters the organic reserve. The stock Phototroph builds an ATP-powered outlet and secretes surplus carbon there. Consumers digest organic matter from the ecosystem. Both producers and consumers can respire one organic carbon with one O₂, returning one inorganic carbon to the water via membrane exchange. Respiration converts 80% of the associated chemical energy to ATP, or 16 units for plain carbon. Minerals are the pooled noncarbon resource used to build and repair cells. Carcasses, spills and digestive waste keep their accounted material and energy.

Organism Overview shows internal gases, organic carbon, fixation and respiration. Metrics includes carbon and oxygen inventories and balances, including atmospheric input/output and background aerobic decay. All units are abstract; carbonate chemistry, pH and anaerobic pathways are outside the current model. See [Gas exchange and material cycles](design/gas-exchange.md) for reactions, phase order, conservation and migration rules.

## Bodies and capabilities

`body.area` is paid envelope area; `body.mass` is the mechanical mass of the scaffold, organs, assembly proteins, reserve minerals and gut minerals. The separate carbon and dissolved-gas pools do not add to this mechanical mass. ATP-like usable energy, gut chemical potential and embodied structural energy are separate. Compartment contents occupy their reserved capacity without being counted twice as internal space.

Growing the envelope creates room and increases upkeep and drag. It does not grant a motor, sensor or energy source. Organs use internal space; membrane-facing modules also use finite attachment capacity that scales with perimeter. Incomplete and damaged organs retain their material and occupied space.

The following family names describe the legacy action language. Current cellular programs use named mechanisms and read local samples through `sensed.Recipe.field`.

| Family | Capability |
| --- | --- |
| `core` | Essential machinery, bounded assembly and basic metabolism/repair |
| `photosynthetic` | Shared-light collection and carbon fixation |
| `metabolic` | Energy-processing throughput |
| `digestive` | Gut storage and processing |
| `reserve` | Additional energy/mineral storage |
| `transporter` | Supply-limited mineral uptake |
| `phagocyte` | Contact capture into digestive space |
| `flagellum` | Thrust and steering, with attachment and drag costs |
| `piercer` | Contact damage and accessible material extraction |
| `cortex` | Protection and active defense capacity |
| `repair` | Additional paid repair throughput |
| `photoreceptor` | Local light and brightness gradient |
| `thermoreceptor` | Temperature and thermal gradient |
| `chemoreceptor` | Local mineral/organic cues and gradients |
| `mechanoreceptor` | Relative flow and physical contact |
| `optical` | Visible object geometry, appearance and motion |

The genome is the body plan. An introduced cell has area 1, one core of mass 0.04, 68 usable energy, 3.4 organic carbon and 0.6 free minerals in the core's bounded compartments. Its frame, core, reserves and fabrication costs are charged to the introduction ledgers. There are no preassembled specialist bodies or genome-name mappings. Photosynthetic units, digestion, motors, sensors, storage and additional cores all have to be grown through ordinary paid ticks.

Programs must establish an energy source before spending their reserves on further development. The built-ins use actual organelle state to sequence construction, then build duplicates for division. This ordering is part of their visible source and can be edited or mutated. Changing a target changes future construction; it never replaces the living cell's existing organs.

Current consumers build capture, digestion and navigation before optional storage and support. Predators assemble a piercer before attempting growth, double their initial exudate-digestion capacity relative to the former genome, and stage the machinery that a daughter will need. Captured food can trigger construction of its missing enzyme even at low energy, provided the cell can pay the actual costs. Cellular construction, envelope repair and fission use actual available energy; empty storage capacity does not impose an additional reserve. Programs set their own energy thresholds.

Current programs tolerate modest loss of working capacity before requesting replacement machinery. A paid `repair()` mechanism responds to actual damage, including during development. This prevents small injuries from filling a cell with duplicate equipment. The construction tolerance is capped below one recipe unit so larger body goals still require every whole copy.

Each `express.<family>` produces that family’s assembly proteins from minerals, organic carbon and energy. Proteins persist between ticks, occupy internal space, add upkeep and turn over into recovered minerals and carbon, with dissipated energy. Count/size targets assemble organs from matching proteins already present at the start of the tick; expressing a gene cannot create a functional organ immediately. Expression and assembly share finite core throughput, energy and space. Leaving every gene on can fill the cell with unused protein and delay the machinery it needs. Regulate expression using `protein.<family>`, actual organ mass and resource reserves. Allocation uses a common starting snapshot; new space, recovered resources and newly functional machinery are used on following ticks. Lowering a target does not remove anything without explicit recycling. Recycling costs processing energy and returns only accounted material and a fraction of retained structural energy. Capacity loss releases mineral, organic-carbon and gut overflow into the environment; excess usable energy dissipates.

Division requires doubled birth area, sufficient energy, cooldown and a feasible packing of actual organs with a viable core in each half. Discrete organs are assigned, never cloned. The working constrictor sets the separation axis. Mounted machines follow their side of the membrane into a daughter. Their position and the storage inherited by each half affect survival. The retained parent keeps its memory and age; the daughter starts with fresh registers, and fresh object tracks. Mutations affect the inherited program, not the inherited body.

There is **no fixed population cap, organic-particle count cap or age-based death timer**. Cells die from physical damage, loss of essential machinery or exhausted energy. Age remains observable; the compatibility signal `lifespan` is zero, meaning no fixed maximum. Resources, mortality, browser memory and execution time constrain practical populations. The former area-4 growth ceiling is gone. A separate geometry bound keeps cell radius at or below 95 world units, retained from the original habitat geometry; maximum envelope area is about 184.18. Collision queries use actual radii. These are simulation abstractions, not biological fidelity claims.

Death returns the cell's remaining body and gut contents to the environment. This also applies to a newborn that immediately dies after inheriting insufficient storage.

## Organic matter and digestion

The [matched pond comparisons](experiments/local-surfaces-pond.json) record how the shipped food web behaves with regulated secretion and composition-based feeding.

The player labels particles by origin (exudate, tissue or waste). A particle can contain soluble carbon, structural material and refractory bound material. Its mixture sets its optical and mechanical properties and the substrates delivered to a consumer’s gut.

| Gut fraction | Enzyme | Use |
| --- | --- | --- |
| Soluble | `digest(soluble)` | Fast assimilation of accessible carbon |
| Structural | `digest(structural)` | Processes body material and produces refractory residue |
| Bound | `digest(bound)` | Slow recovery of less accessible carbon |

Each particle inlet takes portions of packets within reach of its physical mount, up to its own throughput. Engulfing a whole cell requires a fit at the opening and enough free gut space. With an active touch receptor measuring the same particle at that opening, an inlet’s `engage_when` expression can test `peer.resistance_valid` and `peer.resistance`. Gut sensors expose energy and minerals in each fraction. Respiration makes ATP from the assimilated carbon.

New secretions and merged secretion packets are placed just beyond the emitter's capture reach. This prevents immediate reingestion of digestive residue. A cell can still capture its own waste after physically approaching it; there is no ownership immunity.

## Legacy genome language

Programs beginning with `cellular counts;` or `cellular;` follow [the cellular language](design/cellular.md). The action and register syntax below remains supported for existing sources.

Programs are immutable typed expression graphs. Each 1/30-second tick computes `(current signals, old memory) → (action requests, next memory)`. `let` names an expression recomputed every tick. Arithmetic, comparisons, booleans, bounded strings, 2D vectors, conditionals and pure built-ins are supported; there are no user loops, I/O or immediate writes inside expressions.

```text
# Express a source of energy before adding a motor.
# Stop expression when built mass plus stored proteins meets the target.
let powered = organelle.photosynthetic.capacity > 0;
target.photosynthetic.count = 1;
target.photosynthetic.size = 0.08;
express.photosynthetic = if energy > 12 && organelle.photosynthetic.mass + protein.photosynthetic < 0.08 then 1 else 0;
target.flagellum.count = 1;
target.flagellum.size = 0.04;
express.flagellum = if powered && energy.fraction > 0.6 && organelle.flagellum.mass + protein.flagellum < 0.04 then 1 else 0;
recycle.flagellum = 0;

turn = 0;
thrust = if powered && energy.fraction > 0.7 then 0.1 else 0;
photosynthesize = 1;
divide = false;
```

`thrust`, `turn` and `divide` are required. Other action requests are `grow`, `photosynthesize`, `engulf`, `bite`, `defend`, `repair`, `absorb` and `feed`. Optional actions default to zero/false except `absorb` and `feed`, which default to 1. Hardware and resource checks determine what a request achieves.

Every organelle family has `target.<family>.count` (integer), `target.<family>.size` (scalar), `express.<family>` and `recycle.<family>` (0–1 effort). Omitted count is zero, size uses the family minimum, and both efforts are zero. Plain `2` is integer; `2.0` is scalar, so scalar counts require `int()`. Actual family signals are `organelle.<family>.count`, `.mass`, `.capacity`, `.damaged` and `.building`. Capacity is equivalent minimum-sized functional units; building is material still missing before incomplete units first become functional. `protein.<family>` is matching stored construction material; `expression.rate`, `assembly.rate` and `assembly.capacity` expose achieved synthesis, achieved assembly and shared core throughput. The old `build.<family>` spelling remains a compatibility alias for expression, with the same paid protein mechanism.

The editor reference documents all **174 read signals and 81 write signals**, with authoritative types checked against Rust metadata. See [the implemented organelle design](design/organelles.md) and [engine/ABI.md](engine/ABI.md) for the detailed rules and interfaces.

### Physical sensing

External readings require their specific built sensor. `.available` means functioning hardware; `.valid` means a usable return this tick. Invalid numeric/vector readings are zero and tokens are empty. Internal body, energy and memory signals remain available through the core.

Optics exposes `object.offset`, `.area`, `.contrast`, `.pigment` and `.motion`, plus an opaque `.token`. Cells and particles use the same return format. Measurements depend on built hardware, illumination and occlusion; darkness can leave a functioning detector without a valid return. No reading exposes another cell's energy, genome, diet, attack allocation or an “edible” flag. Contact reports pressure and resistance. Attacks and capture resolve on actual contact, reach and facing.

`scan.bearing`, `scan.width` and `scan.target` control the next tick's scan. A remembered token can request reacquisition only within physically detectable objects. The per-cell tracking cache holds at most eight observations and forgets a track after a gap of more than 30 ticks (one simulated second). Eviction can forget it earlier. Tokens never grant remote access or alias a replacement object.

Foragers approach optical returns, follow the edge of large nearby objects to encounter released particles, and steer around resistant contact. When a Forager recognizes a compatible Phototroph it can build adhesive and exchange surfaces. Attached partners move fixed carbon and minerals toward the cell with the lower storage fraction for each resource. A healthy Forager rests its motors over a substantial meal when it has the enzyme to digest its soluble or structural carbon. It continues searching if it carries only material it cannot yet process. Capture still respects total gut occupancy. These heuristics use measured geometry and resistance; sensors do not classify objects as food or reveal their species.

The Forager gives injury escape priority over feeding rest and optical attraction. Its own integrity can trigger that response even when an attacker is outside its contact receptor's coverage. Escape uses the existing paid motors and lasts until repair restores integrity.

Predators remember a recent prey sighting or firm contact for 18 seconds. They pursue visual returns and use two chemical receptors to search when no target is visible. Matching contact markers make them turn away. Tissue and exudate in the gut contribute to feeding rest, and paired thermoreceptors can interrupt a meal to steer toward cooler water.

Chemoreception reads local dissolved minerals and an organic-cue field emitted by release, leakage and decay. Cues diffuse, move with currents and decay; they do not point to a chosen meal. Light sensing is separate from photosynthesis, and temperature and currents affect cells even without their sensors.

### Registers and mutation

Each primitive has 999 sparse addresses: `ri001…ri999` integers, `rf001…rf999` scalars, `rb001…rb999` booleans, `rs001…rs999` strings and `rv001…rv999` vectors. Empty reads return the type's zero value. Strings fit in 128 UTF-8 bytes.

```text
ri001 = ri001 + 1 when !object.valid;
clear ri001 when object.valid;
rs001 = object.token when object.valid;
clear rs001 when !object.valid && ri001 > 30;
scan.target = if populated(rs001) then rs001 else "";
```

All reads and guards see old memory; writes commit together and clear wins. Each populated register costs 0.0025 energy per second, including stored zero, false or an empty string. `populated()` tests occupancy; clearing releases its maintenance cost.

Regulatory mutations patch one typed node in the parent's current source. Physical mutations can change a product half-life, a recipe quantity, a compatible target or the placement of a surface machine. Unrelated comments, formatting and names remain intact. Constants, compatible operators/expressions and same-type references can change; the candidate is recompiled to enforce scope and type correctness. Mutation preserves write destinations and existing rules, so omitted outputs cannot appear through mutation. A valid mutant can still starve or fail to reproduce.

Genomes allow 8,192 expression nodes, 256 KiB of source and 64 nesting levels. A simulation's genome registry has a 2,048-genome / 64 MiB budget; when a new variant does not fit, the daughter inherits the valid parent program. These storage limits do not cap the number of organisms.

## Editor and graph

The CodeMirror editor provides highlighting, context-sensitive autocomplete, undo/redo, type hints and structured compiler errors. **Read signals** and **Write signals** browse types, defaults, requirements and examples. Ctrl/Cmd + Space on an empty statement offers writable outputs; inside an expression it offers readable values of appropriate types. **Save genome** explicitly commits a valid program; typing saves only a recovery draft.

In an organism's read-only Program view, hover, place the caret or select an expression for its typed value. This is a current-snapshot preview using the Rust evaluator, including old memory and register-rule guards. It never advances time, changes RNG state or commits writes. Outputs are requested commands rather than a guarantee of the next physical action. Pause to inspect a stable snapshot; generic catalog genomes have no live values.

| Shortcut | Action |
| --- | --- |
| Ctrl/Cmd + S | Save genome/copy |
| Ctrl/Cmd + Enter | Check types |
| Ctrl/Cmd + Space | Autocomplete |
| Ctrl/Cmd + Z / Shift + Z | Undo / redo |
| F8 / Shift + F8 | Next / previous diagnostic |

**Graph** connects signals, built-ins, outputs and registers. Cyan edges read old values, amber edges write and dashed rose edges clear. Constants and aliases fold into details. Search, focus and bounded graph neighborhoods keep large programs navigable; omitted defaults remain available under Options. Register cycles represent delayed state. This is a dependency graph, separate from the physical Body inspector.

## Persistence and version compatibility

IndexedDB stores documents and complete worlds; localStorage stores preferences and editor drafts. Changing runs checkpoint every two seconds and after key actions, and reopen paused. The save indicator reports failures. The active simulation advances when its main view is visible or its floating player is open, including while the main app shows a catalog. An open player uses a timer when both windows are hidden; browser throttling can slow it. There is no offline catch-up. Web Locks protect a run from competing writers in multiple tabs. Clearing browser site data removes its local saves; export genomes and environments for portable copies.

**Current formats: engine ABI 24, checkpoint 21, genome language 4 with per-contact recipe expressions.** Genome JSON exports use `agar.genome` version 2 with `languageVersion: 4`; environment JSON remains version 1. Earlier saved contact declarations are translated into explicit guards on the matching surfaces when opened. Checkpoint-3 simulations retain their actual organs, resources, memory and source. Their previously unclassified organic particles and gut contents import as tissue; their new protein pools start empty. Checkpoints 4 through 16 also remain readable in continuous mode. Their two-part adhesive hardware keeps its material and stored energy but cannot bind without finished adhesin. Historical attachments and recruitment progress on those surfaces are cleared. If an older save used a compartment with a missing trace ingredient, it reopens with the corrected capacity. Excess stored ATP stays in the cell as chemical energy bound to its existing tissue. New saves retain the actual precursor declarations, installed ingredients, partners and recruitment progress. Previously supplied particles and their accounted resources carry over. Automatic food supply ends at import. Previously built parts keep their recorded investment and existing food keeps its original carbon and energy. Older runs start with zero player withdrawals. Older states receive explicit initial gases and a one-time ledger for existing carbon; their future follows the new chemistry. New saves retain cellular products, ingredient ledgers, replication, mechanism identities and construction controls. Existing recipes default to allowing new starts and requesting no recycling. Corrected optical coverage and directional primitive interactions intentionally change affected trajectories. The old `build.*` source spelling remains an alias for expression.

The organelle model is an intentional compatibility break. Checkpoints 1/2 are rejected before replacing the engine state. Older saved genomes remain viewable, renameable and exportable, but must be manually updated in a new copy before introduction. Their investment fractions and omniscient sensors are not silently converted into free organs. Saved records and old files remain preserved. New and old browser documents keep the same storage namespace.

Checkpoints retain bodies, local organ IDs, construction state, reserves, guts, sparse memory, observations/tracks, scan controls, fields, particles, counters, random state and genome lineage. They are validated and bounded to 128 MiB. Replay is deterministic within the same engine runtime; cross-platform bit-identical floating-point results are not promised. The UI currently does not import/export simulation checkpoints.

## Build and GitHub Pages

To rebuild Rust, install stable Rust through rustup and the WASM target:

```sh
rustup target add wasm32-unknown-unknown
npm run build:wasm
npm run check:engine
npm run build
```

The dependency-free engine is copied to `engine/pkg/agar_engine.wasm`. Vite bundles it into a content-hashed asset. Production builds check the ABI/export contract before bundling. `npm run build:all` rebuilds Rust and the frontend; `npm run preview` serves `dist/` locally. Simulation, compilation and persistence need no runtime server, API keys or external service. Editor dependency licenses are in `public/THIRD_PARTY_NOTICES.txt` and each built site.

For GitHub Pages, the existing [.github/workflows/pages.yml](.github/workflows/pages.yml) rebuilds Rust and the frontend, then publishes `docs/` on `main`. Put the extracted `agar/` contents at your repository root, including `.github/`, and choose **GitHub Actions** in Settings → Pages. The workflow can also be started manually; deployment is restricted to `main`.

For direct branch publishing:

```sh
npm ci
npm run build:pages
```

Publish the regenerated `/docs` directory and disable the Actions deployment workflow to avoid competing methods. If Rust changed, run `npm run build:wasm` first. Alternatively, extract the contents of **agar-pages.zip** to a publishing branch and choose `/(root)`; that archive is already built and needs neither Node nor Rust. Keep `.nojekyll`.

The relative Vite base (`./`) works at both a user Pages root and a repository subpath. Publish complete matching builds: `index.html` and every `assets/` file must come from the same build. The source package's root HTML is Vite input; its `docs/` is publishable. Serve HTTP(S), not `file://`, so WASM can load. An old-engine message requires replacing the deployed assets together; clearing browser saves is unnecessary.

## Source and checks

| Path | Responsibility |
| --- | --- |
| `src/app.ts`, `src/catalog-ui.ts` | Workspace, player, inspectors and catalog editing |
| `src/presets.ts`, `src/catalog.ts`, `src/organelles.ts` | Programs, habitat recipes and display metadata |
| `src/program-editor.ts`, `src/genome-language.ts` | Editor, native diagnostics, assistance and live values |
| `src/genome-network.ts`, `src/genome-visualizer.ts` | Dependency projection and graph UI |
| `src/storage.ts`, `src/session.ts` | Validated persistence and ordered checkpoint saves |
| `src/worker-engine.ts`, `src/simulation-worker-runtime.ts`, `src/simulation-protocol.ts` | Dedicated simulation worker, ordered commands and transferable presentation buffers |
| `src/renderer.ts`, `src/engine.ts` | Canvas display and the typed WASM boundary |
| `engine/src/body.rs`, `engine/src/cellular.rs` | Organs, material-bearing products, recipes, physical work, footprints and conservative partitioning |
| `engine/src/material_allocation.rs` | Sparse material costs, deterministic resource sharing and reusable allocation buffers |
| `engine/src/genome.rs`, `engine/src/genome/` | Typed language, registers, source-preserving mutation and inspection |
| `engine/src/environment.rs`, `engine/src/simulation.rs` | Fields, physical observations, feeding, growth and lifecycle |
| `engine/src/checkpoint.rs`, checkpoint submodules | Complete-state validation and serialization |
| `engine/ABI.md`, `design/cellular.md`, `design/organelles.md` | Engine contract, cellular language and body/sensing rules |
| `experiments/` | Recorded ecosystem runs and their limits |

```sh
npm run check
npm run check:engine
npm run test:engine       # requires Rust
npm run test:wasm
npm run test:language
npm run test:inspection
npm run test:graph
npm run test:storage
npm run test:session
npm run test:player-view
npm run test:environment-controls
npm run test:worker
npm run test:cellular
npm run test:production-chains
npm run test:colony-formation
npm run test:multicellular
npm run test:cellular-ui
npm run test:food-chain
npm run test:scenarios    # launch the default basin and check its ecology
npm run test:predator     # one founder in stock Still water, three seeds
npm run test:ecosystem    # three-lineage control, 15 minutes with births after minute ten
npm run test:ecosystem-turnover  # same control, 75 minutes with births after minute 60
```

Tests cover physical accounting and packing, sensor gates, conservative inheritance, safe mutations, checkpoint replay, real WASM interfaces, editor/Rust metadata agreement, graph bounds and persistence failures. Worker tests use real worker threads and WASM to check ordered commands, transferred buffers, atomic checkpoint capture, partial failures, disposal and main-thread responsiveness. Session tests cover related inspector reads on one tick and final-save ordering. Player tests cover cross-document adoption, catalog navigation, close/return, failure cleanup, explicit pause, hidden-window timer scheduling and stale-callback cancellation. Gas tests cover substrate limits, carbon/oxygen budgets, barriers, signed exchange, conservative resize and checkpoint migration/replay. Food-chain tests check authored consumer development, resistant-contact navigation, prey capture and digestion, and recovery from a low-energy tissue meal. The mixed-ecosystem regression requires surviving and reproducing Forager and Predator lineages, with no supplied organic food. Recorded runs and limits are in [experiments](experiments/README.md). A passing finite run does not guarantee permanent coexistence. Legacy program fixtures remain available through `test:food-chain:legacy` and `test:navigation:legacy`.

Start a new preset to run its revised genomes. Saved simulations resume with the genome sources in their checkpoints.

Run `npm run test:editor-navigation` to exercise the genome editor in Chromium. Run `node scripts/check-ui.mjs` for the player and live inspection checks. Both use an isolated browser context. Build with `npm run build:pages` and serve `docs` on `http://127.0.0.1:4173/`. Set `UI_TEST_URL`, `PLAYWRIGHT_MODULE` or `CHROMIUM_PATH` to use a different preview or browser installation.

## Simulation performance

See [PERFORMANCE.md](PERFORMANCE.md) for the measured optimization results,
correctness checks, and implemented architecture. The engine retains tick buffers
and spatial columns, shares identical field-flow samples, and evaluates a derived
genome plan with folded constants and unreachable bindings removed. Authored
programs remain intact for mutation, inspection and checkpoints.

Each simulation and environment preview runs in a dedicated worker. A completion-driven scheduler
requests at most one advancing batch of eight fixed 1/30-second ticks and adapts
batch size to recent cost. Drawing remains independent of stepping; visible
frames are requested at most about 30 times per second, while hidden or
non-simulation views receive only exact tick/summary updates. Field samples are
requested only for visible overlays; source/barrier metadata stays current
without copying the grid. Selected-cell details are fetched on demand, and
visible inspectors share one trace per unchanged session state. Pausing and closing drain outstanding work
and save checkpoint bytes with their matching summary before releasing the run.

The comparison harness
runs actual release WASM engines on the same fixed-seed worlds and checks complete
checkpoint and render-buffer equality at sampled boundaries.

```sh
npm run bench:simulation
npm run bench:simulation -- --baseline /path/to/compatible-ABI22/engine/pkg/agar_engine.wasm --output experiments/comparison.json
```

The benchmark uses V8's optimizing WASM tier from the start to avoid timing tier
transitions. It reports simulation tick and Rust snapshot-export times separately;
browser rendering and JavaScript snapshot-object construction are outside those
timings. Build both compared engines with the same Rust toolchain.
