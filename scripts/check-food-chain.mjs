import {prepareBloom,placeByTissue} from './feeding-culture.mjs';
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { bytes,fresh,compile,seed,snapshot,advance,graph,body,catalog,moduleUrl,save } from './organelle-harness.mjs';
const args=process.argv.slice(2),measure=args.includes('--measure');let output=null,seeds=[42,7,2026],mutationRate=0,duration=120;
for(let i=0;i<args.length;i++){
  if(args[i]==='--measure')continue;const name=args[i],value=args[++i];assert.ok(['--output','--seed','--mutation','--duration'].includes(name)&&value);
  if(name==='--output')output=value;if(name==='--seed'){const n=Number(value);assert.ok(Number.isInteger(n)&&n>=0&&n<=0xffffffff);seeds=[n];}
  if(name==='--mutation'){mutationRate=Number(value);assert.ok(mutationRate>=0&&mutationRate<=1);}
  if(name==='--duration'){duration=Number(value);assert.ok(Number.isSafeInteger(duration)&&duration>0&&Number.isSafeInteger(duration*30),'Duration must be a positive whole number of simulated seconds.');}
}
assert.ok(measure||args.length===0);
const {LEGACY_PRESETS:PRESETS,BUILTIN_ENVIRONMENTS,BUILTIN_GENOMES,BUILTIN_SIMULATIONS}=await catalog();
const organelleModule=await moduleUrl('../src/organelles.ts');
const {tokenizeGenome}=await import(await moduleUrl('../src/genome-language.ts',{'./organelles':organelleModule}));
const hasLegacyReference=source=>tokenizeGenome(source).some(token=>
  ['identifier','signal','output'].includes(token.kind) && (token.text==='size'||/^(?:invest|food|prey|threat|neighbor)\./u.test(token.text)));
assert.equal(hasLegacyReference('# not guarantees of food.\nrs001="prey.visible"; target.flagellum.size=0.04;'),false,'Comments, strings and legal size targets are not legacy references');
assert.equal(hasLegacyReference('let seen=food.visible;'),true,'Actual legacy references remain rejected');
assert.equal(hasLegacyReference('let area=size;'),true,'The old overloaded size signal remains rejected');
const preset=name=>{const p=PRESETS.find(p=>p.name===name);assert.ok(p);return p;};
const sunlit=BUILTIN_ENVIRONMENTS.find(e=>e.id==='builtin:environment:sunlit-pond');
assert.equal('foodRate' in sunlit.config,false);
const conserved=state=>{
  assert.ok(Math.abs(state.material-state.materialIntroduced)<Math.max(1e-7,state.materialIntroduced*1e-10));
  assert.ok(state.energy<=state.energyIntroduced+state.photosynthesis+1e-7);
};
let commonBody;
for(const p of PRESETS){
  const api=await fresh(),id=compile(api,p.source);
  assert.equal('starterBody' in p,false,'A genome is not paired with an external body recipe');
  const roots=graph(api,id).roots;assert.equal(roots.filter(r=>r.kind==='output').length,81);
  assert.ok(!hasLegacyReference(p.source),'No legacy resource or semantic target oracle in authored programs');
  seed(api,id,1);const before=snapshot(api).cells[0],initial=body(api,before.id);
  assert.equal(before.area,1);assert.equal(before.energy,68);assert.equal(before.capacity,80);
  assert.equal(before.nutrients,.6);assert.equal(before.mineralCapacity,1);
  assert.equal(initial.organs.length,1);assert.equal(initial.organs[0].kind,'core');assert.equal(initial.organs[0].mass,.04);
  const physical={area:initial.area,mass:initial.mass,organs:initial.organs,space:initial.spaceCapacity,surface:initial.surfaceCapacity};
  commonBody??=physical;assert.deepEqual(physical,commonBody,`${p.name} begins with the same physical body`);
  const after=advance(api,1).cells[0];assert.ok(after);
  const constructed=Object.values(after.organs).reduce((sum,organ)=>sum+organ.mass,0)-.04;
  assert.equal(constructed,0,'Newly expressed proteins cannot assemble into organs in the same tick');
  const expressed=Object.values(after.proteins).reduce((sum,mass)=>sum+mass,0);
  assert.ok(expressed>0&&expressed*.6<=.022/30+1e-10,'The genome begins bounded, paid protein synthesis');
  assert.ok(after.energy<before.energy&&after.nutrients<before.nutrients,'Initial construction spends the shared resource reserves');
}
// Action requests alone do not grant hardware, even with food and light present.
{
  const api=await fresh({ambientLight:.8,initialNutrients:2}),id=compile(api,'thrust=1;turn=1;divide=true;grow=1;photosynthesize=1;feed=1;engulf=true;absorb=1;bite=1;');
  seed(api,id,1,600,380);const initial=snapshot(api).cells[0];placeByTissue(api,initial,0);
  const state=advance(api,600),cell=state.cells[0];assert.ok(cell);assert.equal(cell.area,1);
  assert.deepEqual(body(api,cell.id).organs,commonBody.organs);assert.equal(state.photosynthesis,0);assert.equal(state.eaten,0);assert.equal(state.births,0);conserved(state);
}
function stationary(text){
  for(const[name,value]of [['thrust','0'],['turn','0'],['grow','0'],['divide','false']])text=text.replace(new RegExp(`^${name}\\s*=\\s*[^;]+;`,'m'),`${name} = ${value};`);
  return text; // Construction remains the authored genome's responsibility.
}
// The pre-gas genomes are exercised as supplied cultures. Aeration keeps the
// concentrated test bolus from stripping all oxygen from the founder's patch.
const cultureGases = { mode: 'open', initialOxygen: 4, oxygenEquilibrium: 4, exchangeRate: 1 };
async function acquisition(name,light,food=false){
  const api=await fresh({ambientLight:.85,ambientTemperature:.2,initialNutrients:2,gases:cultureGases});
  const id=compile(api,stationary(preset(name).source));
  const prepared=food ? prepareBloom(api,BUILTIN_GENOMES.find(p=>p.name==='Phototroph').source) : {x:600,y:380};
  api.set_environment(.5,light,0);
  const founder=api.place_cell(id,prepared.x,prepared.y);assert.ok(founder);
  const before=snapshot(api), initialRespired=respiredCarbon(api); let after,assimilation=0;
  for(let tick=0;tick<600;tick+=3){after=advance(api,3);assimilation+=after.cells[0].digestionRate*.1;}

  assert.equal(after.population,1);conserved(after);
  return {api,before,after,assimilation,initialRespired};

}
const producerLight=await acquisition('Phototroph',.8),producerDark=await acquisition('Phototroph',0);
const consumerEmpty=await acquisition('Forager',.8),consumerFed=await acquisition('Forager',.8,true);
const mixerLight=await acquisition('Mixotroph',.8),mixerDark=await acquisition('Mixotroph',0),mixerFedDark=await acquisition('Mixotroph',0,true);
assert.ok(producerLight.after.photosynthesis>10&&producerLight.after.cells[0].organs.photosynthetic.capacity>=1,'The genome builds functioning photosynthetic units');
assert.equal(producerDark.after.photosynthesis,0);assert.equal(producerLight.after.cells[0].gutEnergy,0);
assert.ok(producerLight.after.cells[0].organicCarbon>producerDark.after.cells[0].organicCarbon,'Built photosynthetic machinery fixes carbon in light');
assert.equal(consumerEmpty.after.photosynthesis,0);assert.equal(consumerEmpty.after.eaten,0);
assert.ok(consumerFed.after.eaten>0&&consumerFed.assimilation>0,'The genome builds capture and digestion machinery that actually processes organic particles');
function respiredCarbon(api) {
  const pointer = api.metrics_snapshot_ptr();
  return new Float64Array(api.memory.buffer, pointer, 51)[41];
}
assert.ok(consumerFed.after.cells[0].organicCarbon > consumerEmpty.after.cells[0].organicCarbon,
  'Digestion replenishes organic carbon before it becomes usable energy');
const fedAfterMeal = advance(consumerFed.api, 900), emptyAfterMeal = advance(consumerEmpty.api, 900);
assert.ok(fedAfterMeal.cells[0].energy > emptyAfterMeal.cells[0].energy
  && respiredCarbon(consumerFed.api)-consumerFed.initialRespired > respiredCarbon(consumerEmpty.api)-consumerEmpty.initialRespired,
  'The supplied meal subsequently fuels respiration and builds an ATP reserve');
assert.ok(mixerLight.after.photosynthesis>0&&mixerLight.after.cells[0].organs.digestive.capacity>=1&&mixerLight.after.cells[0].organs.phagocyte.capacity>=1,'The same initial cell can construct both energy routes');
assert.equal(mixerDark.after.photosynthesis,0);assert.equal(mixerDark.after.eaten,0);
assert.equal(mixerFedDark.after.photosynthesis,mixerFedDark.before.photosynthesis);assert.ok(mixerFedDark.after.eaten>0&&mixerFedDark.assimilation>0,'A mixotroph can use its constructed organic pathway without light');
const surplus=advance(producerLight.api,1500);assert.ok(surplus.food.some(f=>f.energy>0),'Constructed producers return photosynthetic surplus as organic particles');conserved(surplus);

// Each legacy program develops unchanged, using light or a finite producer-derived meal.
async function lifecycle(name,fed){
  const api=await fresh({width:400,height:400,ambientLight:.85,ambientTemperature:.2,initialNutrients:3,gases:cultureGases});
  const id=compile(api,preset(name).source);
  const site=fed ? prepareBloom(api,BUILTIN_GENOMES.find(p=>p.name==='Phototroph').source,true) : {x:200,y:200};
  const founder=api.place_cell(id,site.x,site.y);assert.ok(founder);
  let state=snapshot(api),tick=0,precedingBody;
  while(tick<5400&&!state.births){
    const parent=state.cells.find(c=>c.id===founder);assert.ok(parent,`${name} survives its development`);
    // Inspect the exact partition tick once the envelope approaches maturity.
    const steps=parent.area>=1.8?1:30;precedingBody=steps===1?body(api,parent.id):undefined;
    state=advance(api,steps);tick+=steps;
  }
  assert.equal(state.births,1,`${name} constructs a reproducing body from the shared minimal newborn`);
  assert.equal(state.population,2);assert.ok(precedingBody,'The division was observed at one-tick resolution');
  const daughter=state.cells.find(c=>c.generation===1);assert.ok(daughter&&daughter.parentId===founder);
  const parentBody=body(api,founder),daughterBody=body(api,daughter.id),combined=[...parentBody.organs,...daughterBody.organs];
  assert.ok(daughterBody.organs.length>1,'A daughter inherits developed machinery instead of resetting to a newborn template');
  assert.equal(new Set(combined.map(organ=>organ.id)).size,combined.length,'Actual organelle records are partitioned without duplication');
  assert.deepEqual(combined.map(organ=>organ.id).sort((a,b)=>a-b),precedingBody.organs.map(organ=>organ.id).sort((a,b)=>a-b),'Division neither invents nor discards organelles');
  for(const organ of precedingBody.organs){const inherited=combined.find(item=>item.id===organ.id);assert.equal(inherited.kind,organ.kind);assert.ok(Math.abs(inherited.mass-organ.mass)<.002,'Inherited mass changes only by ordinary construction in the division tick');}
  assert.ok(daughter.organs[fed?'digestive':'photosynthetic'].capacity>=1,'The daughter retains its actual energy machinery');
  conserved(state);return{seconds:tick/30,eaten:state.eaten};
}
const producerCycle=await lifecycle('Phototroph',false),consumerCycle=await lifecycle('Forager',true);
console.log(`Organelle food-chain checks passed: ${PRESETS.length} genomes share one minimal newborn; paid light/organic/mixed development; unchanged Phototroph/Forager programs divide in ${producerCycle.seconds.toFixed(1)}/${consumerCycle.seconds.toFixed(1)} s; physical organelle inheritance and conserved resources.`);
if(measure){
  const template=BUILTIN_SIMULATIONS.find(s=>s.id==='builtin:simulation:mixed-ecosystem'),env=BUILTIN_ENVIRONMENTS.find(e=>e.id===template.environmentId);
  const populations=template.populations.map(row=>({...BUILTIN_GENOMES.find(g=>g.id===row.genomeId),...row}));
  const checkpoint = save(producerLight.api);
  const checkpointVersion = new DataView(checkpoint.buffer, checkpoint.byteOffset, checkpoint.byteLength).getUint32(8,true);
  const study={startedAt:new Date().toISOString(),abi:producerLight.api.abi_version(),checkpointVersion,wasmSha256:createHash('sha256').update(bytes).digest('hex'),wasmBytes:bytes.length,
    template:template.name,environment:env,populations,mutationRate,seeds,durationSeconds:duration,
    sampleSeconds:[...new Set([...([0,15,30,60,120,180].filter(seconds=>seconds<=duration)),duration])].sort((a,b)=>a-b),results:[]};
  const round=n=>Math.round(n*1000)/1000;
  for(const n of seeds){
    const started=performance.now(),api=await fresh({...env.config,seed:n,mutationRate}),names=new Map();
    for(const p of populations){const id=compile(api,p.source);names.set(id,p.name);seed(api,id,p.count);}
    const founding=[...new Set(names.values())],samples=[];
    const lineage=id=>{if(names.has(id))return names.get(id);const parent=api.genome_parent(id);assert.ok(parent&&parent!==id);const name=lineage(parent);names.set(id,name);return name;};
    let tick=0;
    for(const seconds of study.sampleSeconds){
      const state=advance(api,seconds*30-tick);tick=seconds*30;
      assert.ok(Math.abs(state.material-state.materialIntroduced)<Math.max(1e-7,state.materialIntroduced*1e-10));
      assert.ok(state.energy<=state.energyIntroduced+state.photosynthesis+1e-7);
      const lineages=founding.map(name=>{
        const cells=state.cells.filter(c=>lineage(c.genome)===name),sum=key=>cells.reduce((t,c)=>t+c[key],0);
        return{name,count:cells.length,area:round(sum('area')),material:round(sum('mass')),energy:round(sum('energy')),
          meanEnergyFraction:cells.length?round(cells.reduce((t,c)=>t+c.energy/c.capacity,0)/cells.length):null,
          gutEnergy:round(sum('gutEnergy')),photosynthesisRate:round(sum('photosynthesisRate')),digestionRate:round(sum('digestionRate')),
          growthRate:round(sum('growthRate')),maxGeneration:cells.length?Math.max(...cells.map(c=>c.generation)):null,
          organelleMass:Object.fromEntries(['photosynthetic','digestive','flagellum','piercer','optical'].map(k=>[k,round(cells.reduce((t,c)=>t+c.organs[k].mass,0))]))};
      });
      const sample={seconds,population:state.population,births:state.births,deaths:state.deaths,organicParticles:state.food.length,
        organicEnergy:round(state.food.reduce((t,f)=>t+f.energy,0)),attacks:state.attacks,predations:state.predations,mutations:state.mutations,
        mineralError:state.material-state.materialIntroduced,lineages};samples.push(sample);console.log(JSON.stringify({seed:n,...sample}));
    }
    study.results.push({seed:n,wallSeconds:round((performance.now()-started)/1000),samples});
    if(output)await writeFile(output,JSON.stringify(study,null,2)+'\n');
  }
  if(!output)console.log(JSON.stringify({measurement:study}));else console.log(`Measurements saved to ${output}`);
}
