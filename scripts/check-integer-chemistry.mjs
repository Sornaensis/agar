import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFile, writeFile} from 'node:fs/promises';
import {performance} from 'node:perf_hooks';
import {advance, body, bytes, catalog, compile, error, fresh, restore, save, seed, snapshot} from './organelle-harness.mjs';

const scale=10_000;
const report={wasmSha256:createHash('sha256').update(bytes).digest('hex'),countsPerFormerUnit:scale};
const exact=n=>assert.ok(Math.abs(n*scale-Math.round(n*scale))<1e-5,'Nonintegral stock '+n);
function audit(api) {
  const ptr=api.count_audit_ptr();
  assert.ok(ptr);
  const v=Array.from(new BigUint64Array(api.memory.buffer,ptr,12));
  for (let i=0;i<9;i+=3) assert.equal(v[i]+v[i+2],v[i+1],'Whole-count ledger '+i);
  const losses = new BigInt64Array(api.memory.buffer,api.energy_loss_ptr(),7);
  assert.equal(v[9]+v[11]+losses.reduce((n,x)=>n+x,0n),v[10],'Stored, withdrawn and spent energy must equal input');
  return v;
}
const zero={width:400,height:400,initialNutrients:0,gases:{mode:'sealed',initialOxygen:0,initialCarbon:0},countChemistry:true};
const emitter=await fresh({...zero,sources:[{kind:3,x:200,y:200,radius:20,strength:1/15000}]});
const base=audit(emitter);
advance(emitter,300);
const supplied=audit(emitter)[1]-base[1];
assert.equal(supplied,10n,'One count per second');
report.slowEmitterCountsInTenSeconds=Number(supplied);
const historical=await readFile(new URL('../engine/fixtures/continuous-culture-v16.bin',import.meta.url));
assert.equal(historical.readUInt32LE(8),16);
const old=await fresh(zero);
assert.equal(restore(old,historical),1,error(old)); assert.equal(old.uses_count_chemistry(),0);
assert.equal(old.count_audit_ptr(),0); assert.equal(old.energy_loss_ptr(),0);
const original=save(old);assert.equal(old.set_count_chemistry(1),0);assert.deepEqual(save(old),original);
assert.equal(restore(old,original),1,error(old));assert.deepEqual(save(old),original);

const {BUILTIN_ENVIRONMENTS,BUILTIN_SIMULATIONS,BUILTIN_GENOMES}=await catalog();
const dark=await fresh(zero);
const staged=`cellular counts;
product Frame=scaffold(); product Glue=adhesin(Frame); product Anchor=anchor(); product Skin=membrane();
gene frame: Frame = if pool.Frame < 12 then 1 else 0;
gene finish: Glue = if pool.Glue < 12 then 1 else 0;
gene mounts: Anchor = if pool.Anchor < 12 then 1 else 0;
gene skins: Skin = if pool.Skin < 12 then 1 else 0;
assembly Coat: adhere() { Anchor:80; Skin:80; Glue:40; start_when: built.Coat < 0.019; }`;
dark.place_cell(compile(dark,staged),200,200);
const start=audit(dark);let previous=start[9];
for(let tick=1;tick<=450;tick++){
  const state=advance(dark,1),v=audit(dark);
  assert.ok(v[9]<=previous,'Dark founder created energy at tick '+tick); previous=v[9];
  const cell=state.cells[0];
  for(const n of [cell.energy,cell.nutrients,cell.organicCarbon,cell.internalOxygen,cell.internalInorganicCarbon])exact(n);
  const inv=body(dark,cell.id);
  for(const p of inv.cellular.products)exact(p.mass);
  for(const m of inv.cellular.assemblies)for(const p of m.components)exact(p.mass);
  if(inv.cellular.assemblies.some(m=>m.functionalMass>0)) {
    report.adhesion={tick,remainingATP:Math.round(cell.energy*scale),worldMineralDriftCounts:Number(v[0]-start[0]),
      carbonAllocatedToBody:Math.round((3.4-cell.organicCarbon)*scale),energyDissipated:Number(start[9]-v[9])};
    break;
  }
}
assert.ok(report.adhesion?.tick>2);
const beforeDeath=audit(dark);
assert.equal(dark.set_cell_stores(1,1,0,0),1,error(dark));
const afterDeath=audit(dark);
assert.equal(afterDeath[9]+afterDeath[11]-beforeDeath[11],beforeDeath[9],'Lysis plus explicit ATP withdrawal');
assert.equal(snapshot(dark).population,0);
const same=await fresh();
const checkpoint=save(dark);assert.equal(restore(same,checkpoint),1,error(same));
assert.equal(same.uses_count_chemistry(),1);assert.deepEqual(save(same),checkpoint);

// Identical genome source, setup, seed and launch order isolate the effects
// of counted chemistry. Measure only engine stepping; audit and IO are outside.
async function observe(countChemistry) {
  const template=BUILTIN_SIMULATIONS.find(v=>v.id.endsWith(':hunting-ground'));
  const env=BUILTIN_ENVIRONMENTS.find(v=>v.id===template.environmentId);
  const api=await fresh({...env.config,countChemistry,mutationRate:.1,seed:42});
  for(const row of template.populations) {
    const genome=BUILTIN_GENOMES.find(g=>g.id===row.genomeId);
    seed(api,compile(api,genome.source),row.count);
  }
  let ms=0,firstKill,firstBirth,stored,atHalf;
  for(let sec=1;sec<=120;sec++) {
    const t=performance.now();assert.equal(api.tick(30),1,error(api));ms+=performance.now()-t;
    if(countChemistry) audit(api);
    const state=snapshot(api);
    if(state.predations)firstKill??=sec;
    if(state.births)firstBirth??=sec;
    if(countChemistry && sec===60){stored=save(api);atHalf=state.population;}
  }
  const finish=snapshot(api);
  if(countChemistry) {
    const replay=await fresh();assert.equal(restore(replay,stored),1,error(replay));
    assert.equal(snapshot(replay).population,atHalf);
    assert.deepEqual(save(replay),stored);
    advance(replay,60*30);
    assert.deepEqual(save(replay),save(api),'Saved carries, inventories, inheritance and RNG continue exactly');
    const ledger=audit(api);
    const names=['mineral','carbon','oxygen','energy'];
    report.finalLedger=Object.fromEntries(names.map((name,i)=>[name,{
      held:String(ledger[i*3]),supplied:String(ledger[i*3+1]),removedOrConsumed:String(ledger[i*3+2]),
    }]));
    const afterFission=save(api);
    assert.equal(restore(replay,afterFission),1,error(replay));
    assert.deepEqual(save(replay),afterFission);
    advance(replay,30);advance(api,30);
    assert.deepEqual(save(replay),save(api),'The divided population also resumes exactly');
  }
  return {population:finish.population,births:finish.births,deaths:finish.deaths,successfulKills:finish.predations,
    firstObservedKillSeconds:firstKill,firstObservedBirthSeconds:firstBirth,steppingMilliseconds:Math.round(ms)};
}
// Record unequal physical inheritance and verify that a light-fed daughter can
// still reproduce. A mounted copy can go to either half of the cleavage.
async function checkProducerInheritance(countChemistry) {
  const api = await fresh({seed:42,width:400,height:400,ambientLight:1,initialNutrients:8,countChemistry,
    gases:{mode:'open',initialOxygen:4,initialCarbon:4,exchangeRate:0.8,oxygenEquilibrium:4,carbonEquilibrium:4}});
  const producer = BUILTIN_GENOMES.find(g=>g.name==='Phototroph');
  seed(api,compile(api,producer.source),1);
  const seen = new Set([1]);
  let firstBirth = null;
  let generation = 0;
  const newborns = [];
  for (let seconds=1; seconds<=300 && generation<2; seconds++) {
    const current = advance(api,30);
    for (const child of current.cells) {
      if (seen.has(child.id)) continue;
      seen.add(child.id);
      const parent = current.cells.find(c => c.id === child.parentId);
      if (parent) {
        const hardware = [parent, child].map(c => body(api, c.id).cellular.assemblies);
        const names = ['Solar','Respiration','Minerals','Translation'];
        const sides = hardware.map(parts => names.filter(name =>
          parts.some(a => a.name === name && a.complete && a.functionalMass > 0)));
        assert.ok(names.every(name => sides.some(side => side.includes(name))),
          'The living pair retains its paid essential machinery across division');
        newborns.push({child: child.id, parent: parent.id, inherited: sides[1]});
      }
      generation = Math.max(generation,child.generation);
      if (firstBirth===null) {
        firstBirth = seconds;
        const saved = save(api);
        assert.equal(restore(api,saved),1,error(api));
        assert.deepEqual(save(api),saved);
      }
    }
  }
  assert.ok(generation>=2,'A light-fed daughter must itself reproduce');
  return {firstBirth,generation,newborns};
}
report.producerInheritance={continuous:await checkProducerInheritance(false),counts:await checkProducerInheritance(true)};
report.sameSeed120Seconds={continuous:await observe(false),counts:await observe(true)};
assert.ok(report.sameSeed120Seconds.counts.successfulKills>0 && report.sameSeed120Seconds.counts.births>0);
const out=JSON.stringify(report,null,2)+'\n';
console.log(out);
if(process.argv[2]==='--output') await writeFile(process.argv[3],out);
