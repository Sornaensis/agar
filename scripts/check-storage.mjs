import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

async function moduleUrl(relative, imports = {}) {
  const source = await readFile(new URL(relative, import.meta.url), 'utf8');
  let code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  for (const [name, url] of Object.entries(imports)) {
    code = code.replaceAll(`from '${name}'`, `from ${JSON.stringify(url)}`).replaceAll(`from ${JSON.stringify(name)}`, `from ${JSON.stringify(url)}`);
  }
  return `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`;
}
const valueModule = value => `data:text/javascript;base64,${Buffer.from(`export default ${JSON.stringify(value)};`).toString('base64')}`;
const engineContract = JSON.parse(await readFile(new URL('../src/engine-contract.json', import.meta.url), 'utf8'));
// Vite supplies asset URLs and JSON imports in the browser. These storage tests
// import the same engine constants through equivalent dependency-free modules.
const engineUrl = await moduleUrl('../src/engine.ts', {
  '../engine/pkg/agar_engine.wasm?url': valueModule(new URL('../engine/pkg/agar_engine.wasm', import.meta.url).href),
  './engine-contract.json': valueModule(engineContract),
});
const storage = await import(await moduleUrl('../src/storage.ts', { './engine': engineUrl }));
const presetsUrl = await moduleUrl('../src/presets.ts');
const catalog = await import(await moduleUrl('../src/catalog.ts', { './presets': presetsUrl }));
const { BUILTIN_GENOMES: genomes, BUILTIN_ENVIRONMENTS: environments, BUILTIN_SIMULATIONS: simulations } = catalog;
const copy = value => structuredClone(value);

const slugs = rows => rows.map(row => row.id.split(':').at(-1)).sort();
assert.deepEqual(slugs(genomes), ['crown-rosette','forager','ligand-producer','mixotroph','nurse-rosette','phototroph','predator','pulse-swimmer','rememberer']);
assert.deepEqual(slugs(simulations), ['springwater-basin']);
assert.equal(new Set(environments.map(row => row.id)).size, environments.length);
for (const id of ['still-water','sunlit-pond','thermal-currents','colony-pool','current-bands']) {
  assert.ok(environments.some(row => row.id === 'builtin:environment:' + id), 'Historical environment remains addressable: ' + id);
}
const basin = environments.find(row => row.id === simulations[0].environmentId).config;
assert.ok(basin.width >= 800 && basin.height >= 500 && basin.barriers.length >= 2);
assert.ok([3,4,6].every(kind => basin.sources.some(source => source.kind === kind)));
assert.deepEqual(simulations[0].populations.map(row => row.genomeId).sort(), genomes.map(row => row.id).sort());
for (const genome of genomes) {
  assert.ok(Object.isFrozen(genome));
  assert.throws(() => storage.validateGenomeDocument(genome), /copied/);
  assert.equal(storage.validateGenomeDocument(genome, true).source, genome.source);
  const exported = storage.exportGenome(genome);
  const a = storage.parseGenomeImport(exported), b = storage.parseGenomeImport(exported);
  assert.notEqual(a.id, b.id, 'Repeated imports create independent identities');
  assert.ok(!a.id.startsWith('builtin:'));
  assert.equal(a.source, genome.source);
  assert.equal(a.name, genome.name);
  assert.equal(JSON.parse(exported).id, undefined, 'Portable files do not carry storage identities');
}
for (const environment of environments) {
  assert.ok(Object.isFrozen(environment.config.sources));
  assert.throws(() => storage.validateEnvironmentDocument(environment), /copied/);
  const a = storage.parseEnvironmentImport(storage.exportEnvironment(environment));
  const b = storage.parseEnvironmentImport(storage.exportEnvironment(environment));
  assert.notEqual(a.id, b.id);
  assert.deepEqual(a.config, environment.config);
  a.config.sources.push({ kind: 0, x: 1, y: 1, radius: 50, strength: 0.2 });
  assert.equal(b.config.sources.length, environment.config.sources.length, 'Editing a copy cannot alter the built-in or another copy');
}
for (const simulation of simulations) {
  assert.ok(environments.some(environment => environment.id === simulation.environmentId));
  assert.ok(simulation.populations.every(population => genomes.some(genome => genome.id === population.genomeId)));
  const total = simulation.populations.reduce((sum, population) => sum + population.count, 0);
  assert.ok(simulation.populations.length > 0 && simulation.populations.every(row => Number.isSafeInteger(row.count) && row.count > 0));
  assert.ok(Number.isSafeInteger(total) && total > 0);
  assert.deepEqual(catalog.simulationStartSettings(), {mutationRate: 0.1, countChemistry: true});
}

const portable = JSON.parse(storage.exportGenome(genomes[0]));
assert.equal(portable.format, 'agar.genome');
assert.equal(portable.version,2); assert.equal(portable.languageVersion,4);
const v3 = storage.parseGenomeImport(JSON.stringify({...portable, languageVersion:3}));
assert.equal(v3.languageVersion,3);
assert.equal(JSON.parse(storage.exportGenome(v3)).languageVersion,3);
const legacyGenome = storage.validateGenomeDocument({ id:'genome:old',name:'Old genome',source:'thrust=0;turn=0;divide=false;invest.sensing=.2;',createdAt:1,updatedAt:1 });
assert.equal(legacyGenome.languageVersion,1,'Older stored rows are preserved as older language');
const legacyExport = JSON.parse(storage.exportGenome(legacyGenome));
assert.equal(legacyExport.version,1); assert.equal(legacyExport.source,legacyGenome.source);
assert.throws(() => storage.parseGenomeImport(JSON.stringify(legacyExport)), /older investment language/);
assert.throws(() => storage.parseGenomeImport(JSON.stringify({...portable,languageVersion:2})), /Unsupported genome language/);
for (const population of simulations.flatMap(run=>run.populations)) assert.deepEqual(Object.keys(population).sort(),['count','genomeId'],'A starting population chooses only its genome and count, never preassembled organs');
assert.equal('recommendedStarterBody' in catalog,false,'The catalog does not map genomes to external body recipes');
assert.equal(storage.parseGenomeImport(JSON.stringify({ ...portable, format: 'cellarium.genome' })).source, genomes[0].source, 'Existing genome exports remain importable after the app rename');
const environmentExport = JSON.parse(storage.exportEnvironment(environments[0]));
assert.equal(environmentExport.format, 'agar.environment');
assert.deepEqual(storage.parseEnvironmentImport(JSON.stringify({ ...environmentExport, format: 'cellarium.environment' })).config, environments[0].config, 'Existing environment exports remain importable after the app rename');
for (const invalid of [null, [], {}, { ...portable, version: 1 }, { ...portable, format: 'cellarium.environment' }, { ...portable, id: 'builtin:genome:forager' }, { ...portable, name: '' }, { ...portable, name: 'x'.repeat(81) }, { ...portable, name: 'a\nb' }, { ...portable, source: '' }, { ...portable, source: true }]) {
  assert.throws(() => storage.parseGenomeImport(JSON.stringify(invalid)));
}
assert.throws(() => storage.parseGenomeImport('{'), /JSON/);
assert.throws(() => storage.parseGenomeImport(' '.repeat(storage.MAX_GENOME_IMPORT_BYTES + 1)), /large/);
assert.throws(() => storage.parseGenomeImport(JSON.stringify({ ...portable, source: 'é'.repeat(131073) })), /UTF-8/);
const maxSource = '#' + '\u0000'.repeat(262143);
const maxGenome = storage.newGenome('Boundary', maxSource);
assert.equal(storage.parseGenomeImport(storage.exportGenome(maxGenome)).source, maxSource, 'JSON escaping may exceed source byte size without losing a valid bounded payload');
assert.throws(() => storage.newGenome('Too large', 'x'.repeat(262145)), /UTF-8/);
assert.throws(() => storage.newGenome('Bad description', 'thrust=0;turn=0;divide=false;', 'x'.repeat(501)), /500/);
assert.throws(() => storage.parseEnvironmentImport(JSON.stringify({ ...JSON.parse(storage.exportEnvironment(environments[0])), __proto__: null, sources: [] })), /unsupported/);
assert.throws(() => storage.parseGenomeImport('{"format":"cellarium.genome","version":2,"languageVersion":3,"name":"X","source":"abc","__proto__":{"polluted":true}}'), /unsupported/);
assert.equal({}.polluted, undefined);

const baseConfig = environments[0].config;
for (const retiredRate of [6, 60, 'unused']) {
  const oldRecipe = { ...environmentExport, config: { ...baseConfig, foodRate: retiredRate } };
  const imported = storage.parseEnvironmentImport(JSON.stringify(oldRecipe));
  assert.deepEqual(imported.config, baseConfig);
  assert.deepEqual(JSON.parse(storage.exportEnvironment(imported)).config, baseConfig);
}
for (const [field, invalid] of [['ambientTemperature', -0.1], ['ambientLight', 1.01], ['currentStrength', 31], ['initialNutrients', 5.01]]) {
  assert.throws(() => storage.validateEnvironmentConfig({ ...baseConfig, [field]: invalid }), field);
}
const source = { kind: 0, x: 600, y: 380, strength: 0.8, radius: 200 };
for (const invalid of [{ ...source, kind: 13 }, { ...source, kind: 1.5 }, { ...source, x: 1200 }, { ...source, y: 760 }, { ...source, x: -1 }, { ...source, radius: 19 }, { ...source, radius: 1001 }, { ...source, strength: -0.1 }, { ...source, angle: NaN }, { ...source, length: 4801 }]) {
  assert.throws(() => storage.validateEnvironmentConfig({ ...baseConfig, sources: [invalid] }));
}
assert.throws(() => storage.validateEnvironmentConfig({ ...baseConfig, sources: Array(65).fill(source) }), /64/);
for (const kind of [1, 2, 5, 6, 7]) assert.equal(storage.validateEnvironmentConfig({ ...baseConfig, sources: [{ ...source, kind, strength: -1 }] }).sources[0].strength, -1);
for (const kind of [0, 3, 4, 8, 9, 10, 11, 12]) assert.throws(() => storage.validateEnvironmentConfig({ ...baseConfig, sources: [{ ...source, kind, strength: -1 }] }), /strength/);
for (const patch of [{ width: 1221 }, { width: 4820 }, { height: 380 }, { height: 3060 }]) assert.throws(() => storage.validateEnvironmentConfig({ ...baseConfig, ...patch }), /World/);
const barrier = { x: 1700, y: 900, length: 600, width: 40, angle: Math.PI / 4, floating: true };
const larger = storage.validateEnvironmentConfig({ ...baseConfig, width: 2400, height: 1520, sources: [{ ...source, x: 2000, y: 1200, kind: 6, angle: Math.PI / 2, length: 2000 }], barriers: [barrier] });
const roundTrip = storage.parseEnvironmentImport(storage.exportEnvironment(storage.newEnvironment('Larger habitat', larger)));
assert.deepEqual(roundTrip.config, larger, 'Dimensions, source shape, and floating barrier properties survive export/import');
for (const patch of [{ width: 9 }, { length: 2401 }, { floating: 1 }, { x: 2400 }, { angle: Infinity }]) assert.throws(() => storage.validateEnvironmentConfig({ ...larger, barriers: [{ ...barrier, ...patch }] }));
assert.throws(() => storage.validateEnvironmentConfig({ ...larger, barriers: Array(65).fill(barrier) }), /64/);
const legacy = storage.validateEnvironmentConfig({ foodRate: 0, ambientTemperature: 0.5, ambientLight: 0, currentStrength: 0, initialNutrients: 0.3, sources: [source] });
assert.deepEqual([legacy.width, legacy.height, legacy.barriers.length, legacy.sources[0].angle, legacy.sources[0].length], [1200, 760, 0, 0, 600], 'Previous environment recipes receive explicit geometry defaults');
assert.deepEqual(legacy.gases, { initialOxygen:1, initialCarbon:1, mode:'sealed', exchangeRate:.02, oxygenEquilibrium:1, carbonEquilibrium:1 }, 'Old recipes receive explicit sealed gas defaults');
for (const patch of [{mode:'closed'}, {initialOxygen:-1}, {initialCarbon:101}, {exchangeRate:1.1}, {exchangeRate:NaN}, {oxygenEquilibrium:Infinity}, {carbonEquilibrium:'1'}]) {
  assert.throws(() => storage.validateEnvironmentConfig({...baseConfig,gases:{...baseConfig.gases,...patch}}));
}
const gasConfig = storage.validateEnvironmentConfig({...baseConfig, gases:{initialOxygen:0,initialCarbon:3,mode:'open',exchangeRate:.3,oxygenEquilibrium:2,carbonEquilibrium:.5}, sources:[8,9,10,11,12].map(kind=>({...source,kind}))});
assert.deepEqual(storage.parseEnvironmentImport(storage.exportEnvironment(storage.newEnvironment('Gas habitat', gasConfig))).config,gasConfig,'Gas inventories, boundary and all five gas objects survive export/import');

const summary = { time: 12, population: 22, food: 300, births: 0, deaths: 0, generation: 0 };
const simulation = {
  id: 'simulation:test', name: 'A run', createdAt: 1, updatedAt: 2,
  state: new Uint8Array([1, 2, 3]), environmentName: 'Still water', seed: 42, paused: false, speed: 5,
  genomeNames: { 1: 'Forager' }, history: [{ ...summary, time: 1 }, { ...summary, time: 12 }], summary,
};
assert.equal(storage.validateSimulationRecord(simulation).speed, 5);
const ecologyHistory = [{ time: 0, populations: [[1, 8], [2, 4]] }, { time: 12, populations: [[2, 3]], events: [[2, 0, 1, 3]] }];
assert.deepEqual(storage.validateSimulationRecord({ ...simulation, ecologyHistory }).ecologyHistory, ecologyHistory);
for (const invalid of [
  [{ time: 1, populations: [[1, 3], [1, 4]] }], [{ time: 5, populations: [[1, -4]] }],
  [{ time: 13, populations: [] }], [...ecologyHistory, ecologyHistory[1]],
]) assert.throws(() => storage.validateSimulationRecord({ ...simulation, ecologyHistory: invalid }));
for (const population of [385, 2049, 10000, Number.MAX_SAFE_INTEGER]) {
  const record = storage.validateSimulationRecord({ ...simulation, summary: { ...summary, population }, history: [{ ...summary, population }] });
  assert.equal(record.summary.population, population, 'Simulation metadata has no fixed population ceiling');
  assert.equal(record.history[0].population, population);
}
for (const patch of [{ speed: 4 }, { state: new ArrayBuffer(4) }, { state: new Uint8Array() }, { seed: -1 }, { paused: 1 }, { id: 'builtin:simulation:x' }, { history: Array(721).fill(summary) }, { history: [{ ...summary, time: 13 }] }, { history: [{ ...summary, time: 5 }, { ...summary, time: 4 }] }, { genomeNames: { 2049: 'Bad' } }, { genomeNames: JSON.parse('{"__proto__":"bad"}') }, { summary: { ...summary, population: -1 } }, { summary: { ...summary, population: 1.5 } }, { summary: { ...summary, population: Number.MAX_SAFE_INTEGER + 1 } }, { summary: { ...summary, population: Infinity } }, { summary: { ...summary, time: Infinity } }]) {
  assert.throws(() => storage.validateSimulationRecord({ ...simulation, ...patch }));
}

// Narrow transaction harness: it controls only the get/put/commit callbacks needed
// by rename, without emulating IndexedDB. Browser reload tests exercise the real API.
function renameHarness(initial) {
  const values = new Map(initial ? [[initial.id, copy(initial)]] : []);
  const transactions = [];
  const database = { transaction(names, mode) {
    const pending = [];
    const tx = {
      names, mode, aborted: false,
      objectStore() { return {
        get(id) {
          const request = { result: copy(values.get(id)), onsuccess: null };
          queueMicrotask(() => request.onsuccess());
          return request;
        },
        put(value) { pending.push(copy(value)); },
      }; },
      abort() { this.aborted = true; queueMicrotask(() => this.onabort()); },
      commit() { assert.equal(this.aborted, false); for (const value of pending) values.set(value.id, value); this.oncomplete(); },
    };
    transactions.push(tx); return tx;
  } };
  return { instance: new storage.Storage(database), values, transactions };
}
for (const [kind, item] of [
  ['genome', storage.newGenome('Before', genomes[0].source, 'Newest stored description')],
  ['environment', storage.newEnvironment('Before', environments[1].config, 'Newest stored description')],
]) {
  item.updatedAt = Date.now() + 60000;
  const harness = renameHarness(item), method = kind === 'genome' ? 'renameGenome' : 'renameEnvironment';
  let resolved = false;
  const promise = harness.instance[method](item.id, '  After  ').then(value => { resolved = true; return value; });
  await Promise.resolve(); await Promise.resolve();
  assert.equal(harness.transactions.length, 1);
  assert.equal(harness.transactions[0].mode, 'readwrite');
  assert.equal(resolved, false, 'Rename is not reported saved before transaction completion');
  assert.equal(harness.values.get(item.id).name, 'Before');
  harness.transactions[0].commit();
  const renamed = await promise;
  assert.deepEqual(renamed, { ...item, name: 'After' }, 'Only name and monotonic update time may change');
  assert.equal(harness.values.get(item.id).name, 'After');
  renamed.name = 'Caller mutation';
  assert.equal(harness.values.get(item.id).name, 'After', 'Returned documents do not alias stored data');
  const missing = renameHarness(null);
  await assert.rejects(missing.instance[method](item.id, 'Renamed after delete'), /no longer exists/);
  assert.equal(missing.values.size, 0, 'A stale rename cannot recreate a deleted document');
  assert.equal(missing.transactions[0].aborted, true);
  const invalid = renameHarness(item);
  assert.throws(() => invalid.instance[method]('builtin:genome:forager', 'Bad'), /copied/);
  assert.throws(() => invalid.instance[method](item.id, ''), /name/);
  assert.equal(invalid.transactions.length, 0, 'Invalid identifiers and names never begin a write');
}

const localValues = new Map();
globalThis.localStorage = {
  getItem: key => localValues.get(key) ?? null,
  setItem: (key, value) => { localValues.set(key, value); },
  removeItem: key => { localValues.delete(key); },
};
assert.equal(storage.loadPreferences().tab, 'simulation');
storage.savePreferences({ tab: 'genomes', simulationView: 'metrics', activeSimulationId: 'simulation:test', overlay: 'nutrients', sensors: false, trails: true });
assert.equal(storage.loadPreferences().simulationView, 'metrics');
assert.equal(storage.loadPreferences().overlay, 'nutrients');
assert.equal(storage.loadPreferences().builderOverlay, 'light', 'Old player preferences receive an independent builder field');
assert.equal(storage.loadPreferences().showExtinct,false,'Historical preference records default to living lineages');
storage.savePreferences({...storage.loadPreferences(), builderOverlay:'carbon',showExtinct:true});
assert.equal(storage.loadPreferences().overlay,'nutrients');
assert.equal(storage.loadPreferences().builderOverlay,'carbon');
assert.equal(storage.loadPreferences().showExtinct,true);
for (const overlay of ['oxygen','carbon']) {
  storage.savePreferences({...storage.loadPreferences(),overlay});
  assert.equal(storage.loadPreferences().overlay,overlay);
}
storage.saveGenomeDraft({ name: '', source: '' });
assert.deepEqual(storage.loadGenomeDraft(), { name: '', source: '' }, 'An unfinished editor can be recovered');
storage.clearGenomeDraft();
assert.equal(storage.loadGenomeDraft(), null);
storage.saveEnvironmentDraft({ name: 'Draft', config: baseConfig });
assert.deepEqual(storage.loadEnvironmentDraft(), { name: 'Draft', config: baseConfig });
storage.clearEnvironmentDraft();
assert.equal(storage.loadEnvironmentDraft(), null);
localStorage.setItem = () => { throw new DOMException('Full', 'QuotaExceededError'); };
assert.throws(() => storage.saveGenomeDraft({ name: 'Draft', source: 'thrust=0;' }), /storage is full/);
assert.throws(() => storage.savePreferences(storage.loadPreferences()), /storage is full/);

console.log('Storage checks passed: immutable built-ins, distinct imported copies, versioned JSON round-trips, UTF-8 limits, invalid imports, environment bounds, checkpoint metadata, atomic rename and missing-item rejection, commit completion, history limits, draft recovery, and surfaced quota failures.');
