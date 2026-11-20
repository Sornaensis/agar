import assert from 'node:assert/strict';

// Run against a production preview. Playwright and a browser are optional local
// test tools; the app itself has no browser automation dependency.
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({ headless: true,
  ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
  args: ['--no-sandbox', '--disable-dev-shm-usage'] });
const context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
const page = await context.newPage();
const failures = [];
page.on('pageerror', error => failures.push(error.message));
page.on('console', message => { if (message.type() === 'error') failures.push(message.text()); });
const base = process.env.UI_TEST_URL || 'http://127.0.0.1:4173/';
const textIs = (selector, value) => page.waitForFunction(({ selector, value }) => document.querySelector(selector)?.textContent === value, { selector, value });
const readSaved = () => page.evaluate(() => new Promise((resolve, reject) => {
  const open = indexedDB.open('cellarium');
  open.onerror = () => reject(open.error);
  open.onsuccess = () => {
    const database = open.result, transaction = database.transaction('simulations'), request = transaction.objectStore('simulations').getAll();
    request.onsuccess = () => resolve(request.result.map(({ state, ...metadata }) => ({ ...metadata, stateBytes: state.byteLength })));
    request.onerror = () => reject(request.error);
    transaction.oncomplete = () => database.close();
  };
}));

try {
  await page.goto(base);
  await textIs('#save-state', 'Saved');
  const presetCards = page.locator('[data-action="start-template"]');
  assert.equal(await presetCards.count(), 1);
  assert.ok((await presetCards.first().locator('.preset-founders').innerText()).includes('Predator'));
  const launchControls = () => page.locator('.new-simulation-dialog[open]').evaluate(dialog => ({
    environment: dialog.querySelector('#simulation-environment').value,
    seed: dialog.querySelector('#simulation-seed').value,
    founders: [...dialog.querySelectorAll('[data-population]')].map((input,index) =>
      [input.value, dialog.querySelector(`[data-count="${index}"]`).value]),
  }));
  await page.locator('[data-action="new-simulation"]').click();
  const defaultStart = await launchControls();
  await page.locator('.new-simulation-dialog[open] [data-close]').click();
  await presetCards.first().click();
  assert.deepEqual(await launchControls(), defaultStart);
  await page.locator('.new-simulation-dialog[open] [data-close]').click();
  await presetCards.first().click();
  assert.equal(defaultStart.environment, 'builtin:environment:springwater-basin');
  assert.equal(defaultStart.founders.length, 9);
  while (await page.locator('[data-remove-population]').count() > 1) {
    await page.locator('[data-remove-population]').last().click();
  }
  await page.locator('#simulation-environment').selectOption('builtin:environment:still-water');
  await page.locator('[data-population="0"]').selectOption('builtin:genome:forager');
  await page.locator('#simulation-name').fill('Worker smoke');
  await page.locator('[data-count="0"]').fill('12');
  await page.getByRole('button', { name: /Start simulation/ }).click();
  await page.locator('#world').waitFor();
  await page.getByRole('button', { name: 'Pause simulation' }).click();
  await textIs('#save-state', 'Saved');
  assert.equal(page.workers().length, 1, 'One worker owns the active simulation');
  let saved = await readSaved();
  assert.equal(saved.length, 1); assert.equal(saved[0].paused, true); assert.ok(saved[0].stateBytes > 1000);
  const pausedTime = saved[0].summary.time;

  await page.locator('#world').focus(); await page.keyboard.press('ArrowRight');
  await page.locator('#inspector-content .inspector-metrics').waitFor();
  await page.locator('[data-food-potential] > summary').click();
  await page.waitForFunction(() => Number(document.querySelector('[data-food-pool="total"]')?.textContent.replaceAll(',', '')) > 0);
  const foodEnergy = await page.locator('[data-food-pool]').evaluateAll(fields =>
    Object.fromEntries(fields.map(field => [field.dataset.foodPool,Number(field.textContent.replaceAll(',', ''))])));
  assert.ok(Math.abs(foodEnergy.total-foodEnergy.body-foodEnergy.reserve-foodEnergy.gut-foodEnergy.atp)<0.03);
  await page.locator('[data-food-potential] > summary').click();
  await page.locator('[data-inspector-view="body"]').click();
  await page.locator('.cellular-body').waitFor();
  await page.locator('[data-kind="cellular-budgets"] > summary').click();
  await page.locator('.body-capacity').waitFor();
  await page.locator('[data-inspector-view="memory"]').click();
  await page.waitForFunction(() => document.querySelector('#memory-summary')?.textContent?.includes('registers'));
  await page.locator('[data-inspector-view="program"]').click();
  await page.locator('#organism-program .cm-editor').waitFor();
  await page.locator('#organism-program .program-evaluation-time').waitFor({ state: 'visible' });
  const program = page.locator('#organism-program');
  await program.locator('.program-signal-select.read').selectOption('peer.signal');
  assert.match(await program.locator('.program-reference-example').textContent(), /engage_when: peer.signal_valid && peer.signal > 0.5/);
  await program.locator('.program-signal-select.write').selectOption('engage_when');
  assert.match(await program.locator('.program-reference-example').textContent(), /peer.message != rf001/);
  assert.equal(await program.locator('option[value="avoid_message"],option[value="adhere_message"]').count(), 0);
  await program.locator('.program-reference-close').click();
  await page.locator('[data-action="cell-program"]').click();
  await page.locator('.live-program-dialog .program-evaluation-time').waitFor({ state: 'visible' });
  await page.locator('.live-program-dialog [data-close]').click();

  await page.locator('[data-action="toggle-environment"]').click();
  await page.locator('[data-live-tool]').selectOption('source:6');
  await page.locator('#world').press('Enter');
  await page.locator('#live-environment [data-live-remove]').waitFor();
  // Adjacent changes must preserve one another despite crossing the worker boundary.
  await page.evaluate(() => {
    for (const [field, value] of [['strength', '-.6'], ['radius', '140'], ['angle', '90']]) {
      const input = document.querySelector(`#live-environment [data-object-field="${field}"]`); input.value = value; input.dispatchEvent(new Event('change', { bubbles: true }));
    }
  });
  await textIs('#save-state', 'Saved');
  await page.locator('[data-action="close-environment"]').click();
  await page.locator('[data-view="settings"]').click();
  assert.equal(await page.locator('#setting-current,#setting-temperature,#setting-light').count(),0,'Environment editing is per object');
  await page.locator('#setting-overlay').selectOption('nutrients');
  await page.locator('[data-view="metrics"]').click();
  await textIs('#metric-population', '12');
  const lineage = page.getByRole('button', {name:'Show population history and physiology for Forager'});
  await lineage.click();
  assert.deepEqual(await page.locator('[data-ecology-rows] tr').first().locator('td').allTextContents(),
    ['12','0','0','0','None recorded']);
  assert.match(await page.locator('[data-ecology-detail]').innerText(), /Introduced\s+12/);
  const reserves = await page.locator('.ecology-reserves').innerText();
  assert.match(reserves, /ATP ≤10% · reserve fuel ≥25%\s+0 \/ 12/);
  assert.match(reserves, /Cells with no free minerals\s+0 \/ 12/);
  assert.match(reserves, /Free minerals\s+[\d,]+ counts/);
  assert.match(reserves, /Stored organic carbon\s+[\d,]+ counts/);
  assert.match(reserves, /Food energy in guts\s+0 counts/);
  assert.match(await page.locator('[data-ecology-plot]').getAttribute('aria-label'), /^Forager, 12 at /);
  await page.locator('[data-ecology-selection]').selectOption('0');
  assert.equal(await page.locator('[data-ecology-detail]').isVisible(), false);
  await textIs('[data-budget-status]', 'Minerals, carbon, oxygen and energy balance.');
  await page.locator('[data-energy-detail] > summary').click();
  assert.match(await page.locator('[data-energy-loss]').innerText(), /Difference\s+0$/);
  await page.setViewportSize({width:375,height:812});
  const fits = await page.locator('#ecology-panel').evaluate(el => el.scrollWidth <= el.clientWidth
    && [...el.querySelectorAll('tbody td')].every(td => td.getBoundingClientRect().right <= innerWidth));
  assert.ok(fits, 'Lineage census fits a narrow viewport');
  await page.setViewportSize({width:1400,height:1000});
  const balance = await page.evaluate(() => Object.fromEntries(['introduced-energy','light-energy','withdrawn-energy','held-energy','packet-energy','dissipated-energy']
    .map(key => [key,Number(document.getElementById(key).textContent)])));
  assert.ok(balance['packet-energy']>=0 && balance['held-energy']>=balance['packet-energy']);
  assert.ok(Math.abs(balance['introduced-energy']+balance['light-energy']-balance['held-energy']-balance['withdrawn-energy']-balance['dissipated-energy'])<0.03);
  assert.equal(balance['packet-energy'], 0, 'Freshly introduced cells begin in water with no organic particles');
  assert.equal(await page.locator('#mean-particle-carbon').textContent(), '', 'An empty particle pool has no mean carbon value');
  await page.locator('[data-view="genomes"]').click();
  await page.locator('[data-genome-stats]').waitFor();
  assert.match(await page.locator('[data-genome-stats]').first().textContent(), /12 organisms/);
  await page.locator('[data-view="player"]').click();
  await page.getByRole('button', { name: 'Add organism', exact: true }).click();
  await page.getByRole('button', { name: 'Add Forager', exact: true }).click();
  await page.locator('#cell-tool-status').waitFor();
  const worldBounds = await page.locator('#world').boundingBox();
  await page.mouse.click(worldBounds.x + worldBounds.width * 0.32, worldBounds.y + worldBounds.height * 0.38);
  await page.waitForFunction(() => document.querySelector('#player-caption')?.textContent?.startsWith('13 organisms'));
  await page.locator('[data-action="close-simulation"]').click();
  await page.locator('[data-action="open-simulation"]').waitFor();
  await Promise.all(page.workers().map(worker => worker.waitForEvent('close', { timeout: 5000 })));
  assert.equal(page.workers().length, 0, 'Closing flushes the run and terminates its worker');
  saved = await readSaved();
  assert.equal(saved[0].summary.population, 13); assert.equal(saved[0].summary.time, pausedTime);
  await page.locator('[data-action="open-simulation"]').first().click();
  await page.locator('#world').waitFor();
  assert.equal(page.workers().length, 1);
  await page.locator('[data-action="toggle-environment"]').click();
  await page.locator('#live-environment [data-object-kind="source"]').click();
  assert.ok(Math.abs(Number(await page.locator('#live-environment [data-object-field="strength"]').inputValue())+.6)<1e-6);
  assert.equal(await page.locator('#live-environment [data-object-field="radius"]').inputValue(), '140');
  assert.equal(await page.locator('#live-environment [data-object-field="angle"]').inputValue(), '90');
  await page.locator('[data-action="close-environment"]').click();
  await page.getByRole('button', { name: 'Play simulation' }).click();
  await page.waitForFunction(() => document.querySelector('#play-pause')?.getAttribute('aria-label') === 'Pause simulation');
  await page.locator('[data-view="metrics"]').click();
  await page.locator('[data-view="player"]').click();
  await page.getByRole('button', { name: 'Pause simulation' }).click();
  await textIs('#save-state', 'Saved');
  saved = await readSaved(); assert.ok(saved[0].summary.time >= pausedTime);

  // A second tab must not acquire the run held by the first tab.
  const second = await context.newPage();
  await second.goto(base); await second.locator('.simulation-list, [data-action="open-simulation"]').first().waitFor();
  await second.waitForFunction(() => document.body.textContent?.includes('already open in another tab'));
  assert.equal(second.workers().length, 0); await second.close();

  await page.getByRole('tab', { name: 'Environments', exact: true }).click();
  await page.locator('[data-action="open-environment"][data-id="builtin:environment:sunlit-pond"]').click();
  await page.locator('.environment-editor-dialog[open] .preview-status').waitFor({ state: 'hidden' });
  assert.equal(page.workers().length, 2, 'Environment preview uses its own worker');
  await page.locator('.environment-editor-dialog[open] [data-copy]').click();
  await page.locator('.environment-editor-dialog[open] .preview-status').waitFor({ state: 'hidden' });
  const editableEnvironment = page.getByRole('dialog', { name: 'Edit copy' });
  await editableEnvironment.locator('#environment-width').fill('2400');
  await editableEnvironment.locator('#environment-height').fill('1520');
  await page.locator('.environment-editor-dialog[open] [data-object-field="strength"]').fill('.65');
  await page.locator('.environment-editor-dialog[open] [data-close]').click();
  await page.waitForFunction(() => !document.querySelector('.environment-editor-dialog'));
  await page.getByRole('tab', { name: 'Simulation', exact: true }).click();
  await page.locator('#world').waitFor();
  await page.locator('[data-action="close-simulation"]').click();
  await page.locator('[data-action="open-simulation"]').waitFor();
  await Promise.all(page.workers().map(worker => worker.waitForEvent('close', { timeout: 5000 })));
  assert.equal(page.workers().length, 0, 'All closed workers have been disposed');
  // Import an actual old document via the file chooser, then reopen it from storage.
  await page.getByRole('tab', { name: 'Genomes', exact: true }).click();
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Import genome' }).click();
  await (await chooser).setFiles({name:'Contact import.cell',mimeType:'text/plain',buffer:Buffer.from(
    'cellular counts;\npublish rf001 = 0;\navoid_message rf001;\nproduct Mount = anchor(); product Fiber = filament(); product Fuel = enzyme();\nassembly Tip: pierce() { Mount: 150; Fiber: 200; Fuel: 150; }')});
  const imported = page.getByRole('button',{name:'Open genome Contact import',exact:true});
  await imported.waitFor();
  await page.reload();
  await imported.click();
  const importedCode = page.locator('.genome-editor-dialog[open] #genome-source .cm-content');
  await importedCode.waitFor();
  const upgradedSource = await importedCode.innerText();
  assert.match(upgradedSource,/engage_when:/);
  assert.match(upgradedSource,/peer\.message_valid/);
  assert.doesNotMatch(upgradedSource,/avoid_message/);
  await page.locator('.genome-editor-dialog[open] [data-close]').click();
  assert.deepEqual(failures, [], 'No browser or worker errors');
  console.log('Browser checks passed: production worker loading, pause/save/reopen, adjacent per-source edits, compact metrics, all inspector modes, live evaluation, contact references, persisted legacy .cell import, insertion, exclusive locks, resized asynchronous previews and worker disposal.');
} catch (error) {
  console.error('Browser errors:', failures);
  console.error('Visible page:', (await page.locator('body').innerText()).slice(0, 6000));
  throw error;
} finally { await browser.close(); }
