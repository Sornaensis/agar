import { createDialog, escapeHtml } from './ui';
import { ORGANELLE_CATALOG, MECHANISM_CATALOG } from './organelles';

export function showGuide(): void {
  createDialog('Guide', `
    <p><b>Springwater basin</b> starts with nine genomes. Follow food released by a Phototroph, or select an amber-linked Forager to inspect carbon moving across its dock. Away from the dock, Rememberers follow organic plumes and collect drifting detritus. Change founder counts, environment or seed before launch. Saved runs reopen paused.</p>
    <p>In <b>Metrics → Food web</b>, select a lineage to compare its births, deaths and kills, or check how many cells have full ATP stores and how far they have grown. <b>Resource balance</b> shows where minerals, carbon, oxygen and energy went.</p>
    <p><b>Add organism</b> lets you choose a genome and click its starting position. Turn on <b>Place several</b> to add more. Press Escape to finish.</p>
    <p>Select a cell to see its reserves, colony membership and feeding links. Drag it to move it, or use <b>Move cell</b> or <b>Move linked group</b> and choose a destination. <b>Edit reserves</b> lets you add or remove energy and minerals while running or paused. Zero energy kills the cell.</p>
    <p><b>Pop out</b> keeps the same player running while you edit elsewhere or background the main tab. It keeps advancing when hidden, subject to browser throttling or system sleep; keep the main tab open. <b>Window</b> is the fallback when always-on-top is unavailable. Explicit Pause still stops playback. Without an open player window, a hidden main tab pauses.</p>
    <p>New simulations use whole counts for ATP, minerals, carbon and oxygen. Set emitter and absorber throughput in counts per second. A founder starts with 680,000 ATP counts. Choose <b>Continuous</b> when starting a simulation to compare the earlier resource model. Existing saves reopen in their original mode.</p>
    <details class="guide-section"><summary>Program a cell with products and recipes</summary>
      <p>New genomes begin with <code>cellular counts;</code>. Declare physical products, express them with named genes, then combine their stock into machines. Every founder has one core and finite reserves. Its program must build an energy route before those reserves run out.</p>
      <pre class="guide-code">cellular counts;
product Anchor = anchor();
product Chlorophyll = pigment();
product Catalyst = enzyme();

let need_solar = built.Solar + 0.000001 &lt; 0.04;
gene anchors: Anchor = if need_solar &amp;&amp; pool.Anchor &lt; 20 then 0.2 else 0;
gene pigment_supply: Chlorophyll = if need_solar &amp;&amp; pool.Chlorophyll &lt; 40 then 1 else 0;
gene catalysts: Catalyst = if need_solar &amp;&amp; pool.Catalyst &lt; 20 then 0.2 else 0;

assembly Solar: harvest(light) {
  Anchor: 50;
  Chlorophyll: 300;
  Catalyst: 50;
  start_when: need_solar;
  activity: 1;
}</pre>
      <table><thead><tr><th>Construct</th><th>Meaning</th></tr></thead><tbody>
        <tr><th><code>product</code></th><td>A named material with one role, such as motor, pigment, enzyme or membrane.</td></tr>
        <tr><th><code>gene</code></th><td>Requested expression effort from 0 to 1. Actual synthesis is limited by shared translation work, minerals, energy and space.</td></tr>
        <tr><th><code>assembly</code></th><td>A recipe for one operation with positive component masses. Missing ingredients leave a physical, incomplete machine.</td></tr>
        <tr><th><code>pool.Name</code></th><td>Available whole product counts. Recipes share this stock, and daughters inherit a partition of it.</td></tr>
        <tr><th><code>concentration.Name</code></th><td>Product mass divided by envelope area. Growth dilutes regulatory signals.</td></tr>
        <tr><th><code>built.Name</code></th><td>Functional mass in complete matching assemblies, reduced by damage. Incomplete machines contribute zero.</td></tr>
        <tr><th><code>sensed.Name.field</code></th><td>A typed reading from the named receptor at its actual attachment point.</td></tr>
        <tr><th><code>start_when</code></th><td>Allows another instance to start. Existing partial machines keep installing when this becomes false.</td></tr>
        <tr><th><code>activity</code></th><td>Effort applied by this completed machine; it cannot supply missing components.</td></tr>
        <tr><th><code>recycle</code></th><td>Requests paid dismantling by built recycling machinery. Recovered ingredients become available on later ticks.</td></tr>
        <tr><th><code>fn</code></th><td>A reusable pure expression with numeric arguments; no recursion or hidden state.</td></tr>
      </tbody></table>
      <p>Synthesis and installation are separate steps. A recipe uses stock already present at the start of the tick. Turning a gene off leaves existing products and machines intact. Composition, unfinished parts, requested versus achieved product expression, activity and placement appear in the <b>Body</b> inspector.</p>
      <p><code>propel()</code> needs anchor, motor and filament material. A light harvester needs anchor, pigment and enzyme material. Each required role supplies at least 10% of its recipe’s mass; the total must meet the mechanism’s minimum size. Recipes can share a product, so competing machines can exhaust the same stock.</p>
      <p><code>store(food)</code> holds captured material. Each <code>import(particle)</code> collects within reach of its own opening. Installed enzymes process the soluble, structural or bound fraction of a meal. Likewise, <code>support()</code> provides structural support, <code>translate()</code> provides fabrication work, and energy and mineral storage are separate recipes. Activity regulates operations; physical storage and support remain present.</p>
      <table><thead><tr><th>Mechanism</th><th>Effect</th></tr></thead><tbody>${MECHANISM_CATALOG.map(item=>`<tr><th><code>${item.kind}</code></th><td>${escapeHtml(item.description)}</td></tr>`).join('')}</tbody></table>
      <p>Set <code>at</code> to choose a membrane mount in body-relative radians. Use <code>direction</code> to aim a motor, piercer or optical receptor. Two motors on opposite sides can have separate activity expressions; their actual forces and torque move the cell. Receptors sample at their attachments, piercers act along their direction, and shields protect their local surface. The Body view marks motor attachments and force directions on the selected cell.</p>
      <p>Membrane machinery grows the envelope. Replication and constriction consume time and material. The active constrictor sets the division axis, and each daughter inherits the mounted parts on its side.</p>
    </details>
    <details class="guide-section"><summary>Use regulators as physical memory</summary>
      <pre class="guide-code">product Persistence = regulator(8);
gene remember: Persistence = if sensed.Chemistry.valid
  &amp;&amp; sensed.Chemistry.organic &gt; 0.001 &amp;&amp; concentration.Persistence &lt; 0.008
  then 0.15 else 0;
let persistence = activate(concentration.Persistence, 0.002);</pre>
      <p>This regulator has an eight-second half-life. Its stock rises when a cue triggers expression and decays afterward. Growing the cell dilutes its concentration; division partitions the actual product. The stock, concentration, half-life and expression rate are visible in the Body inspector.</p>
      <p>Regulator stocks store a material history that changes with synthesis, decay, dilution and division. The <b>Rememberer</b> demonstrates how that history can change motor activity. For an exact previous reading or a counter, cellular programs can also use typed registers.</p>
      <p><code>activate(c, k)</code> computes c / (c + k), after clamping both arguments nonnegative; the zero-denominator result is zero. <code>repress(c, k)</code> is its complement. These are pure response curves, with no implicit synthesis or memory.</p>
      <pre class="guide-code">fn slow_when(brake, threshold) = repress(brake, threshold);
let motor_activity = slow_when(concentration.Brake, 0.02);</pre>
      <p>Functions accept zero to eight numeric arguments and infer their result type. Define them before calling them. Their references to earlier <code>let</code> bindings keep lexical scope. Calls inline into the normal bounded expression graph; recursion is unavailable.</p>
    </details>
    <details class="guide-section"><summary>Named receptors and local control</summary>
      <p><code>sensed.LeftEye.level</code> belongs to the declared <code>LeftEye: sense(light)</code> recipe. Its physical attachment, damage and activity determine the observation. A second receptor can sample another side of the cell:</p>
      <pre class="guide-code">let difference = sensed.LeftEye.level - sensed.RightEye.level;
let left_drive = clamp(0.2 - difference, 0, 1);
let right_drive = clamp(0.2 + difference, 0, 1);</pre>
      <p>Light receptors expose <code>level</code>, heat receptors <code>value</code>, and chemical receptors <code>mineral</code> and <code>organic</code>. These are local field values. The program can compare receptors or its regulatory memory to respond to changes.</p>
      <p>Optical receptors expose <code>offset</code>, <code>area</code>, <code>contrast</code>, <code>pigment</code> and <code>motion</code>. Their view respects direction, illumination and occlusion. Contact receptors expose <code>offset</code>, <code>pressure</code>, <code>resistance</code> and the relative-flow fields <code>flow</code>, <code>speed</code> and <code>bearing</code>. Vectors use body coordinates, with offsets measured from the receptor.</p>
      <p>Every receptor has <code>available</code> and <code>valid</code>. Missing hardware reports unavailable. Missing observations report invalid and zero values. Relative flow requires <code>available</code> but can be measured without a valid contact. Each named recipe samples a working instance at its actual mount. Use distinct recipe names to compare readings from two patches. Compare local concentrations or remember an earlier reading to infer a chemical gradient. Touch receptors also measure wall pressure and resistance.</p>
    </details>
    <details class="guide-section"><summary>Start, operate and recycle machinery</summary>
      <p>Several recipes can use the same product stock. Set <code>start_when</code> to choose when each may begin another instance. It defaults to true. Existing partial machines continue paid installation after the condition becomes false, and completed machines use their separate <code>activity</code>.</p>
      <pre class="guide-code">start_when: concentration.Brake &lt; 0.001;
activity: repress(concentration.Brake, 0.005);
recycle: if concentration.Brake &gt; 0.001 then 0.5 else 0;</pre>
      <p>A complete, active <code>recycle()</code> assembly is required to execute recycling requests. Dismantling costs energy and shared work; recovered ingredients keep their actual identities. A positive request stops installation into the same instance. Freed room and recovered material cannot fund other work in that tick.</p>
      <p>Recycling selects existing instances with the recipe’s name, including inherited versions whose geometry changed. It never retypes those ingredients or refunds usable energy. The Body inspector separates allowed starts, requested recycling and achieved recovery.</p>
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
    <details class="guide-section"><summary>Legacy programs: physical sensing and scan controls</summary>
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
      <p>The legacy chemoreceptor outputs include local mineral and organic cues and their gradients. Organic cues spread from release, leakage and decay. Following a cue may lead to usable material, competitors, danger, or nothing. The program must learn from actual encounters and its own gut or energy changes.</p>
    </details>
    <details class="guide-section"><summary>Legacy programs: feed, maintain and divide</summary>
      <p><code>photosynthesize</code> uses built photosynthetic machinery, shared local light and internal inorganic carbon to fix organic carbon and release oxygen. <code>absorb</code> uses transporters to take dissolved minerals. <code>feed</code> controls overall intake and digestion. Captured particles distribute soluble, structural and bound material into three gut pools. Legacy programs address those pools as <code>exudate</code>, <code>tissue</code> and <code>waste</code>, respectively.</p>
      <table><thead><tr><th>Material</th><th>Digestion</th></tr></thead><tbody><tr><th>Soluble</th><td>Readily processed carbon. Accessible carbon enters reserves for respiration.</td></tr><tr><th>Structural</th><td>Slower processing supplies carbon and minerals, leaving refractory waste.</td></tr><tr><th>Bound</th><td>Slow residual material with lower carbon assimilation. Unassimilated minerals return to water; remaining carbon stays in residue.</td></tr></tbody></table>
      <pre class="guide-code">digest.exudate = 1;
digest.tissue = 0.7;
digest.waste = if gut.exudate.energy + gut.tissue.energy &lt; 1
  then 1 else 0.1;</pre>
      <p>The <code>digest.*</code> efforts share finite processing work. All default to 1 except waste, which defaults to 0.35. Inspect <code>gut.&lt;kind&gt;.energy</code> and <code>.nutrients</code> to prioritize what is already inside the cell. External sensors still report physical appearance, contact and chemical cues; they do not reveal hidden food labels.</p>
      <p><code>thrust</code> and <code>turn</code> need flagella. <code>bite</code> needs a piercing apparatus and physical reach. <code>engulf</code> depends on a phagocytic cup, manageable geometry, energy and digestive space. <code>defend</code> and <code>repair</code> spend resources; cortex and repair machinery affect their capacity. Program requests alone cannot supply missing organs.</p>
      <p><code>divide = body.division_ready;</code> requests division after sufficient growth, energy and cooldown, with viable cores and a valid packing arrangement for both halves. Whole discrete organs are assigned to daughters, never cloned. Build at least two units of an essential capability if both daughters should inherit it. A cell may otherwise divide into a daughter lacking its motor or energy source.</p>
      <p>The retained parent keeps its age and registers; the daughter starts with fresh memory and object tracks. The daughter writes registers through its own program. Mutations change its program, not its inherited organs. Cells have no age limit; starvation and damage can still kill them, returning accounted tissue and gut contents to the environment. Organism and organic-particle counts have no fixed caps.</p>
    </details>
    <details class="guide-section"><summary>Follow genome ancestry</summary>
      <p>Open <b>Genomes</b> in a simulation. Each root starts an independent lineage. A new mutation appears beneath the genome it came from. Daughters that inherit an unchanged genome belong to the same branch. Choose <b>Newest variant</b> to jump straight to the latest registered mutation.</p>
      <p>The living count applies to that exact genome. A branch stays visible after its last carrier dies, and you can still open its program and graph. Closing and reopening the simulation preserves the whole tree.</p>
    </details>
    <details class="guide-section"><summary>Program editor and live values</summary>
      <p>Use <b>Inputs &amp; memory</b> to find readings, stocks and registers. <b>Declarations &amp; writes</b> lists recipes, message declarations and register assignments. Ctrl/Cmd + Space suggests names and operations valid at the cursor. The editor shows the active argument inside a call. Open <b>Editor tools</b> to search definitions, navigate references or fold the current assembly. A spelling diagnostic can offer a correction when one visible name matches.</p>
      <table><tbody><tr><th>Ctrl/Cmd + S</th><td>Explicitly save a valid genome or copy.</td></tr><tr><th>Ctrl/Cmd + Enter</th><td>Check types.</td></tr><tr><th>Ctrl/Cmd + Space</th><td>Open autocomplete.</td></tr><tr><th>Ctrl/Cmd + Z / Shift + Z</th><td>Undo / redo, including mutations.</td></tr><tr><th>Ctrl/Cmd + Shift + O</th><td>Search declarations by name.</td></tr><tr><th>F12 / Shift + F12</th><td>Go to definition / find references.</td></tr><tr><th>F2</th><td>Rename the symbol in its scope.</td></tr><tr><th>F8 / Shift + F8</th><td>Next / previous diagnostic.</td></tr></tbody></table>
      <p>Type checking runs after a short editing pause. Click a diagnostic to select its source. Warnings flag settings such as a steering direction on a storage compartment. You can save a valid genome with warnings. Typing saves a recovery draft, while <b>Save genome</b> commits the valid document. Organism programs are read-only. Click, hover or select a complete expression there to see its typed value from the current Rust snapshot. Previewing never applies writes or spends energy. Action values are requests; physical checks still determine the result.</p>
      <p>The genome is a pure reactive program. <code>let</code> binds an immutable expression recomputed each tick. Use arithmetic, comparisons, boolean operators and <code>if … then … else …</code>. Plain whole literals such as <code>2</code> are integers; <code>2.0</code> is scalar. Use <code>int()</code> when converting a scalar count. Statements end in semicolons; <code>#</code> starts a comment.</p>
      <p>Genomes support 8,192 expression nodes, 256 KiB of source and 64 nesting levels. The compiler and runtime bound values and strings.</p>
    </details>
    <details class="guide-section"><summary>Private registers and contact messages</summary>
      <p>Each primitive type has 999 addresses: integers <code>ri001…ri999</code>, scalars <code>rf001…rf999</code>, booleans <code>rb001…rb999</code>, strings <code>rs001…rs999</code> and vectors <code>rv001…rv999</code>.</p>
      <pre class="guide-code">cellular counts;
ri001 = ri001 + 1 when energy.fraction &lt; 0.25;
clear ri001 when energy.fraction &gt;= 0.25;</pre>
      <p>Each organism has its own registers. All reads see the start of the tick. Writes commit together afterward, and clear wins when both fire. Empty registers read as zero, false, empty string or zero vector. <code>populated()</code> distinguishes an empty register from a stored zero. Each occupied register costs 25 ATP counts per second in a whole-count world. The Memory tab shows occupied registers alongside regulator stocks.</p>
      <pre class="guide-code">cellular counts;
publish rf001;
rf001 = 17;
product Frame = scaffold(); product Glue = adhesin(Frame);
product Mount = anchor(); product Skin = membrane();
product Detector = receptor(); product Hair = filament();
gene sensor: Detector = if pool.Detector &lt; 100 then 0.2 else 0;
gene hairs: Hair = if pool.Hair &lt; 100 then 0.2 else 0;
gene backbone: Frame = if pool.Frame &lt; 100 then 0.2 else 0;
gene finish: Glue = if pool.Glue &lt; 100 then 0.2 else 0;
gene mounts: Mount = if pool.Mount &lt; 400 then 0.2 else 0;
gene skins: Skin = if pool.Skin &lt; 400 then 0.2 else 0;
assembly Badge: display() { Mount: 40; Skin: 60; }
assembly Touch: sense(contact) { Mount: 20; Detector: 60; Hair: 20; }
assembly Link: adhere() {
  Mount: 80; Skin: 80; Glue: 40;
  engage_when: peer.division_interface &amp;&amp; populated(rf001)
    &amp;&amp; peer.message_valid &amp;&amp; peer.message == rf001;
}</pre>
      <p><code>publish rf001;</code> selects the marker register for a built <code>display()</code> patch. An ordinary write sets its value. A facing touch receptor reads <code>sensed.Touch.message</code> when <code>sensed.Touch.message_valid</code> is true. Clearing the register or disabling the patch withdraws the marker.</p>
      <p><code>signal rf002;</code> selects a second register for a payload. A touch receptor can read the payload on a facing display. A matched pair of signal junctions can also exchange it at their bonded interface. Zero is a valid marker or payload.</p>
      <p>Attached cells with matching <code>junction(energy)</code>, <code>junction(mineral)</code> or <code>junction(carbon)</code> machinery exchange that resource toward equal storage fractions. A feeding link uses matching, active resource ports on the facing patches of a docked pair. The nurse can open the carbon port next to a juvenile request and keep its other carbon port closed. Open <b>Body → Resource exchange</b> to see signed transfers. Try <b>Nurse rosette</b> for resource sharing or <b>Pulse swimmers</b> for relayed movement. In <b>Crown rosette</b>, a cell checks its two named ports and sends a beacon when both are occupied. Edge cells that receive it activate an extra light harvester. Open <b>Body → Incoming signals</b> to see what a signal junction reads.</p>
      <p>Each <code>import(particle)</code> inlet, <code>pierce()</code>, <code>engulf()</code> or <code>adhere()</code> patch can apply <code>engage_when</code> to each candidate in reach. The example permits a daughter bond when the two cells have facing glue patches and the local touch receptor reads a matching display at their division interface. A docking surface can instead test a partner’s marker or signal on an ordinary encounter. Both facing glue patches must admit the contact and have a free binding site. New encounter bonds take 15 consecutive ticks (half a simulation second). Separation interrupts recruitment.</p>
    </details>
    <details class="guide-section"><summary>Mutation and dependency graph</summary>
      <p>Each mutation edits one typed node against the inherited source, preserving unrelated code, comments and layout. Constants, equivalent references and typed expression changes remain valid programs. Validity does not guarantee survival.</p>
      <p>Cellular programs also offer <b>Silence a gene</b>, <b>Duplicate a gene</b>, <b>Change placement</b>, <b>Change ingredient amount</b>, <b>Change specificity</b> and <b>Change stock half-life</b>. A recipe mutation changes a material dose or shifts a mechanism to a compatible substrate. For example, a soluble-carbon digester can become a structural-carbon digester. Placement changes a future recipe’s attachment or direction. Inherited machinery retains its actual geometry and ingredients; changed definitions cannot take control of it. A same-named recycling request can recover its materials.</p>
      <p><b>Graph</b> shows builtins, signals, outputs and registers. Cyan edges read old values, amber edges write, and dashed rose edges clear. Guards are labeled <code>when</code>. Select a node to inspect its dependencies; the graph describes code, not a cell’s physical organs.</p>
    </details>
    <details class="guide-section"><summary>Floating player and gas exchange</summary>
      <p>Pop out moves the same simulation into an always-on-top picture-in-picture window. It keeps running while you use other parts of agar or another browser tab. Hover over the floating player to show its controls. Move the pointer outside for a clear view of the simulation. Tab or a touch also reveals the controls. Choose Return to tab or close the floating window to dock it. Keep the main tab open. If this browser lacks picture-in-picture, Window opens a separate window with no guaranteed pinning.</p>
      <p>Choose Oxygen or Inorganic carbon to inspect the dissolved fields. Gases &amp; atmosphere controls sealed/open water and finite reservoir exchange. The environment builder also sets starting amounts. Place a mineral, oxygen or inorganic carbon emitter or absorber. Strength controls total throughput, and radius sets its footprint. An aerator exchanges both gases with the reservoir.</p>
      <p>With light, photosynthetic machinery fixes one inorganic carbon and releases one oxygen. Fixed carbon enters the reserve. An active <code>export(carbon)</code> outlet can spend ATP to secrete it. Digestion transfers organic carbon into reserves; <code>respire(oxygen)</code> consumes carbon and oxygen to release usable energy. Producers respire in darkness too. You can read <code>oxygen</code>, <code>carbon.organic</code>, <code>carbon.inorganic</code>, <code>respiration.rate</code> and <code>carbon.fixation.rate</code>. The reaction rates use carbon units per second.</p>
      <p>Carbon and oxygen accounting in Metrics includes external input and removal, fixation, organism respiration and oxygen-limited background decay. The model uses abstract amounts; it does not yet model pH or anaerobic metabolism.</p>
    </details>
    <details class="guide-section"><summary>Environment and resource accounting</summary>
      <p>Click or tap a habitat object to edit it and drag it to move. With the canvas focused, use the arrows to nudge a selected object or Shift + arrows for larger steps. Open Environment to add or remove objects or change world size. The player and builder each remember their field overlay across edits and reloads. Light, heat, chemical emitters and absorbers, vortices, jets, current bands and radial flows each have their own strength and geometry. Opposite current bands can move water in different directions.</p>
      <p>Fixed barriers stay in place; floating barriers drift with local currents. Both block organisms, particles and field transport as full-depth obstacles in the wrapping 2D world. Flow is prescribed rather than pressure-solved. Absorbers remove only available dissolved minerals and record the removed amount in resource accounting.</p>
      <p>Scroll to zoom around the pointer; Shift-drag, middle-drag or right-drag to pan. Touch supports pan and pinch. Use Fit to see the whole world. World dimensions change the physical domain and field grid; resizing preserves existing material.</p>
      <p>Particles containing ligand carbon have a violet ring. Once captured, ligand slows ordinary digestive enzymes. Inspect a particle to read its ligand fraction or open a cell’s Physiology to see its stored ligand. A cell can build an <code>isomerize(carbon)</code> assembly and spend ATP to recover that carbon. A producer can build <code>isomerize(ligand)</code> and <code>export(ligand)</code> to make and release it.</p>
      <p>A new pond starts with zero organic particles. Producers with an active carbon exporter release exudate as gold droplets. Feeding and death release coral tissue fragments, and digestion leaves muted waste grains. The particles and their halos shrink as they lose food value. Chemical emitters can enrich an empty pond. Its first new organic food comes from cells.</p>
      <p>Minerals circulate through water, bodies and particles. Construction transfers them into structures; recycling and death return them. New body material retains 90% of its construction energy. A kill releases that investment along with remaining reserves. Hunters digest the tissue and use oxygen to convert its food value into usable energy. In Metrics, the resource balance checks mineral, carbon and oxygen inventories. Whole-count runs also track energy losses by cause and compare them with the supply.</p>
      <p>The food web table groups mutations with their founding genome. Select a lineage to chart its population and see how recently it reproduced, how it died, how much energy it digests and how far its living cells have grown toward doubling their area. Older saves begin recording these events when opened in this version.</p>
    </details>
    <details class="guide-section"><summary>Saved files and older versions</summary>
      <p>IndexedDB holds genomes, environments and full simulation checkpoints; local storage keeps preferences and unfinished drafts. Export genomes and environments from their menus. Existing v3 organelle programs remain executable. Saves retain the world, organisms, signals, reserve edits and observed lineage events. The player can reopen checkpoints from version 3 onward through the current format. Existing organic matter and resource ledgers carry forward. The retired food supply stops at import. Earlier investment-language documents remain viewable and exportable.</p>
    </details>
  `, 'guide-dialog');
}
