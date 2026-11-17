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

assert.equal(genomes.length, 7);
assert.equal(environments.length, 3);
assert.equal(simulations.length, 3);
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
  assert.ok(simulation.populations.reduce((sum, population) => sum + population.count, 0) <= 384);
}

const portable = JSON.parse(storage.exportGenome(genomes[0]));
assert.equal(portable.format, 'agar.genome');
assert.equal(storage.parseGenomeImport(JSON.stringify({ ...portable, format: 'cellarium.genome' })).source, genomes[0].source, 'Existing genome exports remain importable after the app rename');
const environmentExport = JSON.parse(storage.exportEnvironment(environments[0]));
assert.equal(environmentExport.format, 'agar.environment');
assert.deepEqual(storage.parseEnvironmentImport(JSON.stringify({ ...environmentExport, format: 'cellarium.environment' })).config, environments[0].config, 'Existing environment exports remain importable after the app rename');
for (const invalid of [null, [], {}, { ...portable, version: 2 }, { ...portable, format: 'cellarium.environment' }, { ...portable, id: 'builtin:genome:forager' }, { ...portable, name: '' }, { ...portable, name: 'x'.repeat(81) }, { ...portable, name: 'a\nb' }, { ...portable, source: '' }, { ...portable, source: true }]) {
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
assert.throws(() => storage.parseGenomeImport('{"format":"cellarium.genome","version":1,"name":"X","source":"abc","__proto__":{"polluted":true}}'), /unsupported/);
assert.equal({}.polluted, undefined);

const baseConfig = environments[0].config;
for (const [field, invalid] of [['foodRate', 61], ['foodRate', NaN], ['foodRate', '6'], ['ambientTemperature', -0.1], ['ambientLight', 1.01], ['currentStrength', 31], ['initialNutrients', 5.01]]) {
  assert.throws(() => storage.validateEnvironmentConfig({ ...baseConfig, [field]: invalid }), field);
}
const source = { kind: 0, x: 600, y: 380, strength: 0.8, radius: 200 };
for (const invalid of [{ ...source, kind: 4 }, { ...source, kind: 1.5 }, { ...source, x: 1200 }, { ...source, y: 760 }, { ...source, x: -1 }, { ...source, radius: 39 }, { ...source, radius: 401 }, { ...source, strength: -0.1 }]) {
  assert.throws(() => storage.validateEnvironmentConfig({ ...baseConfig, sources: [invalid] }));
}
assert.throws(() => storage.validateEnvironmentConfig({ ...baseConfig, sources: Array(17).fill(source) }), /16/);
for (const kind of [1, 2]) assert.equal(storage.validateEnvironmentConfig({ ...baseConfig, sources: [{ ...source, kind, strength: -1 }] }).sources[0].strength, -1);

const summary = { time: 12, population: 22, food: 300, births: 0, deaths: 0, generation: 0 };
const simulation = {
  id: 'simulation:test', name: 'A run', createdAt: 1, updatedAt: 2,
  state: new Uint8Array([1, 2, 3]), environmentName: 'Still water', seed: 42, paused: false, speed: 5,
  genomeNames: { 1: 'Forager' }, history: [{ ...summary, time: 1 }, { ...summary, time: 12 }], summary,
};
assert.equal(storage.validateSimulationRecord(simulation).speed, 5);
for (const patch of [{ speed: 4 }, { state: new ArrayBuffer(4) }, { state: new Uint8Array() }, { seed: -1 }, { paused: 1 }, { id: 'builtin:simulation:x' }, { history: Array(721).fill(summary) }, { history: [{ ...summary, time: 13 }] }, { history: [{ ...summary, time: 5 }, { ...summary, time: 4 }] }, { genomeNames: { 2049: 'Bad' } }, { genomeNames: JSON.parse('{"__proto__":"bad"}') }, { summary: { ...summary, population: 385 } }, { summary: { ...summary, time: Infinity } }]) {
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
