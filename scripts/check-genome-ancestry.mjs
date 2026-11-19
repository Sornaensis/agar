// Run against the production preview. Build the fixture from real divisions on
// each run. The browser receives ordinary checkpoint bytes and display metadata.
import assert from 'node:assert/strict';
import {mkdir} from 'node:fs/promises';
import {dirname} from 'node:path';
import {advance, catalog, compile, error, fresh, restore, save, seed, snapshot, source} from './organelle-harness.mjs';

const {chromium} = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const {PRESETS} = await catalog();
const habitat = {width:400,height:400,seed:42,ambientLight:.85,ambientTemperature:.5,
  initialNutrients:3,mutationRate:1,gases:{mode:'open',exchangeRate:.5}};
const mutationLabels = ['Root genome','Constant change','Expression change','Reference change',
  'Gene silencing','Gene duplication','Placement change'];
const independent = '# Spring sample\ncellular; product Mark = enzyme(); gene mark: Mark = 0;';
function registry(api) {
  const state=snapshot(api);
  return Array.from({length:api.genome_count()},(_,i)=>{
    const id=i+1;
    return {id,parent:api.genome_parent(id),kind:api.genome_mutation_kind(id),living:state.cells.filter(c=>c.genome===id).length};
  });
}
function record(api,id,name,genomeNames) {
  const state=snapshot(api), point={time:state.seconds,population:state.population,food:state.food.length,births:state.births,deaths:state.deaths};
  return {id,name,state:Buffer.from(save(api)).toString('base64'),environmentName:'Test culture',seed:42,
    createdAt:1000,updatedAt:1000,paused:true,speed:1,genomeNames,history:[point],
    summary:{...point,generation:Math.max(0,...state.cells.map(c=>c.generation))}};
}

async function generate() {
  const api=await fresh(habitat);
  const founder=compile(api,PRESETS.find(p=>p.name==='Crown rosette').source);
  seed(api,founder,1);
  const otherRoot=compile(api,independent);
  seed(api,otherRoot,1,350,350);
  // No explicit mutation calls. Confirm the naturally registered structure and
  // look up all edges and mutation kinds again from the actual engine registry.
  let state=snapshot(api),ancestor,grandchild;
  while(state.seconds<400 && !grandchild) {
    state=advance(api,5);
    const entries=registry(api);
    if(entries.filter(item=>item.parent===founder).length>=2) {
      grandchild=entries.find(item=>entries.some(p=>p.id===item.parent && p.parent===founder));
      ancestor=grandchild?.parent;
    }
  }
  assert.ok(grandchild && ancestor && state.mutations>=3,'A naturally branching two-generation family');
  assert.notEqual(source(api,ancestor),source(api,founder));
  assert.ok(state.cells.some(c=>c.genome===ancestor));
  // Clear the last carrier of an intermediate genome after its child exists.
  for(const cell of state.cells.filter(c=>c.genome===ancestor))
    assert.equal(api.set_cell_stores(cell.id,1,0,0),1,error(api));
  const unused=compile(api,'# Unseeded control\ncellular;');
  const start=save(api), startTick=state.ticks, initialCount=api.genome_count();
  while(api.genome_count()===initialCount && state.seconds<500) state=advance(api,5);
  assert.ok(api.genome_count()>initialCount && state.ticks-startTick>150);
  const firstNewId=initialCount+1, nextParent=api.genome_parent(firstNewId);
  assert.equal(restore(api,start),1,error(api));
  advance(api,state.ticks-startTick-120);
  assert.equal(api.genome_count(),initialCount,'The saved browser fixture precedes the next genuine mutation');
  const entries=registry(api);
  assert.equal(entries[unused-1].living,0);
  assert.equal(entries[ancestor-1].living,0);
  assert.ok(entries[grandchild.id-1].living>0);
  const main=record(api,'simulation:ancestry-branch','Branching culture',{
    [founder]:'Crown lineage',[otherRoot]:'Spring sample',[unused]:'Unseeded control'});
  const blank=await fresh(habitat);
  const isolatedRoot=compile(blank,'# Separate culture\ncellular;');seed(blank,isolatedRoot,1);
  const other=record(blank,'simulation:ancestry-isolated','Separate culture',{[isolatedRoot]:'Separate founder'});
  const emptyApi=await fresh(habitat);
  const emptyId=compile(emptyApi,'# Never occupied\ncellular;');
  const empty=record(emptyApi,'simulation:ancestry-empty','Unseeded culture',{[emptyId]:'Never occupied'});
  return {api,main,other,empty,initial:entries,ancestor,grandchild:grandchild.id,unused,founder,otherRoot,
    independentEntries:registry(blank),emptyEntries:registry(emptyApi),expectedSource:source(api,ancestor),firstNewId,nextParent};
}

// The spontaneous fixture above covers normal biology. These use the native
// mutation operation to reach the registry limit without thousands of births.
async function stressFixture(deep) {
  const api=await fresh(habitat),limit=deep ? 2048 : 65;
  const root=compile(api,'turn=0.452;thrust=0.564;divide=false;');
  for(let attempt=0;api.genome_count()<limit && attempt<5000;attempt++) {
    const before=api.genome_count(),parent=deep ? before : root;
    const id=api.mutate_genome(parent,9371+attempt,1);
    if(id>before) assert.equal(api.genome_parent(id),parent);
  }
  assert.equal(api.genome_count(),limit,'Native registry contains the entire test pedigree');
  seed(api,limit,1,180,200);
  const label=deep ? 'Deep family' : 'Wide family';
  return {record:record(api,'simulation:ancestry-'+(deep?'deep':'wide'),label,{[root]:label}),
    last:limit,parent:api.genome_parent(limit)};
}

const trial=await generate();
const largeFamilies=[await stressFixture(true),await stressFixture(false)];
const browser=await chromium.launch({headless:true,
  ...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:{}),
  args:['--no-sandbox','--disable-dev-shm-usage']});
const context=await browser.newContext({viewport:{width:1320,height:1000},permissions:['clipboard-read','clipboard-write']});
const page=await context.newPage(), failures=[];
page.on('pageerror',error=>failures.push(error.message));
page.on('console',message=>{if(message.type()==='error') failures.push(message.text());});
const base=process.env.UI_TEST_URL || 'http://127.0.0.1:4173/';
const node=id=>page.locator(`[data-lineage-node="${id}"]`);
const toggle=id=>page.locator(`[data-lineage-toggle="${id}"]`);
const stats=id=>page.locator(`[data-genome-stats="${id}"]`);
const tab=name=>page.locator(`[data-view="${name}"]`).click();
const open=async record=>{
  await page.locator(`[data-action="open-simulation"][data-id="${record.id}"]`).click();
  await tab('genomes');
};
const close=()=>page.locator('[data-action="close-simulation"]').click();

async function assertTree(expected) {
  await page.waitForFunction(n=>document.querySelectorAll('[data-lineage-node]').length===n,expected.length);
  const actual=await page.locator('[data-lineage-node]').evaluateAll(nodes=>nodes.map(el=>{
    const parent=el.parentElement?.closest('[data-lineage-node]');
    return {id:Number(el.dataset.lineageNode),parent:Number(el.dataset.lineageParent || 0),
      nestedUnder:parent ? Number(parent.dataset.lineageNode) : 0,kind:Number(el.dataset.mutationKind),
      root:el.hasAttribute('data-lineage-root')};
  }).sort((a,b)=>a.id-b.id));
  assert.deepEqual(actual,expected.map(({id,parent,kind})=>({id,parent,nestedUnder:parent,kind,root:parent===0})));
  for(const {id,living,kind} of expected) {
    const info=await stats(id).textContent();
    assert.match(info,living ? new RegExp('\\b'+living+' organisms?\\b','i') : /no living organisms/i);
    assert.match(await node(id).locator(':scope > .lineage-row .lineage-note').innerText(),new RegExp(mutationLabels[kind],'i'));
  }
}

async function savedAfter(time) {
  const deadline=Date.now()+30000;
  while(Date.now()<deadline) {
    const encoded=await page.evaluate(async ({id,time})=>{
      const db=await new Promise((resolve,reject)=>{
        const req=indexedDB.open('cellarium');
        req.onsuccess=()=>resolve(req.result); req.onerror=()=>reject(req.error);
      });
      try {
        const row=await new Promise((resolve,reject)=>{
          const req=db.transaction('simulations').objectStore('simulations').get(id);
          req.onsuccess=()=>resolve(req.result); req.onerror=()=>reject(req.error);
        });
        if(!row?.paused || row.summary.time<time) return null;
        const bytes=row.state;
        let encoded='';
        for(let i=0;i<bytes.length;i+=8192) encoded+=String.fromCharCode(...bytes.subarray(i,i+8192));
        return btoa(encoded);
      } finally {db.close();}
    },{id:trial.main.id,time});
    if(encoded) return Buffer.from(encoded,'base64');
    await new Promise(resolve=>setTimeout(resolve,50));
  }
  throw new Error('The paused simulation checkpoint did not reach the observed mutation');
}

try {
  await page.goto(base);
  await page.locator('[data-action="start-template"]').first().waitFor();
  await page.evaluate(records=>new Promise((resolve,reject)=>{
    const request=indexedDB.open('cellarium');
    request.onerror=()=>reject(request.error);
    request.onsuccess=()=>{
      const db=request.result,tx=db.transaction(['simulations','simulationMetadata'],'readwrite');
      for(const {state,...metadata} of records) {
        const decoded=Uint8Array.from(atob(state),char=>char.charCodeAt(0));
        tx.objectStore('simulations').put({...metadata,state:decoded});
        tx.objectStore('simulationMetadata').put(metadata);
      }
      tx.oncomplete=()=>{db.close();resolve();};
      tx.onerror=()=>reject(tx.error);
    };
  }),[trial.main,trial.other,trial.empty,...largeFamilies.map(f=>f.record)]);
  await page.reload();
  await open(trial.main);
  await assertTree(trial.initial);
  assert.doesNotMatch(await stats(trial.unused).textContent(),/extinct|died/i);

  if(process.env.ANCESTRY_SCREENSHOT) {
    await mkdir(dirname(process.env.ANCESTRY_SCREENSHOT),{recursive:true});
    await page.screenshot({path:process.env.ANCESTRY_SCREENSHOT});
  }
  await page.setViewportSize({width:390,height:844});
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth+1),'Ancestry fits a narrow viewport');
  await page.setViewportSize({width:1320,height:1000});
  await toggle(trial.ancestor).click();
  await node(trial.grandchild).waitFor({state:'hidden'});
  assert.equal(await toggle(trial.ancestor).getAttribute('aria-expanded'),'false');
  await node(trial.otherRoot).waitFor();
  await toggle(trial.founder).click();
  await node(trial.ancestor).waitFor({state:'hidden'});
  const newest=page.locator('[data-lineage-newest]');
  assert.match(await newest.innerText(),new RegExp('Newest variant #'+trial.grandchild));
  await newest.click();
  assert.equal(await toggle(trial.founder).getAttribute('aria-expanded'),'true');
  assert.equal(await toggle(trial.ancestor).getAttribute('aria-expanded'),'true');
  await node(trial.grandchild).waitFor();
  assert.ok(await page.locator(`[data-action="live-program"][data-genome="${trial.grandchild}"]`).evaluate(el=>el===document.activeElement));
  await assertTree(trial.initial);

  // Full editor clipboard contents must match the actual mutated program,
  // even though there is no living cell to inspect for this genome.
  const inspect=page.locator(`[data-action="live-program"][data-genome="${trial.ancestor}"]`);
  await inspect.click();
  const dialog=page.locator('.live-program-dialog[open]');
  const title=await dialog.locator('h2').innerText();
  const editor=dialog.locator('.cm-content');
  await editor.click(); await page.keyboard.press('Control+a'); await page.keyboard.press('Control+c');
  assert.equal(await page.evaluate(()=>navigator.clipboard.readText()),trial.expectedSource);
  await dialog.locator('[data-close]').click();
  await page.locator(`[data-action="live-graph"][data-genome="${trial.ancestor}"]`).click();
  const graph=page.locator('.genome-graph-dialog[open]');
  assert.equal(await graph.locator('h2').innerText(),title);
  await graph.locator('.graph-node-list button').first().waitFor();
  await graph.getByRole('button',{name:'Close genome visualizer'}).click();

  await tab('player');
  await page.getByRole('button',{name:'Play simulation'}).click();
  await tab('genomes');
  await node(trial.firstNewId).waitFor({timeout:25000});
  assert.equal(Number(await node(trial.firstNewId).getAttribute('data-lineage-parent')),trial.nextParent);
  assert.match(await newest.innerText(),new RegExp('Newest variant #'+trial.firstNewId));
  await tab('player');
  await page.getByRole('button',{name:'Pause simulation'}).click();
  const paused=await savedAfter(trial.main.summary.time+2);
  assert.equal(restore(trial.api,paused),1,error(trial.api));
  const after=registry(trial.api);
  assert.ok(after.length>trial.initial.length);
  await tab('genomes');
  await assertTree(after);
  await page.reload();
  await assertTree(after);
  await close();
  await open(trial.other);
  await assertTree(trial.independentEntries);
  assert.match(await node(1).innerText(),/Separate founder/);
  assert.doesNotMatch(await page.locator('#active-genomes').innerText(),/Crown lineage|Spring sample/);
  await close();
  await open(trial.empty);
  await assertTree(trial.emptyEntries);
  await newest.waitFor({state:'hidden'});
  assert.doesNotMatch(await node(1).innerText(),/extinct|died/i);
  await close();
  await open(trial.main);
  await assertTree(after);
  // Both a path deeper than the DOM window and a sibling beyond the first
  // page must remain navigable. Test focus and viewport, independent of layout.
  const stress=[];
  await page.setViewportSize({width:390,height:844});
  for(const family of largeFamilies) {
    await close();
    await open(family.record);
    const jump=page.locator('[data-lineage-newest]').filter({hasText:'Newest variant #'+family.last});
    await jump.waitFor();
    const started=Date.now();
    await jump.click({timeout:12000});
    await page.waitForFunction(id=>document.activeElement?.matches(`[data-action="live-program"][data-genome="${id}"]`),family.last,{timeout:12000});
    const elapsed=Date.now()-started;
    const last=node(family.last);
    assert.equal(Number(await last.getAttribute('data-lineage-parent')),family.parent);
    assert.match(await stats(family.last).textContent(),/\b1 organism\b/);
    assert.ok(await page.evaluate(()=>{
      const r=document.activeElement.getBoundingClientRect();
      return r.width>0 && r.left>=0 && r.right<=innerWidth+1 && r.bottom>0 && r.top<innerHeight
        && document.documentElement.scrollWidth<=document.documentElement.clientWidth+1;
    }),'Jump puts the last genome on-screen without horizontal overflow');
    stress.push({registrySize:family.last,lastParent:family.parent,jumpMs:elapsed});
  }
  assert.deepEqual(failures,[]);
  console.log(JSON.stringify({fixtureTick:Math.round(trial.main.summary.time*30),initial:trial.initial,
    extinctIntermediate:trial.ancestor,nestedVariant:trial.grandchild,afterPlayback:after,
    isolated:trial.independentEntries,unseeded:trial.emptyEntries,stress},null,2));
} catch(error) {
  console.error('Browser errors:',failures);
  console.error((await page.locator('body').innerText()).slice(0,5000));
  throw error;
} finally {await browser.close();}
