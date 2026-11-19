import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { advance, catalog, compile, fresh, seed } from './organelle-harness.mjs';

// Every founder, particle and prey position is drawn by the engine. The two
// trials exercise different carbon supplies in the same finite 400 × 400 world.
const { PRESETS } = await catalog();
const predator = PRESETS.find(preset => preset.name === 'Predator').source;
const neutralPrey = 'thrust=0;turn=0;divide=false;feed=0;absorb=0;';
const cases = [['revised', predator]];
if (process.argv.includes('--compare')) {
  const prior = await readFile(new URL('../experiments/fixtures/predator-before-curation.cell', import.meta.url), 'utf8');
  cases.unshift(['prior', prior]);
}

const results = [];
for (const [version, source] of cases) {
  for (const habitat of ['dark exudate', 'lit live prey']) {
    const withPrey = habitat === 'lit live prey';
    for (const randomSeed of [42, 7, 2026]) {
      const api = await fresh({
        seed: randomSeed, width: 400, height: 400, ambientLight: withPrey ? 0.85 : 0,
        ambientTemperature: 0.5, currentStrength: 0, initialNutrients: 3, mutationRate: 0,
        initialFood: withPrey ? 0 : 64, foodRate: withPrey ? 0 : 4,
        gases: { mode: 'open', initialOxygen: 1, initialCarbon: 1,
          exchangeRate: 0.5, oxygenEquilibrium: 1, carbonEquilibrium: 1 },
      });
      const hunterGenome = compile(api, source);
      seed(api, hunterGenome, 1);
      if (withPrey) seed(api, compile(api, neutralPrey), 20);
      let firstDivision = null, peakExudate = 0, peakTissue = 0, state;
      for (let seconds = 1; seconds <= 300; seconds++) {
        state = advance(api, 30);
        if (state.births && firstDivision === null) firstDivision = seconds;
        for (const cell of state.cells.filter(c => c.genome === hunterGenome)) {
          peakExudate = Math.max(peakExudate, cell.gutEnergyByKind[0]);
          peakTissue = Math.max(peakTissue, cell.gutEnergyByKind[1]);
        }
      }
      const living = state.cells.filter(c => c.genome === hunterGenome).length;
      results.push({ version, habitat, seed: randomSeed, living, births: state.births, firstDivision,
        kills: state.predations, captured: state.eaten, peakExudate: +peakExudate.toFixed(2),
        peakTissue: +peakTissue.toFixed(2) });
      assert.ok(Math.abs(state.material - state.materialIntroduced) < 1e-7);
      assert.ok(state.energy <= state.energyIntroduced + state.photosynthesis + 1e-7);
      assert.equal(state.photosynthesis, 0);
      if (version === 'prior') continue;
      assert.ok(living > 0 && state.births > 0, `${habitat}, seed ${randomSeed}: the lineage must survive and reproduce`);
      if (withPrey) {
        assert.ok(state.attacks > 0 && state.predations > 0 && peakTissue > 1,
          'The hunting lineage must kill and ingest real prey tissue');
      } else {
        assert.equal(state.attacks, 0, 'Hunters spare their relatives');
        assert.ok(state.eaten > 0 && peakExudate > 1, 'The lineage must capture scattered exudate');
      }
    }
  }
}
console.table(results);
