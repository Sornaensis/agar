import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { advance, body, catalog, compile, fresh, restore, save, seed, snapshot } from './organelle-harness.mjs';

// Every founder, particle and prey position is drawn by the engine. The two
// trials exercise different carbon supplies in the same finite 400 × 400 world.
const { PRESETS } = await catalog();
const predator = PRESETS.find(preset => preset.name === 'Predator').source;
const neutralPrey = 'thrust=0;turn=0;divide=false;feed=0;absorb=0;';
const pond = {
  width:400, height:400, ambientLight:0.85, ambientTemperature:0.5, currentStrength:0, initialNutrients:3, mutationRate:0,
  gases:{mode:'open',initialOxygen:1,initialCarbon:1,exchangeRate:0.5,oxygenEquilibrium:1,carbonEquilibrium:1},
};
const countChemistry = !process.argv.includes('--continuous');
const cases = [['revised', predator]];
if (process.argv.includes('--compare')) {
  const prior = await readFile(new URL('../experiments/fixtures/predator-before-curation.cell', import.meta.url), 'utf8');
  cases.unshift(['prior', prior]);
}

const results = [];
const failures = [];
for (const [version, source] of cases) {
  for (const habitat of ['dark carrion', 'lit live prey']) {
    const withPrey = habitat === 'lit live prey';
    for (const randomSeed of [42, 7, 2026]) {
      const api = await fresh({...pond, seed:randomSeed, countChemistry});
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
      let firstDivision = null, peakSoluble = 0, peakStructural = 0, state;
      for (let seconds = 1; seconds <= 300; seconds++) {
        state = advance(api, 30);
        for (const cell of state.cells.filter(c => c.genome === hunterGenome)) {
          if (cell.generation > 0) firstDivision ??= seconds;
          peakSoluble = Math.max(peakSoluble, cell.gutEnergyByKind[0]);
          peakStructural = Math.max(peakStructural, cell.gutEnergyByKind[1]);
        }
      }
      const living = state.cells.filter(c => c.genome === hunterGenome).length;
      results.push({ version, habitat, seed: randomSeed, living, births: state.births, firstDivision,
        kills: state.predations, captured: state.eaten, peakSoluble: +peakSoluble.toFixed(2),
        peakStructural: +peakStructural.toFixed(2) });
      assert.ok(Math.abs(state.material - state.materialIntroduced) < 1e-7);
      assert.ok(state.energy <= state.energyIntroduced + state.photosynthesis + 1e-7);
      assert.equal(state.photosynthesis,initialFixation);
      if (version === 'prior') continue;
      if (!living || firstDivision === null) {
        failures.push(`${habitat}, seed ${randomSeed}: ${living} surviving hunters, first division ${firstDivision ?? 'never'}`);
      }
      if (withPrey) {
        assert.ok(state.attacks > 0 && state.predations > 0 && peakStructural > 0,
          'The hunting lineage must kill and ingest real prey tissue');
      } else {
        if (!state.births) assert.equal(state.attacks, 0, 'The sole hunter has no live target');
        assert.ok(initialDeaths>=64 && state.eaten>0 && peakStructural>0,'The dark scavenging lineage must capture real carcass tissue');
      }
    }
  }
}
console.table(results);


// With the founder gone, every subsequent kill before the next fission belongs
// to its daughter. Both generations feed on the same finite prey population.
const api = await fresh({...pond, seed:42, countChemistry:true});
const hunter = compile(api,predator);
const prey = compile(api,neutralPrey);
seed(api,prey,120);
seed(api,hunter,1);
let state = snapshot(api);
const founder = state.cells.find(c=>c.genome===hunter).id;
let daughter, born;
while (!daughter && state.seconds < 300) {
  state = advance(api,30);
  daughter = state.cells.find(c=>c.parentId===founder);
}
assert.ok(daughter, 'A fed hunter must produce a daughter');
const firstSplit = save(api);
// Mounted growth machinery follows its attachment at cleavage. Both
// copies may therefore be inherited by the same half of the cell.
const envelopes = [founder, daughter.id].flatMap(id =>
  body(api,id).cellular.assemblies.filter(a => a.name === 'Envelope' && a.complete));
assert.ok(envelopes.length >= 2 && new Set(envelopes.map(a => a.id)).size === envelopes.length,
  'The two halves inherit distinct, paid instances of envelope growth machinery');
assert.ok(daughter.capacity > 65 && daughter.mineralCapacity > 0 && body(api,daughter.id).assemblyCapacity > 0,
  'The daughter needs funded ATP, mineral storage, and translation to build its own body');
const founderKills = state.predations;
const daughterId = daughter.id;
assert.equal(api.set_cell_stores(founder,1,0,0),1);
let peakStructural = 0;
while (!born && state.seconds < 600) {
  state = advance(api,30);
  daughter = state.cells.find(c=>c.id===daughterId);
  assert.ok(daughter,'The isolated hunter daughter must survive through her first fission');
  peakStructural = Math.max(peakStructural,daughter.gutEnergyByKind[1]);
  born = state.cells.find(c=>c.parentId===daughterId);
}
assert.ok(born && state.predations > founderKills && peakStructural > 1,
  'The hunter daughter must kill live prey, ingest its tissue, and produce a granddaughter');

// Keep both halves in this branch. A serial chain of births can hide a
// packed original parent that hunts indefinitely but never grows again.
assert.equal(restore(api,firstSplit),1);
let repeatBirth;
while (!repeatBirth && (state = advance(api,30)).seconds < 600) {
  repeatBirth = state.cells.find(c => c.parentId === founder && c.id !== daughterId);
}
if (!repeatBirth) failures.push('The original fed parent did not produce a second daughter within 600 seconds');
assert.deepEqual(failures, [], 'Predator lineage outcomes');
