import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { catalog, compile, seed, snapshot, body, save, restore, error, legacyEnvironmentProjection } from './organelle-harness.mjs';

// Compare actual old and new WASM on preserved cellular-family sources. Save
// formats differ (v5/v6), so compare exact world/environment bytes and complete
// body inspection, then migrate the old save and compare subsequent dynamics.
const args = process.argv.slice(2);
let baselinePath, outputPath, checkpointTicks = 900, continuationTicks = 1800;
for (let i = 0; i < args.length; i += 2) {
  const value = args[i + 1];
  assert.ok(value, `Missing value for ${args[i]}`);
  switch (args[i]) {
    case '--baseline': baselinePath = resolve(value); break;
    case '--output': outputPath = resolve(value); break;
    case '--checkpoint-ticks': checkpointTicks = Number(value); break;
    case '--continuation-ticks': continuationTicks = Number(value); break;
    default: throw new Error(`Unknown option ${args[i]}`);
  }
}
assert.ok(baselinePath, 'Pass --baseline /path/to/pre-mechanism/engine/pkg/agar_engine.wasm');
for (const ticks of [checkpointTicks, continuationTicks]) {
  assert.ok(Number.isSafeInteger(ticks) && ticks >= 30 && ticks <= 18000 && ticks % 30 === 0);
}
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const [oldBytes, newBytes] = await Promise.all([
  readFile(baselinePath), readFile(new URL('../engine/pkg/agar_engine.wasm', import.meta.url)),
]);
assert.notEqual(hash(oldBytes), hash(newBytes), 'The baseline must be the previous engine, not a copy of the current WASM');
const [oldModule, newModule] = await Promise.all([WebAssembly.compile(oldBytes), WebAssembly.compile(newBytes)]);
const { CELLULAR_FAMILY_PRESETS } = await catalog();
assert.equal(CELLULAR_FAMILY_PRESETS.length, 8);

async function create(module) {
  const api = (await WebAssembly.instantiate(module, {})).exports;
  assert.ok([10, 11].includes(api.abi_version()), 'Known historical or current ABI');
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
function compare(reference, candidate, label) {
  exactBytes(raw(candidate, 'snapshot_ptr', 'snapshot_len', Float64Array),
    raw(reference, 'snapshot_ptr', 'snapshot_len', Float64Array), `${label}: world`);
  exactBytes(legacyEnvironmentProjection(candidate), legacyEnvironmentProjection(reference), `${label}: environment`);
  const state = snapshot(reference);
  assert.deepEqual(snapshot(candidate), state, `${label}: decoded state and conserved totals`);
  for (const cell of state.cells) {
    assert.deepEqual(body(candidate, cell.id), body(reference, cell.id), `${label}: complete body ${cell.id}`);
  }
  return state;
}
function version(bytes) {
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(8, true);
}

const scenarios = [];
for (const preset of CELLULAR_FAMILY_PRESETS) {
  const baseline = await create(oldModule), current = await create(newModule);
  for (const api of [baseline, current]) seed(api, compile(api, preset.source), 1, 600, 380);
  compare(baseline, current, `${preset.name} introduction`);
  let migrated, migratedReplay, oldVersion, newVersion, comparisons = 1;
  let state = snapshot(baseline);
  const end = checkpointTicks + continuationTicks;
  for (let tick = 0; tick < end; tick += 30) {
    const apis = [baseline, current, migrated, migratedReplay].filter(Boolean);
    if (preset.name !== 'Phototroph') {
      // Identical externally supplied food, based only on the reference world.
      for (const cell of state.cells) {
        for (const api of apis) assert.equal(api.add_food(cell.x, cell.y, 4), 4, error(api));
      }
    }
    for (const api of apis) assert.equal(api.tick(30), 1, error(api));
    state = compare(baseline, current, `${preset.name} tick ${tick + 30}`);
    comparisons++;
    if (migrated) {
      compare(baseline, migrated, `${preset.name} migrated tick ${tick + 30}`);
      assert.deepEqual(save(migratedReplay), save(migrated), 'Migrated state resumes with byte-exact v6 checkpoints');
    }
    if (tick + 30 === checkpointTicks) {
      const checkpoint = save(baseline);
      oldVersion = version(checkpoint);
      assert.equal(oldVersion, 5, 'This regression targets the v5 cellular-family engine');
      migrated = await create(newModule);
      assert.equal(restore(migrated, checkpoint), 1, error(migrated));
      compare(baseline, migrated, `${preset.name} imported checkpoint`);
      const upgraded = save(migrated);
      newVersion = version(upgraded);
      assert.equal(newVersion, 6);
      migratedReplay = await create(newModule);
      assert.equal(restore(migratedReplay, upgraded), 1, error(migratedReplay));
      assert.deepEqual(save(migratedReplay), upgraded, 'Migration produces an exactly restorable current checkpoint');
    }
  }
  assert.ok(state.population > 0, `${preset.name} remains represented at the end of the comparison`);
  const result = { name: preset.name, sourceSha256: hash(preset.source), checkpointTicks,
    continuationTicks, comparisons, baselineCheckpointVersion: oldVersion, currentCheckpointVersion: newVersion,
    finalPopulation: state.population, births: state.births, deaths: state.deaths,
    exactWorldAndEnvironmentBytes: true, exactBodyInspection: true,
    oldCheckpointMigrationAndFuture: true, currentCheckpointReplay: true };
  scenarios.push(result);
  console.log(`${preset.name}: ${comparisons} exact old/new states; v${oldVersion}→v${newVersion} migration + ${continuationTicks} continuation ticks; ${state.births} births.`);
}
const report = {
  baselineWasmSha256: hash(oldBytes), currentWasmSha256: hash(newBytes),
  scope: 'Preserved v5 cellular-family presets, exact physical projections and body inspections; no whole-checkpoint comparison across differing save formats.',
  conditions: 'One founder; seed42; temperature0.5; light0.85; current0; initial minerals3/patch; mutation0. Four supplied particles per living cell each second for non-phototrophs.',
  scenarios,
};
if (outputPath) await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`);
console.log('Cellular compatibility passed: eight old programs retain exact physical trajectories and v5 saves continue under v6.');
