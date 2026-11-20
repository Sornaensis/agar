// Drive the actual catalog editor and native compiler against a local app preview.
import assert from 'node:assert/strict';
const {chromium} = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({headless:true, ...(process.env.CHROMIUM_PATH ? {executablePath:process.env.CHROMIUM_PATH} : {}),
  args:['--no-sandbox','--disable-dev-shm-usage']});
const page = await browser.newPage({viewport:{width:1280,height:960}});
const errors = [];
page.on('pageerror', e => errors.push(e.message));
const editor = page.locator('.genome-editor-dialog[open] .program-editor');
const code = editor.locator('.cm-content');
const panel = editor.getByRole('region', {name:'Source navigation'});
const commands = editor.getByRole('combobox', {name:'Editor commands'});
const source = [
  '# 🧬 Material and gain must stay unchanged here.',
  'cellular;',
  'product Material = enzyme();',
  'let gain = 2;',
  'fn scaleBy(gain) = gain + age;',
  'fn echo(gain) = gain;',
  'gene make: Material = scaleBy(gain) + pool.Material;',
  'assembly Bag: digest(soluble) { Material: 0.03; activity: gain; }',
  'rf001 = built.Bag + scaleBy(gain);',
  'rs001 = "Material";',
  'fn independent(Material) = Material;',
  'product A = anchor(); product R = receptor(); product F = filament();',
  'assembly Touch: sense(contact) { A: .01; R: .01; F: .01; }',
  'rb001 = sensed.Touch.valid;',
  'product Glue = adhesin(Frame); product Frame = scaffold();',
].join('\n');
const ready = () => editor.locator('.program-check-state').filter({hasText:'Valid program'}).waitFor();
const outline = async name => {
  await commands.selectOption('outline');
  const filter = panel.getByRole('searchbox');
  await filter.fill(name);
  await filter.press('Enter');
};
const rename = async name => {
  await page.keyboard.press('F2');
  await panel.getByRole('textbox', {name:'New symbol name'}).fill(name);
  await panel.getByRole('button', {name:'Rename',exact:true}).click();
};
const text = () => code.locator('.cm-line').allTextContents().then(lines => lines.join('\n'));
try {
  await page.goto(process.env.UI_TEST_URL || 'http://127.0.0.1:4173/');
  await page.getByRole('tab', {name:'Genomes',exact:true}).click();
  await page.locator('[data-action="new-genome"]').click();
  const replaceProgram = async program => {
    await code.click();
    await page.keyboard.press('Control+a');
    await page.keyboard.insertText(program);
  };
  await replaceProgram('cellular counts; product Skin=membrane(); assembly Tank:store(food){ Skin:400; at:1.25; direction:0.4; activity:0; }');
  await editor.locator('.program-check-state').filter({hasText:'3 warnings'}).waitFor();
  const warnings = editor.locator('.program-diagnostics [data-severity="warning"]');
  assert.equal(await warnings.count(),3);
  await warnings.first().getByRole('button').click();
  assert.equal(await page.evaluate(() => getSelection()?.toString()), '1.25');
  await page.locator('.genome-editor-dialog[open] #genome-name').fill('Storage warning test');
  await page.locator('.genome-editor-dialog[open] [data-save]').click();
  await page.locator('.genome-editor-dialog[open]').waitFor({state:'detached'});
  await page.getByText('Storage warning test', {exact:true}).first().waitFor();
  await page.locator('[data-action="new-genome"]').click();
  await replaceProgram('cellular; let first = broke; let second = true + 1; let good = 1;');
  await editor.locator('.program-check-state').filter({hasText:'2 errors'}).waitFor();
  assert.equal(await editor.locator('.program-diagnostics [data-severity="error"]').count(),2);
  await replaceProgram(source);
  await ready();

  const typo = source.replace('sensed.Touch.valid;', 'sensed.Touch.vlaid;');
  await code.click(); await page.keyboard.press('Control+a'); await page.keyboard.insertText(typo);
  const invalid = code.locator('.cm-lintRange-error');
  await invalid.first().hover();
  const correction = page.getByRole('button',{name:'Replace with sensed.Touch.valid'});
  await correction.click();
  await ready();
  assert.equal(await text(),source);
  await code.focus(); await page.keyboard.press('Control+z');
  assert.equal(await text(),typo,'One undo restores exactly the diagnostic token');
  await page.keyboard.press('Control+Shift+z'); await ready();

  await outline('make');
  await page.keyboard.press('Home');
  for (let i=0; i<'gene make: Material = scaleBy(ga'.length; i++) await page.keyboard.press('ArrowRight');
  assert.match(await editor.locator('.program-call-argument').innerText(), /Argument 1 of scaleBy · gain/);

  await outline('Material');
  await page.keyboard.press('Shift+F12');
  assert.match(await panel.innerText(), /Material · 3 references/);
  await panel.getByRole('button', {name:/pool\.Material/}).last().click();
  await page.keyboard.press('F12');
  assert.match(await editor.locator('.program-position').innerText(), /Ln 3,/);
  await rename('energy');
  await panel.getByRole('status').filter({hasText:/reserved/}).waitFor();
  assert.equal(await text(),source);
  await panel.getByRole('textbox', {name:'New symbol name'}).fill('Cargo');
  await panel.getByRole('button', {name:'Rename', exact:true}).click();
  await ready();
  const renamed = source.replace('product Material','product Cargo').replace('make: Material','make: Cargo')
    .replace('pool.Material','pool.Cargo').replace('{ Material:', '{ Cargo:');
  assert.equal(await text(), renamed);
  await page.keyboard.press('Control+z');
  assert.equal(await text(), source);
  await page.keyboard.press('Control+Shift+Z');
  assert.equal(await text(), renamed);

  await outline('scaleBy');
  await page.keyboard.press('Home');
  for (let i=0; i<'fn scaleBy(ga'.length; i++) await page.keyboard.press('ArrowRight');
  await rename('age');
  assert.match(await panel.getByRole('status').innerText(), /change which declaration/);
  assert.equal(await text(), renamed);
  await panel.getByRole('textbox', {name:'New symbol name'}).fill('dose');
  await panel.getByRole('button', {name:'Rename', exact:true}).click();
  await ready();
  assert.equal(await text(), renamed.replace('scaleBy(gain) = gain', 'scaleBy(dose) = dose'));

  await commands.selectOption('search');
  const search = editor.locator('.cm-search input[name=search]');
  await search.press('Control+a'); await search.pressSequentially('built.Bag');
  await editor.locator('.cm-search input[name=search]').press('Enter');
  await editor.locator('.cm-search input[name=search]').press('Escape');
  await page.keyboard.press('F12');
  assert.match(await editor.locator('.program-position').innerText(), /Ln 8,/);
  await commands.selectOption('line');
  const line = editor.locator('.cm-goto-line input');
  await line.fill('4'); await line.press('Enter');
  assert.match(await editor.locator('.program-position').innerText(), /Ln 4,/);
  await outline('Touch');
  await page.keyboard.press('Shift+F12');
  await panel.getByRole('button', {name:/sensed.Touch.valid/}).click();
  await rename('ContactTip');
  await ready();
  assert.match(await text(), /sensed\.ContactTip\.valid/);
  await outline('Frame');
  await page.keyboard.press('Shift+F12');
  assert.match(await panel.innerText(), /Frame · 1 reference/);
  await panel.getByRole('button', {name:/adhesin\(Frame\)/}).click();
  await page.keyboard.press('F12');
  assert.match(await editor.locator('.program-position').innerText(), /Ln 15,/);
  const foldingSource = [
    'cellular;', 'product M = enzyme();', 'gene prepare: M = 1;',
    'assembly Pocket:', '  digest(soluble) { # a quoted-looking } ends nothing',
    '  M: 0.03;', '  activity: if "{" == "}" then 0 else 1;', '}',
    '# assembly Fiction: digest(soluble) {', 'rs001 = "assembly Ghost: digest(soluble) {";',
    'assembly Crystal: digest(soluble) {', '  M: 0.02;', '}',
  ].join('\n');
  await code.click(); await page.keyboard.press('Control+a'); await page.keyboard.insertText(foldingSource);
  await ready();
  const foldingGutter = editor.locator('.cm-foldGutter');
  const fold = foldingGutter.locator('[title="Fold line"]');
  const folded = code.locator('.cm-foldPlaceholder');
  assert.equal(await fold.count(),2);
  await fold.first().click();
  assert.equal(await folded.count(),1);
  assert.doesNotMatch(await text(),/M: 0\.03/);
  await outline('product');
  await page.keyboard.press('Shift+F12');
  await panel.getByRole('button', {name:/M: 0\.03/}).click();
  assert.equal(await folded.count(),0, 'A reference jump reveals the selected code');
  assert.match(await text(), /M: 0\.03/);
  await outline('Crystal');
  await page.keyboard.press('Control+Shift+BracketLeft');
  assert.equal(await folded.count(),1);
  await page.keyboard.press('Control+Shift+BracketRight');
  assert.equal(await folded.count(),0);
  await fold.last().click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.insertText('# moved header\n');
  await ready();
  assert.equal(await folded.count(),1,'Unrelated edits keep the right assembly folded');
  await outline('Crystal');
  await page.keyboard.press('Home');
  await page.keyboard.insertText('#');
  assert.equal(await folded.count(),0, 'Commenting out a header removes its obsolete fold');
  assert.equal(await fold.count(),1);
  await page.keyboard.press('Control+z');
  await ready();
  assert.equal(await fold.count(),2);

  await page.locator('.genome-editor-dialog[open] [data-close]').click();

  await page.locator('[data-action="open-genome"][data-id="builtin:genome:predator"]').click();
  assert.equal(await commands.locator('option[value=rename]').isDisabled(), true);
  await page.setViewportSize({width:390,height:844});
  await commands.selectOption('outline');
  await panel.getByRole('searchbox').fill('assembly');
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  assert.ok(await panel.locator('.program-nav-results button').count() > 0);
  await panel.locator('.program-nav-results button').first().click();
  await commands.selectOption('fold');
  assert.ok(await folded.count() > 0, 'Read-only stock assemblies can fold from the command picker');
  await commands.selectOption('fold');
  assert.equal(await folded.count(),0);
  await commands.selectOption('outline');
  await panel.getByRole('searchbox').press('Escape');
  assert.equal(await panel.isVisible(),false);
  assert.deepEqual(errors,[]);
  console.log('Editor outline, scoped references, capture-safe rename, undo/redo, definition jumps, search, folding and read-only inspection passed.');
} finally { await browser.close(); }
