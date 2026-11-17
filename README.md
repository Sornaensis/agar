# agar

A playable, client-only artificial life foundation: a TypeScript interface and canvas renderer backed by an actual Rust WebAssembly simulation. Organic particles and independently programmed single cells move through light, temperature, current, and mineral fields. Autotrophs turn light into energy and release organics; heterotrophs digest particles or prey; mixotrophs invest in both pathways. Cells spend energy and minerals to grow, and allocate their bodies among competing capabilities.

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
| **Genomes** | Browse eight built-in programs and saved genomes. Create or edit a new copy, check types, try a valid mutation, inspect its graph, and save it. Saved genomes can be renamed, exported, imported, or deleted. |

An open simulation has **Player**, **Metrics**, **Settings**, and **Genomes** views. The player fills the available content area, with **Play/Pause**, a **1× / 2× / 5× / 10×** speed selector, and **Add organism** at the top right. The searchable genome picker adds exactly one cell. Click an organism to inspect its state, live memory, and read-only program or dependency graph. You can save its program to the genome catalog without changing that organism.

Metrics show population history, feeding strategies, photosynthesis/digestion/release rates, births, deaths, predation, and the mineral budget. Settings contain mutation rate, external organic input, ambient conditions, and display overlays. The simulation's Genomes view lists programs represented by living organisms. Environment sources are authored in the environment editor, keeping placement tools out of the player.

Library cards open directly, with secondary actions in each card's menu. Saved items and built-ins are grouped separately. Environment editors put the habitat preview beside its settings: select a source to adjust it, or choose a source tool and click the preview to place one. Genome editors keep graph inspection and type checking beside the code, with mutation choices in a disclosure. Arrow keys move between tabs; **Space** toggles playback when the player is focused.

Built-ins stay unchanged. Editing a recipe or program creates a new document, and imports receive new identities. Existing simulations own their compiled programs and environment state, so catalog renames or deletions do not alter their organisms or habitat. Genome import accepts the exported JSON format and plain `.cell` programs; environment import accepts the exported JSON format.

## Program editor

The bundled CodeMirror editor provides syntax highlighting, line numbers, bracket matching, autocomplete, undo/redo, and type/signature help at the caret and on hover. **Read signals** and **Write signals** menus above the code provide types, descriptions, and examples; editable drafts can insert the selected symbol. A mutation is one undoable edit.

Press **Ctrl/Cmd + Space** on an empty statement line for writable outputs and register rules. Inside an expression it offers readable signals, earlier local bindings, typed registers, and functions. Suggestions follow the expected type when the context determines one, and never offer output-only names as readable values. Comments and string contents suppress completion.

Type checking runs after an editing pause and on **Check**. It uses the same Rust compiler as the simulation, without registering a genome or changing a running world. Compiler errors appear as marked source ranges and a clickable diagnostic with line and column. The compiler reports its first error; fixing it exposes any subsequent error. Completed local declarations retain their inferred types when a later statement is incomplete.

**Save genome** / **Save copy** explicitly commits a valid program to the catalog. Typing and checking only update a recovery draft; closing and reopening **New genome** can recover it. Organism programs remain read-only, with the same highlighting and type help.

### Inspect an organism's expressions

Click an organism, open **Program**, and hover, place the caret, or select an expression to see its typed value. Expand the program for more space. Values come from the Rust evaluator for that specific organism and update while its simulation runs; pause playback to examine a stable snapshot. A genome opened from the catalog or active-genome list has no organism state and does not invent values.

The displayed time identifies a **current-state preview**. Reads use the current cell's signals and memory; all pure expressions, including both branches of a conditional, can be inspected. Outputs show requested commands after their normal clamps, while register rules show whether they would apply, including clear-over-write precedence. Inspection does not advance time, change random state, or commit writes. It is not a recording of the last tick or a promise of the next one: the environment and other cells may change before the next evaluation. If the cell dies, its expanded source remains available and live values are cleared.

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

Checkpoint v2 includes gut contents, attack tissue, growth rates, and division birth mass. Earlier v1 worlds are migrated without erasing programs, mutations, registers, or their environment settings. They continue under the new biological rules. **Start a fresh Mixed ecosystem to use the new producer-led recipe**; existing runs keep their original populations and external food supply.

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

Programs compile into immutable, typed expression graphs. A `let` names a signal expression; it does not capture a sensor value once at compilation. Each 1/30-second tick supplies a fresh, shared pre-step world snapshot. The three required outputs are `thrust`, `turn`, and `divide`. Optional action outputs are `grow`, `photosynthesize`, `engulf`, `bite`, `defend`, and `repair`, all defaulting to zero/false. `feed` and `absorb` default to 1. The original five `invest.*` body targets default to 0.2; the new `invest.attack` defaults to 0. Legacy source remains valid, but attacking now requires an explicit attack allocation. Set all six targets when designing a specialist.

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

## Food chain and environments

The model separates three things: **usable energy**, **structural biomass**, and **minerals**. There is no separate carbon resource in this version. Food particles represent organic material: photosynthetic exudates, remains, or an optional external feed.

| Strategy | Energy source | Required investment | Main limitation |
| --- | --- | --- | --- |
| Autotroph | Light | Photosynthesis | Shared light, minerals for growth, and defense costs |
| Heterotroph | Organic particles or prey | Digestion | Finding food, gut capacity, digestive throughput, and assimilation losses |
| Mixotroph | Both | Photosynthesis and digestion | Both capabilities share the same finite body budget |
| Predator | Prey, and optionally particles | Attack plus digestion | Injury/contact requirements, handling cost, digestion time, and defensive prey |

Predation is a heterotrophic strategy, not an extra source of energy. No genome name grants a privileged role. The live Metrics classification uses actual tissue fractions: at least 5% photosynthesis is photosynthetic capacity, and at least 5% digestion is digestive capacity. Cells with both are mixotrophs. Attack-capable counts overlap those groups. These are capability labels; the energy-rate readings show what the cells actually achieve.

**Sunlit pond** and **Thermal currents** begin without externally supplied particles and have zero continuing organic input. Producers release organics and cells eventually become remains. **Still water** deliberately retains external feeding for heterotroph experiments. A new simulation starts with 360 supplied particles only when its recipe has a positive external input rate. Existing saved runs retain their own settings.

The environment editor controls ambient conditions, external organic input, starting minerals, and up to 16 light, heat, vortex, and nutrient sources. Light is sampled from bounded sources. Heat is a persistent coarse field that spreads and moves with the current. Currents are prescribed flows and vortices; cells experience drag relative to the water and particles drift with it. This is a lightweight flow field, without a full fluid solver.

## Feeding and predation

`feed` is a numeric 0–1 effort controlling both organic intake and digestion. Set it to zero for a strict autotroph. A full gut already limits intake: do not turn feeding off solely because the gut is full, since that also pauses digestion.

Particles and captured prey enter a finite gut. Capacity is `100 × size × (body.digestion / 0.2)` raw energy. Processing is bounded by `6 × size × (body.digestion / 0.2) × feed` raw energy/s. Only 65% becomes usable reserve energy; the remainder dissipates. Capturing a cell therefore cannot instantly refill reserves. Gut minerals are transferred proportionally as material is processed; excess free minerals return to the water.

`food.*` selects a sensed particle by `energy / (distance + 20)`, balancing meal size against travel distance. It does not always select the nearest trace. `food.energy` and `food.nutrients` expose the selected particle's raw energy and mineral content; both are zero when no food is visible. The built-in consumers slow down at a meal and while digesting, and predators can scavenge a worthwhile nearby meal before hunting.

The built-in Predator uses `rb001` to remember a hunger cycle. It enters replenishment below 40% reserve and leaves at 75%, or when its gut is sufficiently full. Growth and division wait while it replenishes; feeding stays enabled so digestion can continue. It also swims away from hot water and suspends hunting and growth during that escape. These behaviors are ordinary editable genome code.

`photosynthesize` requests light harvesting, limited by tissue, local light, and competition. Producers draw from overlapping, smoothly weighted capture areas with radius `60 × sqrt(size)`, so collection area scales with biomass. Each patch shares its finite light supply between those demands; the whole habitat supplies at most 2,400 gross energy/s under uniform light level 1. Local light scales the supply. This makes crowding reduce the energy available per cell without imposing a population limit.

At least 12% of gross photosynthesis is allocated to organic release; production beyond reserve capacity can also be released. It becomes particles when space is available; energy that cannot fit the particle budget dissipates. Releases appear outside the producer membrane so consumers can reach them. Nearby releases merge outside the producer body, avoiding a new particle for every cell on every tick. Exudates contain energy but no invented minerals. Organic material decays, returning its minerals to water while energy dissipates.

| Output | Behavior |
| --- | --- |
| `bite` | Spend energy to wound the sensed prey in contact. Requires attack tissue. With feeding active and gut room, a successful bite also transfers some of the prey's existing reserve energy and free minerals into the attacker's gut. |
| `engulf` | Request capture of touching prey with **integrity ≤ 35%** and no more than **1 / 1.6** of the attacker's biomass. Attack and digestion tissue, gut room, energy, and a handling cooldown are also required. |
| `defend` | Spend energy on active defense; armor also provides passive protection. |
| `repair` | Spend energy to restore integrity. Cost scales with body mass. |

Prey sensing prefers a non-kin target, with a same-genome fallback, including larger cells. Programs can inspect prey size and capabilities to decide whether to attack; the engulf size restriction still applies. Threat sensing detects nearby attack-capable cells, including smaller hunters that can bite a larger organism. It reports capability, not intent. New `prey.integrity`, `prey.attack`, and `prey.photosynthesis` signals allow selective hunting and avoidance.

Bites can feed a hunter before it finishes a kill. The raw meal is proportional to defended damage and prey biomass, bounded by the victim's reserves and the attacker's free gut capacity. It still needs digestion on a later tick; structural biomass is not removed by this transfer. Simultaneous attackers share the available reserve proportionally.

Targets retain stable IDs throughout a tick. A body can be consumed only once; simultaneous bite damage cannot resurrect a killed cell. Gut contents, reserves, and embodied growth energy are counted once during predation and death. All minerals remain in a cell, a particle, or the water, even when the particle limit is full. See [engine/ABI.md](engine/ABI.md) for exact coefficients and resolution rules.

## Mineral cycle

Each unit of biomass binds **one mineral**. A cell can store another `2 × size` free minerals. A founder starts at size 1 with 0.5 free minerals. Growth needs both energy and minerals; light alone cannot create biomass.

- `absorb` takes dissolved minerals independently of digestive tissue. Its maximum rate is `0.25 × sqrt(size) × absorb` minerals/s, limited by local supply, energy, and reserve space. It costs one energy per mineral absorbed. Cells sharing a scarce patch share its supply proportionally.
- Supplied particles carry 0.25 minerals and 18–24 raw energy each. Intake and digestion transfer energy and minerals in the same proportion. Exudates carry no minerals; remains carry the minerals of the dead organism, including its gut.
- Particle decay returns minerals to water at 2%/s. Diffusion and currents transport dissolved minerals conservatively across the wrapping world.
- Division splits body minerals, free reserves, and gut minerals. Remodeling only rearranges existing tissue.

Nutrient springs, introduced cells, and externally supplied particles are explicit inputs. A full-strength spring adds 1.5 minerals/s across its radius. **Metrics** shows minerals in water, organic particles, and organisms including guts, plus their total. Internal recycling never adds to the cumulative input ledger.

Use `nutrient.stored`, `nutrient.capacity`, and `nutrient.fraction` for free reserves. `nutrient.level` samples the dissolved field, and `nutrient.angle` points toward increasing concentration. `gut.energy`, `gut.capacity`, and `gut.fraction` describe undigested contents. `digestion.rate` reports usable energy gained/s; `exudation.rate` reports organic energy actually released/s. Sensors observe pre-step state.

## Invested cell capabilities

Six numeric outputs request fractions of the cell's body. Each clamps to 0–1. A sum above one normalizes proportionally; a smaller sum leaves unspecialized core. Defaults are 0.2 for the original five capabilities and zero for attack. Specify all six explicitly for a specialist.

| Output | What the actual tissue provides |
| --- | --- |
| `invest.propulsion` | Acceleration, turning, and associated movement cost |
| `invest.photosynthesis` | Light harvesting capacity and active upkeep |
| `invest.digestion` | Gut capacity and organic processing throughput |
| `invest.armor` | Passive protection, active defense, and thermal resistance |
| `invest.sensing` | Food, neighbor, prey, and threat reach: `40 + 400 × fraction` |
| `invest.attack` | Bite strength and prey handling; higher tissue upkeep |

A newly introduced founder adopts its normalized investment targets on the first tick. Daughters inherit the parent's actual tissues. Subsequent changes remodel at most `0.08 × size` biomass/s and cost **12 energy per biomass reassigned**. They affect actions on the next tick and give no refund when a capability is removed. All signal reads still use the shared pre-step state.

Each non-attack allocation costs `0.30 × fraction × size` energy/s in tissue upkeep; attack costs `0.80 × fraction × size`. Basal use is `(0.35 × size + tissue upkeep) × (0.5 + temperature.value)`. Attack therefore has a continuing cost even when a predator is not hunting.

Actual fractions are `body.propulsion`, `body.photosynthesis`, `body.digestion`, `body.armor`, `body.sensing`, and `body.attack`; `body.unassigned` reports core. The inspector shows these allocations alongside gut contents, growth, and lifespan. Green internal bodies indicate photosynthetic tissue, an amber vesicle reflects gut fullness, and a small red chevron marks attack tissue; membrane color still identifies the genome.

```text
# A strict autotroph. Provide light and dissolved minerals.
invest.propulsion = 0.10;
invest.photosynthesis = 0.55;
invest.digestion = 0;
invest.armor = 0.25;
invest.sensing = 0.10;
invest.attack = 0;

feed = 0;
absorb = 1;
photosynthesize = 1;
turn = clamp(light.angle * 1.8, -1, 1);
thrust = if light.level < 0.5 then 0.25 else 0;
defend = if threat.visible then 1 else 0;
repair = if integrity < 0.95 then 0.5 else 0;
grow = if maturity < 1 && energy.fraction > 0.5 then 1 else 0;
divide = maturity >= 1 && energy.fraction > 0.65;
```

Eight built-in programs include strict **Phototroph**, **Forager**, **Predator**, and **Mixotroph** strategies. Typed mutations can change allocations and feeding rules without making an invalid program. Their ecological success remains dependent on the environment and competition.

## Growth and lifecycle

`size` measures biomass and 2D area relative to a founder. Introduced cells start at **1**. Bounds remain **0.5–4**, and radius is `7 × sqrt(size)`. Doubling biomass doubles area, not radius.

`grow` requests 0–1 effort. Full growth adds at most **`0.05 × sqrt(size)` biomass/s**, costing **80 energy and one stored mineral per biomass**. Growth preserves 20% of the enlarged energy capacity, and stops when minerals, reserve energy, or size limit prevent it. Negative requests clamp to zero; they never shrink a cell or refund energy. `growth.rate` reports the biomass actually built/s.

| Quantity | Rule |
| --- | --- |
| Energy capacity | `200 × size` |
| Free mineral capacity | `2 × size`, in addition to body/gut minerals |
| Register upkeep | `0.0025 × populated registers` energy/s |
| Movement cost | `1.45 × thrust × size × (body.propulsion / 0.2)` energy/s |
| Division requirements | At least **twice birth biomass**, `energy ≥ 90 × size`, and a 12-second cooldown |
| Division cost | `12 × size` energy; remaining energy, biomass, free minerals, and gut contents split equally |
| Lifespan | 300 simulated seconds; age is chronological and the retained parent's age continues after division |

`birth.size` records the starting mass for the current growth cycle. `maturity` runs from zero at that mass to one after doubling. A size-1 founder must grow to size 2 before producing two size-1 cells. Both halves then set their birth size to the split mass and must grow again. Daughters start at age zero with empty registers; the retained parent keeps its memory and age. Programs without growth remain valid but cannot divide immediately from founder mass.

Cells die from depleted energy, destroyed integrity, or reaching `lifespan`. Remains include structural material, stored resources, and undigested material. A finite lifetime makes recycling part of a sustained ecosystem rather than requiring every producer to be killed by a predator.

Larger cells have greater contact reach, capacity, collision mass, and upkeep. Growth is slower relative to mass as cells get larger. Size alone does not improve sensing or commanded acceleration; these depend on body allocation.

**There is no fixed population cap.** Starting populations, adding organisms, and division can all pass the former 384-cell limit. Local light, food, minerals, upkeep, and mortality determine which populations can sustain themselves. Collisions use a local spatial search instead of checking every possible cell pair. Browser memory and execution time still constrain practical simulation size; allocation failures are reported, and complete saves retain their existing 128 MiB file-size limit.

## Mutations

The default mutation chance is **10% per daughter at division**. Set it in simulation **Settings** to 0% for exact inheritance or 100% to attempt a mutation at every division. A mutation changes the daughter's immutable genome; the parent's program is preserved. Each variant records its parent genome and mutation category. Organism inspection exposes its actual source read-only; **Save to Genomes** keeps a separate catalog copy.

The genome editor provides **Mutate** with four choices: Any, Constants, Expressions, and References. This changes the draft without introducing cells. Save the genome, then choose it in a new simulation or the player's **Add organism** menu.

Mutations operate on the typed expression graph:

| Category | Allowed changes |
| --- | --- |
| Constants | Perturb bounded integer/scalar/vector values, flip booleans, or edit bounded strings while preserving the type. |
| Expressions | Change a compatible operator/function, or add/remove one typed unary operation at a selected expression. |
| References | Substitute a same-type sensor, register read, or earlier in-scope immutable binding. |

Each successful mutation patches **one node in the parent's existing source**. Comments, local names, statement order, and text outside that node remain intact; programs are not flattened into generated temporary bindings. Descendants inherit the changed source, so successive mutations accumulate through the lineage. A draft mutation has the same behavior and can be undone in one step.

Construction preserves operand types, function arity, required output types, lexical scope, and acyclic dependencies. Arithmetic retains the interpreter's finite-value rules. The patched source is recompiled with the authoritative compiler and must fit the source, token, node, and nesting budgets. A failed or unchanged candidate leaves the parent intact. Older saved variants retain whatever source they already had; later mutations preserve that source rather than reconstructing lost comments.

These guarantees concern program validity. A valid mutation can still produce an organism that moves poorly, cannot reproduce, or starves. Numeric compatibility distinguishes integer/scalar types, but does not distinguish physical units.

A simulation stores at most **2,048 genomes**, subject to its **64 MiB source/program budget**, including introduced programs and evolved variants. When no applicable change fits the program limits, or a new variant cannot be stored, the daughter keeps the valid parent genome. Existing variants can still be reused at capacity. Checkpoints preserve the registry, lineage, and mutation counters across reloads. Catalog editing is independent of a simulation's registry; draft mutations do not count as mutated births.

Mutation also preserves register types, guard types, legal addresses, and synchronous transition semantics. Exporting and recompiling a mutant must preserve its future memory updates as well as its immediate outputs. Strings and vectors obey the same finite storage and value limits after mutation.

Mutations edit existing expressions and register reads; they preserve write destinations and rules. Optional actions must be present in the authored genome to evolve. Mutation does not insert an omitted `grow` or any other default output.

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
npm run test:inspection
npm run test:storage
npm run test:session
npm run test:food-chain
```

Native tests cover the language, synchronous registers, valid mutations, editor analysis, energy/mineral accounting, body capabilities, physical interactions, and complete checkpoints. Checkpoint tests exercise all 4,995 occupied registers, exact future mutation behavior, and atomic rejection of corrupt or inconsistent state.

WASM integration checks exercise the actual compiled engine, including the 8,192-node limit and byte-identical continuation after restoring into an independent instance. Graph tests cover bounded navigation and typed dependencies. Language checks compare the editor catalog with Rust names and compiled result types, and verify typed register suggestions, comment/string isolation, and UTF-16 cursor ranges. Storage/session checks cover portable copies, immutable built-ins, rejected imports, transaction failures, aligned checkpoint metadata, asynchronous save ordering, and exclusive ownership of an open run.

Inspection checks exercise native expression values through the real TypeScript/WASM boundary, Unicode source ranges, implicit output defaults, current memory, clear precedence, restored worlds, malformed responses, and unchanged checkpoint bytes before/after inspection.

The food-chain check compares several fixed-seed mixed ecosystems with mutation disabled, reports trophic populations over time, and checks producer-to-consumer energy transfer. It is a regression check, not a guarantee of perpetual coexistence. The recorded before/after runs and their limitations are in [experiments/README.md](experiments/README.md).

The same seed and action sequence are reproducible within the same engine runtime; cross-platform bit-identical floating-point replay is not promised.

## Planned organelle model

[design/organelles.md](design/organelles.md) proposes paid cell expansion and individually grown organelles with internal-space and membrane constraints, separate sensor capabilities, and generic observations instead of semantic food/prey sensors. It defines resource accounting, construction, damage, division, language controls, and an implementation sequence. This is a design plan; the current simulator still uses body allocations.
