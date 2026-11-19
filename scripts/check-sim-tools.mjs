import assert from 'node:assert/strict';
import { module, restore, snapshot, body } from './organelle-harness.mjs';

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
  assert.equal(restore(api,Uint8Array.from(record.bytes)),1);
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

try {
  await page.goto(process.env.UI_TEST_URL || 'http://127.0.0.1:4173/');
  await page.locator('[data-id="builtin:simulation:nurse-rosette"][data-action="start-template"]').click();
  await page.locator('[data-count="0"]').fill('0');
  await page.getByRole('button',{name:/Start simulation/}).click();
  await page.getByRole('button',{name:'Pause simulation'}).click();
  await page.waitForFunction(() => document.getElementById('save-state')?.textContent === 'Saved');
  const initial = await saved();
  assert.equal(initial.population,0);
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
  await action(child,'edit-reserves');
  await child.locator('#reserve-energy').fill('50');
  await child.locator('#reserve-minerals').fill('0.3');
  await child.getByRole('button',{name:'Apply reserves'}).click();
  await savedWhen(s => s.cells[1].energy===50 && s.cells[1].nutrients===.3);
  await action(child,'edit-reserves');
  await child.getByRole('button',{name:'Subtract 10 energy'}).click();
  await child.getByRole('button',{name:'Apply reserves'}).click();
  await savedWhen(s => s.cells[1].energy===40 && s.cells[1].nutrients===.3);
  await action(child,'edit-reserves');
  for (let index = 0; index < 4; index++) await child.getByRole('button',{name:'Subtract 10 energy'}).click();
  await child.getByRole('button',{name:'Set energy to zero'}).waitFor();
  assert.ok(await child.locator('.reserve-zero').isVisible());
  await child.getByRole('button',{name:'Cancel',exact:true}).click();
  await tool(child,'Phototroph');
  await child.locator('#world').press('Escape');
  await action(child,'return-player');
  await page.locator('#world').waitFor();
  await action(page,'close-simulation'); await page.locator('[data-action="open-simulation"]').first().click();
  await page.getByRole('button',{name:'Play simulation'}).waitFor();
  await page.evaluate(() => { delete window.testCamera; });
  const reopened = await saved();
  assert.ok(atPoint(reopened,2,d) && reopened.cells[1].energy===40 && reopened.cells[1].nutrients===.3 && reopened.ticks===initial.ticks);

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
  const afterLiveEdit = await savedWhen(s => s.paused && s.ticks>reopened.ticks && Math.abs(s.withdrawn - reopened.withdrawn - 10)<1e-8);
  assert.ok(afterLiveEdit.cells[0].energy>0,'A running withdrawal charges exactly ten ATP to the intervention ledger');

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
  await page.waitForFunction(() => Number(document.querySelector('[data-food-pool="total"]')?.textContent)>0);
  for (const [pool,energy] of Object.entries(predicted)) {
    const shown = Number(await page.locator(`[data-food-pool="${pool}"]`).textContent());
    assert.ok(Math.abs(shown-energy)<0.0051,pool+' matches the saved organism');
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
  assert.ok(carcass.food.some(f => f.kind===1 && readout===`Tissue particle · ${Number(f.energy.toPrecision(4))} food energy · ${Number(f.minerals.toPrecision(4))} minerals`),
    'The tap reads measured particle energy and minerals');
  assert.deepEqual(errors,[]);
  console.log('Player edits passed: exact repeated placement, pinch/pan/cancel, blocked and transformed destinations, direct drag, narrow pop-out editing, live energy withdrawals, checkpoint reopen, an intact parent–daughter colony move, and measured body/packet food energy.');
} catch (error) {
  console.error('Visible player:',(await page.locator('body').innerText()).slice(-3000));
  throw error;
} finally { await browser.close(); }
