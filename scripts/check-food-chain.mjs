import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { bytes,fresh,compile,seed,snapshot,advance,graph,catalog,recipes,moduleUrl } from './organelle-harness.mjs';
const args=process.argv.slice(2),measure=args.includes('--measure');let output=null,seeds=[42,7,2026],mutationRate=0,duration=900;
for(let i=0;i<args.length;i++){
  if(args[i]==='--measure')continue;const name=args[i],value=args[++i];assert.ok(['--output','--seed','--mutation','--duration'].includes(name)&&value);
  if(name==='--output')output=value;if(name==='--seed'){const n=Number(value);assert.ok(Number.isInteger(n)&&n>=0&&n<=0xffffffff);seeds=[n];}
  if(name==='--mutation'){mutationRate=Number(value);assert.ok(mutationRate>=0&&mutationRate<=1);}
  if(name==='--duration'){duration=Number(value);assert.ok(Number.isSafeInteger(duration)&&duration>0&&Number.isSafeInteger(duration*30),'Duration must be a positive whole number of simulated seconds.');}
}
assert.ok(measure||args.length===0);
const {PRESETS,BUILTIN_ENVIRONMENTS,BUILTIN_GENOMES,BUILTIN_SIMULATIONS}=await catalog();
const organelleModule=await moduleUrl('../src/organelles.ts');
const {tokenizeGenome}=await import(await moduleUrl('../src/genome-language.ts',{'./organelles':organelleModule}));
const hasLegacyReference=source=>tokenizeGenome(source).some(token=>
  ['identifier','signal','output'].includes(token.kind) && (token.text==='size'||/^(?:invest|food|prey|threat|neighbor)\./u.test(token.text)));
assert.equal(hasLegacyReference('# not guarantees of food.\nrs001="prey.visible"; target.flagellum.size=0.04;'),false,'Comments, strings and legal size targets are not legacy references');
assert.equal(hasLegacyReference('let seen=food.visible;'),true,'Actual legacy references remain rejected');
assert.equal(hasLegacyReference('let area=size;'),true,'The old overloaded size signal remains rejected');
const preset=name=>{const p=PRESETS.find(p=>p.name===name);assert.ok(p);return p;};
const sunlit=BUILTIN_ENVIRONMENTS.find(e=>e.id==='builtin:environment:sunlit-pond');
assert.equal(sunlit.config.foodRate,0,'Primary-production habitat has no implicit organic food input');
for(const p of PRESETS){
  const api=await fresh();const id=compile(api,p.source);assert.ok(recipes.includes(p.starterBody));
  const roots=graph(api,id).roots;assert.equal(roots.filter(r=>r.kind==='output').length,78);
  assert.ok(!hasLegacyReference(p.source),'No legacy resource or semantic target oracle in authored programs');
  seed(api,id,1,p.starterBody);const before=snapshot(api).cells[0];advance(api,1);const after=snapshot(api).cells[0];
  assert.ok(after,'A supplied founder is viable with its paired program');
  assert.ok(after.mass<=before.mass+0.01,'First evaluation cannot introduce free specialist tissue');
}
function stationary(text){
  for(const[name,value]of [['thrust','0'],['turn','0'],['grow','0'],['divide','false']])text=text.replace(new RegExp(`^${name}\\s*=\\s*[^;]+;`,'m'),`${name} = ${value};`);
  text=text.replace(/^build\.[a-z]+\s*=\s*[^;]+;/gm,match=>match.replace(/=[^;]+;/,'= 0;'));
  return text;
}
async function acquisition(name,light,food=false){
  const api=await fresh({ambientLight:light,initialNutrients:2});const p=preset(name),id=compile(api,stationary(p.source));seed(api,id,1,p.starterBody,600,380);
  const before=snapshot(api);if(food)api.add_food(before.cells[0].x,before.cells[0].y,256);
  const after=advance(api,600);assert.equal(after.population,1);assert.ok(Math.abs(after.material-after.materialIntroduced)<1e-7);
  return{before,after};
}
const producerLight=await acquisition('Phototroph',.8),producerDark=await acquisition('Phototroph',0);
const consumerEmpty=await acquisition('Forager',.8),consumerFed=await acquisition('Forager',.8,true);
const mixerLight=await acquisition('Mixotroph',.8),mixerDark=await acquisition('Mixotroph',0);
assert.ok(producerLight.after.cells[0].energy>producerLight.before.cells[0].energy,'Physical photosynthetic units harvest light');
assert.ok(producerDark.after.cells[0].energy<producerDark.before.cells[0].energy);assert.equal(producerLight.after.cells[0].gutEnergy,0);
assert.ok(producerLight.after.food.length>0&&producerLight.after.food.some(f=>f.energy>0),'Photosynthetic surplus supplies organic particles');
assert.equal(producerDark.after.food.length,0);assert.equal(consumerEmpty.after.photosynthesis,0);
assert.ok(consumerFed.after.cells[0].energy>consumerEmpty.after.cells[0].energy,'Phagocytic and digestive organelles process contacting organic particles');
assert.ok(mixerLight.after.cells[0].energy>mixerDark.after.cells[0].energy,'Mixed bodies support both energy routes');
console.log(`Organelle food-chain checks passed: ${PRESETS.length} compiled programs with explicit starter bodies; paid specialization; light/organic/mixed acquisition; finite compartments and conserved minerals.`);
if(measure){
  const template=BUILTIN_SIMULATIONS.find(s=>s.id==='builtin:simulation:mixed-ecosystem'),env=BUILTIN_ENVIRONMENTS.find(e=>e.id===template.environmentId);
  const populations=template.populations.map(row=>({...BUILTIN_GENOMES.find(g=>g.id===row.genomeId),...row}));
  const study={startedAt:new Date().toISOString(),abi:8,checkpointVersion:3,wasmSha256:createHash('sha256').update(bytes).digest('hex'),wasmBytes:bytes.length,
    template:template.name,environment:env,populations,mutationRate,seeds,durationSeconds:duration,
    sampleSeconds:[...new Set([...([0,30,120,299,300,600,900].filter(seconds=>seconds<=duration)),duration])].sort((a,b)=>a-b),results:[]};
  const round=n=>Math.round(n*1000)/1000;
  for(const n of seeds){
    const started=performance.now(),api=await fresh({...env.config,seed:n,mutationRate,initialFood:env.config.foodRate>0?360:0}),names=new Map();
    for(const p of populations){const id=compile(api,p.source);names.set(id,p.name);seed(api,id,p.count,p.starterBody);}
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
