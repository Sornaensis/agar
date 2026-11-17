import { createDialog, escapeHtml } from './ui';
import { ORGANELLE_CATALOG } from './organelles';

export function showGuide(): void {
  createDialog('Guide', `
    <p>Start from a simulation preset, or combine an environment and genomes. Only the open simulation advances; hidden browser tabs pause. Runs save automatically and reopen paused.</p>
    <p>Click a cell to inspect its state, physical body, memory or program. <b>Add organism</b> introduces one cell with the chosen genome.</p>
    <details class="guide-section"><summary>Grow a body</summary>
      <p>Every new cell starts with an envelope, one core and finite energy and mineral reserves. Its genome must build its energy source, storage, movement and sensors through ordinary paid construction. Daughters inherit the organs their parent actually grew.</p>
      <p><code>body.area</code> is the supported envelope, which sets collision geometry and interior room. <code>body.mass</code> is actual mineral material in the frame, organs, reserves and gut. Energy is tracked separately. A larger empty body has upkeep and drag; it does not automatically gain capabilities.</p>
      <pre class="guide-code">target.flagellum.count = 2;
target.flagellum.size = 0.04;
build.flagellum = if energy.fraction &gt; 0.6 then 1 else 0;
recycle.flagellum = 0;
grow = if body.maturity &lt; 1 &amp;&amp; energy.fraction &gt; 0.65 then 0.5 else 0;</pre>
      <p>Each family has an integer count target, a scalar size target, and build/recycle effort from 0 to 1. Omitted targets are zero count and minimum unit size; omitted efforts are zero. Lowering a target leaves existing organs intact until you explicitly recycle them.</p>
      <p>Construction shares finite assembly time, energy, minerals, internal space and membrane attachments. Incomplete and damaged organs still occupy room. New capabilities become available on the following tick. Recycled material and compartment overflow remain accounted; dismantling never refunds all construction energy.</p>
      <p>The <b>Body</b> inspector compares actual units with target counts, shows internal and membrane capacity, construction and recycling rates, and each unit’s mass and integrity. Inspect <code>organelle.&lt;family&gt;.count</code>, <code>.mass</code>, <code>.capacity</code>, <code>.damaged</code> and <code>.building</code> from the program.</p>
      <table><thead><tr><th>Family</th><th>Capability</th></tr></thead><tbody>${ORGANELLE_CATALOG.map(item => `<tr><th><code>${item.kind}</code></th><td>${escapeHtml(item.description)}</td></tr>`).join('')}</tbody></table>
    </details>
    <details class="guide-section"><summary>Sense physical surroundings</summary>
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
    <details class="guide-section"><summary>Feed, maintain and divide</summary>
      <p><code>photosynthesize</code> uses built photosynthetic and metabolic machinery to harvest shared local light. <code>absorb</code> uses transporters to take dissolved minerals. <code>feed</code> controls capture and digestion; raw gut energy becomes usable energy only after processing and assimilation losses. A mixed body pays for both routes.</p>
      <p><code>thrust</code> and <code>turn</code> need flagella. <code>bite</code> needs a piercing apparatus and physical reach. <code>engulf</code> depends on a phagocytic cup, manageable geometry, energy and digestive space. <code>defend</code> and <code>repair</code> spend resources; cortex and repair machinery affect their capacity. Program requests alone cannot supply missing organs.</p>
      <p><code>divide = body.division_ready;</code> requests division after sufficient growth, energy and cooldown, with viable cores and a valid packing arrangement for both halves. Whole discrete organs are assigned to daughters, never cloned. Build at least two units of an essential capability if both daughters should inherit it. A cell may otherwise divide into a daughter lacking its motor or energy source.</p>
      <p>The retained parent keeps its age and registers; the daughter starts with empty memory and no object tracks. Mutations change its program, not its inherited organs. Cells die from starvation, damage or age, returning accounted material to the environment. There is no fixed population cap.</p>
    </details>
    <details class="guide-section"><summary>Program editor and live values</summary>
      <p><b>Read signals</b> and <b>Write signals</b> browse types, defaults, hardware requirements and examples. Ctrl/Cmd + Space on an empty statement suggests writable signals and registers. Inside an expression it suggests appropriate readable values, functions and visible bindings.</p>
      <table><tbody><tr><th>Ctrl/Cmd + S</th><td>Explicitly save a valid genome or copy.</td></tr><tr><th>Ctrl/Cmd + Enter</th><td>Check types.</td></tr><tr><th>Ctrl/Cmd + Space</th><td>Open autocomplete.</td></tr><tr><th>Ctrl/Cmd + Z / Shift + Z</th><td>Undo / redo, including mutations.</td></tr><tr><th>F8 / Shift + F8</th><td>Next / previous diagnostic.</td></tr></tbody></table>
      <p>Type checking runs after a short editing pause; click an error to jump to its source. Typing saves a recovery draft, while <b>Save genome</b> commits the valid document. Organism programs are read-only. Click, hover or select a complete expression there to see its typed value from the current Rust snapshot. Previewing never applies writes or spends energy. Action values are requests; physical checks still determine the result.</p>
      <p>The genome is a pure reactive program. <code>let</code> binds an immutable expression recomputed each tick. Use arithmetic, comparisons, boolean operators and <code>if … then … else …</code>. Plain whole literals such as <code>2</code> are integers; <code>2.0</code> is scalar. Use <code>int()</code> when converting a scalar count. Statements end in semicolons; <code>#</code> starts a comment.</p>
      <p>Genomes support 8,192 expression nodes, 256 KiB of source and 64 nesting levels. The compiler and runtime bound values and strings.</p>
    </details>
    <details class="guide-section"><summary>Remember across ticks</summary>
      <p>Each primitive type has 999 addresses: integers <code>ri001…ri999</code>, scalars <code>rf001…rf999</code>, booleans <code>rb001…rb999</code>, strings <code>rs001…rs999</code> and vectors <code>rv001…rv999</code>.</p>
      <pre class="guide-code">ri001 = ri001 + 1 when !object.valid;
clear ri001 when object.valid;
rs001 = object.token when object.valid;
clear rs001 when !object.valid &amp;&amp; ri001 &gt; 60;</pre>
      <p>All reads use memory from the start of the tick. Writes commit together afterward; clear wins if both fire. Empty values read as zero, false, empty string or zero vector. <code>populated()</code> distinguishes empty from stored zero. Every populated register costs 0.0025 energy/s, including stored false and empty strings. Clear unused memory to release that cost.</p>
    </details>
    <details class="guide-section"><summary>Mutation and dependency graph</summary>
      <p>Each mutation edits one typed node against the inherited source, preserving unrelated code, comments and layout. Constants, equivalent references and typed expression changes remain valid programs. Validity does not guarantee survival.</p>
      <p><b>Graph</b> shows builtins, signals, outputs and registers. Cyan edges read old values, amber edges write, and dashed rose edges clear. Guards are labeled <code>when</code>. Select a node to inspect its dependencies; the graph describes code, not a cell’s physical organs.</p>
    </details>
    <details class="guide-section"><summary>Environment and resource accounting</summary>
      <p>Edit a copy of an environment to change light, temperature, currents, dissolved minerals and sources. New runs use that recipe; saved simulations retain their own world. Currents carry cells and particles through a wrapping landscape with inertia and drag.</p>
      <p>Sunlit pond and Thermal currents have no external organic input. Still water supplies particles for heterotroph experiments. Producers release some captured energy as organic material; dead cells also supply consumers. Crowded photosynthetic cells compete for finite local light.</p>
      <p>Minerals circulate through water, bodies and particles. Construction transfers them into structures; recycling and death return them. Usable, raw gut and embodied structural energy remain separate, and maintenance, movement and conversion dissipate energy. Metrics includes introduced resources, total held energy and harvested light.</p>
    </details>
    <details class="guide-section"><summary>Saved files and older versions</summary>
      <p>IndexedDB holds genomes, environments and full simulation checkpoints; local storage keeps preferences and unfinished drafts. Export genomes and environments from their menus. An older investment-language genome remains viewable and exportable; edit a new copy to update it. Checkpoint 1/2 simulations cannot be reinterpreted as paid organelle bodies and remain preserved. Existing checkpoint 3 organelle simulations still open with their actual bodies intact.</p>
    </details>
  `, 'guide-dialog');
}
