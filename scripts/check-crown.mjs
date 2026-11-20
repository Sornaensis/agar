import { scenarioFixtures } from './scenario-fixtures.mjs';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { advance, body, catalog, compile, fresh, restore, save, seed, snapshot } from './organelle-harness.mjs';
import { crownObservation, sampleCrown } from './crown-observation.mjs';

const { BUILTIN_ENVIRONMENTS, BUILTIN_GENOMES, simulationStartSettings } = await catalog();
const template = scenarioFixtures.find(s => s.id.endsWith(':crown-rosette'));
const {config} = BUILTIN_ENVIRONMENTS.find(e => e.id === template.environmentId);
const source = BUILTIN_GENOMES.find(g => g.id === template.populations[0].genomeId).source;
const disabled = source.replace(/(assembly (?:Left|Right)Bus: junction\(signal\) \{[^}]*?activity: )1;/g, (_, rule) => rule + '0;');
assert.notEqual(source, disabled);
let openReleaseTime = 0;
const closedOutlet = source.replace(/(assembly RimOutlet: export\(carbon\) \{[^}]*?activity: )[^;]+/, (_,prefix)=>prefix+'0');
assert.notEqual(source,closedOutlet);
for (const [label, program] of [['open',source],['closed',disabled],['closed outlet',closedOutlet]]) {
  const api=await fresh({...config,...simulationStartSettings(config),mutationRate:0,seed:template.seed});
  seed(api,compile(api,program),1);
  let state=snapshot(api);
  assert.equal(state.population,1); assert.equal(state.food.length,0);
  assert.equal(body(api,1).cellular.assemblies.length,0);
  const observation=crownObservation();
  let builtClosedOutlet = false, peakSecretion = 0;
  while(state.seconds<360 && (!observation.firstSpecialization
      || (label==='open' && !observation.firstRimSecretion)
      || (label==='closed outlet' && (!builtClosedOutlet || state.seconds<openReleaseTime+30)))){
    state=advance(api,60);
    peakSecretion = Math.max(peakSecretion,...state.cells.map(cell=>cell.exudationRate));
    sampleCrown(api,state,observation);
    if (label==='closed outlet' && !builtClosedOutlet) builtClosedOutlet = state.cells.some(c =>
      body(api,c.id).cellular.assemblies.some(a=>a.name==='RimOutlet' && a.complete && a.activity===0));
  }
  assert.ok(state.births>=2 && observation.maximumAttachedNeighbors>=2,'One founder paid for a multi-contact colony');
  assert.equal(Boolean(observation.firstSpecialization), label!=='closed');
  assert.equal(Boolean(observation.firstRimSecretion), label==='open');
  if (label==='open') openReleaseTime=observation.firstRimSecretion.second;
  if (label==='closed outlet') assert.ok(builtClosedOutlet,'The rim completed the silenced outlet');
  if (label!=='open') assert.equal(peakSecretion,0,'The isolated colony needs a transmitting center and working outlet for active secretion');
  if(label==='closed'){
    assert.equal(observation.maximumOccupiedPorts,0);
    assert.equal(observation.peakActiveRimHarvest,0);
    assert.ok(state.cells.some(cell=>body(api,cell.id).cellular.assemblies.some(a=>a.name.endsWith('Bus')&&a.complete&&a.activity===0)));
  } else if (label==='open') {
    const copy=await fresh();
    assert.equal(restore(copy,save(api)),1);
    advance(copy,60); advance(api,60);
    assert.deepEqual(save(copy),save(api),'Junction sampling replays from the same checkpoint');
  }
  console.log(label,JSON.stringify(observation));
}
const previous=await fresh();
// ABI 15, seed 42, one uniformly scattered Pulse swimmer in Colony pool at tick 3150.
const legacy = await readFile(new URL('../engine/fixtures/attached-v11.bin',import.meta.url));
assert.equal(legacy.readUInt32LE(8),11);
assert.equal(restore(previous,legacy),1);
assert.equal(body(previous,1).colony.neighbors,0,'Historical adhesion contained no finished adhesin');
const copy=await fresh();
assert.equal(restore(copy,save(previous)),1);
advance(copy,60); advance(previous,60);
assert.deepEqual(save(copy),save(previous),'The archived source and inert hardware round-trip and continue deterministically');
console.log('Crown rosette and v11 colony migration passed.');
