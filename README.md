# agar

A playable, client-only artificial-life simulator: a TypeScript interface and canvas renderer backed by a Rust WebAssembly engine. Single cells carry reactive programs, sparse typed memory and individually built organelles. Cells compete for light, dissolved minerals and organic material in a wrapping 2D environment with heat and currents.

## Run

Use Node.js 22.12 or newer. The source includes a compiled WASM module, so Rust is not needed for frontend work.

```sh
npm ci
npm run dev
```

Open the local URL printed by Vite. In **Simulation**, start a preset or choose an environment, genomes and population counts. Every introduced cell begins with the same minimal structure and finite reserves; its genome grows its capabilities. **Sunlit pond** uses producer programs; **Still water** supplies organic particles for consumer experiments. **Mixed ecosystem** combines producer, scavenger, mixed and hunter programs. Their survival depends on resources and behavior.

## Workspace

| View | Use |
| --- | --- |
| Simulation | Open saved runs or start a new one. Saved runs reopen paused. |
| Environments | Create, inspect, edit a new copy, rename, import, export or delete a habitat recipe. |
| Genomes | Create or copy programs, check types, mutate a draft, inspect its graph and explicitly save. |

An open simulation has **Player**, **Metrics**, **Settings** and **Genomes** tabs. The player has Play/Pause, 1×/2×/5×/10× speed and **Add organism**. Choose a genome to add one cell. Click a cell for **Overview**, **Body**, **Memory** and **Program**. Its program and memory are read-only; saving its genome creates a separate catalog document.

The Body inspector compares actual organ counts and mass with the last construction requests. Expand a family to inspect stable local unit IDs, built mass, integrity and functional state. Internal space, membrane attachment use, family-specific protein stocks and achieved expression/assembly/recycling/repair rates remain visible. Metrics includes resource totals, introduced material and energy, harvested light, populations and feeding capabilities.

Built-ins stay unchanged. Editing or importing creates independent catalog items. Running simulations own their programs and environment state, so catalog renames and deletions do not alter existing worlds.

## Bodies and capabilities

`body.area` is paid envelope area; `body.mass` is actual material in the scaffold, organs, assembly proteins, reserve minerals and gut minerals. Usable energy, raw gut energy and embodied structural energy are separate. Energy does not add material mass. Compartment contents occupy their reserved capacity without being counted twice as internal space.

Growing the envelope creates room and increases upkeep and drag. It does not grant a motor, sensor or energy source. Organs use internal space; membrane-facing modules also use finite attachment capacity that scales with perimeter. Incomplete and damaged organs retain their material and occupied space.

| Family | Capability |
| --- | --- |
| `core` | Essential machinery, bounded assembly and basic metabolism/repair |
| `photosynthetic` | Shared-light collection and organic release |
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

The genome is the body plan. An introduced cell has area 1, one core of mass 0.04, 68 usable energy and 0.6 free minerals in the core's bounded compartments. Its frame, core, reserves and fabrication costs are charged to the introduction ledgers. There are no preassembled specialist bodies or genome-name mappings. Photosynthetic units, digestion, motors, sensors, storage and additional cores all have to be grown through ordinary paid ticks.

Programs must establish an energy source before spending their reserves on further development. The built-ins use actual organelle state to sequence construction, then build duplicates for division. This ordering is part of their visible source and can be edited or mutated. Changing a target changes future construction; it never replaces the living cell's existing organs.

Each `express.<family>` produces that family’s assembly proteins from minerals and energy. Proteins persist between ticks, occupy internal space, add upkeep and turn over into returned minerals plus heat. Count/size targets assemble organs from matching proteins already present at the start of the tick; expressing a gene cannot create a functional organ immediately. Expression and assembly share finite core throughput, energy and space. Leaving every gene on can fill the cell with unused protein and delay the machinery it needs. Regulate expression using `protein.<family>`, actual organ mass and resource reserves. Allocation uses a common starting snapshot; new space, recovered resources and newly functional machinery are used on following ticks. Lowering a target does not remove anything without explicit recycling. Recycling costs processing energy and returns only accounted material and a fraction of retained structural energy. Capacity loss spills excess contents into the environment instead of silently discarding them.

Division requires doubled birth area, sufficient energy, cooldown and a feasible packing of actual organs with a viable core in each half. Discrete organs are assigned, never cloned. Build duplicate essential capabilities before division if both daughters should inherit them. The retained parent keeps its memory and age; the daughter starts with empty registers and no object tracks. Mutations affect the inherited program, not the inherited body.

There is **no fixed population cap, organic-particle count cap or age-based death timer**. Cells die from physical damage, loss of essential machinery or exhausted energy. Age remains observable; the compatibility signal `lifespan` is zero, meaning no fixed maximum. Resources, mortality, browser memory and execution time constrain practical populations. The former area-4 growth ceiling is gone. A separate geometry bound keeps cell radius at or below 95 world units, one eighth of the shorter world dimension; maximum envelope area is about 184.18. Collision queries use actual radii. These are simulation abstractions, not biological fidelity claims.

## Organic matter and digestion

Organic matter retains its composition through ingestion, gut storage, spills, death and division:

| Material | Origin | Processing |
| --- | --- | --- |
| Exudate | Released photosynthetic surplus and supplied organic meals | Fast, readily assimilated carbon source |
| Tissue | Dead cells, extracted cell contents and engulfed structural material | Slower, nutrient-rich processing that leaves digestive residue |
| Waste | Unassimilated tissue residue | Slow, low-yield recovery; the remainder dissipates or remineralizes |

`digest.exudate`, `digest.tissue` and `digest.waste` allocate the same finite digestive throughput. Their default efforts are 1, 1 and 0.35. They cannot grant digestion without a functioning digestive organ. Read `gut.<kind>.energy` and `.nutrients` to choose which enzymes to prioritize; `feed` still regulates ingestion. Gut composition of an engulfed organism stays intact instead of turning its waste back into fresh tissue. Decay returns minerals to the environment and dissipates energy. Same-kind local merging combines accounted contents; particle counts never trigger deletion or artificial conversion.

Particle appearance differs, but genome sensors continue to report physical properties and local chemical cues rather than an omniscient food/prey category.

## Genome language

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

The editor reference documents all **169 read signals and 81 write signals**, with authoritative types checked against Rust metadata. See [the implemented organelle design](design/organelles.md) and [engine/ABI.md](engine/ABI.md) for the detailed rules and interfaces.

### Physical sensing

External readings require their specific built sensor. `.available` means functioning hardware; `.valid` means a usable return this tick. Invalid numeric/vector readings are zero and tokens are empty. Internal body, energy and memory signals remain available through the core.

Optics exposes `object.offset`, `.area`, `.contrast`, `.pigment` and `.motion`, plus an opaque `.token`. Cells and particles use the same return format. Measurements depend on built hardware, illumination and occlusion; darkness can leave a functioning detector without a valid return. No reading exposes another cell's energy, genome, diet, attack allocation or an “edible” flag. Contact reports pressure and resistance. Attacks and capture resolve on actual contact, reach and facing.

`scan.bearing`, `scan.width` and `scan.target` control the next tick's scan. A remembered token can request reacquisition only within physically detectable objects. The per-cell tracking cache holds at most eight observations and forgets a track after a gap of more than 30 ticks (one simulated second). Eviction can forget it earlier. Tokens never grant remote access or alias a replacement object.

Built-in scavengers use small apparent area as a tentative particle clue and soft contact as a feeding opportunity. They steer around resistant surfaces instead of remaining stopped against them. These heuristics use measured geometry and resistance, not an edible or species signal.

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

Mutations patch one typed node in the parent's current source. Unrelated comments, formatting and names remain intact. Constants, compatible operators/expressions and same-type references can change; the candidate is recompiled to enforce scope and type correctness. Mutation preserves write destinations and existing rules, so omitted outputs cannot appear through mutation. A valid mutant can still starve or fail to reproduce.

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

IndexedDB stores documents and complete worlds; localStorage stores preferences and editor drafts. Changing runs checkpoint every two seconds and after key actions, and reopen paused. The save indicator reports failures. Only the open, visible simulation advances; there is no offline catch-up. Web Locks protect a run from competing writers in multiple tabs. Clearing browser site data removes its local saves; export genomes and environments for portable copies.

**Current formats: engine ABI 10, checkpoint 4, genome language 3.** Genome JSON exports use `agar.genome` version 2 with `languageVersion: 3`; environment JSON remains version 1. Plain `.cell` source is checked with the current compiler. Checkpoint-3 simulations retain their actual organs, resources, memory and source. Their previously unclassified organic particles and gut contents import as tissue; their new protein pools start empty. The next save uses checkpoint 4. Continued runs use the updated physical rules, so historical trajectories change. The old `build.*` source spelling is accepted as expression; programs can be edited to regulate their protein stocks explicitly.

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
| `src/renderer.ts`, `src/engine.ts` | Canvas display and the typed WASM boundary |
| `engine/src/body.rs` | Organs, footprints, construction, recovery and conservative partitioning |
| `engine/src/genome.rs`, `engine/src/genome/` | Typed language, registers, source-preserving mutation and inspection |
| `engine/src/environment.rs`, `engine/src/simulation.rs` | Fields, physical observations, feeding, growth and lifecycle |
| `engine/src/checkpoint.rs`, checkpoint submodules | Complete-state validation and serialization |
| `engine/ABI.md`, `design/organelles.md` | Detailed engine contract and body/sensing design |
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
npm run test:food-chain
node scripts/check-preset-navigation.mjs
```

Tests cover physical accounting and packing, sensor gates, conservative inheritance, safe mutations, checkpoint replay, real WASM interfaces, editor/Rust metadata agreement, graph bounds and persistence failures. The short preset-navigation fixture tests escape from a real resistant contact followed by particle capture and digestion. Ecosystem checks report survival and extinctions across fixed seeds; they do not guarantee permanent coexistence.
