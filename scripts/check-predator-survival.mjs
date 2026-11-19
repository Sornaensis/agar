import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { advance, catalog, compile, fresh, seed } from './organelle-harness.mjs';

// Starts each trial through the same habitat, food and mutation settings as the browser.
const { BUILTIN_ENVIRONMENTS, BUILTIN_GENOMES, simulationStartSettings } = await catalog();
const water = BUILTIN_ENVIRONMENTS.find(item => item.id === 'builtin:environment:still-water').config;
const args = process.argv.slice(2);
const compare = args.includes('--compare');
const files = args.filter(arg => arg !== '--compare');
assert.ok(files.length <= 1 && files.every(path => !path.startsWith('--')), 'Usage: check-predator-survival.mjs [--compare] [candidate.cell]');
const candidate = files.length ? await readFile(files[0], 'utf8')
  : BUILTIN_GENOMES.find(item => item.id === 'builtin:genome:predator').source;

async function runTrials(source, label, config) {
  const outcomes = [];
  for (const randomSeed of [42, 7, 2026]) {
    const api = await fresh({ ...config, ...simulationStartSettings(config), seed: randomSeed });
    seed(api, compile(api, source), 1);
    let ticks = 0;
    for (const seconds of [300, 600]) {
      const state = advance(api, seconds * 30 - ticks);
      ticks = state.ticks;
      outcomes.push({ genome: label, size: `${config.width}×${config.height}`, particlesPerSecond: config.foodRate, seed: randomSeed, seconds,
        living: state.population, births: state.births, captured: state.eaten, particlesInWater: state.food.length });
      assert.equal(state.photosynthesis, 0, 'The dark-water trial is entirely food-powered');
      if (state.mutations === 0) assert.equal(state.predations, 0, 'Clones bearing the founder marker spare one another');
    }
  }
  console.table(outcomes);
  return outcomes.filter(row => row.seconds === 600);
}

if (compare) {
  const prior = await readFile(new URL('../experiments/fixtures/predator-before-curation.cell', import.meta.url), 'utf8');
  const historicalWater = { ...water, width: 1200, height: 760 };
  if (historicalWater.width !== water.width || historicalWater.height !== water.height) {
    await runTrials(prior, 'Former', historicalWater);
    await runTrials(candidate, 'Revised', historicalWater);
    const matchedRate = Math.round(water.foodRate * historicalWater.width * historicalWater.height / (water.width * water.height));
    const largeFed = await runTrials(candidate, 'Revised', { ...historicalWater, foodRate: matchedRate });
    assert.ok(largeFed.every(row => row.living > 0 && row.births > 0),
      'The revised lineage also reproduces in the large habitat at comparable food input per area');
  }
  await runTrials(prior, 'Former', water);
}
const endpoints = await runTrials(candidate, files.length ? 'Candidate' : 'Revised', water);
assert.ok(endpoints.every(row => row.living > 0 && row.captured > 0),
  'Each founder lineage must feed and survive ten minutes in the supplied habitat');
assert.ok(endpoints.every(row => row.births > 0), 'Each founder lineage must reproduce from scattered food');
