import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

async function moduleUrl(relative, imports = {}) {
  const source = await readFile(new URL(relative, import.meta.url), 'utf8');
  let code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  for (const [name, url] of Object.entries(imports)) code = code.replaceAll(`from '${name}'`, `from ${JSON.stringify(url)}`);
  return `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`;
}
const ui = await moduleUrl('../src/ui.ts');
const controls = await import(await moduleUrl('../src/environment-controls.ts', { './ui': ui }));
// Exercise the generated numeric bindings without requiring a browser process.
const fields = html => [...html.matchAll(/<input\b([^>]+)>/g)].map(([, attributes]) => {
  const attrs = Object.fromEntries([...attributes.matchAll(/([\w-]+)="([^"]*)"/g)].map(([, key, value]) => [key, value]));
  return { value: attrs.value, min: attrs.min, max: attrs.max,
    dataset: { objectField: attrs['data-object-field'], presentedValue: attrs['data-presented-value'] },
    get valueAsNumber() { return this.value === '' ? NaN : Number(this.value); }, getAttribute(key) { return attrs[key] ?? null; } };
});
const form = inputs => ({ querySelectorAll() { return inputs; } });
const source = { ...controls.defaultSource(6, 1199.99999, 12.345678), strength: Math.fround(.7), angle: Math.PI / 7 };
const inputs = fields(controls.sourceFields(source, 1200, 760));
assert.equal(inputs.find(input => input.dataset.objectField === 'strength').value, '0.7');
assert.equal(inputs.find(input => input.dataset.objectField === 'x').value, '1199.99');
assert.deepEqual(controls.readObjectFields(form(inputs), source, 1200, 760), source, 'Viewing rounded controls must not round the stored recipe.');
inputs.find(input => input.dataset.objectField === 'strength').value = '0.45';
const changed = controls.readObjectFields(form(inputs), source, 1200, 760);
assert.equal(changed.strength, .45);
assert.equal(changed.x, source.x);
assert.equal(changed.angle, source.angle, 'Editing strength must preserve unedited angle precision.');
inputs.find(input => input.dataset.objectField === 'angle').value = '90';
assert.equal(controls.readObjectFields(form(inputs), source, 1200, 760).angle, Math.PI / 2);
const absorber = controls.defaultSource(4, 30, 30);
const absorberInputs = fields(controls.sourceFields(absorber, 1200, 760));
absorberInputs.find(input => input.dataset.objectField === 'strength').value = '-0.1';
assert.throws(() => controls.readObjectFields(form(absorberInputs), absorber, 1200, 760), /Strength/);
const barrier = { ...controls.defaultBarrier(true, 24.123456, 45.654321), angle: .123456789 };
const barrierInputs = fields(controls.barrierFields(barrier, 400, 400));
barrierInputs.push({ dataset: { objectField: 'floating' }, value: 'false' });
assert.deepEqual(controls.readObjectFields(form(barrierInputs), barrier, 400, 400), { ...barrier, floating: false });
console.log('Environment controls: precision-preserving display, bounded coordinates, independent edits, degree conversion and absorber validation passed.');
