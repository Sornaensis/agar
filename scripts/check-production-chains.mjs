import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, readdir, writeFile } from 'node:fs/promises';
import { advance, body, bytes, catalog, compile, error, fresh, restore, save, snapshot } from './organelle-harness.mjs';

const close = (a,b,reason) => assert.ok(Math.abs(a-b) < 1e-8, `${reason}: ${a} vs ${b}`);
const sum = (rows, fn) => rows.reduce((total, item) => total + fn(item),0);
const value = (rows, name) => rows.find(x => x.name === name)?.mass ?? 0;
const dt = 1/30;
const adhesionProbe = `cellular;
product Frame = scaffold();
product Glue = adhesin(Frame);
product Mount = anchor();
product Skin = membrane();
gene backbone: Frame = if pool.Frame < 0.002 then 1 else 0;
gene finish: Glue = if pool.Glue < 0.002 then 1 else 0;
gene mount: Mount = if pool.Mount < 0.002 then 1 else 0;
gene skin: Skin = if pool.Skin < 0.002 then 1 else 0;
assembly Coat: adhere() {
  Glue: 0.006; Mount: 0.007; Skin: 0.007;
  start_when: built.Coat < 0.019;
}`;
const dark = {width:400,height:400,ambientLight:0,initialNutrients:0,
  gases:{initialOxygen:0,initialCarbon:0,mode:'closed'}};
const energyCosts = { Frame:40, Glue:25, Mount:50, Skin:40 };
const roles = Object.keys(energyCosts);
const report = {
  wasmSha256:createHash('sha256').update(bytes).digest('hex'),
  ticksPerSecond:30, knockoutObservationTicks:360, controls:{}, founders:[],
};

// Captured with the shipped ABI19 engine at tick 120. Its 1e-12 completion
// tolerance activated a tank that was missing all of its 1e-13 scaffold input.
// The live player then deposited ATP into the overstated capacity.
{
  const legacy = await readFile(new URL('../engine/fixtures/incomplete-storage-v15.bin',import.meta.url));
  assert.equal(legacy.readUInt32LE(8),15);
  assert.equal(createHash('sha256').update(legacy).digest('hex'),'67a7af507a1fcc4ceefebda5ee56c114757451db5a88960d43f6a986fb52f4ef');
  const api = await fresh();
  assert.equal(restore(api,legacy),1,error(api));
  const state = snapshot(api), cell = state.cells[0], inventory = body(api,cell.id);
  assert.equal(state.ticks,120);
  assert.equal(cell.capacity,80);
  assert.equal(cell.energy,80);
  close(inventory.foodPotential.body,43.56678430446711 + 20,'Excess historic ATP remains recoverable in tissue');
  close(inventory.foodPotential.reserve,65.74608476842955,'Historic organic reserves are untouched');
  assert.equal(inventory.cellular.assemblies.length,2);
  for (const [index,tank] of inventory.cellular.assemblies.entries()) {
    assert.equal(tank.complete,false);
    assert.equal(tank.functionalMass,0);
    const trace = tank.components.find(part=>part.name==='Trace');
    assert.equal(trace.mass,0);
    assert.equal(trace.requiredMass,1e-13);
    assert.equal(value(tank.components,'Wall'),[0.03999999999905861,0.010956357633156602][index],'Original installed membrane');
  }
  // Independent reference totals sampled before saving in ABI19.
  const totals = new Float64Array(api.memory.buffer,api.metrics_snapshot_ptr(),api.presentation_len());
  close(totals[18],209.31286907289666,'Historical stored energy');
  close(totals[19],214.4553483983746,'Historical supplied energy');
  close(totals[20],120.99000476837158,'Historical mineral');
  close(totals[21],120.99000476837158,'Historical supplied mineral');
  close(totals[36],403.7900000000002,'Historical system carbon');
  close(totals[37],403.79,'Historical supplied carbon');
  assert.deepEqual(Array.from(totals.slice(51,53)),[0,0],'Migration records no player withdrawals');
  const migrated = save(api);
  assert.equal(new DataView(migrated.buffer,migrated.byteOffset).getUint32(8,true),17);
  const replay = await fresh();
  assert.equal(restore(replay,migrated),1,error(replay));
  assert.deepEqual(save(replay),migrated,'Migration is canonical on immediate v16 resave');
  advance(api,30); advance(replay,30);
  assert.deepEqual(save(api),save(replay),'The previously overfull store resumes identically');
}

// This probe has no uptake, respiration, growth, recycling, light, or food.
// Only its initial reserves can supply new stock and installed ingredients.
async function followChain(source, expectedActive) {
  const api = await fresh(dark);
  api.place_cell(compile(api,source),200,200);
  const initial = snapshot(api), founder = initial.cells[0], initialBody = body(api,1).foodPotential.body;
  let priorPools = [], priorInstalled = {}, energyBefore = initial.energy;
  let boundInput = 0, rawInput = 0, retained = 0, firstPrecursor, firstFinished, firstFunctional;
  for (let tick=1; tick<=report.knockoutObservationTicks; tick++) {
    const state = advance(api,1), cell = state.cells[0], inventory = body(api,1);
    assert.equal(state.population,1);
    const pools = inventory.cellular.products, machines = inventory.cellular.assemblies;
    const produced = Object.fromEntries(roles.map(name => [name, (pools.find(p=>p.name===name)?.expressionRate ?? 0) * dt]));
    const installed = Object.fromEntries(roles.map(name => [name,sum(machines,m=>value(m.components,name))]));
    assert.ok(produced.Glue <= value(priorPools,'Frame')+1e-10, 'Finishing requires starting precursor');
    for (const name of roles) {
      const added = installed[name] - (priorInstalled[name] ?? 0);
      const used = added + (name === 'Frame' ? produced.Glue : 0);
      assert.ok(used <= value(priorPools,name)+1e-10,'Cannot use newly made '+name+' as stock this tick');
      close(value(pools,name),value(priorPools,name)+produced[name]-used,'Pool movement for '+name);
      boundInput += produced[name]*energyCosts[name] + added*30;
    }
    rawInput += produced.Frame + produced.Mount + produced.Skin;
    close(founder.nutrients-cell.nutrients,rawInput,'Mineral debits');
    close(founder.organicCarbon-cell.organicCarbon,rawInput,'Organic carbon debits');
    close(rawInput,sum(pools,p=>p.mass) + sum(machines,m=>m.mass),'Dry mass in products and parts');
    retained = inventory.foodPotential.body - initialBody - 20*rawInput;
    close(retained,0.9*boundInput,'Embodied fabrication investment');
    assert.ok(founder.energy-cell.energy+1e-9>=boundInput,'Fabrication ATP was actually spent');
    assert.ok(state.energy<=energyBefore+1e-9,'Closed unlit world cannot gain energy');
    close(state.material,initial.material,'World mineral inventory');
    if (produced.Frame > 0) firstPrecursor ??= tick;
    if (produced.Glue > 0) firstFinished ??= tick;
    if (machines.some(m=>m.functionalMass>0)) firstFunctional ??= tick;
    if (expectedActive && firstFunctional) break;
    energyBefore=state.energy; priorPools=pools; priorInstalled=installed;
  }
  assert.equal(Boolean(firstFunctional),expectedActive);
  if (expectedActive) assert.ok(firstPrecursor < firstFinished && firstFinished < firstFunctional);
  return {firstPrecursor:firstPrecursor??null,firstFinished:firstFinished??null,firstFunctional:firstFunctional??null,
    rawCarbonAndMinerals:rawInput,fabricationATP:boundInput,embodiedATP:retained};
}
report.controls.complete = await followChain(adhesionProbe,true);
for (const gene of ['backbone','finish','mount','skin']) {
  const source=adhesionProbe.replace(new RegExp('gene '+gene+'(: \\w+ = )[^;]+;'),(_,suffix)=>'gene '+gene+suffix+'0;');
  assert.ok(source.includes('= 0;') && source!==adhesionProbe);
  const result=await followChain(source,false);
  if (gene==='backbone' || gene==='finish') assert.equal(result.firstFinished,null);
  report.controls['without_'+gene]=result;
}

// Follow the eight playable founders through their first paid machinery. Full
// life-cycle trials exercise later specialized organs, including predators.
const {PRESETS}=await catalog();
for (const preset of PRESETS) {
  const api=await fresh({width:400,height:400,ambientLight:.85,initialNutrients:3});
  api.place_cell(compile(api,preset.source),200,200);
  assert.equal(body(api,1).cellular.assemblies.length,0);
  let firstInstall, firstActive, firstName, priorProducts=[];
  for (let tick=1; tick<=1800 && !firstActive; tick++) {
    assert.equal(api.tick(1),1);
    const {cellular}=body(api,1);
    if (!firstInstall && cellular.assemblies.some(a=>a.mass>0)) {
      firstInstall=tick;
      assert.ok(priorProducts.some(p=>p.mass>0),preset.name+' installed material from prior stock');
    }
    for (const a of cellular.assemblies) {
      if (a.functionalMass<=0) continue;
      assert.ok(a.complete && a.components.every(p=>p.mass+1e-10>=p.requiredMass));
      firstActive=tick; firstName??=a.name;
    }
    priorProducts=cellular.products;
  }
  assert.ok(firstInstall>1 && firstActive>firstInstall,preset.name+' requires multiple ticks before function');
  report.founders.push({name:preset.name,firstInstall,firstActive,firstMachine:firstName});
}
// Use compiler-validated recipe ingredients for every shipped source, including
// archived examples. A named product still needs a gene that can request it.
const examples=new URL('../examples/',import.meta.url);
const entries=await readdir(examples,{recursive:true});
report.catalog={sources:0,recipes:0,ingredientDeclarations:0,adhesiveRecipes:0};
for (const file of entries.filter(name=>name.endsWith('.cell')).sort()) {
  const source=await readFile(new URL(file,examples),'utf8');
  const api=await fresh(dark);
  api.place_cell(compile(api,source),200,200);
  const inventory=body(api,1).cellular;
  const text=source.replace(/#[^\n]*/g,'');
  const produced=new Set([...text.matchAll(/\bgene\s+\w+\s*:\s*(\w+)\s*=/g)].map(m=>m[1]));
  for (const recipe of inventory.recipes) {
    for (const part of recipe.components) {
      assert.ok(produced.has(part.name),file+' has no gene for '+recipe.name+' ingredient '+part.name);
      report.catalog.ingredientDeclarations++;
    }
    if (recipe.kind==='adhere()') {
      report.catalog.adhesiveRecipes++;
      for (const role of ['anchor','membrane','adhesin']) assert.ok(recipe.components.some(part=>part.role===role));
      for (const glue of recipe.components.filter(part=>part.role==='adhesin')) {
        const definition=inventory.products.find(product=>product.name===glue.name);
        assert.ok(definition.precursor && produced.has(definition.precursor),file+' has no manufactured precursor for '+glue.name);
      }
    }
  }
  report.catalog.sources++; report.catalog.recipes+=inventory.recipes.length;
}
console.log(JSON.stringify(report,null,2));
if (process.argv[2]==='--output' && process.argv[3]) await writeFile(process.argv[3],JSON.stringify(report,null,2)+'\n');
