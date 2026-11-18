import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { catalog, moduleUrl, compile, seed, snapshot, body, source, save, restore, error } from './organelle-harness.mjs';

// Migration and behavior equivalence are different checks. New optical hardware
// was previously assigned a zero field of view, so its corrected future is
// intentionally not compared with the old engine. Every imported physical state
// must nevertheless be preserved, and its new-format future must replay exactly.
const options = {
  baseline: resolve('../cellarium-local-control-baseline/engine/pkg/agar_engine.wasm'),
  output: resolve('experiments/local-control-migration.json'),
  checkpointTicks: 1200,
  continuationTicks: 60,
  phototrophTicks: 2700,
};
const names = {
  '--baseline': 'baseline', '--output': 'output',
  '--checkpoint-ticks': 'checkpointTicks', '--continuation-ticks': 'continuationTicks',
  '--phototroph-ticks': 'phototrophTicks',
};
const args = process.argv.slice(2);
for (let i = 0; i < args.length; i += 2) {
  const key = names[args[i]], value = args[i + 1];
  assert.ok(key && value, `Expected a known option and value, received ${args[i]}`);
  options[key] = key.endsWith('Ticks') ? Number(value) : resolve(value);
}
for (const [key, value] of Object.entries(options)) {
  if (key.endsWith('Ticks')) assert.ok(Number.isSafeInteger(value) && value >= 30 && value <= 18000 && value % 30 === 0, key);
}
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const [oldBytes, newBytes] = await Promise.all([
  readFile(options.baseline), readFile(new URL('../engine/pkg/agar_engine.wasm', import.meta.url)),
]);
assert.notEqual(hash(oldBytes), hash(newBytes), 'The baseline must be the preceding v6 engine');
const [oldModule, newModule] = await Promise.all([WebAssembly.compile(oldBytes), WebAssembly.compile(newBytes)]);
const { MECHANISM_PRESETS_V6 } = await catalog();
assert.equal(MECHANISM_PRESETS_V6.length, 8);
const oldPresetsPath = resolve(dirname(options.baseline), '../../src/presets.ts');
const { PRESETS: originalPresets } = await import(await moduleUrl(oldPresetsPath));
for (const preset of MECHANISM_PRESETS_V6) {
  assert.equal(preset.source, originalPresets.find(p => p.name === preset.name)?.source,
    `${preset.name}: the preserved v6 program must be byte-identical to the baseline source`);
}

async function create(module) {
  const api = (await WebAssembly.instantiate(module, {})).exports;
  assert.equal(api.abi_version(), 10);
  api.set_environment(0.5, 0.85, 0);
  api.set_initial_nutrients(3);
  api.set_food_rate(0);
  api.set_mutation_rate(0);
  assert.equal(api.reset_empty(42, 0), 1, error(api));
  return api;
}
function raw(api, pointer, length, Type) {
  const p = api[pointer]();
  return new Type(api.memory.buffer, p, api[length]()).slice();
}
function exactBytes(actual, expected, label) {
  const a = Buffer.from(actual.buffer, actual.byteOffset, actual.byteLength);
  const b = Buffer.from(expected.buffer, expected.byteOffset, expected.byteLength);
  if (a.equals(b)) return;
  let offset = 0;
  while (offset < Math.min(a.length, b.length) && a[offset] === b[offset]) offset++;
  const index = Math.floor(offset / actual.BYTES_PER_ELEMENT);
  throw new Error(`${label} differs at element ${index}: ${actual[index]} vs ${expected[index]} (${a.length}/${b.length} bytes)`);
}
const newDiagnostics = new Set(['startAllowed', 'recycleRequested', 'recycleRate']);
function physicalBody(value) {
  if (Array.isArray(value)) return value.map(physicalBody);
  if (!value || typeof value !== 'object') return value;
  return Object.fromEntries(Object.entries(value).filter(([key]) => !newDiagnostics.has(key))
    .map(([key, item]) => [key, physicalBody(item)]));
}
function compare(reference, candidate, label) {
  exactBytes(raw(candidate, 'snapshot_ptr', 'snapshot_len', Float64Array),
    raw(reference, 'snapshot_ptr', 'snapshot_len', Float64Array), `${label}: world`);
  exactBytes(raw(candidate, 'environment_snapshot_ptr', 'environment_snapshot_len', Float32Array),
    raw(reference, 'environment_snapshot_ptr', 'environment_snapshot_len', Float32Array), `${label}: environment`);
  const state = snapshot(reference);
  assert.deepEqual(snapshot(candidate), state, `${label}: decoded state and conserved totals`);
  for (const cell of state.cells) {
    assert.deepEqual(physicalBody(body(candidate, cell.id)), physicalBody(body(reference, cell.id)),
      `${label}: physical body ${cell.id}`);
  }
  return state;
}
function version(bytes) {
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(8, true);
}
function feed(api, state, name) {
  if (name === 'Phototroph') return;
  for (const cell of state.cells) assert.equal(api.add_food(cell.x, cell.y, 4), 4, error(api));
}
function tick(api, count) { assert.equal(api.tick(count), 1, error(api)); }

const migrations = [];
for (const preset of MECHANISM_PRESETS_V6) {
  const baseline = await create(oldModule);
  seed(baseline, compile(baseline, preset.source), 1, 600, 380);
  let state = snapshot(baseline);
  for (let elapsed = 0; elapsed < options.checkpointTicks; elapsed += 30) {
    feed(baseline, state, preset.name);
    tick(baseline, 30);
    state = snapshot(baseline);
  }
  assert.ok(state.population > 0, `${preset.name} survives the supplied migration fixture`);
  const machinery = state.cells.flatMap(cell => body(baseline, cell.id).cellular.assemblies);
  assert.ok(machinery.some(machine => machine.complete), `${preset.name} includes physically completed machinery`);
  const checkpoint = save(baseline);
  assert.equal(version(checkpoint), 6);
  const migrated = await create(newModule);
  assert.equal(restore(migrated, checkpoint), 1, error(migrated));
  compare(baseline, migrated, `${preset.name}: immediate v6 import`);
  for (const cell of state.cells) assert.equal(source(migrated, cell.genome), preset.source);
  const upgraded = save(migrated);
  assert.equal(version(upgraded), 7);
  const replay = await create(newModule);
  assert.equal(restore(replay, upgraded), 1, error(replay));
  assert.deepEqual(save(replay), upgraded, `${preset.name}: upgraded state restores exactly`);
  for (let elapsed = 0; elapsed < options.continuationTicks; elapsed += 30) {
    const current = snapshot(migrated);
    feed(migrated, current, preset.name);
    feed(replay, current, preset.name);
    tick(migrated, 30); tick(replay, 30);
    assert.deepEqual(save(replay), save(migrated), `${preset.name}: v7 continuation is byte-exact`);
  }
  migrations.push({
    name: preset.name, sourceSha256: hash(preset.source), preservedSourceMatchesBaseline: true,
    checkpointTicks: options.checkpointTicks, continuationTicks: options.continuationTicks,
    baselineCheckpointVersion: 6, currentCheckpointVersion: 7,
    completeMachinesAtMigration: machinery.filter(machine => machine.complete).length,
    partialMachinesAtMigration: machinery.filter(machine => !machine.complete).length,
    exactImportedWorldAndEnvironmentBytes: true, exactImportedPhysicalBodies: true,
    currentCheckpointAndContinuationExact: true,
  });
  console.log(`${preset.name}: exact v6 physical import; v7 replay ${options.continuationTicks} ticks.`);
}

// This source has no optical receptors, contact weaponry or directional shields.
// Its unchanged behavior gives a separate end-to-end equivalence fixture for the
// allocation, identity caches, reused vectors and work-phase summary changes.
const phototroph = MECHANISM_PRESETS_V6.find(preset => preset.name === 'Phototroph');
const baseline = await create(oldModule), current = await create(newModule);
for (const api of [baseline, current]) seed(api, compile(api, phototroph.source), 1, 600, 380);
let state = compare(baseline, current, 'Phototroph introduction'), comparisons = 1;
for (let elapsed = 0; elapsed < options.phototrophTicks; elapsed += 30) {
  tick(baseline, 30); tick(current, 30);
  state = compare(baseline, current, `Phototroph tick ${elapsed + 30}`);
  comparisons++;
}
assert.ok(state.births > 0, 'The unchanged Phototroph comparison must include division');
const report = {
  baselineWasmSha256: hash(oldBytes), currentWasmSha256: hash(newBytes),
  conditions: 'One founder; seed42; temperature0.5; light0.85; current0; initial minerals3/patch; mutation0. Four supplied particles per living cell each second for non-phototroph migration fixtures.',
  migrationScope: 'All eight preserved v6 mechanism programs: exact source, immediate physical state and environment after v6 import, then byte-exact v7 restore and continuation. Only newly added control-report diagnostics are omitted from cross-version body inspection comparison.',
  intentionalBehaviorChanges: 'Old/new future equivalence is not asserted for optical programs: v7 corrects their formerly zero optical viewing width. Directional surface interactions are also a deliberate new behavior.',
  migrations,
  unchangedPhototrophTrajectory: {
    sourceSha256: hash(phototroph.source), ticks: options.phototrophTicks, comparisons,
    births: state.births, deaths: state.deaths, finalPopulation: state.population,
    exactWorldAndEnvironmentBytes: true, exactPhysicalBodiesAndConservedTotals: true,
  },
};
await writeFile(options.output, `${JSON.stringify(report, null, 2)}\n`);
console.log(`Local control migration passed: eight exact v6 physical imports; v7 replay; ${comparisons} unchanged Phototroph samples including ${state.births} births.`);
