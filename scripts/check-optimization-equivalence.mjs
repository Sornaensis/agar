import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { catalog, compile, seed, save, restore, snapshot, source, error } from './organelle-harness.mjs';

// Correctness only: no timing claims or altered physiology. This deliberately
// instantiates both modules directly; fresh() requires the current ABI.
let baselinePath, outputPath;
for (let i = 2; i < process.argv.length; i += 2) {
  const option = process.argv[i], value = process.argv[i + 1];
  assert.ok(value, `Missing value for ${option}`);
  if (option === '--baseline') baselinePath = resolve(value);
  else if (option === '--output') outputPath = resolve(value);
  else throw new Error(`Unknown option ${option}`);
}
assert.ok(baselinePath, 'Usage: node --no-liftoff scripts/check-optimization-equivalence.mjs --baseline /path/to/ABI12.wasm [--output report.json]');

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const descriptions = [];
for (const [name, path, abi] of [
  ['baseline', baselinePath, 12],
  ['current', new URL('../engine/pkg/agar_engine.wasm', import.meta.url), 13],
]) {
  const bytes = await readFile(path);
  descriptions.push({ name, abi, sha256: hash(bytes), module: await WebAssembly.compile(bytes) });
}
const { PRESETS } = await catalog();
const preset = name => {
  const value = PRESETS.find(item => item.name === name);
  assert.ok(value, `Missing shipped preset ${name}`);
  return value.source;
};

function bytesOf(value) {
  return Buffer.from(value.buffer, value.byteOffset, value.byteLength);
}
function sameBytes(left, right, label) {
  const a = bytesOf(left), b = bytesOf(right);
  if (a.equals(b)) return;
  let offset = 0;
  while (offset < Math.min(a.length, b.length) && a[offset] === b[offset]) offset++;
  // Save checksums occupy bytes 16..19. Also report the first payload mismatch.
  let payload = 20;
  while (payload < Math.min(a.length, b.length) && a[payload] === b[payload]) payload++;
  throw new Error(`${label}: first different byte ${offset}; first payload difference ${payload}; lengths ${a.length}/${b.length}`);
}
function projection(api, pointerName, lengthName, bytesPerValue) {
  const pointer = api[pointerName](), length = api[lengthName]();
  assert.ok(pointer && length, `${pointerName}: ${error(api)}`);
  return new Uint8Array(api.memory.buffer, pointer, length * bytesPerValue).slice();
}
let comparisons = 0;
function compare(pair, label) {
  const states = pair.map(save);
  for (const state of states) assert.equal(new DataView(state.buffer, state.byteOffset, state.byteLength).getUint32(8, true), 9);
  sameBytes(states[0], states[1], `${label}: canonical checkpoint`);
  for (const [pointer, length, size] of [
    ['snapshot_ptr', 'snapshot_len', 8],
    ['environment_snapshot_ptr', 'environment_snapshot_len', 4],
  ]) {
    sameBytes(projection(pair[0], pointer, length, size), projection(pair[1], pointer, length, size), `${label}: ${pointer}`);
  }
  comparisons++;
  return states[0];
}
async function instances() {
  const pair = [];
  for (const description of descriptions) {
    const api = (await WebAssembly.instantiate(description.module, {})).exports;
    assert.equal(api.abi_version(), description.abi);
    pair.push(api);
  }
  return pair;
}
function call(pair, label, method, args, expected = 1) {
  for (const api of pair) assert.equal(api[method](...args), expected, `${label}: ${error(api)}`);
  compare(pair, label);
}
function update(pair, label, method, args) {
  for (const api of pair) api[method](...args);
  compare(pair, label);
}
function add(pair, label, method, args) {
  const ids = pair.map(api => api[method](...args));
  assert.ok(ids[0] > 0 && ids[1] > 0, `${label}: ${pair.map(error).join('; ')}`);
  assert.equal(ids[0], ids[1], `${label}: identifiers`);
  compare(pair, label);
  return ids[0];
}
function advance(pair, ticks, label) {
  for (let remaining = ticks; remaining > 0;) {
    const steps = Math.min(remaining, 120);
    for (const api of pair) assert.equal(api.tick(steps), 1, `${label}: ${error(api)}`);
    remaining -= steps;
    compare(pair, `${label}, ${ticks - remaining}/${ticks} ticks`);
  }
}
async function initialized({ initialFood = 0, mutationRate = 0, current = 0 } = {}) {
  const pair = await instances();
  for (const api of pair) {
    assert.equal(api.set_world_size(1200, 760), 1, error(api));
    api.set_environment(0.5, 0.85, current);
    api.set_initial_nutrients(3);
    api.set_food_rate(0);
    api.set_mutation_rate(mutationRate);
    assert.equal(api.set_initial_gases(1, 1), 1, error(api));
    assert.equal(api.set_gas_exchange(1, 0.5, 1, 1), 1, error(api));
    assert.equal(api.reset_empty(42, initialFood), 1, error(api));
  }
  compare(pair, 'initial habitat');
  return pair;
}
function introduce(pair, name, count, x, y) {
  const ids = pair.map(api => compile(api, preset(name)));
  assert.equal(ids[0], ids[1], `${name}: compiled identifier`);
  for (let i = 0; i < pair.length; i++) seed(pair[i], ids[i], count, x, y);
  compare(pair, `introduce ${count} ${name}`);
  return ids[0];
}
async function restoreAndContinue(pair, label, ticks = 240) {
  const checkpoint = compare(pair, `${label}: before restore`);
  const resumed = await instances();
  // Exercise both directions; checkpoint 9 is shared by ABI12 and ABI13.
  assert.equal(restore(resumed[0], save(pair[1])), 1, error(resumed[0]));
  assert.equal(restore(resumed[1], save(pair[0])), 1, error(resumed[1]));
  sameBytes(compare(resumed, `${label}: restored`), checkpoint, `${label}: restore roundtrip`);
  for (let remaining = ticks; remaining > 0;) {
    const steps = Math.min(remaining, 120);
    advance(pair, steps, `${label}: uninterrupted`);
    advance(resumed, steps, `${label}: resumed`);
    sameBytes(save(pair[0]), save(resumed[0]), `${label}: baseline continuation`);
    sameBytes(save(pair[1]), save(resumed[1]), `${label}: current continuation`);
    // Include all grid values and floating objects in the continuation check.
    for (let i = 0; i < pair.length; i++) sameBytes(
      projection(pair[i], 'environment_snapshot_ptr', 'environment_snapshot_len', 4),
      projection(resumed[i], 'environment_snapshot_ptr', 'environment_snapshot_len', 4),
      `${label}: engine ${i} restored fields`);
    remaining -= steps;
  }
  return { checkpointBytes: checkpoint.length, checkpointSha256: hash(checkpoint), continuationTicks: ticks };
}

// The same supplied habitat as the established cellular lifecycle fixture.
// Near fission, advance one tick at a time to compare the actual birth boundary.
const phototroph = await initialized({ mutationRate: 1 });
introduce(phototroph, 'Phototroph', 1, 600, 380);
let state = snapshot(phototroph[0]);
while (!state.births && state.ticks < 150 * 30) {
  const parent = state.cells.find(cell => cell.id === 1);
  assert.ok(parent, 'Phototroph survives the established supplied lifecycle fixture');
  const steps = Math.min(parent.area >= 1.9 ? 1 : 30, 120 - state.ticks % 120, 150 * 30 - state.ticks);
  for (const api of phototroph) assert.equal(api.tick(steps), 1, error(api));
  state = snapshot(phototroph[0]);
  if (state.ticks % 120 === 0 || state.births) compare(phototroph, `Phototroph tick ${state.ticks}`);
}
assert.equal(state.births, 1, 'Phototroph completes its paid reproductive cycle within 150 seconds');
assert.equal(state.population, 2);
const daughter = state.cells.find(cell => cell.generation === 1);
assert.ok(daughter && daughter.parentId === 1, 'An actual daughter inherits the paid machinery');
assert.equal(state.mutations, 1, 'Mutation rate 1 exercises a new program at the first birth');
assert.notEqual(daughter.genome, 1, 'The daughter carries the automatically mutated program');
assert.equal(source(phototroph[0], daughter.genome), source(phototroph[1], daughter.genome));
const division = { tick: state.ticks, seconds: state.seconds, daughterId: daughter.id,
  daughterGenome: daughter.genome, mutations: state.mutations,
  restore: await restoreAndContinue(phototroph, 'post-division inherited and mutated state') };

// A separate modest scene exercises cache invalidation under runtime edits.
const mixed = await initialized({ initialFood: 48, current: 3 });
introduce(mixed, 'Phototroph', 4, 580, 380);
introduce(mixed, 'Forager', 2, 640, 420);
introduce(mixed, 'Mixotroph', 1, 700, 350);
const light = add(mixed, 'light emitter', 'add_source', [0, 240, 240, 0.4, 180]);
const flow = add(mixed, 'current band', 'add_source', [6, 650, 380, 0.35, 140]);
call(mixed, 'current-band geometry', 'update_source', [flow, 6, 650, 380, 0.35, 140, 0.12, 900]);
add(mixed, 'nutrient absorber', 'add_source', [4, 980, 500, 0.3, 100]);
advance(mixed, 900, 'mixed development');
call(mixed, 'change flow kind, position and sign', 'update_source', [flow, 2, -25, 780, -0.65, 250, -0.7, 1100]);
advance(mixed, 120, 'edited vortex');
call(mixed, 'change light emitter into cooler', 'update_source', [light, 1, 320, 260, -0.15, 180, 0.5, 700]);
const moving = add(mixed, 'fixed barrier', 'add_barrier', [350, 300, 180, 24, 0.45, 0]);
const fixed = add(mixed, 'floating barrier', 'add_barrier', [900, 500, 140, 18, -0.3, 1]);
advance(mixed, 120, 'fixed and floating barriers');
call(mixed, 'enlarge occupied world', 'set_world_size', [1680, 1040]);
advance(mixed, 120, 'enlarged world');
call(mixed, 'make first barrier floating', 'update_barrier', [moving, 490, 410, 220, 30, 0.8, 1]);
call(mixed, 'fix and rotate second barrier', 'update_barrier', [fixed, 1200, 720, 160, 20, -0.4, 0]);
call(mixed, 'seal gas boundary', 'set_gas_exchange', [0, 0.02, 0.7, 0.9]);
add(mixed, 'oxygen absorber', 'add_source', [9, 1000, 500, 0.4, 170]);
add(mixed, 'inorganic-carbon emitter', 'add_source', [10, 660, 420, 0.25, 180]);
advance(mixed, 120, 'sealed gas scene');
update(mixed, 'change ambient fields and background flow', 'set_environment', [0.6, 0.65, 8]);
call(mixed, 'remove vortex', 'remove_source', [flow]);
call(mixed, 'remove fixed barrier', 'remove_barrier', [fixed]);
advance(mixed, 120, 'remaining floating barrier');
call(mixed, 'shrink occupied world', 'set_world_size', [1000, 640]);
advance(mixed, 120, 'resized scalar and gas fields');

// Baseline clear/add/update and the new atomic path must normalize to the same
// records, identifiers and subsequent state, including scalar-ABI f32 rounding.
const sources = [
  [5, -28.123456789, 648.25, -0.6, 80, 0.4, 280],
  [6, 600, 250.5, 0.2, 75, -1.2, 700],
  [4, 780.1, 440.2, 0.3, 110, 0, 600],
  [12, 450, 300, 0.2, 120, 0, 600],
];
mixed[0].clear_sources();
for (const [kind, x, y, strength, radius, angle, length] of sources) {
  const id = mixed[0].add_source(kind, x, y, strength, radius);
  assert.ok(id, error(mixed[0]));
  assert.equal(mixed[0].update_source(id, kind, x, y, strength, radius, angle, length), 1, error(mixed[0]));
}
const length = sources.length * 56, pointer = mixed[1].alloc(length);
assert.ok(pointer);
try {
  const view = new DataView(mixed[1].memory.buffer, pointer, length);
  sources.flat().forEach((value, index) => view.setFloat64(index * 8, value, true));
  assert.equal(mixed[1].replace_sources(pointer, sources.length), 1, error(mixed[1]));
} finally { mixed[1].dealloc(pointer, length); }
compare(mixed, 'atomic source replacement versus scalar normalization');
call(mixed, 'reopen gas boundary with new equilibria', 'set_gas_exchange', [1, 0.02, 1.1, 0.8]);
advance(mixed, 120, 'reconfigured currents and aerator');
const edits = { finalTick: snapshot(mixed[0]).ticks,
  restore: await restoreAndContinue(mixed, 'runtime edits and floating-barrier state') };

const report = { passed: true, checkpointVersion: 9,
  engines: descriptions.map(({ name, abi, sha256 }) => ({ name, abi, sha256 })),
  comparisons, division, edits,
  compared: ['canonical checkpoint bytes', 'full cell/particle snapshot bytes', 'all environmental field and object bytes'],
  excludes: ['runtime genome allocation diagnostics', 'timing claims'] };
if (outputPath) await writeFile(outputPath, JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report, null, 2));
