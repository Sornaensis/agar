import { createDialog, escapeHtml } from './ui';
import { ORGANELLE_CATALOG, MECHANISM_CATALOG } from './organelles';

export function showGuide(): void {
  createDialog('Guide', `
    <p>Start from a simulation preset, or combine an environment and genomes. Only the open simulation advances; hidden browser tabs pause. Runs save automatically and reopen paused.</p>
    <p>Click a cell to inspect its state, physical body, memory or program. <b>Add organism</b> introduces one cell with the chosen genome.</p>
    <details class="guide-section" open><summary>Program a cell with products and recipes</summary>
      <p>New genomes begin with <code>cellular;</code>. Declare physical products, express them with named genes, then combine their stock into machines. Every founder has one core and finite reserves. Its program must build an energy route before those reserves run out.</p>
      <pre class="guide-code">cellular;
product Anchor = anchor();
product Chlorophyll = pigment();
product Catalyst = enzyme();

let need_solar = built.Solar &lt; 0.04;
gene anchors: Anchor = if need_solar &amp;&amp; pool.Anchor &lt; 0.002 then 0.2 else 0;
gene pigment_supply: Chlorophyll = if need_solar &amp;&amp; pool.Chlorophyll &lt; 0.004 then 1 else 0;
gene catalysts: Catalyst = if need_solar &amp;&amp; pool.Catalyst &lt; 0.002 then 0.2 else 0;

assembly Solar: harvest(light) {
  Anchor: 0.005;
  Chlorophyll: 0.03;
  Catalyst: 0.005;
  activity: 1;
}</pre>
      <table><thead><tr><th>Construct</th><th>Meaning</th></tr></thead><tbody>
        <tr><th><code>product</code></th><td>A named material with one role, such as motor, pigment, enzyme or membrane.</td></tr>
        <tr><th><code>gene</code></th><td>Requested expression effort from 0 to 1. Actual synthesis is limited by shared translation work, minerals, energy and space.</td></tr>
        <tr><th><code>assembly</code></th><td>A recipe for one operation with positive component masses. Missing ingredients leave a physical, incomplete machine.</td></tr>
        <tr><th><code>pool.Name</code></th><td>Available product mass. Recipes share this stock, and daughters inherit a partition of it.</td></tr>
        <tr><th><code>built.Name</code></th><td>Functional mass in complete matching assemblies, reduced by damage. Incomplete machines contribute zero.</td></tr>
        <tr><th><code>activity</code></th><td>Effort applied by this completed machine; it cannot supply missing components.</td></tr>
      </tbody></table>
      <p>Synthesis and installation are separate steps. A recipe uses stock already present at the start of the tick. Turning a gene off leaves existing products and machines intact. Composition, unfinished parts, requested versus achieved product expression, activity and placement appear in the <b>Body</b> inspector.</p>
      <p><code>propel()</code> needs anchor, motor and filament material. A light harvester needs anchor, pigment and enzyme material. Each required role supplies at least 10% of its recipe’s mass; the total must meet the mechanism’s minimum size. Recipes can share a product, so competing machines can exhaust the same stock.</p>
      <p>Build independent operations: <code>store(food)</code> holds food, <code>import(particle)</code> captures it, and <code>digest(tissue)</code> processes only tissue. Storage cannot digest; a tissue enzyme cannot process waste. Likewise, <code>support()</code> provides structural support, <code>translate()</code> provides fabrication work, and energy and mineral storage are separate recipes. Activity regulates operations; physical storage and support remain present.</p>
      <table><thead><tr><th>Mechanism</th><th>Effect</th></tr></thead><tbody>${MECHANISM_CATALOG.map(item=>`<tr><th><code>${item.kind}</code></th><td>${escapeHtml(item.description)}</td></tr>`).join('')}</tbody></table>
      <p>Set <code>at</code> and <code>direction</code> to body-relative angles in radians. Two motors on opposite sides can have separate activity expressions; their actual forces and torque move the cell. The Body view marks motor attachments and force directions on the selected cell.</p>
      <p>Membrane machinery grows the envelope. Replicators perform paid genome replication; a division ring advances constriction after sufficient growth and replication. These processes need time and material, and division partitions the body that was built.</p>
    </details>
    <details class="guide-section"><summary>Use regulators as physical memory</summary>
      <pre class="guide-code">product Persistence = regulator(8);
gene remember: Persistence = if chemical.valid
  &amp;&amp; chemical.organic &gt; 0.001 &amp;&amp; pool.Persistence &lt; 0.008
  then 0.15 else 0;
let keep_searching = pool.Persistence &gt; 0.002;</pre>
      <p>This regulator has an eight-second half-life. Its stock rises when a cue triggers expression and decays afterward. Growing the cell dilutes its concentration; division partitions the actual product. The stock, concentration, half-life and expression rate are visible in the Body inspector.</p>
      <p>Cellular programs read resource and body state, product stocks and local sensor measurements. External measurements still need working receptors and valid readings. Their lasting state comes from physical products and machinery. The <b>Rememberer</b> preset demonstrates a regulator-driven change in motor activity.</p>
    </details>
    <details class="guide-section"><summary>Legacy programs: grow a body</summary>
      <p>Every introduced cell starts with an envelope, one core and finite energy and mineral reserves. Its genome expresses the proteins needed to develop an energy source, storage, movement and sensors. Daughters inherit the organs and unassembled proteins their parent actually made.</p>
      <p><code>body.area</code> is the supported envelope, which sets collision geometry and interior room. <code>body.mass</code> is actual mineral material in the frame, organs, reserves and gut. Energy is tracked separately. A larger empty body has upkeep and drag; it does not automatically gain capabilities.</p>
      <pre class="guide-code">target.flagellum.count = 2;
target.flagellum.size = 0.04;
let missing = max(0, 0.08 - organelle.flagellum.mass);
express.flagellum = if energy &gt; 30
  &amp;&amp; protein.flagellum &lt; min(0.012, missing) then 1 else 0;
recycle.flagellum = 0;
grow = if body.maturity &lt; 1 &amp;&amp; energy.fraction &gt; 0.65 then 0.5 else 0;</pre>
      <p><code>express.&lt;family&gt;</code> allocates core work to that family’s assembly proteins. Synthesis spends minerals and energy now; only proteins already present at the start of a tick can assemble toward the family’s count and size targets. A flagellum’s proteins cannot become a photoreceptor. Targets continue using existing stock after expression stops.</p>
      <p>Expression and assembly share finite core throughput with growth, recycling and repair. Unassembled proteins consume internal space and upkeep, and turn over with a 45-second half-life. Their mineral material is recovered, but synthesis energy is lost. Expressing everything can crowd out useful construction and drain reserves. Staged programs establish an energy route, grow room and then duplicate selected organs, using <code>protein.&lt;family&gt;</code> and actual organ mass to stop overproduction.</p>
      <p>Each family has an integer count target, a scalar unit-size target, and expression/recycle effort from 0 to 1. Omitted targets are zero count and minimum unit size; omitted efforts are zero. Lowering a target leaves existing organs intact until explicitly recycled. The older <code>build.&lt;family&gt;</code> spelling is accepted as an alias for expression, with the same protein-synthesis behavior.</p>
      <p>The <b>Body</b> inspector compares actual units and protein stocks with targets, shows shared core capacity and achieved expression/assembly rates, and each unit’s mass and integrity. Inspect <code>organelle.&lt;family&gt;.count</code>, <code>.mass</code>, <code>.capacity</code>, <code>.damaged</code>, <code>.building</code> and <code>protein.&lt;family&gt;</code> from the program. Incomplete and damaged organs still occupy room; only functional organs provide capabilities.</p>
      <table><thead><tr><th>Family</th><th>Capability</th></tr></thead><tbody>${ORGANELLE_CATALOG.map(item => `<tr><th><code>${item.kind}</code></th><td>${escapeHtml(item.description)}</td></tr>`).join('')}</tbody></table>
    </details>
    <details class="guide-section"><summary>Physical sensing and legacy scan controls</summary>
      <p>Light, heat, chemicals, flow, contact and optical observations require their own working sensor organs. <code>.available</code> reports working hardware; <code>.valid</code> reports a usable reading this tick. Invalid numbers and vectors are zero and tokens are empty. Always guard a reading: zero without a valid sensor does not mean cold water or zero distance.</p>
      <pre class="guide-code">let seen = object.available &amp;&amp; object.valid;
turn = if seen then clamp(angle(object.offset), -1, 1) else 0;
thrust = if seen &amp;&amp; gut.fraction &lt; 0.5 then 0.3 else 0;
rs001 = object.token when seen;
scan.target = if populated(rs001) then rs001 else "";
scan.bearing = 0;
scan.width = 6.283185;</pre>
      <p>Optical returns describe estimated offset, apparent area, contrast, visible pigment and motion. Cells and particles use the same format. They never expose energy, genome, diet or an “edible” flag. Sensor size and count constrain quality and coverage; light and occlusion also affect visibility.</p>
      <p>Tokens identify observed objects. A remembered token can ask for reacquisition within a physical scan; it cannot read an unseen location or attack remotely. Each cell remembers at most eight observed objects; a track expires after a gap of more than 30 ticks (one second), or earlier if evicted. Scan direction, width and token take effect on the following tick. Contact reports pressure and resistance. Contact feeding can still happen without sensors.</p>
      <p>Chemoreceptors sample local dissolved minerals and organic cues, plus their gradients. Organic cues spread from release, leakage and decay. Following a cue may lead to usable material, competitors, danger, or nothing. The program must learn from actual encounters and its own gut or energy changes.</p>
    </details>
    <details class="guide-section"><summary>Legacy programs: feed, maintain and divide</summary>
      <p><code>photosynthesize</code> uses built photosynthetic and metabolic machinery to harvest shared local light. <code>absorb</code> uses transporters to take dissolved minerals. <code>feed</code> controls overall intake and digestion. Captured exudate, tissue and waste remain separate in the gut and compete for digestive and metabolic processing.</p>
      <table><thead><tr><th>Material</th><th>Digestion</th></tr></thead><tbody><tr><th>Exudate</th><td>Readily processed organic releases. Converts 80% of raw energy to usable reserves.</td></tr><tr><th>Cell tissue</th><td>Slower processing converts 65% to usable energy and leaves 20% as refractory waste. Tissue also carries structural minerals.</td></tr><tr><th>Waste</th><td>Slow residual material, with a 32% energy yield. Unassimilated minerals return to water and remaining energy dissipates.</td></tr></tbody></table>
      <pre class="guide-code">digest.exudate = 1;
digest.tissue = 0.7;
digest.waste = if gut.exudate.energy + gut.tissue.energy &lt; 1
  then 1 else 0.1;</pre>
      <p>The <code>digest.*</code> efforts share finite processing work. All default to 1 except waste, which defaults to 0.35. Inspect <code>gut.&lt;kind&gt;.energy</code> and <code>.nutrients</code> to prioritize what is already inside the cell. External sensors still report physical appearance, contact and chemical cues; they do not reveal hidden food labels.</p>
      <p><code>thrust</code> and <code>turn</code> need flagella. <code>bite</code> needs a piercing apparatus and physical reach. <code>engulf</code> depends on a phagocytic cup, manageable geometry, energy and digestive space. <code>defend</code> and <code>repair</code> spend resources; cortex and repair machinery affect their capacity. Program requests alone cannot supply missing organs.</p>
      <p><code>divide = body.division_ready;</code> requests division after sufficient growth, energy and cooldown, with viable cores and a valid packing arrangement for both halves. Whole discrete organs are assigned to daughters, never cloned. Build at least two units of an essential capability if both daughters should inherit it. A cell may otherwise divide into a daughter lacking its motor or energy source.</p>
      <p>The retained parent keeps its age and registers; the daughter starts with empty memory and no object tracks. Mutations change its program, not its inherited organs. Cells have no age limit; starvation and damage can still kill them, returning accounted tissue and gut contents to the environment. Organism and organic-particle counts have no fixed caps.</p>
    </details>
    <details class="guide-section"><summary>Program editor and live values</summary>
      <p>Cellular programs have <b>Inputs &amp; stocks</b> and <b>Products &amp; recipes</b> reference menus. Ctrl/Cmd + Space suggests declarations at the top level, typed mechanisms and their targets after an assembly name, product names inside recipes, and readable stocks, sensors, functions and local bindings inside expressions. A summary counts products, genes and recipes. Legacy programs keep their existing Read signals and Write signals menus.</p>
      <table><tbody><tr><th>Ctrl/Cmd + S</th><td>Explicitly save a valid genome or copy.</td></tr><tr><th>Ctrl/Cmd + Enter</th><td>Check types.</td></tr><tr><th>Ctrl/Cmd + Space</th><td>Open autocomplete.</td></tr><tr><th>Ctrl/Cmd + Z / Shift + Z</th><td>Undo / redo, including mutations.</td></tr><tr><th>F8 / Shift + F8</th><td>Next / previous diagnostic.</td></tr></tbody></table>
      <p>Type checking runs after a short editing pause; click an error to jump to its source. Typing saves a recovery draft, while <b>Save genome</b> commits the valid document. Organism programs are read-only. Click, hover or select a complete expression there to see its typed value from the current Rust snapshot. Previewing never applies writes or spends energy. Action values are requests; physical checks still determine the result.</p>
      <p>The genome is a pure reactive program. <code>let</code> binds an immutable expression recomputed each tick. Use arithmetic, comparisons, boolean operators and <code>if … then … else …</code>. Plain whole literals such as <code>2</code> are integers; <code>2.0</code> is scalar. Use <code>int()</code> when converting a scalar count. Statements end in semicolons; <code>#</code> starts a comment.</p>
      <p>Genomes support 8,192 expression nodes, 256 KiB of source and 64 nesting levels. The compiler and runtime bound values and strings.</p>
    </details>
    <details class="guide-section"><summary>Legacy programs: memory registers</summary>
      <p>Each primitive type has 999 addresses: integers <code>ri001…ri999</code>, scalars <code>rf001…rf999</code>, booleans <code>rb001…rb999</code>, strings <code>rs001…rs999</code> and vectors <code>rv001…rv999</code>.</p>
      <pre class="guide-code">ri001 = ri001 + 1 when !object.valid;
clear ri001 when object.valid;
rs001 = object.token when object.valid;
clear rs001 when !object.valid &amp;&amp; ri001 &gt; 60;</pre>
      <p>All reads use memory from the start of the tick. Writes commit together afterward; clear wins if both fire. Empty values read as zero, false, empty string or zero vector. <code>populated()</code> distinguishes empty from stored zero. Every populated register costs 0.0025 energy/s, including stored false and empty strings. Clear unused memory to release that cost.</p>
    </details>
    <details class="guide-section"><summary>Mutation and dependency graph</summary>
      <p>Each mutation edits one typed node against the inherited source, preserving unrelated code, comments and layout. Constants, equivalent references and typed expression changes remain valid programs. Validity does not guarantee survival.</p>
      <p>Cellular programs also offer <b>Silence a gene</b> and <b>Duplicate a gene</b>. These change expression in the authored source. Inherited physical products and assemblies retain their original composition.</p>
      <p><b>Graph</b> shows builtins, signals, outputs and registers. Cyan edges read old values, amber edges write, and dashed rose edges clear. Guards are labeled <code>when</code>. Select a node to inspect its dependencies; the graph describes code, not a cell’s physical organs.</p>
    </details>
    <details class="guide-section"><summary>Environment and resource accounting</summary>
      <p>Edit a copy of an environment to change light, temperature, currents, dissolved minerals and sources. New runs use that recipe; saved simulations retain their own world. Currents carry cells and particles through a wrapping landscape with inertia and drag.</p>
      <p>Sunlit pond and Thermal currents have no external organic input. Still water supplies organic particles for heterotroph experiments. Producers release exudate; bites and dead cells supply tissue, and tissue digestion releases waste. On the canvas these appear as gold droplets, coral fragments and muted waste grains. Crowded photosynthetic cells compete for finite local light.</p>
      <p>Minerals circulate through water, bodies and particles. Construction transfers them into structures; recycling and death return them. Usable, raw gut and embodied structural energy remain separate, and maintenance, movement and conversion dissipate energy. Metrics includes introduced resources, total held energy and harvested light.</p>
    </details>
    <details class="guide-section"><summary>Saved files and older versions</summary>
      <p>IndexedDB holds genomes, environments and full simulation checkpoints; local storage keeps preferences and unfinished drafts. Export genomes and environments from their menus. Existing v3 organelle programs remain executable. Cellular mode is explicit, so older sources retain their behavior. New checkpoint 6 saves preserve product stocks, physical recipes, incomplete components and replication progress; checkpoints 3, 4 and 5 remain readable. Earlier investment-language documents remain viewable and exportable.</p>
    </details>
  `, 'guide-dialog');
}
