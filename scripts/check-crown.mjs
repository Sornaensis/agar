import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { advance, body, catalog, compile, fresh, restore, save, seed, snapshot } from './organelle-harness.mjs';
import { crownObservation, sampleCrown } from './crown-observation.mjs';

const { BUILTIN_SIMULATIONS, BUILTIN_ENVIRONMENTS, BUILTIN_GENOMES, simulationStartSettings } = await catalog();
const template = BUILTIN_SIMULATIONS.find(s => s.id.endsWith(':crown-rosette'));
const {config} = BUILTIN_ENVIRONMENTS.find(e => e.id === template.environmentId);
const source = BUILTIN_GENOMES.find(g => g.id === template.populations[0].genomeId).source;
const disabled = source.replace(/(assembly Bus: junction\(signal\) \{[^}]*?activity: )1;/, (_, rule) => rule + '0;');
assert.notEqual(source, disabled);
for (const [label, program] of [['open',source],['closed',disabled]]) {
  const api=await fresh({...config,...simulationStartSettings(config),seed:template.seed});
  seed(api,compile(api,program),1);
  let state=snapshot(api);
  assert.equal(state.population,1); assert.equal(state.food.length,0);
  assert.equal(body(api,1).cellular.assemblies.length,0);
  const observation=crownObservation();
  while(state.seconds<360 && !observation.firstSpecialization){
    state=advance(api,60);
    sampleCrown(api,state,observation);
  }
  assert.ok(state.births>=2 && observation.maximumAttachedNeighbors>=2,'One founder paid for a multi-contact colony');
  assert.equal(Boolean(observation.firstSpecialization), label==='open');
  if(label==='closed'){
    assert.equal(observation.maximumSignalPartners,0);
    assert.equal(observation.peakActiveRimHarvest,0);
    assert.ok(state.cells.some(cell=>body(api,cell.id).cellular.assemblies.some(a=>a.name==='Bus'&&a.complete&&a.activity===0)));
  } else {
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
assert.equal(body(previous,1).colony.neighbors,1);
advance(previous,60);
const ptr=previous.snapshot_ptr();
assert.equal(createHash('sha256').update(new Uint8Array(previous.memory.buffer,ptr,previous.snapshot_len()*8)).digest('hex'),
  '5dfb4e3b6058197c4d650df6251a05bef9b29a31cee7dc302224c18fea8e60a3','An attached v11 family advances identically with its old genome');
console.log('Crown rosette and v11 attached-colony replay passed.');
