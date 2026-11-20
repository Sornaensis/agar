import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { catalog, compile, error, fresh, module as currentModule, save, seed } from './organelle-harness.mjs';

// Replay the shipped basin from its real launch settings and founders. The
// resulting raw engine checkpoint can be passed to benchmark-simulation.mjs.
const [output, secondsArg = '1200', wasmPath] = process.argv.slice(2);
const seconds = Number(secondsArg);
assert.ok(output && Number.isSafeInteger(seconds) && seconds > 0,
  'Usage: node scripts/mature-basin-checkpoint.mjs OUTPUT [SECONDS] [WASM]');
const engine = wasmPath ? await WebAssembly.compile(await readFile(wasmPath)) : currentModule;
const { BUILTIN_SIMULATIONS, BUILTIN_ENVIRONMENTS, BUILTIN_GENOMES, simulationStartSettings } = await catalog();
const basin = BUILTIN_SIMULATIONS.find(item => item.id === 'builtin:simulation:springwater-basin');
const habitat = BUILTIN_ENVIRONMENTS.find(item => item.id === basin.environmentId);
const api = await fresh({ ...habitat.config, ...simulationStartSettings(), seed: basin.seed }, engine);
for (const population of basin.populations) {
  const genome = BUILTIN_GENOMES.find(item => item.id === population.genomeId);
  seed(api, compile(api, genome.source), population.count);
}
for (let elapsed = 0; elapsed < seconds;) {
  const span = Math.min(60, seconds - elapsed);
  for (let left = span * 30; left > 0; left -= 120) {
    if (!api.tick(Math.min(120, left))) throw new Error(error(api));
  }
  elapsed += span;
  const ptr = api.summary_snapshot_ptr();
  const summary = new Float64Array(api.memory.buffer, ptr, api.presentation_len());
  console.log(`${elapsed}s: ${summary[1]} cells, ${summary[2]} particles`);
}
await writeFile(output, save(api));
