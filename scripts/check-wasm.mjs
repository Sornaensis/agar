import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { checkEngineLoader } from './check-engine-loader.mjs';
import { fresh,compile,seed,snapshot,advance,save,restore,error,source,graph,body,memory,evaluation,json,catalog,kinds } from './organelle-harness.mjs';
const {PRESETS}=await catalog();
const idle='thrust=0;turn=0;divide=false;absorb=0;feed=0;';
const producer=PRESETS.find(p=>p.name==='Phototroph').source;
const api=await fresh({ambientLight:.8,initialNutrients:3,initialFood:360,foodRate:2});
const id=compile(api,producer);seed(api,id,22,'producer');
const original=save(api);const before=snapshot(api);assert.equal(before.population,22);
advance(api,90);const after=save(api);assert.notDeepEqual(after,original);
assert.equal(restore(api,original),1);advance(api,90);assert.deepEqual(save(api),after,'Seeded state, bodies, environment, memory and random sequences replay');
const stable=save(api);
for(const source_ of ['thrust=true;turn=0;divide=false;','thrust=food.angle;turn=0;divide=false;',idle+'target.flagellum.count=.5;',idle+'scan.target=5;']){
  const b=new TextEncoder().encode(source_),p=api.alloc(b.length);new Uint8Array(api.memory.buffer,p,b.length).set(b);assert.equal(api.compile_genome(p,b.length),0);assert.ok(error(api));api.dealloc(p,b.length);assert.deepEqual(save(api),stable);
}
const schema=json(api,'language_schema_ptr','language_schema_len');assert.equal(schema.version,3);assert.equal(schema.signals.length,144);assert.equal(schema.outputs.length,78);
const cell=snapshot(api).cells[0];const b=body(api,cell.id);assert.ok(b.organs.length>0);assert.equal(b.requested.length,16);
assert.equal(evaluation(api,cell.id).signals.length,144);assert.equal(evaluation(api,cell.id).outputs.length,78);assert.deepEqual(save(api),stable,'Inspection does not advance or commit');
const empty=await fresh();const idleId=compile(empty,idle);seed(empty,idleId,1,'bare');
const blank=snapshot(empty).cells[0];assert.equal(blank.organs.photosynthetic.count,0);assert.equal(blank.organs.flagellum.count,0);
const noSensors=evaluation(empty,blank.id);for(const name of ['light.available','heat.available','chemical.available','flow.available','contact.available','object.available'])assert.equal(noSensors.signals.find(s=>s.name===name).value,false,name);
const controls=compile(empty,idle+'target.flagellum.count=2;target.flagellum.size=.05;build.flagellum=0;');seed(empty,controls,1,'bare');advance(empty,120);
assert.ok(snapshot(empty).cells.every(c=>c.organs.flagellum.count===0),'Targets alone supply no hardware');

// Source-based mutations edit the inherited source, preserve comments, and
// recompile every generated variant against the new mixed output types.
const mutationSource='# retain this comment\nlet target = 2;\nri001 = ri001 + 1;\n'+idle+'target.flagellum.count=target;target.flagellum.size=.05;build.flagellum=.5;scan.target=rs001;';
const mutationApi=await fresh();const parent=compile(mutationApi,mutationSource);
for(const kind of [1,2,3])for(let n=1;n<=60;n++){
  const next=mutationApi.mutate_genome(parent,n,kind);assert.ok(next);const changed=source(mutationApi,next);assert.ok(changed.startsWith('# retain this comment\n'));
  assert.ok(!changed.includes('let g0'),'Mutations never serialize a new generated program');compile(mutationApi,changed);assert.equal(graph(mutationApi,next).roots.filter(r=>r.kind==='output').length,78);
  assert.equal(mutationApi.mutate_genome(parent,n,kind),next,'Mutation replay retains provenance and IDs');
}
assert.equal(source(mutationApi,parent),mutationSource);

const registers=await fresh();let registerSource=idle;
for(let i=1;i<=999;i++){const n=String(i).padStart(3,'0');registerSource+=`ri${n}=0;rf${n}=0.0;rb${n}=false;rs${n}="";rv${n}=vec(0,0);`;}
const registerId=compile(registers,registerSource);seed(registers,registerId,1,'bare');advance(registers,1);assert.equal(memory(registers,1).length,4995);
const registerSave=save(registers),registerCopy=await fresh();assert.equal(restore(registerCopy,registerSave),1);assert.deepEqual(save(registerCopy),registerSave);
advance(registerCopy,3);advance(registers,3);assert.deepEqual(save(registerCopy),save(registers));

const large=await fresh();const largeId=compile(large,idle);seed(large,largeId,2049,'bare');assert.equal(snapshot(large).population,2049);
const largeSave=save(large);const copied=await fresh();assert.equal(restore(copied,largeSave),1);assert.equal(snapshot(copied).population,2049);assert.deepEqual(save(copied),largeSave);
for(const count of [-1,.5,NaN,Infinity,0xffffffff]){assert.equal(large.seed_cells(largeId,count,-1,-1,0),0);assert.ok(error(large));assert.deepEqual(save(large),largeSave);}
assert.equal(large.seed_cells(largeId,1,-1,-1,99),0);assert.deepEqual(save(large),largeSave);
for(const file of ['trophic-v1.bin','trophic-v2.bin']){const bytes=new Uint8Array(await readFile(new URL('../engine/fixtures/'+file,import.meta.url)));assert.equal(restore(large,bytes),0);assert.match(error(large),/Unsupported simulation version.*preserved/);assert.deepEqual(save(large),largeSave);}
const invalid=largeSave.slice();invalid[invalid.length-1]^=1;assert.equal(restore(large,invalid),0);assert.deepEqual(save(large),largeSave);

// Real authored growth and repeat fission; no arbitrary biological population cap.
const cycle=await fresh({ambientLight:.85,initialNutrients:5});const cycleId=compile(cycle,producer);seed(cycle,cycleId,1,'producer');
const material=cycle.nutrient_total();let state;
for(let n=0;n<25;n++){state=advance(cycle,120);assert.ok(Math.abs(state.material-material)<1e-7);assert.ok(state.energy<=state.energyIntroduced+state.photosynthesis+1e-7);}
assert.ok(state.births>0,'A producer completes a funded physical growth/division cycle');
assert.ok(state.cells.some(c=>c.generation>=1));
const saved=save(cycle),resumed=await fresh();assert.equal(restore(resumed,saved),1);advance(cycle,120);advance(resumed,120);assert.deepEqual(save(cycle),save(resumed));
for(const cell of state.cells)assert.ok(kinds.every(k=>cell.organs[k].mass>=0));
console.log(`WASM organelle integration passed: ABI8, typed144/78schema, five register banks, original-source mutations, physical bodies, closed material, checkpoint3 replay, uncapped2,049-organism restore, ${state.births} producer births.`);
await checkEngineLoader();
