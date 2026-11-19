import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
export const bytes=await readFile(new URL('../engine/pkg/agar_engine.wasm',import.meta.url));
export const module=await WebAssembly.compile(bytes);
export const kinds=['core','photosynthetic','metabolic','digestive','reserve','transporter','phagocyte','flagellum','piercer','cortex','repair','photoreceptor','thermoreceptor','chemoreceptor','mechanoreceptor','optical'];
export const STRIDE=163;
export const error=api=>new TextDecoder().decode(new Uint8Array(api.memory.buffer,api.error_ptr(),api.error_len()));
export async function fresh(config={},compiledModule=module){
  const api=(await WebAssembly.instantiate(compiledModule,{})).exports;assert.equal(api.abi_version(),15);
  assert.equal(api.set_world_size(config.width??1200,config.height??760),1,error(api));
  api.set_environment(config.ambientTemperature??.5,config.ambientLight??0,config.currentStrength??0);
  api.set_initial_nutrients(config.initialNutrients??.3);api.set_food_rate(config.foodRate??0);api.set_mutation_rate(config.mutationRate??0);
  const gases=config.gases??{};
  assert.equal(api.set_initial_gases(gases.initialOxygen??1,gases.initialCarbon??1),1,error(api));
  assert.equal(api.set_gas_exchange(gases.mode==='open'?1:0,gases.exchangeRate??.02,gases.oxygenEquilibrium??1,gases.carbonEquilibrium??1),1,error(api));
  for(const s of config.sources??[]){const id=api.add_source(s.kind,s.x,s.y,s.strength,s.radius);assert.ok(id,error(api));assert.equal(api.update_source(id,s.kind,s.x,s.y,s.strength,s.radius,s.angle??0,s.length??600),1,error(api));}
  for(const b of config.barriers??[])assert.ok(api.add_barrier(b.x,b.y,b.length,b.width,b.angle,Number(b.floating)),error(api));
  assert.equal(api.reset_empty(config.seed??42,config.initialFood??0),1,error(api));return api;
}
export function compile(api,source){
  const data=new TextEncoder().encode(source),ptr=api.alloc(data.length);assert.ok(ptr);
  try{new Uint8Array(api.memory.buffer,ptr,data.length).set(data);const id=api.compile_genome(ptr,data.length);assert.ok(id,error(api));return id;}
  finally{api.dealloc(ptr,data.length);}
}
export function seed(api,id,count,x=-1,y=-1){assert.equal(api.seed_cells(id,count,x,y),count,error(api));}
export function json(api,pointer,length,...args){const p=api[pointer](...args);assert.ok(p,error(api));return JSON.parse(new TextDecoder().decode(new Uint8Array(api.memory.buffer,p,api[length]())));}
export const graph=(api,id)=>json(api,'genome_graph_ptr','genome_graph_len',id);
export const body=(api,id)=>json(api,'cell_body_ptr','cell_body_len',id);
export const memory=(api,id)=>json(api,'cell_memory_ptr','cell_memory_len',id);
export const evaluation=(api,id)=>json(api,'cell_evaluation_ptr','cell_evaluation_len',id);
export function source(api,id){const p=api.genome_source_ptr(id);return new TextDecoder().decode(new Uint8Array(api.memory.buffer,p,api.genome_source_len()));}
export function save(api){const p=api.save_state_ptr(),n=api.save_state_len();assert.ok(p&&n,error(api));return new Uint8Array(api.memory.buffer,p,n).slice();}
export function restore(api,data){const p=api.alloc_state(data.length);assert.ok(p);try{new Uint8Array(api.memory.buffer,p,data.length).set(data);return api.restore_state(p,data.length);}finally{api.dealloc(p,data.length);}}
// Historical migration comparisons use the original habitat projection. The
// Gas channels and geometry are excluded from this historical habitat projection.
export function legacyEnvironmentProjection(api){
  const p=api.environment_snapshot_ptr(),data=new Float32Array(api.memory.buffer,p,api.environment_snapshot_len()).slice();
  if(api.abi_version()<11)return data;
  assert.equal(data[7],1200);assert.equal(data[8],760);assert.equal(data[9],0);assert.equal(data[10],0);
  const header=api.abi_version()>=12?21:11,stride=api.abi_version()>=12?7:5;
  const end=header+data[0]*data[1]*stride,values=[...data.slice(0,7)];
  for(let i=header;i<end;i+=stride)values.push(...data.slice(i,i+5));
  for(let i=0;i<data[5];i++)values.push(...data.slice(end+i*8,end+i*8+6));
  return Float32Array.from(values);
}
export function snapshot(api){
  const p=api.snapshot_ptr(),v=new Float64Array(api.memory.buffer,p,api.snapshot_len()).slice();
  assert.ok(v.every(Number.isFinite));assert.equal(v.length,12+v[2]*STRIDE+v[3]*5);assert.equal(v[9],0);assert.equal(v[10],0);
  const cells=Array.from({length:v[2]},(_,i)=>{
    const n=12+i*STRIDE;
    const c={id:v[n],genome:v[n+1],x:v[n+2],y:v[n+3],vx:v[n+4],vy:v[n+5],angle:v[n+6],energy:v[n+7],age:v[n+8],generation:v[n+9],parentId:v[n+10],radius:v[n+11],
      area:v[n+12],mass:v[n+13],birthArea:v[n+14],capacity:v[n+15],metabolism:v[n+16],memoryUsed:v[n+17],memoryCost:v[n+18],integrity:v[n+19],photosynthesisRate:v[n+20],
      nutrients:v[n+21],mineralCapacity:v[n+22],uptakeRate:v[n+23],gutEnergy:v[n+24],gutCapacity:v[n+25],gutNutrients:v[n+26],digestionRate:v[n+27],growthRate:v[n+28],maturity:v[n+29],lifespan:v[n+30],exudationRate:v[n+31],upkeep:v[n+32],
      spaceUsed:v[n+33],spaceCapacity:v[n+34],surfaceUsed:v[n+35],surfaceCapacity:v[n+36],divisionReady:v[n+37]!==0,opticalRange:v[n+38],
      gutEnergyByKind:Array.from(v.slice(n+135,n+138)),gutMineralsByKind:Array.from(v.slice(n+138,n+141)),
      proteins:Object.fromEntries(kinds.map((kind,k)=>[kind,v[n+141+k]])),expressionRate:v[n+157],
      organicCarbon:v[n+158],internalOxygen:v[n+159],internalInorganicCarbon:v[n+160],respirationRate:v[n+161],carbonFixationRate:v[n+162],
      organs:Object.fromEntries(kinds.map((kind,k)=>{const at=n+39+k*6;return[kind,{count:v[at],mass:v[at+1],capacity:v[at+2],damaged:v[at+3],building:v[at+4],functionalMass:v[at+5]}];}))};
    assert.ok(Number.isInteger(c.id)&&c.id>0);assert.ok(c.x>=0&&c.x<v[7]&&c.y>=0&&c.y<v[8]);
    assert.ok(c.energy>0&&c.energy<=c.capacity+1e-8);assert.ok(c.area>=.25&&c.area<185&&Math.abs(c.radius-7*Math.sqrt(c.area))<1e-9);
    assert.ok(c.spaceUsed<=c.spaceCapacity+1e-8&&c.surfaceUsed<=c.surfaceCapacity+1e-8);
    assert.ok(c.nutrients>=0&&c.nutrients<=c.mineralCapacity+1e-8&&c.gutEnergy>=0&&c.gutEnergy<=c.gutCapacity+1e-8);
    assert.ok(c.mass>=.35*c.area+c.nutrients+c.gutNutrients-1e-8&&c.age>=0&&c.lifespan===0&&c.integrity>0);
    assert.ok(Math.abs(c.gutEnergyByKind.reduce((a,b)=>a+b,0)-c.gutEnergy)<1e-8);
    assert.ok(Math.abs(c.gutMineralsByKind.reduce((a,b)=>a+b,0)-c.gutNutrients)<1e-8);
    return c;
  });
  const food=Array.from({length:v[3]},(_,i)=>{const n=12+v[2]*STRIDE+i*5;return{x:v[n],y:v[n+1],energy:v[n+2],minerals:v[n+3],kind:v[n+4]};});
  return{ticks:v[0],seconds:v[1],population:v[2],births:v[4],deaths:v[5],eaten:v[6],cells,food,
    material:api.nutrient_total(),materialIntroduced:api.nutrient_input_total(),energy:api.energy_total(),energyIntroduced:api.energy_input_total(),
    photosynthesis:api.photosynthesis_total(),attacks:api.attack_count(),predations:api.predation_count(),mutations:api.mutation_count(),genomes:api.genome_count()};
}
export function advance(api,ticks){while(ticks>0){const n=Math.min(ticks,120);assert.equal(api.tick(n),1,error(api));ticks-=n;}return snapshot(api);}
export async function moduleUrl(relative,imports={}){
  const input=await readFile(new URL(relative,import.meta.url),'utf8');let js=ts.transpileModule(input,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText;
  for(const[name,url]of Object.entries(imports))js=js.replaceAll(`from '${name}'`,`from ${JSON.stringify(url)}`).replaceAll(`from ${JSON.stringify(name)}`,`from ${JSON.stringify(url)}`);
  return `data:text/javascript;base64,${Buffer.from(js).toString('base64')}`;
}
export async function catalog(){const presetsUrl=await moduleUrl('../src/presets.ts');return{...await import(presetsUrl),...await import(await moduleUrl('../src/catalog.ts',{'./presets':presetsUrl}))};}
