# agar

A playable, client-only artificial life foundation: a TypeScript interface and canvas renderer backed by an actual Rust WebAssembly simulation. Food and independently programmed single cells move through light, temperature, current, and nutrient fields. Cells spend energy and minerals to grow, and allocate their bodies among competing capabilities.

## Run

Use Node.js 22.12 or newer. A compiled WASM module is included, so Rust is not required to run or edit the frontend.

```sh
npm ci
npm run dev
```

Open the local URL printed by Vite. Start a preset from **Simulation**, or create a simulation with an environment, seed, and starting genomes. Daughters inherit the parent's genome, with a configurable chance of one valid mutation at division.

## Workspace

| Tab | Workflow |
| --- | --- |
| **Simulation** | Open a saved run, start one of three presets, or combine an environment with starting populations. Saved runs reopen paused. |
| **Environments** | Browse built-in and saved recipes. Create a recipe or edit an existing one as a new copy; rename, export, import, and delete saved recipes. |
| **Genomes** | Browse seven built-in programs and saved genomes. Create or edit a new copy, check types, try a valid mutation, inspect its graph, and save it. Saved genomes can be renamed, exported, imported, or deleted. |

An open simulation has **Player**, **Metrics**, **Settings**, and **Genomes** views. The player fills the available content area, with **Play/Pause**, a **1× / 2× / 5× / 10×** speed selector, and **Add organism** at the top right. The searchable genome picker adds exactly one cell. Click an organism to inspect its state, live memory, and read-only program or dependency graph. You can save its program to the genome catalog without changing that organism.

Metrics show population history, births, deaths, evolution, predation, and the nutrient budget. Settings contain mutation rate, food supply, ambient conditions, and display overlays. The simulation's Genomes view lists programs represented by living organisms. Environment sources are authored in the environment editor, keeping placement tools out of the player.

Library cards open directly, with secondary actions in each card's menu. Saved items and built-ins are grouped separately. Environment editors put the habitat preview beside its settings: select a source to adjust it, or choose a source tool and click the preview to place one. Genome editors keep graph inspection and type checking beside the code, with mutation choices in a disclosure. Arrow keys move between tabs; **Space** toggles playback when the player is focused.

Built-ins stay unchanged. Editing a recipe or program creates a new document, and imports receive new identities. Existing simulations own their compiled programs and environment state, so catalog renames or deletions do not alter their organisms or habitat. Genome import accepts the exported JSON format and plain `.cell` programs; environment import accepts the exported JSON format.

## Program editor

The bundled CodeMirror editor provides syntax highlighting, line numbers, bracket matching, autocomplete, undo/redo, and type/signature help at the caret and on hover. Suggestions include signals, builtins, outputs, typed registers, and previously declared local signals. A mutation is one undoable edit.

Type checking runs after an editing pause and on **Check**. It uses the same Rust compiler as the simulation, without registering a genome or changing a running world. Compiler errors appear as marked source ranges and a clickable diagnostic with line and column. The compiler reports its first error; fixing it exposes any subsequent error. Completed local declarations retain their inferred types when a later statement is incomplete.

**Save genome** / **Save copy** explicitly commits a valid program to the catalog. Typing and checking only update a recovery draft; closing and reopening **New genome** can recover it. Organism programs remain read-only, with the same highlighting and type help.

| Shortcut | Action |
| --- | --- |
| Ctrl/Cmd + S | Save genome or copy |
| Ctrl/Cmd + Enter | Check types |
| Ctrl/Cmd + Space | Open autocomplete |
| Ctrl/Cmd + Z | Undo |
| Ctrl/Cmd + Shift + Z | Redo |
| F8 / Shift + F8 | Next / previous diagnostic |
| Tab / Shift + Tab | Move focus out of the editor |

All editor code is bundled into the static site; no editor service or CDN is needed. Dependency licenses are included in `public/THIRD_PARTY_NOTICES.txt` and the built site.

## Saving and resuming

**IndexedDB** stores genomes, environment recipes, and complete simulation checkpoints. **localStorage** stores navigation/display preferences and unfinished editor drafts. Saves happen every two seconds while a simulation changes, and after actions such as pausing, changing settings, or adding an organism. The header reports **Saving…**, **Saved**, or a visible save failure; a failed simulation save pauses playback.

A checkpoint retains exact positions and velocities, food, evolving fields, energy and minerals, body allocations, every occupied register, random state, counters, genome lineage, and compiled program structure. Resuming continues that saved state instead of reconstructing it from the starting seed. Checkpoints are versioned, bounded to 128 MiB, and validated before replacing an engine's state.

Only the open simulation advances. Switching to a catalog or hiding the browser tab suspends advancement; there is no offline catch-up. Reopening a saved simulation pauses it until **Play**. Web Locks prevent two tabs from writing competing versions of the same run. A recent unsaved interval can be lost if the browser or device closes abruptly; closing-page saves are best effort.

Data belongs to this browser and site origin. Clearing site storage, using another browser/profile, or changing the site's origin will not carry it over. Export genomes and environments as JSON to keep portable copies. Simulation checkpoints currently remain in this browser; the UI does not import or export runs.

The rename to **agar** preserves existing browser saves. New portable files use `agar.genome` and `agar.environment`; exports from earlier versions remain accepted.

## Rebuild Rust

With stable Rust installed through rustup:

```sh
rustup target add wasm32-unknown-unknown
npm run build:wasm
```

This compiles the dependency-free engine and replaces `engine/pkg/agar_engine.wasm`. Refresh the development page after rebuilding; rebuild the frontend before publishing the changed engine.

```sh
npm run check:engine # Verify the included WASM against the frontend contract
npm run build       # TypeScript and engine checks, then static frontend build
npm run build:all   # Rebuild Rust, then build the frontend
npm run preview    # Serve the built frontend locally
```

The resulting `dist/` is a static application; simulation, genome compilation, and persistence run entirely in the browser. It needs no application server, remote database, API keys, or runtime network service.

`npm run check:engine` validates the included binary's ABI and required exports against `src/engine-contract.json`. Production builds enforce this check too, so a frontend that requires new engine functions cannot silently ship with an incompatible older binary. Checking an included binary does not require Rust; rebuilding it does.

## GitHub Pages

The source package includes [.github/workflows/pages.yml](.github/workflows/pages.yml). Put the contents of the extracted `agar/` folder at your repository root, including `.github/`. In **Settings → Pages → Build and deployment → Source**, select **GitHub Actions**. Push to `main`, or run **Deploy agar to GitHub Pages** manually from the Actions tab with `main` selected.

The workflow installs Node.js 22 and stable Rust with the WASM target, runs `npm ci`, rebuilds the Rust engine, builds the frontend, and publishes the resulting `docs/` artifact. It uses GitHub's built-in token; no personal token or custom secret is needed. Build permissions are read-only; only the deployment job receives Pages/OIDC write permissions. Manual runs from another branch build an artifact but do not deploy. If your publishing branch is named differently, change both `push.branches` and the deployment job's branch condition in the workflow.

The prebuilt `docs/` directory remains available for publishing directly from a branch. For that option, select **Deploy from a branch**, choose your branch and **`/docs`**, and disable the **Deploy agar to GitHub Pages** workflow in the Actions tab so the two methods do not compete.

To update the Pages build after editing the frontend:

```sh
npm ci
npm run build:pages
```

For branch publishing, commit the regenerated `docs/` alongside your source. The Actions workflow rebuilds it automatically, so committed build output is not required for that route. If you edit the Rust engine and build locally, run `npm run build:wasm` before `npm run build:pages`. To preview this build locally, run `npm run preview -- --outDir docs`.

Vite uses a relative base (`./`) and imports the engine as an asset. Production output contains a content-hashed file such as `assets/agar_engine-HASH.wasm`, and its JavaScript references that exact filename. Updating the engine changes its URL, so new JavaScript does not reuse an older binary cached at a fixed WASM path. One build works at both `https://USER.github.io/REPOSITORY/` and `https://USER.github.io/` without changing a repository name in the configuration. The local build includes `.nojekyll` for direct branch publishing; the Actions artifact is already treated as a built static site.

Alternatively, extract **the contents** of `agar-pages.zip` into the root of your publishing branch, keeping `.nojekyll`. In Pages settings, select that branch and **`/(root)`**. This archive already contains the built HTML, JavaScript, CSS, favicon, and WASM; Node and Rust are not needed to publish it.

The root `index.html` in the **source** package is Vite input, so publish its built `docs/` directory. The standalone **Pages** archive has the built `index.html` at its root. Serve either build over HTTP(S), rather than opening it with `file://`, so the browser can fetch the WASM module.

If a published page reports an engine-version mismatch, replace the complete published build with the matching Pages archive or redeploy the rebuilt `docs/` output, then reload the page. Keep `index.html` and all its `assets/` files from the same build; copying only the JavaScript or WASM can mix versions. Rust build commands run in the source checkout or Actions workflow, not in the deployed browser. Browser save data does not need to be cleared for an asset update.

See the official [GitHub Pages workflow documentation](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages), [publishing-source instructions](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site), and [Vite relative-base documentation](https://vite.dev/guide/build.html#relative-base).

## Genome semantics

```text
# All expressions are signals, sampled on every simulation step.
let wander = sin(age * 0.7 + id) * 0.6;
turn = if food.visible then clamp(food.angle * 1.8, -1, 1) else wander;
thrust = if food.visible then 0.8 else 0.45;
grow = if size < 2 && energy > 70 * size then 1 else 0;
divide = size >= 2 && energy > 125 * size && age > 5;
```

Programs compile into immutable, typed expression graphs. A `let` names a signal expression; it does not capture a sensor value once at compilation. Each 1/30-second tick supplies a fresh, shared pre-step world snapshot. The three required outputs are `thrust`, `turn`, and `divide`. Optional action outputs are `grow`, `photosynthesize`, `engulf`, `bite`, `defend`, and `repair`, all defaulting to zero/false. `absorb` defaults to 1, and the five `invest.*` body targets each default to 0.2. Legacy programs therefore retain a balanced body and automatically absorb available minerals.

The expression language remains pure; explicit register rules define the next memory state. A tick computes `(sensors, old memory) → (action requests, next memory)`. There is no immediate assignment inside an expression, I/O, recursion, or user-controlled loop. Arithmetic, comparisons, booleans, strings, 2D vectors, conditionals, and bounded built-ins are supported. Rust applies the requested actions under the world's energy, contact, and capacity rules. Invalid code leaves the running world untouched.

The engine owns a wrapping 2D world, drag and cell separation, local sensing, feeding, starvation, and energy-conserving division with an explicit energy cost. This foundation supports independently programmed single cells.

## Genome visualizer

Use **Graph** in a genome's catalog view/editor, an organism's **Program** view, or the simulation's **Genomes** view. Draft inspection uses a separate compiler instance; it does not introduce cells or modify a running simulation. Invalid code reports a compile error and leaves the running world intact.

The graph connects input signals, builtin operations, action outputs, and registers. Constants appear as inline values; `let` aliases collapse into their underlying signals and remain available in details. Each register address has one node shared by its reads, writes, and clears. Separate calls and operators remain separate nodes, even when they have the same name.

Connections are colored by their role: **cyan reads**, **amber writes**, **rose dashed clears**, and muted computation links. Arrows run from a dependency to its consumer; guards have separate `when` connections. A direct signal-to-action or signal-to-register connection uses the destination's write color. Node details retain types, aliases, literal values, and rule information.

Register reads observe **previous-tick memory**. Write and clear connections return to the same register node but are marked as changes for **the next tick**; visible cycles therefore represent delayed state, not immediate feedback. All values and guards read the old state, writes commit together, and clear wins. This is a structural view rather than a display of live intermediate values.

Search for a signal, operation, output, or address; select a node to inspect its connections, use **Focus** or double-click to explore its neighborhood, and return with **Overview**. Drag empty space to pan; **Fit** and zoom controls adjust the view. Under **Options**, **Defaults** reveals omitted actions, and **Constant outputs** adds isolated literal-valued actions to the diagram. Authored constant outputs remain available in the list; an all-constant program shows them automatically.

Each diagram shows at most 112 nodes; focused views follow up to three connections from the selected node. Omitted-node counts, frontier markers, and a searchable, paginated list keep large programs navigable. Unused expressions are left out of the network but still count toward the compiler's 8,192-node limit.

## Typed registers

Every cell has five sparse register banks. Each bank has exactly **999 addresses**, numbered `001` through `999`:

| Prefix | Primitive type | Empty read |
| --- | --- | --- |
| `ri` | Signed 32-bit integer | `0` |
| `rf` | Finite scalar / floating-point number | `0.0` |
| `rb` | Boolean | `false` |
| `rs` | String, at most 128 UTF-8 bytes | `""` |
| `rv` | 2D vector | `vec(0, 0)` |

```text
# Count ticks without visible food, then free the counter on sighting food.
ri001 = ri001 + 1 when !food.visible;
clear ri001 when food.visible;

# Hysteresis: stay hungry until energy reaches 80% of capacity.
let hungry = energy.fraction < 0.4
             || (rb001 && energy.fraction < 0.8);
rb001 = hungry;
rs001 = if hungry then "forage" else "rest";

# A running estimate and a remembered current vector.
rf001 = if populated(rf001)
        then rf001 * 0.95 + light.level * 0.05 else light.level;
rv001 = vec(flow.x, flow.y);

thrust = if hungry then 0.8 else 0.2;
turn = if food.visible then clamp(food.angle * 1.8, -1, 1)
       else sin(age * 0.7 + id) * 0.6;
grow = if size < 2 && energy.fraction > 0.6 then 1 else 0;
divide = size >= 2 && energy.fraction > 0.8;
```

Every expression, action, write, and guard reads **the previous tick's memory**. All register changes commit together after evaluation. An unguarded assignment writes every tick; `when` makes it conditional. If no rule fires, the old value is retained. Each address may have one write and one clear rule; if both fire, **clear wins**, independent of statement order. `populated(register)` distinguishes an empty slot from a stored zero, `false`, empty string, or zero vector.

Each populated register costs **0.0025 energy per simulated second**, based on occupancy at the start of the tick. Writing a value starts upkeep next tick; clearing removes it next tick. Even a stored zero or `false` costs energy until cleared. Reading an empty slot does not populate it. Daughters and newly introduced cells start with empty memory; the parent retains its own memory. Saved simulations retain register occupancy and values. The organism's **Memory** view exposes upkeep and searchable, paginated values without allowing edits.

Integers have saturating arithmetic; mixed numeric expressions widen integers to scalars, and division returns a scalar. Use `int(value)` for an integer result and `float(value)` for explicit widening. Register writes require the bank's type (integer-to-scalar widening is allowed). State is bounded: there are no dynamically allocated register addresses or unbounded strings.

New helpers include `vec`, `x`, `y`, `length`, `dot`, `normalize`, `angle`, `sqrt`, `sign`, `atan2`, `lerp`, and deterministic smooth `noise(seed, phase)`. Strings support equality and bounded concatenation; `length(text)` counts Unicode characters. Try **Rememberer** for stateful behavior, **Phototroph** in a lit habitat, and **Predator** alongside smaller cells.

## Genome limits

The **8,192 limit counts expression nodes**, rather than characters or cells. Sources may use up to **256 KiB of UTF-8**, **65,536 tokens**, and **64 nested expression levels**. These separate limits keep parsing and execution bounded while allowing much larger genomes than the original build.

The immutable genome registry is limited to **2,048 entries and 64 MiB of estimated source/program storage**. A mutation that cannot fit keeps the valid parent genome. Compilation errors leave the world untouched.

## Environment and interactions

Open **Environments** to use Still water, Sunlit pond, or Thermal currents, or create a new recipe. Its editor controls ambient conditions, food supply, starting minerals, and up to 16 light, heat, vortex, and nutrient sources. Click the preview to place a source; its position, strength, and radius can be adjusted or the source removed. Negative heat cools water, and a vortex's sign reverses its rotation. Preview overlays show light, temperature, water flow, and dissolved nutrients.

A new simulation initializes its dynamic fields from the chosen recipe. Simulation **Settings** can change food supply and ambient conditions for that run; the saved recipe remains independent. Reopening a run restores its evolved fields rather than resetting them.

Light is sampled immediately from bounded sources. Temperature is a persistent coarse field that spreads and moves with the current. Currents are prescribed periodic flows and vortices: cells feel drag relative to the water and food drifts with it. This version does not solve Navier–Stokes or model objects displacing water.

| Optional output | Behavior |
| --- | --- |
| `photosynthesize` | Numeric 0–1 request for light-derived energy; active uptake costs energy and cells share a finite local light budget. |
| `engulf` | Boolean request to consume a touching cell when the engine's size and energy requirements allow it. |
| `bite` | Numeric 0–1 attack effort against a sensed prey target in contact. |
| `defend` | Numeric 0–1 effort that reduces incoming bite damage at an energy cost. |
| `repair` | Numeric 0–1 request to restore membrane integrity by spending energy. |

New signals include `energy.fraction`, `memory.used`, `memory.cost`, `light.level`, `light.angle`, `temperature.value`, `temperature.angle`, `flow.x`, `flow.y`, `flow.speed`, `flow.angle`, `integrity`, and prey/threat sensing. Angle signals are relative to the cell's heading. All sensors use the same pre-step world and memory state.

Predation targets keep stable IDs through a tick. A prey body can be consumed only once, bite damage is resolved together, and killed cells cannot regain energy and revive. Recovered energy and the energy equivalent of biomass are accounted for once; recoverable remains become food. All prey minerals are recovered into the eater or the environment. Digestion controls bite strength and the cooldown between successful engulfments; armor controls passive protection, active defense, and thermal resistance. See [engine/ABI.md](engine/ABI.md) for the exact ecological coefficients and resolution rules.

## Nutrient cycle

There is one generic mineral resource, separate from energy. **Every unit of biomass binds one mineral**, and a cell can additionally store up to `2 × size` free minerals. Founders start with size 1 and a reserve of 0.5. Growth consumes one stored mineral and 20 energy per biomass added; light alone cannot create biomass.

Minerals circulate through dissolved water, food, cell bodies, and reserves:

- The water starts with 0.3 minerals per 20 × 20 patch. Currents transport minerals, and diffusion spreads them across patch boundaries and both wrapping seams. These transfers conserve mass.
- `absorb` controls dissolved uptake. Full uptake with the balanced digestive allocation takes up to `0.25 × size` minerals/s and costs one energy per mineral actually absorbed. Cells competing for a scarce patch share it proportionally. Digestion investment scales this rate.
- Introduced food carries 0.25 minerals per particle as well as 18–24 energy. Eating is partial and limited by digestive throughput; energy and minerals leave a particle in the same proportion. Cells can eat for either resource, and excess minerals return to water.
- Food decays at 2%/s exponentially, returning its minerals to the water while its energy dissipates. Deaths and predation recycle all body and reserve minerals. Even a full food-particle limit cannot discard mineral mass.
- Division splits biomass and free reserves equally. Remodeling changes allocation within an existing body and neither creates nor consumes minerals.

Nutrient springs, introduced cells, and newly supplied food are explicit material inputs. A full-strength spring adds 1.5 minerals/s total, spread over its radius. Radius changes distribution, not total production. **Metrics** reports dissolved, food, and cell minerals plus total material and cumulative inputs. A new simulation starts a ledger from the configured water, founders, and food; checkpoints retain that ledger.

Use `nutrient.stored`, `nutrient.capacity`, and `nutrient.fraction` to inspect reserves. `nutrient.level` reports bilinearly sampled mineral mass per patch; `nutrient.angle` points toward increasing concentration relative to the cell's heading. Uptake withdraws from the patch containing the cell after movement. Sensors remain frozen for the tick, while newly absorbed minerals can fund growth later in that same tick.

## Invested cell capabilities

Five numeric outputs set desired fractions of the cell's body. Each target clamps to 0–1. Targets totaling more than one normalize proportionally; a smaller total leaves an unspecialized core. Defaults are 0.2 for each capability, so specify all five when designing a specialist.

| Output | What its actual allocation changes |
| --- | --- |
| `invest.propulsion` | Acceleration, turning ability, and movement energy cost |
| `invest.photosynthesis` | Light absorption capacity and its active upkeep; local light is still shared |
| `invest.digestion` | Food-processing throughput, dissolved uptake, bite strength, and engulf cooldown |
| `invest.armor` | Passive bite protection, active defense, and resistance to thermal stress |
| `invest.sensing` | Food, neighbor, prey, and threat sensing range: `40 + 400 × fraction` |

Targets are requests for gradual remodeling. Cells can reassign at most `0.08 × size` biomass/s, paying **12 energy per biomass reassigned**. Changes take effect for actions on the next tick. Existing mass is moved between specialties and core, so switching a capability off gives no energy refund. Specialized mass also costs upkeep: resting metabolism is `(0.35 + 0.30 × assigned fraction) × size × (0.5 + temperature.value)` energy/s. A balanced body retains the original `0.65 × size` cost at neutral temperature.

The actual fractions are available as `body.propulsion`, `body.photosynthesis`, `body.digestion`, `body.armor`, and `body.sensing`; `body.unassigned` reports core, and `sense.range` reports current reach. The inspector shows these actual allocations, reserve capacity, uptake, remodeling, and body upkeep. Body upkeep is already included in the displayed basal cost.

```text
# A mineral-seeking phototroph; provide light in the environment.
invest.propulsion = 0.12;
invest.photosynthesis = 0.50;
invest.digestion = 0.20;
invest.armor = 0.08;
invest.sensing = 0.10;

let needs_minerals = nutrient.fraction < 0.25;
turn = clamp((if needs_minerals then nutrient.angle else light.angle) * 1.8, -1, 1);
thrust = if needs_minerals then 0.5 else 0.08;
absorb = 1;
photosynthesize = if light.level > 0.08 then 1 else 0;
grow = if size < 2 && energy.fraction > 0.45
       && nutrient.stored > 0.02 then 1 else 0;
divide = size >= 2 && energy.fraction > 0.72;
```

The existing typed mutation rules also cover the new signals and explicitly authored targets. All seven example genomes now specify body allocations. Their success depends on available energy, minerals, food, and competition.

## Growth and metabolism

`size` measures biomass and 2D area relative to a newly introduced cell. Founders start at **1**, the minimum daughter size is **0.5**, and the maximum size is **4**. Radius is `7 × sqrt(size)`: doubling biomass doubles area, not radius.

`grow` is a pure numeric output clamped to 0–1. Full growth adds up to **0.25 biomass/second**, costing **20 energy and one stored mineral per biomass**. Growth is capped by available energy, mineral reserves, and maximum size. Zero stops growth; negative values do not shrink cells or refund energy. A poor growth program can spend its last energy and die. Use a reserve condition such as `energy > 70 * size && nutrient.stored > 0.02` to avoid that.

| Quantity | Rule |
| --- | --- |
| Resting energy use | `(0.35 + 0.30 × assigned body fraction) × size × (0.5 + temperature.value)` per second |
| Register upkeep | `0.0025 × populated registers` per second, in addition to resting cost |
| Movement energy use | `1.45 × thrust × size × (body.propulsion / 0.2)` per second, in addition to resting cost |
| Energy storage | `200 × size` maximum |
| Free mineral storage | `2 × size` maximum, separate from the minerals bound in biomass |
| Division requirements | `size ≥ 1`, `energy ≥ 90 × size`, an expired four-second cooldown, and a free cell slot |
| Division cost | `12 × size` energy, then energy, biomass, and free minerals split equally; body fractions are inherited |

Division conserves biomass: a size-2 cell becomes two size-1 cells. A size-1 cell can divide into two size-0.5 cells, which must grow before dividing again. Programs omitting `grow` still compile, but their offspring cannot create more biomass through division alone.

Larger cells have greater food-contact reach, larger collision bodies, and more resistance to displacement. Size alone does not improve sensing range or commanded acceleration; body allocation determines those abilities. The inspector shows current size, resting cost, energy and mineral capacity, and actual capabilities; rendering and selection follow the actual radius. Compare **Conserver** (target size 1), **Drifter** (1.5), **Forager** (2), and **Giant** (3.5). Each target is a condition in the genome, so discrete growth steps can overshoot it slightly.

Growth expressions and the `size` sensor use the same typed mutation system as the other signals. Mutations can change growth rates, target sizes, and energy thresholds while preserving program validity.

## Mutations

The default mutation chance is **10% per daughter at division**. Set it in simulation **Settings** to 0% for exact inheritance or 100% to attempt a mutation at every division. A mutation changes the daughter's immutable genome; the parent's program is preserved. Each variant records its parent genome and mutation category. Organism inspection exposes its actual source read-only; **Save to Genomes** keeps a separate catalog copy.

The genome editor provides **Mutate** with four choices: Any, Constants, Expressions, and References. This changes the draft without introducing cells. Save the genome, then choose it in a new simulation or the player's **Add organism** menu.

Mutations operate on the typed expression graph:

| Category | Allowed changes |
| --- | --- |
| Constants | Perturb bounded integer/scalar/vector values, flip booleans, or edit bounded strings while preserving the type. |
| Expressions | Replace operators with compatible signatures, simplify a subtree, or insert a wrapper whose input/output types fit. |
| References | Substitute a same-type sensor, register read, or earlier in-scope immutable binding. |

Construction preserves operand types, function arity, required output types, lexical scope, and acyclic dependencies. Arithmetic retains the interpreter's finite-value rules. The serializer emits shallow, topologically ordered bindings so sharing cannot expand into an exponential source tree. Exported variants must fit the source, token, node, and nesting budgets and must compile back to an equivalent program.

These guarantees concern program validity. A valid mutation can still produce an organism that moves poorly, cannot reproduce, or starves. Numeric compatibility distinguishes integer/scalar types, but does not distinguish physical units.

A simulation stores at most **2,048 genomes**, subject to its **64 MiB source/program budget**, including introduced programs and evolved variants. When no applicable change fits the program limits, or a new variant cannot be stored, the daughter keeps the valid parent genome. Existing variants can still be reused at capacity. Checkpoints preserve the registry, lineage, and mutation counters across reloads. Catalog editing is independent of a simulation's registry; draft mutations do not count as mutated births.

Mutation also preserves register types, guard types, legal addresses, and synchronous transition semantics. Exporting and recompiling a mutant must preserve its future memory updates as well as its immediate outputs. Strings and vectors obey the same finite storage and value limits after mutation.

Mutations edit existing expressions and register reads; they preserve write destinations and rules. Optional actions must be present in the authored genome to evolve, apart from the existing treatment of omitted `grow` as a mutable zero.

Mutation randomness is separate from the physics and food random stream. The same seed, programs, settings, and action sequence reproduce the same evolution within the same engine runtime.

See [engine/ABI.md](engine/ABI.md) for exact sensor units, language limits, physics constants, tick ordering, and the raw WASM ABI.

## Source layout

| Path | Responsibility |
| --- | --- |
| `src/main.ts`, `src/app.ts` | Application entry, tab navigation, simulation player, inspection, and fixed-step clock |
| `src/storage.ts` | IndexedDB documents/checkpoints, import/export validation, and localStorage preferences/drafts |
| `src/session.ts` | Consistent checkpoint capture, ordered saves, and per-simulation tab ownership |
| `src/catalog.ts`, `src/catalog-ui.ts` | Built-in recipes and separate genome/environment editors |
| `src/program-editor.ts`, `src/program-editor.css` | CodeMirror editor, undo/redo, completions, type help, and compiler diagnostics |
| `src/genome-language.ts` | Rust-aligned symbol catalog, typed register suggestions, and source tokenization |
| `src/ui.ts`, `src/guide.ts` | Shared controls, dialogs, and concise in-app help |
| `src/renderer.ts` | Canvas world rendering and cell selection |
| `src/engine.ts` | Shared WASM compilation, isolated engine instances, typed exports, and snapshots |
| `src/engine-contract.json` | Shared ABI and required-export contract for the loader and build checks |
| `src/genome-visualizer.ts` | Bounded, interactive views of the compiler's typed dependency graph |
| `src/genome-network.ts` | Builtin/register network projection, folded constants and aliases, and bounded graph neighborhoods |
| `src/presets.ts` | Example genome programs |
| `engine/src/genome.rs` | Bounded typed parser, expression/state evaluator, sparse register banks, and mutation |
| `engine/src/environment.rs` | Light sources, heat, currents, conservative mineral transport, and field snapshots |
| `engine/src/simulation.rs` | Physics, sensing, memory upkeep, mineral accounting, body allocation, growth, predation, and division |
| `engine/src/lib.rs` | WASM API, program storage, and memory boundary |
| `engine/src/checkpoint.rs`, `engine/src/*/checkpoint.rs` | Versioned complete-state serialization, validation, and atomic restore |
| `engine/pkg/agar_engine.wasm` | Included Rust build imported by Vite and emitted as a hashed production asset |
| `public/.nojekyll` | Copied into static builds for GitHub Pages |
| `docs/` | Generated GitHub Pages build, intended to be committed for branch publishing |
| `scripts/build-wasm.mjs` | Rust-to-WASM build and copy step |
| `scripts/check-wasm.mjs` | JavaScript integration checks against the actual WASM module |
| `scripts/check-genome-language.mjs` | Language catalog/compiler agreement, typed registers, tokenization, and cursor ranges |
| `scripts/check-storage.mjs`, `scripts/check-session.mjs` | Persistence formats, transaction behavior, save ordering, failures, and tab ownership checks |
| `.github/workflows/pages.yml` | Rebuilds Rust and the frontend, then publishes the Pages artifact on `main` |

## Check correctness

```sh
npm run check
npm run check:engine
npm run test:engine   # Requires Rust
npm run test:wasm
npm run test:graph
npm run test:language
npm run test:storage
npm run test:session
```

The 128 native tests cover the language, synchronous registers, valid mutations, editor analysis, energy/mineral accounting, body capabilities, physical interactions, and complete checkpoints. Checkpoint tests exercise all 4,995 occupied registers, exact future mutation behavior, and atomic rejection of corrupt or inconsistent state.

WASM integration checks exercise the actual compiled engine, including the 8,192-node limit and byte-identical continuation after restoring into an independent instance. Graph tests cover bounded navigation and typed dependencies. Language checks compare the editor catalog with Rust names and compiled result types, and verify typed register suggestions, comment/string isolation, and UTF-16 cursor ranges. Storage/session checks cover portable copies, immutable built-ins, rejected imports, transaction failures, aligned checkpoint metadata, asynchronous save ordering, and exclusive ownership of an open run.

The same seed and action sequence are reproducible within the same engine runtime; cross-platform bit-identical floating-point replay is not promised.
