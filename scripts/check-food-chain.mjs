import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import ts from 'typescript';

// Default: short acquisition/recipe checks. --measure adds a bounded, three-seed
// 300-second study; population outcomes are reported, not forced to coexist.
const arguments_ = process.argv.slice(2);
const measure = arguments_.includes('--measure');
const outputIndex = arguments_.indexOf('--output');
const output = outputIndex >= 0 ? arguments_[outputIndex + 1] : null;
if (outputIndex >= 0) assert.ok(output && !output.startsWith('--'), '--output needs a file path');
for (let index = 0; index < arguments_.length; index++) {
  if (arguments_[index] === '--output') { index++; continue; }
  assert.equal(arguments_[index], '--measure', `Unknown argument: ${arguments_[index]}`);
}

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
const presetsUrl = await moduleUrl('../src/presets.ts');
const { PRESETS } = await import(presetsUrl);
const { BUILTIN_ENVIRONMENTS, BUILTIN_GENOMES, BUILTIN_SIMULATIONS } = await import(await moduleUrl('../src/catalog.ts', { './presets': presetsUrl }));
const binary = await readFile(new URL('../engine/pkg/agar_engine.wasm', import.meta.url));
const module = await WebAssembly.compile(binary);
const wasmHash = createHash('sha256').update(binary).digest('hex');
const STRIDE = 63;
const capabilities = ['propulsion', 'photosynthesis', 'digestion', 'armor', 'sensing', 'attack'];
const round = number => Math.round(number * 1000) / 1000;
const preset = name => { const found = PRESETS.find(item => item.name === name); assert.ok(found, `Missing preset: ${name}`); return found; };

async function fresh(config, seed = 42, initialFood = config.foodRate > 0 ? 360 : 0) {
  const instance = await WebAssembly.instantiate(module, {}), api = instance.exports;
  assert.equal(api.abi_version(), 5, 'Rebuild the trophic engine with npm run build:wasm.');
  api.set_environment(config.ambientTemperature, config.ambientLight, config.currentStrength);
  api.set_initial_nutrients(config.initialNutrients); api.set_food_rate(config.foodRate); api.clear_sources();
  for (const source of config.sources) assert.ok(api.add_source(source.kind, source.x, source.y, source.strength, source.radius));
  api.set_mutation_rate(0); api.reset_empty(seed, initialFood);
  return api;
}
function compile(api, source) {
  const encoded = new TextEncoder().encode(source), pointer = api.alloc(encoded.length);
  assert.ok(pointer);
  try {
    new Uint8Array(api.memory.buffer, pointer, encoded.length).set(encoded);
    const id = api.compile_genome(pointer, encoded.length);
    assert.ok(id, new TextDecoder().decode(new Uint8Array(api.memory.buffer, api.error_ptr(), api.error_len())));
    return id;
  } finally { api.dealloc(pointer, encoded.length); }
}
function graph(api, id) {
  const pointer = api.genome_graph_ptr(id);
  return JSON.parse(new TextDecoder().decode(new Uint8Array(api.memory.buffer, pointer, api.genome_graph_len())));
}
function snapshot(api) {
  const pointer = api.snapshot_ptr();
  const data = new Float32Array(api.memory.buffer, pointer, api.snapshot_len()).slice();
  assert.equal(data.length, 12 + data[2] * STRIDE + data[3] * 4);
  const cells = Array.from({ length: data[2] }, (_, index) => {
    const at = 12 + index * STRIDE;
    return { id: data[at], genome: data[at + 1], x: data[at + 2], y: data[at + 3],
      energy: data[at + 7], age: data[at + 8], generation: data[at + 9], parentId: data[at + 10], size: data[at + 18], capacity: data[at + 19],
      light: data[at + 23], integrity: data[at + 38], photosynthesisRate: data[at + 39], nutrients: data[at + 40],
      body: [...data.slice(at + 45, at + 50), data[at + 53]], gutEnergy: data[at + 54], gutCapacity: data[at + 55],
      gutNutrients: data[at + 56], digestionRate: data[at + 57], growthRate: data[at + 58], birthSize: data[at + 59],
      maturity: data[at + 60], lifespan: data[at + 61], exudationRate: data[at + 62] };
  });
  let foodEnergy = 0;
  for (let index = 0; index < data[3]; index++) foodEnergy += data[12 + data[2] * STRIDE + index * 4 + 2];
  const state = { seconds: data[1], population: data[2], foodParticles: data[3], foodEnergy, births: data[4], deaths: data[5],
    foodParticlesEaten: data[6], predations: Number(api.predation_count()), biteTicks: Number(api.attack_count()),
    photosynthesisEnergy: Number(api.photosynthesis_total()), cells };
  assert.ok(state.population <= 384);
  for (const cell of cells) {
    assert.ok(Number.isFinite(cell.energy) && cell.energy > 0 && cell.energy <= cell.capacity + 0.001);
    assert.ok(Number.isFinite(cell.gutEnergy) && cell.gutEnergy >= 0 && cell.gutEnergy <= cell.gutCapacity + 0.001);
    assert.ok(cell.body.every(value => Number.isFinite(value) && value >= 0 && value <= 1));
    assert.ok(cell.body.reduce((sum, value) => sum + value, 0) <= 1.00001, 'All six capabilities share one body budget');
    assert.ok(cell.maturity >= 0 && cell.maturity <= 1);
    assert.ok(cell.birthSize > 0 && cell.age <= cell.lifespan + 0.04);
  }
  return state;
}
function advance(api, ticks) {
  while (ticks > 0) { const count = Math.min(120, ticks); api.tick(count); ticks -= count; }
  return snapshot(api);
}

const still = BUILTIN_ENVIRONMENTS.find(item => item.id === 'builtin:environment:still-water');
const sunlit = BUILTIN_ENVIRONMENTS.find(item => item.id === 'builtin:environment:sunlit-pond');
const thermal = BUILTIN_ENVIRONMENTS.find(item => item.id === 'builtin:environment:thermal-currents');
assert.ok(still.config.foodRate > 0, 'Still water remains an explicitly fed heterotroph sandbox');
assert.equal(sunlit.config.foodRate, 0, 'Sunlit food must come from producers or remains');
assert.equal(thermal.config.foodRate, 0, 'Thermal food must come from producers or remains');
assert.ok(sunlit.config.ambientLight > 0 && thermal.config.ambientLight > 0);

const inspection = await fresh(still.config);
const physiology = new Map();
for (const item of PRESETS) {
  const compiled = graph(inspection, compile(inspection, item.source));
  const constant = name => {
    const root = compiled.roots.find(root => root.kind === 'output' && root.name === name);
    assert.ok(root && root.value !== null, `${item.name} explicitly sets ${name}`);
    const node = compiled.nodes[root.value];
    assert.equal(node.kind, 'constant', `${item.name} has an explicit initial ${name} allocation`);
    return Number(node.label);
  };
  const body = Object.fromEntries(capabilities.map(name => [name, constant('invest.' + name)]));
  assert.ok(Object.values(body).every(value => value >= 0 && value <= 1));
  assert.ok(Object.values(body).reduce((sum, value) => sum + value, 0) <= 1.000001);
  physiology.set(item.name, { ...body, feed: constant('feed') });
}
for (const name of ['Forager', 'Drifter', 'Conserver', 'Giant', 'Rememberer']) {
  const body = physiology.get(name);
  assert.equal(body.feed, 1); assert.equal(body.photosynthesis, 0); assert.equal(body.attack, 0); assert.ok(body.digestion > 0);
}
assert.deepEqual([physiology.get('Phototroph').feed, physiology.get('Phototroph').digestion, physiology.get('Phototroph').attack], [0, 0, 0]);
assert.ok(physiology.get('Phototroph').photosynthesis > 0);
assert.equal(physiology.get('Predator').photosynthesis, 0);
assert.equal(physiology.get('Predator').feed, 1);
assert.ok(physiology.get('Predator').attack > 0 && physiology.get('Predator').digestion > 0);
assert.ok(physiology.get('Mixotroph').photosynthesis > 0 && physiology.get('Mixotroph').digestion > 0);
assert.equal(physiology.get('Mixotroph').feed, 1); assert.equal(physiology.get('Mixotroph').attack, 0);

// Hold movement, growth, and division constant to isolate energy acquisition.
function stationary(source) {
  for (const [name, value] of [['thrust', '0'], ['turn', '0'], ['grow', '0'], ['divide', 'false']]) {
    const expression = new RegExp(`^${name}\\s*=\\s*[^;]+;`, 'm');
    assert.ok(expression.test(source)); source = source.replace(expression, `${name} = ${value};`);
  }
  return source;
}
async function acquisition(name, light, organicFood = false) {
  const api = await fresh({ foodRate: 0, ambientTemperature: 0.5, ambientLight: light, currentStrength: 0, initialNutrients: 1, sources: [] }, 42, 0);
  const id = compile(api, stationary(preset(name).source));
  assert.equal(api.seed_cells(id, 1, 600, 380), 1);
  const before = snapshot(api), cell = before.cells[0];
  if (organicFood) api.add_food(cell.x, cell.y, 256);
  const after = advance(api, 600);
  assert.equal(after.cells.length, 1, 'A short acquisition experiment should not divide or lose its only cell');
  return { before, after };
}
const producerLight = await acquisition('Phototroph', 0.6);
const producerDark = await acquisition('Phototroph', 0);
const consumerEmpty = await acquisition('Forager', 0.6);
const consumerFed = await acquisition('Forager', 0.6, true);
const mixerLight = await acquisition('Mixotroph', 0.6);
const mixerDark = await acquisition('Mixotroph', 0);
assert.ok(producerLight.after.cells[0].energy > producerLight.before.cells[0].energy, 'A producer gains energy from light without external food');
assert.ok(producerDark.after.cells[0].energy < producerDark.before.cells[0].energy, 'An unfed producer spends its reserve in the dark');
assert.equal(producerLight.after.cells[0].gutEnergy, 0, 'A pure autotroph does not consume its own exudate');
assert.ok(producerLight.after.cells[0].nutrients > producerLight.before.cells[0].nutrients, 'Mineral uptake works with zero digestive tissue');
assert.ok(producerLight.after.foodParticles > 0 && producerLight.after.foodEnergy > 0, 'Living producers supply organic food without an external food rate');
assert.equal(producerDark.after.foodParticles, 0);
assert.equal(consumerEmpty.after.photosynthesisEnergy, 0, 'Light does not feed a heterotroph');
assert.ok(consumerEmpty.after.cells[0].energy < consumerEmpty.before.cells[0].energy);
assert.ok(consumerFed.after.cells[0].energy > consumerEmpty.after.cells[0].energy, 'Organic food supplies the consumer through its gut');
assert.ok(mixerLight.after.cells[0].energy > mixerDark.after.cells[0].energy, 'A mixotroph can use light as an alternate energy source');

console.log(`Food-chain mechanics passed: ${PRESETS.length} compiled presets, explicit six-capability budgets, distinct diets, independent mineral uptake, and producer-fed organic food.`);

if (measure) {
  const template = BUILTIN_SIMULATIONS.find(item => item.id === 'builtin:simulation:mixed-ecosystem');
  const environment = BUILTIN_ENVIRONMENTS.find(item => item.id === template.environmentId);
  const populations = template.populations.map(item => ({ ...BUILTIN_GENOMES.find(genome => genome.id === item.genomeId), count: item.count }));
  const study = { startedAt: new Date().toISOString(), wasmSha256: wasmHash, wasmBytes: binary.length, abi: 5,
    template: template.name, environment, populations, mutationRate: 0, initialFood: environment.config.foodRate > 0 ? 360 : 0,
    // Sampling just before the lifespan limit distinguishes consumer starvation
    // from founders reaching old age at the final 300-second checkpoint.
    seeds: [42, 7, 2026], sampleSeconds: [0, 30, 120, 299, 300], results: [] };
  for (const seed of study.seeds) {
    const started = performance.now(), api = await fresh(environment.config, seed), names = new Map();
    for (const item of populations) {
      const id = compile(api, item.source); names.set(id, item.name);
      assert.equal(api.seed_cells(id, item.count, -1, -1), item.count);
    }
    const samples = [];
    let tick = 0;
    for (const seconds of study.sampleSeconds) {
      const state = advance(api, seconds * 30 - tick); tick = seconds * 30;
      assert.ok(state.cells.every(cell => names.has(cell.genome)), 'Mutation-free measurements retain their founding genomes');
      const lineages = [...names].map(([id, name]) => {
        const cells = state.cells.filter(cell => cell.genome === id);
        const sum = field => cells.reduce((total, cell) => total + cell[field], 0);
        return { name, count: cells.length, biomass: round(sum('size')), energy: round(sum('energy')),
          meanEnergy: cells.length ? round(sum('energy') / cells.length) : null,
          meanEnergyFraction: cells.length ? round(cells.reduce((total, cell) => total + cell.energy / cell.capacity, 0) / cells.length) : null,
          meanSize: cells.length ? round(sum('size') / cells.length) : null,
          meanIntegrity: cells.length ? round(sum('integrity') / cells.length) : null,
          meanAge: cells.length ? round(sum('age') / cells.length) : null,
          oldestAge: cells.length ? round(Math.max(...cells.map(cell => cell.age))) : null,
          founders: cells.filter(cell => cell.parentId === 0).length,
          gutEnergy: round(sum('gutEnergy')), digestionRate: round(sum('digestionRate')),
          photosynthesisRate: round(sum('photosynthesisRate')), exudationRate: round(sum('exudationRate')),
          growthRate: round(sum('growthRate')), maxGeneration: cells.length ? Math.max(...cells.map(cell => cell.generation)) : null };
      });
      const { cells: _cells, ...totals } = state;
      const sample = { ...totals, foodEnergy: round(totals.foodEnergy), photosynthesisEnergy: round(totals.photosynthesisEnergy), lineages };
      samples.push(sample); console.log(JSON.stringify({ seed, ...sample }));
    }
    study.results.push({ seed, wallSeconds: round((performance.now() - started) / 1000), samples });
    if (output) await writeFile(output, JSON.stringify(study, null, 2) + '\n');
  }
  if (!output) console.log(JSON.stringify({ measurement: study }));
  else console.log(`Food-chain measurements saved to ${output}`);
} else if (output) throw new Error('--output requires --measure');
