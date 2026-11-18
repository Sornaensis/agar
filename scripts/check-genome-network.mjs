import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

// Use the project's installed compiler, so this check also runs on Node 22
// versions that do not enable native TypeScript loading by default.
async function moduleUrl(relative, imports = {}) {
  const source = await readFile(new URL(relative, import.meta.url), 'utf8');
  let code = ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  }).outputText;
  for (const [name, url] of Object.entries(imports)) {
    code = code.replaceAll(`from '${name}'`, `from ${JSON.stringify(url)}`)
      .replaceAll(`from ${JSON.stringify(name)}`, `from ${JSON.stringify(url)}`);
  }
  return `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`;
}
const networkUrl = await moduleUrl('../src/genome-network.ts');
const { buildGenomeNetwork, networkSlice } = await import(networkUrl);
// Import the real layout function without constructing the DOM-facing class.
const { layoutNetwork } = await import(await moduleUrl('../src/genome-visualizer.ts', { './genome-network': networkUrl }));
const { PRESETS, MECHANISM_PRESETS_V6, CELLULAR_FAMILY_PRESETS, LEGACY_PRESETS } = await import(await moduleUrl('../src/presets.ts'));
const { instance } = await WebAssembly.instantiate(await readFile(new URL('../engine/pkg/agar_engine.wasm', import.meta.url)), {});
const api = instance.exports;
const decode = (pointer, length) => new TextDecoder().decode(new Uint8Array(api.memory.buffer, pointer, length));
function compile(source) {
  const bytes = new TextEncoder().encode(source);
  const pointer = api.alloc(bytes.length);
  assert.ok(pointer, 'Test source fits the supported byte limit');
  try {
    new Uint8Array(api.memory.buffer, pointer, bytes.length).set(bytes);
    const id = api.compile_genome(pointer, bytes.length);
    assert.ok(id, decode(api.error_ptr(), api.error_len()));
    return id;
  } finally { api.dealloc(pointer, bytes.length); }
}
function compilerGraph(id) {
  const pointer = api.genome_graph_ptr(id);
  assert.ok(pointer);
  return JSON.parse(decode(pointer, api.genome_graph_len()));
}
function project(source) {
  const graph = compilerGraph(compile(source));
  const before = JSON.stringify(graph);
  const network = buildGenomeNetwork(graph);
  assert.equal(JSON.stringify(graph), before, 'Projection does not mutate its compiler graph');
  validate(network,graph.roots.filter(root=>root.kind==='output').length);
  if(graph.mode==='cellular') {
    assert.ok(network.nodes.filter(node=>node.kind==='output').every(node=>/^(gene|activity|start_when|recycle)\./.test(node.label)));
    assert.ok(!network.nodes.some(node=>node.kind==='register'),'Cellular state is physical product stock');
  }
  assert.equal(network.expressionCount, graph.nodes.length);
  assert.equal(network.unusedCount, graph.nodes.filter(node => !node.reachable).length);
  assert.equal(network.constantCount, graph.nodes.filter(node => node.reachable && node.kind === 'constant').length);
  assert.equal(network.aliasCount, graph.nodes.filter(node => node.reachable && node.kind === 'reference').length);
  assert.deepEqual(buildGenomeNetwork(graph), network, 'Projection IDs and order are deterministic');
  return { graph, network };
}
function validate(network,outputCount=81) {
  const nodes = new Map(network.nodes.map(node => [node.id, node]));
  assert.equal(nodes.size, network.nodes.length, 'Projected node IDs are unique');
  assert.equal(new Set(network.edges.map(edge => edge.id)).size, network.edges.length, 'Parallel ports have distinct edge identities');
  assert.equal(network.nodes.filter(node => node.kind === 'output').length, outputCount,'Every authored/default compiler output is represented');
  assert.ok(network.nodes.every(node => !['constant', 'reference'].includes(node.kind)), 'Literal and alias boxes are absent');
  for (const edge of network.edges) {
    const from = nodes.get(edge.from), to = nodes.get(edge.to);
    assert.ok(from && to, 'Every endpoint exists');
    if (edge.temporal) {
      assert.equal(to.kind, 'register');
      assert.ok(['write', 'clear'].includes(edge.kind));
      assert.equal(to.level, 0, 'Commits never raise register layout rank');
    } else assert.ok(to.level > from.level, 'Same-tick expression edges remain acyclic');
    if (edge.guard) assert.equal(edge.port, 'when');
  }
}
function assertSlice(network, slice, maximum) {
  assert.ok(slice.nodes.length <= maximum);
  const ids = new Set(slice.nodes.map(node => node.id));
  assert.equal(ids.size, slice.nodes.length);
  assert.equal(slice.omitted, slice.total - slice.nodes.length);
  for (const edge of slice.edges) {
    assert.ok(ids.has(edge.from) && ids.has(edge.to));
    assert.ok(network.edges.includes(edge), 'A slice cannot invent a shortcut or feedback edge');
  }
  const expected = network.nodes.filter(node => ids.has(node.id) && network.edges.some(edge =>
    (edge.from === node.id && !ids.has(edge.to)) || (edge.to === node.id && !ids.has(edge.from)))).map(node => node.id);
  assert.deepEqual(slice.frontier, expected, 'Every cut connection has a visible frontier');
}
const legacy = 'thrust=0;turn=0;divide=false;';
const minimal = project(legacy).network;
assert.equal(minimal.nodes.length, 81);
assert.equal(minimal.edges.length, 0);
assert.equal(minimal.nodes.find(node => node.id === 'output:thrust').implicit, false);
assert.equal(minimal.nodes.find(node => node.id === 'output:photosynthesize').constants[0].value, '0');
assert.equal(minimal.nodes.find(node => node.id === 'output:absorb').constants[0].value, '1');
assert.equal(minimal.nodes.find(node => node.id === 'output:express.optical').constants[0].value, '0');
assert.equal(minimal.nodes.find(node => node.id === 'output:express.optical').implicit, true);
assert.equal(networkSlice(minimal, null).omitted, 0);

const alias = project(`let raw=energy;let alias=raw;
thrust=clamp((alias+energy)*0.001,0,1);turn=alias;divide=false;`).network;
const energy = alias.nodes.filter(node => node.id === 'input:energy');
assert.equal(energy.length, 1, 'Repeated reads share the same builtin input');
assert.ok(energy[0].sourceIds.length >= 3);
assert.deepEqual(energy[0].bindings, ['raw', 'alias']);
const sum = alias.nodes.find(node => node.kind === 'operation' && node.label === '+');
const repeated = alias.edges.filter(edge => edge.from === 'input:energy' && edge.to === sum.id);
assert.equal(repeated.length, 2, 'Repeated operands are not deduplicated');
assert.deepEqual(repeated.map(edge => edge.port), ['left', 'right']);
assert.ok(repeated.every(edge => edge.kind === 'read'));
assert.ok(alias.edges.some(edge => edge.kind === 'compute'));
const sameCalls = project('thrust=sin(age)+sin(age);turn=0;divide=false;').network;
assert.equal(sameCalls.nodes.filter(node => node.kind === 'operation' && node.label === 'sin').length, 2, 'Independent builtin calls stay separate');

const constantAliases = project(`let named=0.25;let copied=named;
thrust=copied;turn=0;divide=false;rf001=copied when true;clear rf001 when false;`).network;
const literal = constantAliases.nodes.find(node => node.id === 'output:thrust').constants[0];
assert.equal(literal.value, '0.25');
assert.deepEqual(literal.bindings, ['named', 'copied'], 'Folded literal aliases remain available in details');
const constantRegister = constantAliases.nodes.find(node => node.id === 'register:rf001');
assert.deepEqual(constantRegister.rootIds, ['write:rf001', 'clear:rf001']);
assert.ok(constantRegister.constants.some(value => value.port === 'value' && value.rule === 'write:rf001' && value.value === '0.25'));
assert.ok(constantRegister.constants.some(value => value.port === 'when' && value.rule === 'write:rf001' && value.value === 'true'));
assert.ok(constantRegister.constants.some(value => value.port === 'when' && value.rule === 'clear:rf001' && value.value === 'false'));

const mixedSource = `let fuel=energy;
ri001=ri001+1 when !object.valid;clear ri001 when object.valid;
rf001=fuel+rf001 when energy<100;clear rf001 when energy>180;
rb001=populated(rf001);rs001="<img src=x onerror=alert(1)>" when true;
rv001=flow.vector;
thrust=clamp(rf001*0.01,0,1);turn=if rb001 then light.bearing else chemical.mineral_bearing;divide=false;`;
const mixed = project(mixedSource).network;
for (const [address, type] of [['ri001','integer'],['rf001','scalar'],['rb001','boolean'],['rs001','string'],['rv001','vec2']]) {
  const matches = mixed.nodes.filter(node => node.id === `register:${address}`);
  assert.equal(matches.length, 1);
  assert.equal(matches[0].type, type);
}
const occupancy = mixed.nodes.find(node => node.kind === 'operation' && node.label === 'populated');
assert.equal(occupancy.type, 'boolean');
assert.ok(mixed.edges.some(edge => edge.from === 'register:rf001' && edge.to === occupancy.id && edge.kind === 'read' && edge.port === 'occupancy'));
assert.ok(mixed.edges.some(edge => edge.to === 'register:rf001' && edge.kind === 'write' && edge.guard && edge.temporal));
assert.ok(mixed.edges.some(edge => edge.to === 'register:rf001' && edge.kind === 'clear' && edge.guard && edge.temporal));
assert.ok(mixed.nodes.find(node => node.id === 'register:rs001').constants.some(value => value.value.includes('<img')), 'User text remains literal metadata');
const occupancyOnly = project(`${legacy} rb001=populated(rf999);`).network;
assert.equal(occupancyOnly.nodes.find(node => node.id === 'register:rf999').type, 'scalar', 'Occupancy does not change an address to boolean');
assert.equal(occupancyOnly.nodes.find(node => node.id === 'register:rf999').sourceIds.length, 0, 'An occupancy check is not a value read');

const self = project('rf001=rf001;clear rf001;thrust=rf001;turn=0;divide=false;').network;
assert.ok(self.edges.some(edge => edge.from === 'register:rf001' && edge.to === edge.from && edge.kind === 'write' && edge.temporal));
assert.ok(self.nodes.find(node => node.id === 'register:rf001').constants.some(value => value.port === 'clear' && value.value === 'always' && value.rule === 'clear:rf001'));
const registerNeighborhood = networkSlice(mixed, 'register:rf001');
assertSlice(mixed, registerNeighborhood, 112);
assert.ok(registerNeighborhood.nodes.some(node => node.id === 'input:energy'), 'Focus includes upstream write dependencies');
assert.ok(registerNeighborhood.nodes.some(node => node.id === 'output:thrust'), 'Focus includes downstream readers');
assertSlice(self, networkSlice(self, 'register:rf001', 1), 1);
assertSlice(mixed, networkSlice(mixed, null, 0), 0);
assert.deepEqual(networkSlice(mixed, 'missing'), { nodes: [], edges: [], total: 0, omitted: 0, frontier: [] });

for (const preset of [...PRESETS,...MECHANISM_PRESETS_V6,...CELLULAR_FAMILY_PRESETS,...LEGACY_PRESETS]) {
  const network = project(preset.source).network;
  const slice = networkSlice(network, null);
  assertSlice(network, slice, 112);
  for (const [areaWidth, areaHeight] of [[1080, 570], [650, 420]]) {
    const layout = layoutNetwork(slice, areaWidth, areaHeight);
    assert.equal(layout.points.size, slice.nodes.length, `${preset.name}: every displayed node has a position`);
    assert.ok(Number.isFinite(layout.width) && layout.width > 0 && Number.isFinite(layout.height) && layout.height > 0);
    // Match the renderer's 150×52 node rectangles, including adaptive columns.
    for (const node of slice.nodes) {
      const point = layout.points.get(node.id);
      assert.ok(point && Number.isFinite(point.x) && Number.isFinite(point.y), `${preset.name}: finite position for ${node.id}`);
      assert.ok(point.x >= 0 && point.y >= 0 && point.x + 150 <= layout.width && point.y + 52 <= layout.height,
        `${preset.name}: ${node.id} stays inside the graph canvas`);
    }
    for (const edge of slice.edges) if (!edge.temporal) {
      const from = layout.points.get(edge.from), to = layout.points.get(edge.to);
      assert.ok(from.x + 150 < to.x, `${preset.name}: ${edge.id} follows strictly left-to-right ranks`);
    }
  }
}
const parent = compile(mixedSource);
let variants = 0;
for (const kind of [1, 2, 3]) for (let seed = 1; seed <= 8; seed++) {
  const id = api.mutate_genome(parent, seed, kind);
  assert.ok(id);
  validate(buildGenomeNetwork(compilerGraph(id)));
  if (id !== parent) variants++;
}

const largest = project(`let unused=0;thrust=${Array(4095).fill('1').join('+')};turn=0;divide=false;`).network;
assert.equal(largest.expressionCount, 8192);
assert.equal(largest.unusedCount, 1);
assert.equal(largest.nodes.filter(node => node.kind === 'operation').length, 4094);
const overview = networkSlice(largest, null);
assertSlice(largest, overview, 112);
assert.ok(overview.omitted > 3000 && overview.frontier.length > 0);
const center = largest.nodes.filter(node => node.kind === 'operation')[2000];
const neighborhood = networkSlice(largest, center.id, 112, 8);
assertSlice(largest, neighborhood, 112);
assert.ok(neighborhood.nodes.some(node => node.level < center.level));
assert.ok(neighborhood.nodes.some(node => node.level > center.level));
assert.equal(neighborhood.total, 4095, 'Focus total excludes unrelated constant-only actions');
assert.deepEqual(networkSlice(largest, center.id, 112, 8), neighborhood, 'Bounded navigation is deterministic');
assertSlice(largest, networkSlice(largest, null, Number.POSITIVE_INFINITY, Number.NaN), 112);

const banks = Array.from({ length: 999 }, (_, index) => {
  const suffix = String(index + 1).padStart(3, '0');
  return [['ri','0'],['rf','0.0'],['rb','false'],['rs','""'],['rv','vec(0,0)']]
    .map(([prefix, value]) => `${prefix}${suffix}=${value};clear ${prefix}${suffix};`).join('');
}).join('\n');
const complete = project(legacy + banks);
assert.equal(complete.graph.roots.length, 10071);
assert.equal(complete.network.nodes.filter(node => node.kind === 'register').length, 4995);
assert.equal(complete.network.nodes.find(node => node.id === 'register:rv999').rootIds.length, 2);
assertSlice(complete.network, networkSlice(complete.network, null), 112);
assertSlice(complete.network, networkSlice(complete.network, 'register:rv999'), 112);

// A legal shared literal can have thousands of aliases and thousands of uses.
// Metadata must stay linear in aliases + uses, rather than storing their product.
const aliasChain = Array.from({ length: 6000 }, (_, index) => `let a${index}=${index === 0 ? '0' : `a${index - 1}`};`).join('');
const fanOut = Array.from({ length: 999 }, (_, index) => {
  const suffix = String(index + 1).padStart(3, '0');
  return `ri${suffix}=a5999;rf${suffix}=a5999;`;
}).join('');
const fanOutGraph = compilerGraph(compile(aliasChain + legacy + fanOut));
assert.equal(fanOutGraph.nodes.length, 8001, 'Alias fan-out fixture stays within the authored genome budget');
const fanOutNetwork = buildGenomeNetwork(fanOutGraph);
validate(fanOutNetwork);
const inlineUses = fanOutNetwork.nodes.flatMap(node => node.constants).filter(value => value.rule?.startsWith('write:'));
assert.equal(inlineUses.length, 1998);
const sharedAliases = inlineUses[0].bindings;
assert.equal(sharedAliases.length, 6000);
assert.equal(new Set(sharedAliases).size, 6000);
assert.ok(Object.isFrozen(sharedAliases), 'Shared constant aliases are immutable at runtime');
assert.ok(inlineUses.every(value => value.bindings === sharedAliases), 'All uses share the one deduplicated alias array');
assert.equal([...new Set(inlineUses.map(value => value.bindings))].reduce((sum, aliases) => sum + aliases.length, 0), 6000, 'Stored aliases do not multiply with consumer count');
assertSlice(fanOutNetwork, networkSlice(fanOutNetwork, null), 112);

console.log(`Genome network checks passed: ${PRESETS.length + MECHANISM_PRESETS_V6.length + CELLULAR_FAMILY_PRESETS.length + LEGACY_PRESETS.length} local-control/mechanism/cellular-family/legacy preset layouts, ${variants} typed variants, builtin/alias projection, all primitive registers, read/write/clear timing, exact ports, literal defaults, 8192-node chains, 10071 roots, linear alias metadata, and bounded bidirectional navigation.`);
