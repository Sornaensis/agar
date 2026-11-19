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
  for (const habitat of ['dark carrion', 'lit live prey']) {
    const withPrey = habitat === 'lit live prey';
    for (const randomSeed of [42, 7, 2026]) {
      const api = await fresh({
        seed: randomSeed, width: 400, height: 400, ambientLight: 0.85,
        ambientTemperature: 0.5, currentStrength: 0, initialNutrients: 3, mutationRate: 0,
        gases: { mode: 'open', initialOxygen: 1, initialCarbon: 1,
          exchangeRate: 0.5, oxygenEquilibrium: 1, carbonEquilibrium: 1 },
      });
      const hunterGenome = compile(api, source);
      let initialDeaths=0, initialFixation=0;
      if (!withPrey) {
        // Grow a scattered producer cohort and let heat stress generate real
        // carcasses. The hunter starts after cooling, in darkness.
        const producer=compile(api,PRESETS.find(p=>p.name==='Phototroph').source);
        seed(api,producer,64);
        let bloom=advance(api,60*30); assert.ok(bloom.photosynthesis>0);
        api.set_environment(1,.85,0);
        while(bloom.population && bloom.seconds<150) bloom=advance(api,30);
        assert.equal(bloom.population,0);assert.ok(bloom.food.some(f=>f.kind===1));
        initialFixation=bloom.photosynthesis; initialDeaths=bloom.deaths;
        api.set_environment(.25,0,0);
      }
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
      assert.equal(state.photosynthesis,initialFixation);
      if (version === 'prior') continue;
      assert.ok(living > 0 && state.births > 0, `${habitat}, seed ${randomSeed}: the lineage must survive and reproduce`);
      if (withPrey) {
        assert.ok(state.attacks > 0 && state.predations > 0 && peakTissue > 1,
          'The hunting lineage must kill and ingest real prey tissue');
      } else {
        assert.equal(state.attacks, 0, 'Hunters spare their relatives');
        assert.ok(initialDeaths>=64 && state.eaten>0 && peakTissue>1,'The dark scavenging lineage must capture real carcass tissue');
      }
    }
  }
}
console.table(results);
