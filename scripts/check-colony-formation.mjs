import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { advance, body, catalog, compile, error, fresh, restore, save, snapshot } from './organelle-harness.mjs';

const { BUILTIN_ENVIRONMENTS, PRESETS } = await catalog();
const pool = BUILTIN_ENVIRONMENTS.find(env => env.id.endsWith(':colony-pool')).config;
const nurse = PRESETS.find(genome => genome.name === 'Nurse rosette').source;
const recruit = nurse.replaceAll('engage_when: peer.division_interface;', 'engage_when: true;');
assert.notEqual(nurse, recruit);

const degree = (api, id) => body(api, id).colony.neighbors;
function hasSurface(api, id) {
  return body(api, id).cellular.assemblies.some(machine =>
    machine.kind === 'adhere()' && machine.complete && machine.functionalMass * machine.activity > 0);
}
function placeBeside(api) {
  const [a,b] = snapshot(api).cells;
  assert.equal(api.move_cell(b.id, a.x + a.radius + b.radius + 0.05, a.y, 0), 1, error(api));
}
async function growIndependent(first, second) {
  const api = await fresh({ ...pool, seed:42, mutationRate:0 });
  // The placement command preserves polarity. Measure the deterministic
  // founder headings, then author test patches facing the shared interface.
  const before = save(api);
  const probe = compile(api, 'cellular;');
  api.place_cell(probe,160,180); api.place_cell(probe,270,180);
  const [a,b] = snapshot(api).cells;
  assert.equal(restore(api,before),1,error(api));
  const oriented = (source, angle) => source.replace(/(assembly (?:Left|Right)Glue: adhere\(\) \{[^}]*?at: )[^;]+/g,
    (_,prefix)=>prefix + Math.atan2(Math.sin(angle),Math.cos(angle)));
  assert.equal(api.place_cell(compile(api, oriented(first,-a.angle)),160,180),1,error(api));
  assert.equal(api.place_cell(compile(api, oriented(second,Math.PI-b.angle)),270,180),2,error(api));
  let state = snapshot(api);
  while (state.seconds < 80 && !state.cells.every(cell => hasSurface(api, cell.id))) state = advance(api, 5);
  assert.equal(state.population,2);
  assert.ok(state.cells.every(cell => cell.parentId === 0 && hasSurface(api, cell.id)));
  placeBeside(api);
  return api;
}

for (const [label, first, second, expected] of [
  ['Independent Nurse founders', nurse, nurse, false],
  ['One recruitment offer', recruit, nurse, false],
  ['Two recruitment offers', recruit, recruit, true],
]) {
  const api = await growIndependent(first,second);
  let bondedTicks = 0, firstBond = null;
  for (let tick = 1; tick <= 60; tick++) {
    const { cells:[a,b] } = advance(api,1);
    assert.ok(Math.hypot(a.x-b.x,a.y-b.y) <= a.radius+b.radius+1, 'The founders remained in physical contact');
    const neighbors = degree(api,a.id);
    assert.equal(degree(api,b.id),neighbors);
    assert.equal(neighbors, expected && tick >= 15 ? 1 : 0);
    if (neighbors) { firstBond ??= tick; bondedTicks++; }
  }
  assert.equal(api.move_cell(1,120,120,1),expected ? 2 : 1,error(api));
  console.log(label,{firstBond,bondedTicks});
}

// An interrupted encounter cannot use contact time accrued before separation.
// Restore a pending pair and confirm that it completes on the same next tick.
const offered = await growIndependent(recruit,recruit);
advance(offered,10);
assert.equal(offered.move_cell(2,270,180,0),1,error(offered));
advance(offered,1);
placeBeside(offered);
advance(offered,14);
assert.equal(degree(offered,1),0);
const pending = save(offered), replay = await fresh();
assert.equal(restore(replay,pending),1,error(replay));
for (const api of [offered,replay]) {
  advance(api,1);
  assert.equal(degree(api,1),1);
}
assert.deepEqual(save(offered),save(replay));
console.log('Interrupted recruitment restarts; checkpointed recruitment resumes on the same tick.');

// Earlier versions recorded bonds between two-part surfaces. Preserve the
// organisms' real inventories and genome sources, without installing new glue.
for (const [file,version] of [['independent-founders-v14.bin',14],['family-v14.bin',14],['unprocessed-adhesion-v15.bin',15]]) {
  const api = await fresh();
  const old = await readFile(new URL('../engine/fixtures/' + file,import.meta.url));
  assert.equal(old.readUInt32LE(8),version);
  assert.equal(restore(api,old),1,error(api));
  for (const id of [1,2]) {
    assert.equal(degree(api,id),0);
    const data=body(api,id);
    const oldGlue=data.cellular.assemblies.find(m=>m.kind==='adhere()' && m.mass>0);
    assert.ok(oldGlue && oldGlue.functionalMass===0,'Historical material remains present and inert');
    assert.ok(!data.cellular.products.some(p=>p.role==='adhesin' && p.mass>0));
    if (version===15) {
      const previousBody = [86.04656449973311,81.83648886430608][id-1];
      assert.ok(Math.abs(data.foodPotential.body-previousBody)<1e-8,'Migrating a bond preserves the paid body investment');
    }
  }
  const upgraded = save(api), copy = await fresh();
  assert.equal(new DataView(upgraded.buffer).getUint32(8,true),20);
  assert.equal(restore(copy,upgraded),1,error(copy));
  advance(api,1); advance(copy,1);
  assert.equal(degree(api,1),0);
  assert.deepEqual(save(api),save(copy));
}
console.log('Historic adhesive material and investment persist without granting unproduced glue or bonds.');
