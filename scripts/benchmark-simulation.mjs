import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { resolve } from 'node:path';
import { catalog, compile, seed, save, restore, error, snapshot, STRIDE } from './organelle-harness.mjs';

// Compare release WASM engines on identical worlds. Timers exclude setup,
// checkpoint comparison, JS hydration, worker transfers and browser rendering.
const args = process.argv.slice(2);
let baselinePath, outputPath, scenarioName, samples = 3, ticks = 60;
for (let i = 0; i < args.length; i += 2) {
  const value = args[i + 1];
  assert.ok(value, `Missing value for ${args[i]}`);
  switch (args[i]) {
    case '--baseline': baselinePath = resolve(value); break;
    case '--output': outputPath = resolve(value); break;
    case '--scenario': scenarioName = value; break;
    case '--samples': samples = Number(value); break;
    case '--ticks': ticks = Number(value); break;
    default: throw new Error(`Unknown option ${args[i]}`);
  }
}
assert.ok(Number.isSafeInteger(samples) && samples >= 1 && samples <= 25);
assert.ok(Number.isSafeInteger(ticks) && ticks >= 1 && ticks <= 18000);
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const currentPath = new URL('../engine/pkg/agar_engine.wasm', import.meta.url);
const engines = [];
for (const [name, path] of baselinePath ? [['baseline', baselinePath], ['optimized', currentPath]] : [['current', currentPath]]) {
  const bytes = await readFile(path);
  engines.push({ name, module: await WebAssembly.compile(bytes), sha256: sha256(bytes) });
}
const { PRESETS, BUILTIN_ENVIRONMENTS } = await catalog();
const pond = BUILTIN_ENVIRONMENTS.find(item => item.id === 'builtin:environment:sunlit-pond').config;
const calm = { ambientTemperature: 0.5, ambientLight: 0.8, currentStrength: 0, initialNutrients: 1, sources: [] };
const gasCalm = { ...calm, ambientLight: 0.85, initialNutrients: 3,
  gases: { initialOxygen: 1, initialCarbon: 1, mode: 'open', exchangeRate: 0.02, oxygenEquilibrium: 1, carbonEquilibrium: 1 } };
const fieldConfig = (current, floating) => ({ ...gasCalm, width: 4800, height: 3040,
  ambientLight: 0.6, initialNutrients: 0.3, currentStrength: current,
  sources: [
    { kind: 6, x: 2400, y: 1368, strength: 0.5, radius: 547.2, angle: 0.12, length: 4800 },
    { kind: 2, x: 3360, y: 1672, strength: 0.4, radius: 668.8 },
    { kind: 1, x: 1200, y: 912, strength: 0.4, radius: 864 },
    { kind: 3, x: 1440, y: 1216, strength: 0.6, radius: 100 },
    { kind: 4, x: 3600, y: 2128, strength: 0.4, radius: 100 },
    { kind: 8, x: 1920, y: 1064, strength: 0.5, radius: 100 },
    { kind: 10, x: 2880, y: 1976, strength: 0.5, radius: 100 },
  ], barriers: Array.from({ length: floating === null ? 0 : 3 }, (_, i) => ({
    x: 4800 * (0.25 + 0.22 * i), y: 3040 * (0.35 + 0.1 * i), length: 180, width: 30,
    angle: 0.2 + i * 0.35, floating,
  })),
});
const dormantSource = Array.from({ length: 512 }, (_, i) =>
  `let dormant${i} = sin(energy + ${i}.0) + cos(age + ${i}.0);`).join('\n')
  + '\nthrust=0;turn=0;divide=false;feed=0;absorb=0;';
const scenarios = [
  { name: 'mixed-84', config: pond, warmup: 900, populations: [['Phototroph', 72], ['Forager', 6], ['Mixotroph', 4], ['Predator', 2]] },
  { name: 'mixed-floating-barriers', config: { ...pond, barriers: [
    { x: 350, y: 280, length: 180, width: 30, angle: 0.2, floating: true },
    { x: 700, y: 380, length: 180, width: 30, angle: 0.55, floating: true },
    { x: 960, y: 480, length: 180, width: 30, angle: 0.9, floating: false },
  ] }, warmup: 600, populations: [['Phototroph', 72], ['Forager', 6], ['Mixotroph', 4], ['Predator', 2]], mutationRate: 0.1 },
  { name: 'producers-1000-default', config: gasCalm, warmup: 600, populations: [['Phototroph', 1000]] },
  { name: 'producers-1000-large', config: { ...gasCalm, width: 4800, height: 3040 }, warmup: 600, populations: [['Phototroph', 1000]] },
  { name: 'foragers-1000-large', config: { ...gasCalm, width: 4800, height: 3040 }, tissueTarget: 20000, warmup: 600, populations: [['Forager', 1000]] },
  { name: 'tissue-20000-default', config: { ...gasCalm, currentStrength: 8 }, tissueTarget: 20000, warmup: 10, populations: [] },
  { name: 'tissue-100000-large', config: { ...gasCalm, width: 4800, height: 3040, currentStrength: 8 }, tissueTarget: 100000, warmup: 10, populations: [] },
  { name: 'maximum-fields-current8', config: fieldConfig(8, null), warmup: 30, populations: [] },
  { name: 'maximum-fields-current8-fixed3', config: fieldConfig(8, false), warmup: 30, populations: [] },
  { name: 'maximum-fields-current8-floating3', config: fieldConfig(8, true), warmup: 30, populations: [] },
  // Optional synthetic stress: 512 unused dynamic bindings, not a preset forecast.
  { name: 'dormant-graph', optional: true, config: calm, warmup: 90, populations: [['Dormant graph', 80]], source: dormantSource },
];
const aliases = { 'particles-20000-default': 'tissue-20000-default', 'particles-100000-large': 'tissue-100000-large' };
const requested = scenarioName?.split(',').map(name => aliases[name] ?? name);
if (requested) for (const name of requested) assert.ok(scenarios.some(item => item.name === name), `Unknown scenario ${name}`);
const chosen = scenarios.filter(item => requested ? requested.includes(item.name) : !item.optional);
function advance(api, count) {
  for (let left = count; left > 0; left -= 120) {
    if (api.tick(Math.min(left, 120)) !== 1) throw new Error(error(api));
  }
}
async function create(engine, scenario) {
  const api = (await WebAssembly.instantiate(engine.module, {})).exports;
  assert.equal(api.abi_version(), 22, `${engine.name}: this benchmark requires a compatible ABI22 engine`);
  const c = scenario.config;
  assert.equal(api.set_world_size(c.width ?? 1200, c.height ?? 760), 1, error(api));
  api.set_environment(c.ambientTemperature, c.ambientLight, c.currentStrength);
  api.set_initial_nutrients(c.initialNutrients);
  api.set_mutation_rate(scenario.mutationRate ?? 0);
  const gases = c.gases ?? {};
  assert.equal(api.set_initial_gases(scenario.tissueTarget ? 0 : gases.initialOxygen ?? 1, gases.initialCarbon ?? 1), 1, error(api));
  const setBoundary = open => assert.equal(api.set_gas_exchange(Number(open), gases.exchangeRate ?? 0.02,
    gases.oxygenEquilibrium ?? 1, gases.carbonEquilibrium ?? 1), 1, error(api));
  setBoundary(!scenario.tissueTarget && gases.mode === 'open');
  for (const source of c.sources ?? []) {
    assert.ok(api.add_source_configured(source.kind, source.x, source.y, source.strength, source.radius,
      source.angle ?? 0, source.length ?? 600), error(api));
  }
  for (const barrier of c.barriers ?? []) assert.ok(api.add_barrier(barrier.x, barrier.y,
    barrier.length, barrier.width, barrier.angle, Number(barrier.floating)), error(api));
  assert.equal(api.reset_empty(42), 1, error(api));
  assert.equal(api.set_count_chemistry(1), 1, error(api));
  const donorFounders = scenario.tissueTarget ? produceTissue(api, scenario.tissueTarget) : 0;
  if (donorFounders) setBoundary(gases.mode === 'open');
  for (const [name, count] of scenario.populations) {
    const source = scenario.source ?? PRESETS.find(item => item.name === name)?.source;
    assert.ok(source, name);
    seed(api, compile(api, source), count);
  }
  return { api, donorFounders };
}

// The particle load comes from ordinary founder lysis in temporarily sealed,
// anoxic water. Restore the scene's surface boundary before living populations
// are introduced. No setup work is included in measured tick/export times.
function produceTissue(api, target) {
  const id = compile(api, 'thrust=0;turn=0;divide=false;absorb=0;feed=0;');
  const donorBudget = Math.ceil(target / 4);
  let donors = 0, particles = 0;
  while (particles < target && donors < donorBudget) {
    const batch = Math.min(512, donorBudget - donors, Math.max(16, Math.ceil((target - particles) / 4)));
    seed(api, id, batch);
    const ptr = api.render_snapshot_ptr();
    const rows = new Float64Array(api.memory.buffer, ptr + 12 * 8, batch * 21);
    const ids = Array.from({ length: batch }, (_, i) => rows[i * 21]);
    for (const cell of ids) assert.equal(api.set_cell_stores(cell, 1, 0.0001, 0), 1, error(api));
    advance(api, 1);
    donors += batch;
    const summary = projection(api, 'summary_snapshot_ptr');
    assert.equal(summary[1], 0, 'Every donor must undergo ordinary lysis');
    assert.equal(summary[4], donors);
    particles = summary[2];
  }
  assert.ok(particles >= target, `${donors} donors produced ${particles} tissue packets, below the ${target} workload`);
  return donors;
}
function bytesOf(value) {
  return value instanceof ArrayBuffer ? Buffer.from(value) : Buffer.from(value.buffer, value.byteOffset, value.byteLength);
}
function sameBytes(a, b, label) {
  const left = bytesOf(a), right = bytesOf(b);
  if (!left.equals(right)) {
    let at = 0;
    while (at < Math.min(left.length, right.length) && left[at] === right[at]) at++;
    throw new Error(`${label}: state differs at byte ${at} (${left.length} vs ${right.length} bytes)`);
  }
}
function median(values) {
  const ordered = [...values].sort((a, b) => a - b);
  const mid = Math.floor(ordered.length / 2);
  return ordered.length % 2 ? ordered[mid] : (ordered[mid - 1] + ordered[mid]) / 2;
}
function rawSnapshot(api) {
  const p = api.snapshot_ptr(), n = api.snapshot_len();
  const world = new Float64Array(api.memory.buffer, p, n).slice();
  const e = api.environment_snapshot_ptr(), m = api.environment_snapshot_len();
  return { world, environment: new Float32Array(api.memory.buffer, e, m).slice() };
}
function projection(api, method, ...args) {
  const pointer = api[method](...args), length = api.presentation_len();
  return new Float64Array(api.memory.buffer, pointer, length).slice();
}
function expectedRender(api, full) {
  let generation = 0;
  const values = [full[0], full[1], full[2], full[3], full[4], full[5], full[6], full[7], full[8], full[11], api.mutation_rate(), 0];
  const offsets = [0, 1, 2, 3, 4, 5, 6, 11, 7, 15, 24, 25, 19, 20, 81, 83, 94, 95, 47, 89, 9];
  for (let i = 0; i < full[2]; i++) {
    const at = 12 + i * STRIDE;
    for (const offset of offsets) values.push(full[at + offset]);
    generation = Math.max(generation, full[at + 9]);
  }
  values[11] = generation;
  for (let i = 12 + full[2] * STRIDE; i < full.length; i++) values.push(full[i]);
  return new Float64Array(values);
}
function expectedMetrics(api, full) {
  let generation = 0;
  const trophic = [0, 0, 0, 0], flow = [0, 0, 0], organic = [0, 0, 0], groups = new Map();
  for (let i = 0; i < full[2]; i++) {
    const at = 12 + i * STRIDE, id = full[at + 1], age = full[at + 9];
    const photo = full[at + 39 + 1 * 6 + 5] > 0, digestive = full[at + 39 + 3 * 6 + 5] > 0;
    if (photo || digestive) trophic[photo && digestive ? 2 : photo ? 0 : 1]++;
    if (full[at + 39 + 8 * 6 + 5] > 0) trophic[3]++;
    flow[0] += full[at + 20]; flow[1] += full[at + 27]; flow[2] += full[at + 31];
    generation = Math.max(generation, age);
    const group = groups.get(id) ?? [0, 0]; group[0]++; group[1] = Math.max(group[1], age); groups.set(id, group);
  }
  let packetEnergy = -0;
  for (let at = 12 + full[2] * STRIDE; at < full.length; at += 5) {
    organic[full[at + 4]]++;
    packetEnergy += full[at + 2];
  }
  return new Float64Array([
    full[0], full[1], full[2], full[3], full[4], full[5], full[6], generation,
    api.mutation_rate(), api.mutation_count(), api.mutation_skipped_count(), api.genome_count(), api.genome_limit(), api.genome_bytes(), api.genome_byte_limit(),
    api.predation_count(), api.attack_count(), api.photosynthesis_total(), api.energy_total(), api.energy_input_total(),
    api.nutrient_total(), api.nutrient_input_total(), api.nutrient_dissolved_total(), api.nutrient_food_total(), api.nutrient_cell_total(),
    ...trophic, ...flow, ...organic, groups.size,
    ...projection(api, 'metrics_snapshot_ptr').slice(36, 51),
    api.energy_withdrawn_total(), api.mineral_withdrawn_total(), packetEnergy,
    ...[...groups].sort((a, b) => a[0] - b[0]).flatMap(([id, group]) => [id, ...group]),
  ]);
}
function validateProjections(api, referenceApi, full, label) {
  if (!api.render_snapshot_ptr) return null;
  const render = projection(api, 'render_snapshot_ptr');
  const geometry = expectedRender(referenceApi, full);
  sameBytes(render.subarray(0, geometry.length), geometry, `${label} compact render`);
  const colony = render.subarray(geometry.length);
  sameBytes(colony, projection(referenceApi, 'render_snapshot_ptr').subarray(geometry.length), `${label} colony activity`);
  assert.equal(colony.length % 4, 0);
  let priorRow = -1;
  for (let i = 0; i < colony.length; i += 4) {
    const [row, neighbors, valid, payload] = colony.subarray(i, i + 4);
    assert.ok(Number.isInteger(row) && row > priorRow && row < full[2] && Number.isInteger(neighbors) && neighbors >= 0);
    assert.ok((valid === 0 && payload === 0 && neighbors > 0) || (valid === 1 && Number.isFinite(payload)));
    priorRow = row;
  }
  const metrics = projection(api, 'metrics_snapshot_ptr'), expected = expectedMetrics(referenceApi, full);
  const genomeBytes = { reference: expected[13], current: metrics[13] };
  assert.equal(metrics[13], api.genome_bytes());
  // This one diagnostic measures resident derived execution plans now. No
  // checkpoint, physical value, population count or other header is normalized.
  expected[13] = metrics[13];
  sameBytes(metrics, expected, `${label} metrics (genomeBytes accounting normalized)`);
  const selectedId = full[2] ? full[12] : null;
  if (selectedId !== null) sameBytes(projection(api, 'cell_snapshot_ptr', selectedId), full.slice(12, 12 + STRIDE), `${label} selected cell`);
  const summary = projection(api, 'summary_snapshot_ptr');
  sameBytes(summary, new Float64Array([full[1], full[2], full[3], full[4], full[5], expected[7],
    full[0]]), `${label} summary`);
  let metadataBytes;
  if (api.environment_metadata_ptr) {
    const full = rawSnapshot(api).environment;
    const ptr = api.environment_metadata_ptr();
    const metadata = new Float32Array(api.memory.buffer, ptr, api.environment_metadata_len()).slice();
    sameBytes(metadata, new Float32Array([...full.slice(0, 21), ...full.slice(21 + full[0] * full[1] * 7)]), `${label} metadata`);
    metadataBytes = metadata.byteLength;
  }
  const carbonError = metrics[36] + metrics[49] - metrics[37];
  const oxygenError = metrics[44] + metrics[45] + metrics[48] + metrics[41] + metrics[42] - metrics[40] - metrics[47];
  assert.ok(Math.abs(carbonError) < 1e-7 + metrics[37] * 1e-10, `${label} carbon conservation`);
  assert.ok(Math.abs(oxygenError) < 1e-7 + (metrics[47] + metrics[40]) * 1e-10, `${label} oxygen conservation`);
  return { selectedId, genomeBytes, payloadBytes: { compactRender: render.byteLength, metrics: metrics.byteLength,
    selectedCell: selectedId === null ? 0 : STRIDE * 8, summary: summary.byteLength, environmentMetadata: metadataBytes } };
}
function exportMeasurements(api, selectedId) {
  const operations = {
    legacyCombined: () => { api.snapshot_ptr(); api.environment_snapshot_ptr(); },
    legacyWorld: () => api.snapshot_ptr(),
    environment: () => api.environment_snapshot_ptr(),
  };
  if (api.render_snapshot_ptr) {
    operations.compactRender = () => api.render_snapshot_ptr();
    operations.metrics = () => api.metrics_snapshot_ptr();
    operations.summary = () => api.summary_snapshot_ptr();
    if (selectedId !== null) operations.selectedCell = () => api.cell_snapshot_ptr(selectedId);
  }
  if (api.environment_metadata_ptr) operations.environmentMetadata = () => api.environment_metadata_ptr();
  const result = {};
  for (const [name, operation] of Object.entries(operations)) {
    operation();
    const start = performance.now();
    for (let n = 0; n < 30; n++) operation();
    result[name] = (performance.now() - start) / 30;
  }
  return result;
}
const report = {
  runtime: process.version, runtimeFlags: process.execArgv, platform: process.platform, architecture: process.arch,
  abi: 22, resourceArithmetic: 'whole counts', seed: 42, measuredTicks: ticks, samples, scratchWarmTicks: 5, warmupCheckInterval: 120, exportCallsPerSample: 30,
  note: 'Release WASM in Node/V8. Tick and Rust export timings exclude JS hydration/copies, worker transfer/scheduling, main-thread rendering and setup. Medians of identical trajectories with alternating engine order.',
  tissuePreparation: 'Scattered ordinary founders start in anoxic water with a sealed surface. Their ATP is withdrawn to one ATP count before one upkeep tick per cohort. The scenario surface boundary is restored before seeding resident populations and warming the world.',
  equivalence: 'Canonical checkpoints and legacy world/environment buffers must match exactly. Compact projections match those legacy values; only metrics header[13] (genomeBytes) is normalized because derived execution-plan storage is now counted.',
  engines: engines.map(({ name, sha256 }) => ({ name, sha256 })), scenarios: [],
};
for (const scenario of chosen) {
  console.log(`Preparing ${scenario.name}...`);
  const setups = await Promise.all(engines.map(engine => create(engine, scenario)));
  const apis = setups.map(setup => setup.api);
  const initialParticles = projection(apis[0], 'summary_snapshot_ptr')[2];
  for (let left = scenario.warmup; left > 0; left -= 120) {
    for (const api of apis) advance(api, Math.min(left, 120));
    if (apis.length === 2) sameBytes(save(apis[0]), save(apis[1]), `${scenario.name} warmup ${scenario.warmup - Math.max(0, left - 120)}`);
  }
  const starting = apis.map(save);
  // Warm branches in the measured trajectory; --no-liftoff avoids tier changes.
  for (const api of apis) advance(api, 5 + ticks);
  if (apis.length === 2) sameBytes(save(apis[0]), save(apis[1]), `${scenario.name} trial`);
  const timings = engines.map(() => []), exportTimings = engines.map(() => []);
  const payloads = [], accounting = [];
  let finalState, finalCheckpoint, finalRaw, timedStartParticles;
  for (let sample = 0; sample < samples; sample++) {
    const order = [...apis.keys()];
    if (sample % 2) order.reverse();
    const states = [], snapshots = [];
    for (const index of order) {
      const api = apis[index];
      assert.equal(restore(api, starting[index]), 1, error(api));
      advance(api, 5);
      timedStartParticles = projection(api, 'summary_snapshot_ptr')[2];
      const start = performance.now();
      advance(api, ticks);
      timings[index].push((performance.now() - start) / ticks);
      states[index] = save(api);
      snapshots[index] = rawSnapshot(api);
      exportTimings[index].push(exportMeasurements(api, snapshots[index].world[2] ? snapshots[index].world[12] : null));
      sameBytes(states[index], save(api), `${scenario.name} read-only exports`);
    }
    if (apis.length === 2) {
      sameBytes(states[0], states[1], `${scenario.name} sample ${sample + 1}`);
      sameBytes(snapshots[0].world, snapshots[1].world, `${scenario.name} legacy world snapshot`);
      sameBytes(snapshots[0].environment, snapshots[1].environment, `${scenario.name} environment snapshot`);
    }
    for (const [index, api] of apis.entries()) {
      const checked = validateProjections(api, apis[0], snapshots[0].world, `${scenario.name} sample ${sample + 1}`);
      payloads[index] = { legacyWorld: snapshots[index].world.byteLength, environment: snapshots[index].environment.byteLength, ...checked?.payloadBytes };
      accounting[index] = checked?.genomeBytes ?? { reference: api.genome_bytes(), current: api.genome_bytes() };
      sameBytes(states[index], save(api), `${scenario.name} projection validation is read-only`);
    }
    if (sample === 0) {
      finalState = snapshot(apis.at(-1)); finalCheckpoint = states.at(-1); finalRaw = snapshots.at(-1);
    }
  }
  const result = {
    name: scenario.name, width: scenario.config.width ?? 1200, height: scenario.config.height ?? 760,
    synthetic: scenario.optional ?? false, warmupTicks: scenario.warmup, initialPopulations: scenario.populations, initialParticles,
    tissueTarget: scenario.tissueTarget ?? 0, donorFounders: setups[0].donorFounders, timedStartParticles,
    finalPopulation: finalState.population, finalParticles: finalState.food.length,
    births: finalState.births, deaths: finalState.deaths, mutations: finalState.mutations,
    checkpointIdentical: apis.length === 2, legacyBuffersIdentical: apis.length === 2, compactProjectionsValidated: apis.some(api => api.render_snapshot_ptr),
    checkpointBytes: finalCheckpoint.byteLength, checkpointSha256: sha256(finalCheckpoint),
    legacyWorldSha256: sha256(bytesOf(finalRaw.world)), environmentSha256: sha256(bytesOf(finalRaw.environment)),
    measurements: engines.map((engine, i) => {
      const exports = Object.fromEntries(Object.keys(exportTimings[i][0]).map(name => [name, median(exportTimings[i].map(sample => sample[name]))]));
      return { engine: engine.name, tickMs: median(timings[i]), tickSamplesMs: timings[i], snapshotExportMs: exports.legacyCombined, exportMs: exports, exportSamplesMs: exportTimings[i], payloadBytes: payloads[i], genomeBytes: accounting[i].current, wasmMemoryBytes: apis[i].memory.buffer.byteLength };
    }),
  };
  if (engines.length === 2) {
    result.speedup = result.measurements[0].tickMs / result.measurements[1].tickMs;
    result.tickTimeReductionPercent = 100 * (1 - result.measurements[1].tickMs / result.measurements[0].tickMs);
  }
  report.scenarios.push(result);
  if (outputPath) await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ name: result.name, donorFounders: result.donorFounders, initialParticles, timedStartParticles, finalParticles: result.finalParticles, checkpointIdentical: result.checkpointIdentical, measurements: result.measurements.map(({ engine, tickMs, exportMs, payloadBytes }) => ({ engine, tickMs, exportMs, payloadBytes })), speedup: result.speedup }));
}
if (outputPath) await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`);
