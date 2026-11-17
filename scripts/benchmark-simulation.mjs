import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { resolve } from 'node:path';
import { catalog, compile, seed, save, restore, error, snapshot } from './organelle-harness.mjs';

// Compare actual release WASM engines on identical worlds. Timings exclude
// setup, checkpoint comparison, JS snapshot parsing, and rendering.
const args = process.argv.slice(2);
let baselinePath, outputPath, scenarioName, samples = 5, ticks = 300;
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
const currentPath = new URL('../engine/pkg/agar_engine.wasm', import.meta.url);
const engines = [];
for (const [name, path] of baselinePath ? [['baseline', baselinePath], ['optimized', currentPath]] : [['current', currentPath]]) {
  const bytes = await readFile(path);
  engines.push({ name, module: await WebAssembly.compile(bytes), sha256: createHash('sha256').update(bytes).digest('hex') });
}
const { PRESETS, BUILTIN_ENVIRONMENTS } = await catalog();
const pond = BUILTIN_ENVIRONMENTS.find(item => item.id === 'builtin:environment:sunlit-pond').config;
const calm = { ambientTemperature: 0.5, ambientLight: 0.8, currentStrength: 0, initialNutrients: 1, foodRate: 0, sources: [] };
const scenarios = [
  { name: 'mixed-ecosystem', config: pond, food: 0, warmup: 1800, populations: [['Phototroph', 72], ['Forager', 6], ['Mixotroph', 4], ['Predator', 2]], mutationRate: 0.1 },
  { name: 'dense-producers', config: calm, food: 0, warmup: 900, populations: [['Phototroph', 400]] },
  { name: 'particle-heavy', config: calm, food: 10000, warmup: 600, populations: [['Forager', 80]] },
  { name: 'dynamic-fields', config: pond, food: 0, warmup: 300, populations: [] },
];
const chosen = scenarioName ? scenarios.filter(item => item.name === scenarioName) : scenarios;
assert.ok(chosen.length, `Unknown scenario ${scenarioName}`);
function advance(api, count) {
  for (let left = count; left > 0; left -= 120) {
    if (api.tick(Math.min(left, 120)) !== 1) throw new Error(error(api));
  }
}
async function create(engine, scenario) {
  const api = (await WebAssembly.instantiate(engine.module, {})).exports;
  const c = scenario.config;
  api.set_environment(c.ambientTemperature, c.ambientLight, c.currentStrength);
  api.set_initial_nutrients(c.initialNutrients);
  api.set_food_rate(c.foodRate);
  api.set_mutation_rate(scenario.mutationRate ?? 0);
  for (const source of c.sources) assert.ok(api.add_source(source.kind, source.x, source.y, source.strength, source.radius));
  assert.equal(api.reset_empty(42, scenario.food), 1, error(api));
  for (const [name, count] of scenario.populations) {
    const preset = PRESETS.find(item => item.name === name);
    assert.ok(preset, name);
    seed(api, compile(api, preset.source), count);
  }
  return api;
}
function sameBytes(a, b, label) {
  const left = Buffer.from(a), right = Buffer.from(b);
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
  const world = new Uint8Array(api.memory.buffer, p, n * 8).slice();
  const e = api.environment_snapshot_ptr(), m = api.environment_snapshot_len();
  return { world, environment: new Uint8Array(api.memory.buffer, e, m * 4).slice() };
}
const report = {
  runtime: process.version, runtimeFlags: process.execArgv, platform: process.platform, architecture: process.arch,
  seed: 42, measuredTicks: ticks, samples, warmupCheckInterval: 120,
  note: 'Release WASM in Node/V8. Tick and Rust snapshot export timings only; not browser rendering or JS object hydration. Medians of repeated identical trajectories, alternating engine order.',
  engines: engines.map(({ name, sha256 }) => ({ name, sha256 })), scenarios: [],
};
for (const scenario of chosen) {
  console.log(`Preparing ${scenario.name}...`);
  const apis = await Promise.all(engines.map(engine => create(engine, scenario)));
  // Compare complete checkpoints at sampled boundaries, including development,
  // rather than relying only on final population counts or resource totals.
  for (let left = scenario.warmup; left > 0; left -= 120) {
    for (const api of apis) advance(api, Math.min(left, 120));
    if (apis.length === 2) sameBytes(save(apis[0]), save(apis[1]), `${scenario.name} warmup ${scenario.warmup - Math.max(0, left - 120)}`);
  }
  const starting = apis.map(save);
  // Warm every branch used by the measured trajectory before recording times.
  // For reproducible V8 steady-state measurements, run with --no-liftoff.
  for (const api of apis) advance(api, ticks);
  if (apis.length === 2) sameBytes(save(apis[0]), save(apis[1]), `${scenario.name} trial`);
  const timings = engines.map(() => []), exports = engines.map(() => []);
  let finalState;
  for (let sample = 0; sample < samples; sample++) {
    const order = [...apis.keys()];
    if (sample % 2) order.reverse();
    const states = [], snapshots = [];
    for (const index of order) {
      const api = apis[index];
      assert.equal(restore(api, starting[index]), 1, error(api));
      const start = performance.now();
      advance(api, ticks);
      timings[index].push((performance.now() - start) / ticks);
      states[index] = save(api);
      snapshots[index] = rawSnapshot(api);
      // Read-only export cost is separate from ticks. No JS hydration here.
      const exportStart = performance.now();
      for (let n = 0; n < 30; n++) {
        api.snapshot_ptr();
        api.environment_snapshot_ptr();
      }
      exports[index].push((performance.now() - exportStart) / 30);
    }
    if (apis.length === 2) {
      sameBytes(states[0], states[1], `${scenario.name} sample ${sample + 1}`);
      sameBytes(snapshots[0].world, snapshots[1].world, `${scenario.name} world snapshot`);
      sameBytes(snapshots[0].environment, snapshots[1].environment, `${scenario.name} environment snapshot`);
    }
    if (sample === 0) finalState = snapshot(apis.at(-1));
  }
  const result = {
    name: scenario.name, warmupTicks: scenario.warmup, initialPopulations: scenario.populations, initialParticles: scenario.food,
    finalPopulation: finalState.population, finalParticles: finalState.food.length,
    births: finalState.births, deaths: finalState.deaths, mutations: finalState.mutations,
    checkpointIdentical: apis.length === 2, renderBuffersIdentical: apis.length === 2,
    measurements: engines.map((engine, i) => ({ engine: engine.name, tickMs: median(timings[i]), tickSamplesMs: timings[i], snapshotExportMs: median(exports[i]) })),
  };
  if (engines.length === 2) {
    result.speedup = result.measurements[0].tickMs / result.measurements[1].tickMs;
    result.tickTimeReductionPercent = 100 * (1 - result.measurements[1].tickMs / result.measurements[0].tickMs);
  }
  report.scenarios.push(result);
  console.log(JSON.stringify(result));
}
if (outputPath) await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`);
