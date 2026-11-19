import assert from 'node:assert/strict';

const {chromium}=await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:{}),
  args:['--no-sandbox','--disable-dev-shm-usage','--disable-blink-features=DocumentPictureInPictureAPI']});
const context=await browser.newContext({viewport:{width:1250,height:920},hasTouch:true});
const errors=[];
context.on('page',surface=>surface.on('pageerror',error=>errors.push(error.message)));
const page=await context.newPage();
page.setDefaultTimeout(6000);
const field=(key)=>page.locator(`[data-object-field="${key}"]:visible`);
const action=(name,surface=page)=>surface.locator(`[data-action="${name}"]`).click();
const read=async(key)=>Number(await field(key).inputValue());
const near=(a,b,eps=.04)=>assert.ok(Math.abs(a-b)<eps,`expected ${a} ≈ ${b}`);
async function expectValue(key,expected,eps=.04){
  await page.waitForFunction(({key,expected,eps})=>Array.from(document.querySelectorAll(`[data-object-field="${key}"]`))
    .some(input=>input.checkVisibility()&&Math.abs(Number(input.value)-expected)<eps),{key,expected,eps});
}
async function enter(key,value){await field(key).fill(String(value));await field(key).press('Tab');}
async function watch(canvas){
  await canvas.evaluate(c=>{c.addEventListener('camerachange',event=>{c.cameraSeen=event.detail;});});
}
async function position(canvas,width,height,worldX,worldY){
  const r=await canvas.boundingBox(),view=await canvas.evaluate(c=>c.cameraSeen??null);
  const scale=Math.min(r.width/width,r.height/height)*(view?.zoom??1);
  return {x:Math.round(r.x+r.width/2+(worldX-(view?.x??width/2))*scale),
    y:Math.round(r.y+r.height/2+(worldY-(view?.y??height/2))*scale),scale};
}
async function drag(from,to){
  await page.mouse.move(from.x,from.y);await page.mouse.down();
  await page.mouse.move(to.x,to.y,{steps:8});await page.mouse.up();
}
async function readyPreview(){
  await page.locator('.preview-status').waitFor({state:'hidden'});
}
const pairLabels=['Mineral emitter','Mineral absorber','Oxygen emitter','Oxygen absorber','Inorganic carbon emitter','Inorganic carbon absorber'];
try{
  await page.goto(process.env.UI_TEST_URL || 'http://127.0.0.1:4173/');
  await page.locator('[data-id="builtin:simulation:nurse-rosette"][data-action="start-template"]').click();
  await page.locator('[data-count="0"]').fill('0');
  await page.getByRole('button',{name:/Start simulation/}).click();
  await page.getByRole('button',{name:'Pause simulation'}).click();
  const player=page.locator('#world'),overlay=page.locator('#player-overlay');
  assert.equal(await overlay.inputValue(),'none');
  await watch(player);
  await action('toggle-environment');
  const tool=page.locator('[data-live-tool]');
  for(const [i,kind] of [3,4,8,9,10,11].entries())assert.equal(await tool.locator(`option[value="source:${kind}"]`).textContent(),pairLabels[i]);
  await tool.selectOption('source:6');
  await enter('strength',-.65);await enter('radius',78);await enter('angle',35);
  await player.press('Enter');await page.locator('[data-live-remove]').waitFor();
  assert.equal(await overlay.inputValue(),'none');
  near(await read('x'),200);near(await read('angle'),35);
  await action('close-environment');await overlay.selectOption('oxygen');
  await page.locator('[data-camera="player:in"]').click();
  const start=await position(player,400,400,200,200);
  await page.keyboard.down('Shift');await drag(start,{x:start.x-39,y:start.y+25});await page.keyboard.up('Shift');
  assert.equal(await page.locator('#live-environment').isVisible(),false,'Pan over a source leaves it untouched');
  const p=await position(player,400,400,200,200),q=await position(player,400,400,243,217);
  await page.touchscreen.tap(p.x,p.y);
  assert.equal(await page.locator('[data-object-title]').textContent(),'Current band');
  near(await read('x'),200);assert.equal(await overlay.inputValue(),'oxygen');
  await action('close-environment');await drag(p,q);
  const movedX=200+(q.x-p.x)/p.scale,movedY=200+(q.y-p.y)/p.scale;
  await expectValue('x',movedX);await expectValue('y',movedY);
  near(await read('radius'),78);near(await read('strength'),-.65);near(await read('angle'),35);
  await player.press('Shift+ArrowRight');await expectValue('x',movedX+10);
  assert.equal(await overlay.inputValue(),'oxygen');

  // Grabbing off-center on an angled floating wall preserves its center-to-grab offset.
  await tool.selectOption('barrier:floating');
  await enter('angle',30);await enter('length',74);await enter('width',12);
  const wall=await position(player,400,400,241,144);
  await page.mouse.click(wall.x,wall.y);await page.locator('[data-object-title]').filter({hasText:'Floating barrier'}).waitFor();
  const bx=await read('x'),by=await read('y');await action('close-environment');
  const b=await position(player,400,400,bx+15,by+8.66),d={x:b.x+54,y:b.y+32};
  await drag(b,d);await expectValue('x',bx+54/b.scale);await expectValue('y',by+32/b.scale);
  assert.equal(await field('floating').inputValue(),'true');near(await read('angle'),30);near(await read('length'),74);near(await read('width'),12);
  const finalWallX=await read('x'),finalWallY=await read('y');

  // Changing the preference from either window updates the other live control.
  await action('close-environment');
  const opened=context.waitForEvent('page');await action('popout-player');const child=await opened;
  await child.setViewportSize({width:420,height:720});
  await page.locator('[data-view="settings"]').click();
  await child.mouse.move(210,350);
  await child.waitForFunction(()=>document.body.dataset.playerControls==='pointer');
  await child.locator('#player-overlay').selectOption('carbon');assert.equal(await page.locator('#setting-overlay').inputValue(),'carbon');
  await page.locator('#setting-overlay').selectOption('none');assert.equal(await child.locator('#player-overlay').inputValue(),'none');
  await child.mouse.move(215,340);
  await action('toggle-environment',child);
  const drawer=child.locator('#live-environment');
  const drawerPoint=await drawer.evaluate(el=>{
    const r=el.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+20};
  });
  await child.mouse.move(-12,340);
  await child.waitForFunction(({x,y})=>{
    const el=document.querySelector('#live-environment');
    return document.body.dataset.playerControls==='hidden' && !el.hidden
      && !el.checkVisibility({opacityProperty:true}) && document.elementFromPoint(x,y)?.id==='world';
  },drawerPoint);
  await child.mouse.move(210,345);
  await child.waitForFunction(()=>document.querySelector('#live-environment').checkVisibility({opacityProperty:true}));
  await action('close-environment',child);
  await action('return-player',child);
  await page.locator('[data-view="player"]').click();
  await action('close-simulation');await page.locator('[data-action="open-simulation"]').first().click();
  assert.equal(await overlay.inputValue(),'none');
  const restored=await position(player,400,400,finalWallX,finalWallY);
  await page.touchscreen.tap(restored.x,restored.y);
  near(await read('x'),finalWallX);near(await read('y'),finalWallY);
  await page.locator('[data-live-remove]').click();await page.locator('[data-object-kind="barrier"]').waitFor({state:'detached'});

  // Dissolved gas readout advances even with no field overlay. A running touch drag uses the same world coordinates.
  await tool.selectOption('source:8');await player.press('Enter');
  await page.locator('[data-object-title]').filter({hasText:'Oxygen emitter'}).waitFor();
  await page.locator('#live-environment .gas-settings summary').click();
  const gasReadout=page.locator('[data-live-gas-readout]');
  await page.waitForFunction(()=>document.querySelector('[data-live-gas-readout]').textContent.includes('oxygen'));
  const baseline=await gasReadout.textContent();
  await page.getByRole('button',{name:'Play simulation'}).click();
  await page.waitForFunction(base=>document.querySelector('[data-live-gas-readout]').textContent!==base,baseline);
  await action('close-environment');
  const t=await position(player,400,400,200,200);
  const touch=await context.newCDPSession(page);
  await touch.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x:t.x,y:t.y,id:1}]});
  await touch.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:t.x+40,y:t.y+24,id:1}]});
  await touch.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
  await touch.detach();
  await expectValue('x',200+40/t.scale,.65);await expectValue('y',200+24/t.scale,.65);
  assert.equal(await overlay.inputValue(),'none');
  await page.getByRole('button',{name:'Pause simulation'}).click();

  // Repeat the same actual gestures against the worker-backed design preview.
  await page.locator('[data-tab="environments"]').click();await action('new-environment');
  await page.locator('#environment-name').fill('Chemical field test');await readyPreview();
  const preview=page.locator('.environment-canvas'),builderOverlay=page.locator('#preview-overlay');
  await watch(preview);await builderOverlay.selectOption('carbon');
  const builderTool=page.locator('[data-environment-tool]');
  await builderTool.selectOption('source:9');await enter('strength',.45);await enter('radius',137);
  await preview.press('Enter');await readyPreview();
  await page.locator('[data-camera="preview:in"]').click();
  const a=await position(preview,1200,760,600,380),z={x:a.x+63,y:a.y-29};
  await drag(a,z);await readyPreview();
  await expectValue('x',600+63/a.scale);await expectValue('y',380-29/a.scale);
  near(await read('strength'),.45);near(await read('radius'),137);assert.equal(await builderOverlay.inputValue(),'carbon');
  await preview.press('ArrowUp');await readyPreview();near(await read('y'),380-29/a.scale-1);
  await builderTool.selectOption('barrier:fixed');await enter('length',152);await enter('width',20);await enter('angle',-25);
  await preview.press('Enter');await readyPreview();
  const gr=await position(preview,1200,760,623,369),end={x:gr.x-43,y:gr.y+22};
  await drag(gr,end);await readyPreview();await expectValue('x',600-43/gr.scale);await expectValue('y',380+22/gr.scale);
  near(await read('angle'),-25);assert.equal(await field('floating').inputValue(),'false');
  const sourcePoint=await position(preview,1200,760,600+63/a.scale,380-29/a.scale-1);
  await page.touchscreen.tap(sourcePoint.x,sourcePoint.y);await page.getByRole('heading',{name:'Oxygen absorber'}).waitFor();
  assert.equal(await builderOverlay.inputValue(),'carbon');assert.equal(await page.locator('#environment-food').count(),0);
  await enter('strength',.62);
  await page.waitForFunction(()=>JSON.parse(localStorage.getItem('cellarium.environment-draft.v1'))?.config.sources[0].strength===.62);
  await enter('strength',.45);
  await page.locator('[data-save]').click();await page.locator('dialog[open]').waitFor({state:'detached'});
  await page.reload();await page.getByRole('button',{name:'Open environment Chemical field test'}).click();await readyPreview();
  assert.equal(await builderOverlay.inputValue(),'carbon');near(await read('radius'),137);near(await read('strength'),.45);
  await page.locator('dialog [data-close]').click();await page.locator('[data-tab="simulation"]').click();
  assert.equal(await overlay.inputValue(),'none');
  assert.deepEqual(errors,[]);
  console.log('Player and builder object picking, touch, drag offset, camera/keyboard controls, six chemical tools, persistence and pop-out overlays passed.');
}catch(error){
  console.error(errors,(await page.locator('body').innerText()).slice(-2300));throw error;
}finally{
  await browser.close();
}
