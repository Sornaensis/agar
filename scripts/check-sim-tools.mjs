import assert from 'node:assert/strict';
import { module, restore, snapshot, body, error as engineError } from './organelle-harness.mjs';

// Exercise the built app and read its persisted checkpoints through the public
// engine ABI. The player has no automation-only state or editing endpoints.
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({headless:true,
  ...(process.env.CHROMIUM_PATH ? {executablePath:process.env.CHROMIUM_PATH} : {}),
  args:['--no-sandbox','--disable-dev-shm-usage','--disable-blink-features=DocumentPictureInPictureAPI']});
const context = await browser.newContext({viewport:{width:1200,height:920},hasTouch:true});
const page = await context.newPage();
const errors = [];
context.on('page', child => child.on('pageerror', error => errors.push(error.message)));
page.on('pageerror', error => errors.push(error.message));
const api = (await WebAssembly.instantiate(module,{})).exports;

async function saved() {
  const record = await page.evaluate(() => new Promise((resolve,reject) => {
    const open = indexedDB.open('cellarium');
    open.onerror = () => reject(open.error);
    open.onsuccess = () => {
      const db = open.result, tx = db.transaction('simulations'), request = tx.objectStore('simulations').getAll();
      request.onsuccess = () => { const {state,paused} = request.result[0]; resolve({bytes:Array.from(state),paused}); };
      tx.oncomplete = () => db.close();
    };
  }));
  assert.equal(restore(api,Uint8Array.from(record.bytes)),1,engineError(api));
  const state = snapshot(api), at = api.snapshot_ptr(), data = new Float64Array(api.memory.buffer,at,12);
  return {...state,width:data[7],height:data[8],paused:record.paused,withdrawn:api.energy_withdrawn_total()};
}
async function savedWhen(check) {
  const until = Date.now()+10000;
  do {
    const state = await saved();
    if (check(state)) return state;
    await new Promise(resolve => setTimeout(resolve,100));
  } while (Date.now() < until);
  throw new Error('The requested edit did not reach the saved checkpoint');
}
async function tool(surface,genome) {
  await surface.locator('[data-action="add-organism"]').click();
  await surface.getByRole('button',{name:'Add '+genome,exact:true}).click();
  await surface.locator('#cell-tool-bar').waitFor();
}
async function point(surface,state,rx,ry) {
  const box = await surface.locator('#world').boundingBox();
  const view = await surface.evaluate(() => window.testCamera ?? null);
  const zoom = view?.zoom ?? 1, scale = Math.min(box.width/state.width,box.height/state.height)*zoom;
  const x = Math.round(box.x+box.width*rx), y = Math.round(box.y+box.height*ry);
  return {x,y, wx:(view?.x ?? state.width/2)+(x-box.x-box.width/2)/scale,
    wy:(view?.y ?? state.height/2)+(y-box.y-box.height/2)/scale, scale};
}
const atPoint = (state,id,p) => {
  const cell = state.cells.find(c => c.id === id);
  return cell && Math.hypot(cell.x-p.wx,cell.y-p.wy)<1.5/p.scale;
};
const action = (surface,name) => surface.locator(`[data-action="${name}"]`).click();

async function controls(surface,mode) {
  await surface.waitForFunction(expected => document.body.dataset.playerControls === expected,mode,{timeout:5000});
}
async function enterPlayer(surface) {
  const {width,height}=surface.viewportSize();
  await surface.mouse.move(width/2,height/2);
  await controls(surface,'pointer');
}
async function leavePlayer(surface) {
  await surface.mouse.move(-12,surface.viewportSize().height/2);
  await controls(surface,'hidden');
}
async function checkCanvasFrame(surface) {
  const bounds=await surface.locator('#world').evaluate(canvas=>{
    const r=canvas.getBoundingClientRect();
    return [r.x,r.y,innerWidth-r.right,innerHeight-r.bottom];
  });
  assert.ok(bounds.every(n=>Math.abs(n)<=1),'Canvas must fill all four edges of the popup: '+bounds.join(', '));
}
async function controlCenters(surface) {
  return surface.locator('.player-toolbar, .player-view-controls, #player-caption, #cell-inspector').evaluateAll(elements=>
    elements.filter(e=>e.checkVisibility({opacityProperty:true})).map(e=>{
      const r=e.getBoundingClientRect();
      return {x:r.x+r.width/2,y:r.y+Math.min(8,r.height/2)};
    }));
}
async function expectCleanCanvas(surface,points) {
  await surface.waitForFunction(points=>{
    const layers=[...document.querySelector('.player-stage').children,document.querySelector('#player-window-toast')];
    return layers.every(el=>el.id==='world'||!el.checkVisibility({opacityProperty:true,visibilityProperty:true}))
      && points.every(p=>document.elementFromPoint(p.x,p.y)?.id==='world');
  },points,{timeout:5000});
}
async function tabTo(surface,selector) {
  const limit=await surface.locator('button, input, select, [tabindex="0"]').count()+2;
  for(let i=0;i<limit;i++){
    if(await surface.locator(selector).evaluate(el=>el===document.activeElement))return;
    await surface.keyboard.press('Tab');
  }
  assert.fail('Keyboard focus did not reach '+selector);
}

try {
  await page.goto(process.env.UI_TEST_URL || 'http://127.0.0.1:4173/');
  await page.locator('[data-action="new-simulation"]').click();
  await page.locator('#simulation-environment').selectOption('builtin:environment:colony-pool');
  assert.equal(await page.locator('#resource-arithmetic').inputValue(),'counts');
  while (await page.locator('[data-remove-population]').count() > 1) {
    await page.locator('[data-remove-population]').last().click();
  }
  await page.locator('[data-population="0"]').selectOption('builtin:genome:nurse-rosette');
  await page.locator('[data-count="0"]').fill('0');
  await page.getByRole('button',{name:/Start simulation/}).click();
  await page.getByRole('button',{name:'Pause simulation'}).click();
  await page.waitForFunction(() => document.getElementById('save-state')?.textContent === 'Saved');
  const initial = await saved();
  assert.equal(api.uses_count_chemistry(),1,'The normal gallery launches a whole-count world');
  assert.equal(initial.population,0);
  await page.locator('[data-view="metrics"]').click();
  await page.getByText('Exact count budgets',{exact:true}).click();
  for (const pool of ['mineral','carbon','oxygen']) {
    assert.equal(await page.locator('#count-'+pool+'-difference').textContent(),'0');
    assert.ok(BigInt(await page.locator('#count-'+pool+'-held').textContent())>0n);
  }
  await page.locator('[data-view="player"]').click();
  await tool(page,'Nurse rosette');
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('#cell-tool-bar').isVisible(),false);

  await tool(page,'Nurse rosette');
  await page.locator('[data-repeat-placement]').check();
  // Two-finger navigation must cancel placement even after one finger lifts.
  const touch = await context.newCDPSession(page);
  const fingers = (spread,y) => [{x:450-spread,y,id:0},{x:450+spread,y,id:1}];
  await touch.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:fingers(30,390)});
  await touch.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:fingers(68,406)});
  await touch.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
  await touch.detach();
  await page.locator('[data-camera="player:fit"]').click();
  assert.equal((await saved()).population,0);
  const a = await point(page,initial,.23,.38), b = await point(page,initial,.44,.48);
  await page.touchscreen.tap(a.x,a.y);
  await page.waitForFunction(() => document.getElementById('cell-tool-status')?.textContent.includes('1 added'));
  await page.mouse.click(b.x,b.y);
  await page.waitForFunction(() => document.getElementById('cell-tool-status')?.textContent.includes('2 added'));
  await action(page,'cancel-cell-tool');
  const placed = await savedWhen(s => s.population === 2);
  assert.ok(atPoint(placed,1,a) && atPoint(placed,2,b),'Both taps land exactly where shown, without seed scatter');
  await page.waitForFunction(()=>document.querySelector('[data-value="attachment"]')?.textContent==='Unattached');
  assert.ok(await page.locator('[data-action="move-colony"]').isDisabled(),'Two same-genome founders have no colony action without a bond');

  await action(page,'move-cell');
  await page.mouse.click(a.x,a.y);
  await page.waitForFunction(() => document.getElementById('toast')?.textContent.toLowerCase().match(/blocked|overlap|obstruct|occup|clear/));
  assert.ok(await page.locator('#cell-tool-bar').isVisible(),'Invalid drop keeps move mode available');
  await page.locator('#world').evaluate(canvas => canvas.addEventListener('camerachange', event => { window.testCamera = event.detail; }));
  await page.locator('[data-camera="player:in"]').click();
  await page.mouse.move(b.x,b.y); await page.keyboard.down('Shift');
  await page.mouse.down(); await page.mouse.move(b.x-75,b.y-40,{steps:6}); await page.mouse.up(); await page.keyboard.up('Shift');
  assert.ok(await page.locator('#cell-tool-bar').isVisible(),'Panning did not place or move a cell');
  const c = await point(page,initial,.34,.64);
  await page.touchscreen.tap(c.x,c.y);
  await savedWhen(s => atPoint(s,2,c));
  const d = await point(page,initial,.46,.71);
  await page.mouse.move(c.x,c.y); await page.mouse.down(); await page.mouse.move(d.x,d.y,{steps:7}); await page.mouse.up();
  assert.equal((await savedWhen(s => atPoint(s,2,d))).ticks,initial.ticks,'Dragging preserves the paused clock');

  const childOpened = context.waitForEvent('page',{timeout:10000});
  await action(page,'popout-player');
  const child = await childOpened;
  await child.locator('#world').waitFor();
  await child.setViewportSize({width:420,height:720});
  assert.equal(await child.locator('.player-window-header, [data-player-window-name], .player-window-mode, .player-window-shell > header').count(),0);
  assert.ok((await child.title()).endsWith(' · agar player'));
  assert.equal(await child.locator('body').getAttribute('data-player-window-mode'),'window');
  await checkCanvasFrame(child);
  await enterPlayer(child);
  await child.locator('#playback-speed').click();
  await child.keyboard.press('Escape');
  assert.ok(await child.locator('#playback-speed').evaluate(b=>b===document.activeElement));
  const visibleControls=await controlCenters(child);
  assert.equal(visibleControls.length,4,'Toolbar, fields and camera, caption and selected organism inspector are visible');
  await leavePlayer(child);
  await expectCleanCanvas(child,visibleControls);
  await enterPlayer(child);
  assert.equal((await controlCenters(child)).length,4);
  await leavePlayer(child);
  await child.keyboard.press('Tab');
  await controls(child,'keyboard');
  await tabTo(child,'[data-player-hide]');
  await child.keyboard.press('Enter');
  await controls(child,'hidden');
  await enterPlayer(child);
  await action(child,'edit-reserves');
  assert.equal(await child.locator('#reserve-energy').getAttribute('step'),'1');
  await child.locator('#reserve-energy').fill('500000');
  await child.locator('#reserve-minerals').fill('3001');
  await leavePlayer(child);
  await child.waitForFunction(()=>{
    const dialog=document.querySelector('dialog[open]');
    return dialog && Number(getComputedStyle(dialog).opacity)===0
      && Number(getComputedStyle(dialog,'::backdrop').opacity)===0;
  });
  await enterPlayer(child);
  assert.equal(await child.locator('#reserve-energy').inputValue(),'500000');
  await child.getByRole('button',{name:'Apply reserves'}).click();
  await savedWhen(s => s.cells[1].energy===50 && s.cells[1].nutrients===.3001);
  await action(child,'edit-reserves');
  await child.getByRole('button',{name:'Subtract 10 energy'}).click();
  await child.getByRole('button',{name:'Apply reserves'}).click();
  await savedWhen(s => s.cells[1].energy===49.999 && s.cells[1].nutrients===.3001);
  await action(child,'edit-reserves');
  await child.locator('#reserve-energy').fill('0');
  await child.getByRole('button',{name:'Set energy to zero'}).waitFor();
  assert.ok(await child.locator('.reserve-zero').isVisible());
  await child.getByRole('button',{name:'Cancel',exact:true}).click();
  await tool(child,'Phototroph');
  const toolBeforeTouch=await child.locator('#cell-tool-status').textContent();
  await leavePlayer(child);
  await child.touchscreen.tap(210,330);
  await controls(child,'touch');
  assert.equal(await child.locator('#cell-tool-status').textContent(),toolBeforeTouch,'The reveal tap preserves the active placement tool');
  await child.locator('[data-player-hide]').tap();
  await controls(child,'hidden');
  await child.touchscreen.tap(210,330);
  await controls(child,'touch');
  await child.locator('#world').press('Escape');
  await enterPlayer(child);
  await action(child,'return-player');
  await page.locator('#world').waitFor();
  await action(page,'close-simulation'); await page.locator('[data-action="open-simulation"]').first().click();
  await page.getByRole('button',{name:'Play simulation'}).waitFor();
  await page.evaluate(() => { delete window.testCamera; });
  const reopened = await saved();
  assert.equal(reopened.population,2,'Neither touch reveal placed an organism');
  assert.ok(atPoint(reopened,2,d) && reopened.cells[1].energy===49.999 && reopened.cells[1].nutrients===.3001 && reopened.ticks===initial.ticks);

  // Wait for an actual daughter to attach. Use the paid native colony, then
  // compare all members' saved coordinates before and after one UI move.
  await page.locator('#world').press('ArrowRight');
  await page.getByRole('button',{name:'Play simulation'}).click();
  await action(page,'edit-reserves');
  await page.getByRole('button',{name:'Subtract 10 energy'}).click();
  await page.waitForFunction(() => parseInt(document.getElementById('player-caption')?.textContent.split(':').at(-1)) >= 2);
  await page.getByRole('button',{name:'Apply reserves'}).click();
  await page.waitForFunction(() => !document.querySelector('.reserve-editor[open]'));
  await page.getByRole('button',{name:'Pause simulation'}).click();
  const afterLiveEdit = await savedWhen(s => s.paused && s.ticks>reopened.ticks && Math.abs(s.withdrawn - reopened.withdrawn - .001)<1e-8);
  assert.ok(afterLiveEdit.cells[0].energy>0,'A running withdrawal charges exactly ten ATP counts to the intervention ledger');

  await page.locator('#playback-speed').selectOption('10');
  await page.getByRole('button',{name:'Play simulation'}).click();
  await page.waitForFunction(() => parseInt(document.getElementById('player-caption')?.textContent) >= 3,null,{timeout:60000});
  await page.getByRole('button',{name:'Pause simulation'}).click();
  const beforeMove = await savedWhen(s => s.paused && s.population >= 3);
  const parent = beforeMove.cells.find(cell => beforeMove.cells.some(child => child.parentId===cell.id) && body(api,cell.id).colony.neighbors>0);
  assert.ok(parent,'An actual connected parent and daughter are present');
  const daughter = beforeMove.cells.find(cell => cell.parentId===parent.id);
  const canvas = page.locator('#world');
  await canvas.focus();
  for (let i=0;i<beforeMove.population;i++) {
    if ((await page.locator('#cell-inspector .organism-title .muted').textContent())?.includes('Organism #'+parent.id+' ·')) break;
    await canvas.press('ArrowRight');
  }
  const directPeers=body(api,parent.id).colony.neighborIds;
  const members=new Set([parent.id]);
  for(const id of members)for(const peer of body(api,id).colony.neighborIds)members.add(peer);
  assert.ok(directPeers.includes(daughter.id));
  const expectedMembership=`${members.size}-cell colony`;
  await page.waitForFunction(label=>document.querySelector('[data-value="attachment"]')?.textContent===label,expectedMembership);
  assert.ok(await page.locator('[data-action="move-colony"]').isEnabled(),'A confirmed mother–daughter bond enables the group move');
  await action(page,'move-colony');
  const dest = await point(page,beforeMove,.34,.59);
  await page.touchscreen.tap(dest.x,dest.y);
  const shifted = await savedWhen(s => atPoint(s,parent.id,dest));
  const moved = shifted.cells.find(c => c.id===parent.id), companion = shifted.cells.find(c => c.id===daughter.id);
  assert.ok(Math.abs((moved.x-parent.x)-(companion.x-daughter.x))<1e-7 && Math.abs((moved.y-parent.y)-(companion.y-daughter.y))<1e-7,
    JSON.stringify({beforeTick:beforeMove.ticks,afterTick:shifted.ticks,from:[parent.x,parent.y,daughter.x,daughter.y],to:[moved.x,moved.y,companion.x,companion.y]}));
  assert.equal(shifted.ticks,beforeMove.ticks);

  // A player can compare a live body's gross yield against a real physical
  // packet. An explicit ATP withdrawal is separated from what is left as food.
  await tool(page,'Forager');
  const freshPosition = await point(page,shifted,.23,.73);
  await page.touchscreen.tap(freshPosition.x,freshPosition.y);
  const withSubject = await savedWhen(s => s.population===shifted.population+1);
  const newCell = withSubject.cells.at(-1);
  const predicted = body(api,newCell.id).foodPotential;
  await page.locator('[data-food-potential] > summary').click();
  await page.waitForFunction(() => Number(document.querySelector('[data-food-pool="total"]')?.textContent.replaceAll(',',''))>0);
  for (const [pool,energy] of Object.entries(predicted)) {
    const shown = Number((await page.locator(`[data-food-pool="${pool}"]`).textContent()).replaceAll(',',''));
    assert.equal(shown,Math.round(energy*10000),pool+' matches the saved organism in whole counts');
  }
  await action(page,'edit-reserves');
  await page.locator('#reserve-energy').fill('0');
  await page.getByRole('button',{name:'Set energy to zero'}).click();
  const carcass = await savedWhen(s => s.deaths===withSubject.deaths+1);
  assert.equal(carcass.ticks,withSubject.ticks);
  const energyInFood = s => s.food.reduce((sum,f)=>sum+f.energy,0);
  assert.ok(Math.abs(energyInFood(carcass)-energyInFood(withSubject)-predicted.total+predicted.atp)<1e-8);
  const tissue = carcass.food.find(f => f.kind===1 && Math.hypot(f.x-newCell.x,f.y-newCell.y)<30);
  assert.ok(tissue);
  const box = await canvas.boundingBox(), scale = Math.min(box.width/carcass.width,box.height/carcass.height);
  await page.touchscreen.tap(Math.round(box.x+box.width/2+(tissue.x-carcass.width/2)*scale),Math.round(box.y+box.height/2+(tissue.y-carcass.height/2)*scale));
  await page.waitForFunction(() => document.getElementById('toast')?.textContent.startsWith('Tissue particle ·'));
  const readout = await page.locator('#toast').textContent();
  assert.ok(carcass.food.some(f => f.kind===1 && readout===`Tissue particle · ${Math.round(f.energy*10000).toLocaleString()} food energy · ${Math.round(f.minerals*10000).toLocaleString()} minerals`),
    'The tap reads measured particle energy and minerals');

  // Keep the mouse physically outside the popup throughout the observation.
  // Compare the visible clock with a later worker checkpoint and actual body age.
  const nextPopup=context.waitForEvent('page');
  await action(page,'popout-player');
  const running=await nextPopup;
  await running.locator('#world').waitFor();
  await running.setViewportSize({width:720,height:420});
  await checkCanvasFrame(running);
  await enterPlayer(running);
  await running.getByRole('button',{name:'Play simulation'}).click();
  await leavePlayer(running);
  await expectCleanCanvas(running,[]);
  const startCaption=await running.locator('#player-caption').textContent();
  const startFrame=await running.locator('#world').evaluate(canvas=>canvas.toDataURL());
  await running.waitForFunction(start=>{
    const seconds=text=>{const bits=text.match(/(\d+):(\d+)$/);return bits?Number(bits[1])*60+Number(bits[2]):-1;};
    return seconds(document.querySelector('#player-caption').textContent)>=seconds(start)+3;
  },startCaption);
  await running.waitForFunction(start=>document.querySelector('#world').toDataURL()!==start,startFrame,{polling:200,timeout:5000});
  await controls(running,'hidden');
  await enterPlayer(running);
  await running.getByRole('button',{name:'Pause simulation'}).click();
  const progressed=await savedWhen(s=>s.paused && s.ticks>=carcass.ticks+60);
  assert.ok(progressed.cells.some(c=>carcass.cells.some(prev=>prev.id===c.id && c.age>prev.age)),'A living body aged while controls were hidden');
  await leavePlayer(running);
  await running.keyboard.press('Tab');
  await tabTo(running,'[data-action="return-player"]');
  const docked=running.waitForEvent('close');
  // Docking can close the popup on keydown, before Chromium sends keyup.
  await running.keyboard.press('Enter').catch(error=>{ if(!running.isClosed()) throw error; });
  await docked;
  await page.locator('#world').waitFor();
  await page.mouse.move(1,1);
  assert.equal(await page.locator('body').getAttribute('data-player-controls'),null);
  assert.ok(await page.locator('.player-toolbar').evaluate(el=>el.checkVisibility({opacityProperty:true})));
  await page.getByRole('button',{name:'Play simulation'}).waitFor();
  assert.deepEqual(errors,[]);
  console.log('Player edits and real popup passed: placement, navigation, reserve edits, checkpoint reopen, colony move, food energy, full-frame canvas, hover leave/return after focus, modal draft, keyboard and touch reveal/dismissal, undisturbed playback and docking.');
} catch (error) {
  console.error('Visible player:',(await page.locator('body').innerText()).slice(-3000));
  throw error;
} finally { await browser.close(); }
