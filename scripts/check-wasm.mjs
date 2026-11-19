import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { checkEngineLoader } from './check-engine-loader.mjs';
import { fresh,compile,seed,snapshot,advance,save,restore,error,source,graph,body,memory,evaluation,json,catalog,kinds } from './organelle-harness.mjs';
const {LEGACY_PRESETS:PRESETS}=await catalog();
const idle='thrust=0;turn=0;divide=false;absorb=0;feed=0;';
const producer=PRESETS.find(p=>p.name==='Phototroph').source;
const api=await fresh({ambientLight:.8,initialNutrients:3});
const id=compile(api,producer);seed(api,id,22);
const original=save(api);const before=snapshot(api);assert.equal(before.population,22);
advance(api,90);const after=save(api);assert.notDeepEqual(after,original);
assert.equal(restore(api,original),1);advance(api,90);assert.deepEqual(save(api),after,'Seeded state, bodies, environment, memory and random sequences replay');
const stable=save(api);
for(const source_ of ['thrust=true;turn=0;divide=false;','thrust=food.angle;turn=0;divide=false;',idle+'target.flagellum.count=.5;',idle+'scan.target=5;']){
  const b=new TextEncoder().encode(source_),p=api.alloc(b.length);new Uint8Array(api.memory.buffer,p,b.length).set(b);assert.equal(api.compile_genome(p,b.length),0);assert.ok(error(api));api.dealloc(p,b.length);assert.deepEqual(save(api),stable);
}
const schema=json(api,'language_schema_ptr','language_schema_len');assert.equal(schema.version,3);assert.equal(schema.signals.length,174);assert.equal(schema.outputs.length,81);
const cell=snapshot(api).cells[0];const b=body(api,cell.id);assert.ok(b.organs.length>0);assert.equal(b.requested.length,16);
assert.equal(evaluation(api,cell.id).signals.length,174);assert.equal(evaluation(api,cell.id).outputs.length,81);assert.deepEqual(save(api),stable,'Inspection does not advance or commit');
const empty=await fresh();const idleId=compile(empty,idle);seed(empty,idleId,1);
const blank=snapshot(empty).cells[0];assert.equal(blank.organs.photosynthetic.count,0);assert.equal(blank.organs.flagellum.count,0);
const noSensors=evaluation(empty,blank.id);for(const name of ['light.available','heat.available','chemical.available','flow.available','contact.available','object.available'])assert.equal(noSensors.signals.find(s=>s.name===name).value,false,name);
const controls=compile(empty,idle+'target.flagellum.count=2;target.flagellum.size=.05;build.flagellum=0;');seed(empty,controls,1);advance(empty,120);
assert.ok(snapshot(empty).cells.every(c=>c.organs.flagellum.count===0),'Targets alone supply no hardware');

// Source-based mutations edit the inherited source, preserve comments, and
// recompile every generated variant against the new mixed output types.
const mutationSource='# retain this comment\nlet target = 2;\nri001 = ri001 + 1;\n'+idle+'target.flagellum.count=target;target.flagellum.size=.05;build.flagellum=.5;scan.target=rs001;';
const mutationApi=await fresh();const parent=compile(mutationApi,mutationSource);
for(const kind of [1,2,3])for(let n=1;n<=60;n++){
  const next=mutationApi.mutate_genome(parent,n,kind);assert.ok(next);const changed=source(mutationApi,next);assert.ok(changed.startsWith('# retain this comment\n'));
  assert.ok(!changed.includes('let g0'),'Mutations never serialize a new generated program');compile(mutationApi,changed);assert.equal(graph(mutationApi,next).roots.filter(r=>r.kind==='output').length,81);
  assert.equal(mutationApi.mutate_genome(parent,n,kind),next,'Mutation replay retains provenance and IDs');
}
assert.equal(source(mutationApi,parent),mutationSource);

const registers=await fresh();let registerSource=idle;
for(let i=1;i<=999;i++){const n=String(i).padStart(3,'0');registerSource+=`ri${n}=0;rf${n}=0.0;rb${n}=false;rs${n}="";rv${n}=vec(0,0);`;}
const registerId=compile(registers,registerSource);seed(registers,registerId,1);advance(registers,1);assert.equal(memory(registers,1).length,4995);
const registerSave=save(registers),registerCopy=await fresh();assert.equal(restore(registerCopy,registerSave),1);assert.deepEqual(save(registerCopy),registerSave);
advance(registerCopy,3);advance(registers,3);assert.deepEqual(save(registerCopy),save(registers));

const large=await fresh();const largeId=compile(large,idle);seed(large,largeId,2049);assert.equal(snapshot(large).population,2049);
const largeSave=save(large);const copied=await fresh();assert.equal(restore(copied,largeSave),1);assert.equal(snapshot(copied).population,2049);assert.deepEqual(save(copied),largeSave);
for(const count of [-1,.5,NaN,Infinity,0xffffffff]){assert.equal(large.seed_cells(largeId,count,-1,-1),0);assert.ok(error(large));assert.deepEqual(save(large),largeSave);}
assert.equal(large.seed_cells.length,4,'Introduction accepts only genome, count and position');
for(const file of ['trophic-v1.bin','trophic-v2.bin']){const bytes=new Uint8Array(await readFile(new URL('../engine/fixtures/'+file,import.meta.url)));assert.equal(restore(large,bytes),0);assert.match(error(large),/Unsupported simulation version.*preserved/);assert.deepEqual(save(large),largeSave);}
const invalid=largeSave.slice();invalid[invalid.length-1]^=1;assert.equal(restore(large,invalid),0);assert.deepEqual(save(large),largeSave);

// Existing organelle-model saves keep their actual grown structures. Removing
// body selection changes introductions, not the bodies already in a checkpoint.
const previous=await fresh();
const previousSave=new Uint8Array(await readFile(new URL('../engine/fixtures/organelle-v3.bin',import.meta.url)));

assert.equal(restore(previous,previousSave),1,error(previous));
const migrated=save(previous);assert.equal(new DataView(migrated.buffer,migrated.byteOffset,migrated.byteLength).getUint32(8,true),17);
assert.equal(source(previous,1),JSON.parse(await readFile(new URL('../engine/fixtures/organelle-v3.json',import.meta.url),'utf8')).source);
const priorCell=snapshot(previous).cells[0];
assert.ok(priorCell.organs.photosynthetic.mass>0 && priorCell.organs.digestive.mass>0,'Restoring an existing cell never reconstructs it from the minimal founder');
assert.ok(Object.values(priorCell.proteins).every(mass=>mass===0),'Legacy bodies receive no free assembly proteins');
assert.ok(snapshot(previous).food.every(p=>p.kind===1),'Unclassified legacy organic matter is explicitly imported as tissue');
const migratedCopy=await fresh();assert.equal(restore(migratedCopy,migrated),1,error(migratedCopy));
advance(previous,120);advance(migratedCopy,120);assert.deepEqual(save(previous),save(migratedCopy),'Imported v3 state resumes deterministically under the new model');

// Real authored growth and repeat fission; no arbitrary biological population cap.
const cycle=await fresh({ambientLight:.85,initialNutrients:5});const cycleId=compile(cycle,producer);seed(cycle,cycleId,1);
const material=cycle.nutrient_total();let state;
for(let n=0;n<25;n++){state=advance(cycle,120);assert.ok(Math.abs(state.material-material)<1e-7);assert.ok(state.energy<=state.energyIntroduced+state.photosynthesis+1e-7);}
assert.ok(state.births>0,'A producer completes a funded physical growth/division cycle');
assert.ok(state.cells.some(c=>c.generation>=1));
const saved=save(cycle),resumed=await fresh();assert.equal(restore(resumed,saved),1);advance(cycle,120);advance(resumed,120);assert.deepEqual(save(cycle),save(resumed));
for(const cell of state.cells)assert.ok(kinds.every(k=>cell.organs[k].mass>=0));

// Particle counts and age are outcomes of the physical simulation, not quotas.
const particles=await fresh({width:2400,height:1520,gases:{initialOxygen:0}});
assert.equal(snapshot(particles).food.length,0);
assert.equal('add_food' in particles,false);
assert.equal('set_food_rate' in particles,false);
// Ordinary founders run out of ATP at the next upkeep in dark, anoxic
// water. Their existing tissue and reserve carbon supply every packet.
const donor=compile(particles,idle), donorCount=512;
seed(particles,donor,donorCount);
const founders=snapshot(particles);
for(const cell of founders.cells) assert.equal(particles.set_cell_stores(cell.id,1,1e-7,0),1,error(particles));
const debris=advance(particles,1);
assert.equal(debris.deaths,donorCount);
assert.equal(debris.population,0);
assert.ok(debris.food.length>=2101 && debris.food.every(f=>f.kind===1),'Ordinary lysis stores more than 2,100 tissue packets');
assert.ok(Math.abs(debris.material - founders.material)<1e-7,'Lysis preserves the original minerals');
const particleState=save(particles),particleCopy=await fresh();
assert.equal(restore(particleCopy,particleState),1,error(particleCopy));assert.deepEqual(save(particleCopy),particleState);
advance(particles,3);advance(particleCopy,3);assert.deepEqual(save(particleCopy),save(particles),'Large organic inventories replay identically');

const longLived=await fresh({ambientLight:.85,initialNutrients:5});
const stationaryProducer=producer.replace(/^divide\s*=\s*[^;]+;/m,'divide = false;').replace(/^thrust\s*=\s*[^;]+;/m,'thrust = 0;');
const longLivedId=compile(longLived,stationaryProducer);seed(longLived,longLivedId,1);
const older=advance(longLived,10800);assert.equal(older.population,1,'A funded, healthy cell survives beyond five minutes');
assert.ok(older.cells[0].age>359);assert.equal(older.cells[0].lifespan,0);
const olderState=save(longLived),olderCopy=await fresh();assert.equal(restore(olderCopy,olderState),1,error(olderCopy));
assert.deepEqual(save(olderCopy),olderState,'Older organisms remain valid saved state');
console.log(`WASM organelle integration passed: ABI21, typed schema, five register banks, original-source mutations, genome-grown bodies, closed material, checkpoint17 replay and v3/v4 compatibility, uncapped2,049-organism restore, ${state.births} producer births.`);
await checkEngineLoader();
