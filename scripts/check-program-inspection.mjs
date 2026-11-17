import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

// Exercise the actual TypeScript boundary and compiled Rust evaluator together.
const contract = JSON.parse(await readFile(new URL('../src/engine-contract.json', import.meta.url), 'utf8'));
const source = (await readFile(new URL('../src/engine.ts', import.meta.url), 'utf8'))
  .replace("import engineUrl from '../engine/pkg/agar_engine.wasm?url';", 'const engineUrl = "https://agar.invalid/inspection.wasm";')
  .replace("import engineContract from './engine-contract.json';", `const engineContract = ${JSON.stringify(contract)};`);
const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { Engine, validateGenomeEvaluation } = await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
const bytes = await readFile(new URL('../engine/pkg/agar_engine.wasm', import.meta.url));
const originalFetch = globalThis.fetch;
globalThis.fetch = async () => new Response(bytes);
let engine;
try { engine = await Engine.load(); } finally { globalThis.fetch = originalFetch; }

const program = '# café 🌱 — unchanged source\r\n' + [
  'let count = ri001 + 1;',
  'let localEnergy = energy;',
  'let builtMotors = organelle.flagellum.count;',
  'let seen = object.available && object.valid;',
  'let tracked = object.token;',
  'let answer = if rb001 then 7 else 9;',
  'ri001 = count;',
  'clear ri001 when age > 1;',
  'rb001 = !populated(rb001);',
  'rf001 = localEnergy;',
  'rs001 = "🌱";',
  'rv001 = vec(energy.fraction, light.level);',
  'target.flagellum.count = 2; target.flagellum.size = 0.04;',
  'build.flagellum = 0.5; scan.target = object.token;',
  'thrust = 2; turn = -2; divide = false;',
].join('\n');
const genome = engine.compile(program);
engine.resetEmpty(42, 0);
engine.seed(genome, 2);
const cellId = engine.snapshot().cells[0].id;
const before = engine.saveState();
const trace = engine.getCellEvaluation(cellId);
assert.equal(trace.source, program);
assert.equal(trace.cellId, cellId);
assert.equal(trace.genomeId, genome);
assert.equal(trace.semantics, 'current-snapshot');
assert.equal(trace.tick, 0);
assert.equal(trace.bindings.find(item => item.name === 'count').value, 1);
assert.equal(trace.bindings.find(item => item.name === 'answer').value, 9);
assert.equal(trace.bindings.find(item => item.name === 'builtMotors').type,'integer');
assert.equal(trace.bindings.find(item => item.name === 'builtMotors').value,0);
assert.equal(trace.bindings.find(item => item.name === 'seen').value,false);
assert.equal(trace.bindings.find(item => item.name === 'tracked').value,'');
assert.equal(trace.outputs.find(item => item.name === 'target.flagellum.count').value,2);
assert.equal(trace.outputs.find(item => item.name === 'target.flagellum.count').type,'integer');
assert.equal(trace.outputs.find(item => item.name === 'scan.target').type,'string');
assert.equal(trace.signals.length,144); assert.equal(trace.outputs.length,78);
const bodyBefore=engine.cellBody(cellId);
assert.ok(bodyBefore.spaceUsed<=bodyBefore.spaceCapacity);
assert.ok(bodyBefore.surfaceUsed<=bodyBefore.surfaceCapacity);
assert.equal(bodyBefore.requested.find(item=>item.kind==='flagellum').count,0,'Body inspection shows last committed requests; preview does not commit them');
assert.ok(bodyBefore.organs.every(organ=>organ.id>0&&organ.mass>0));
for (const binding of trace.bindings) assert.equal(program.slice(binding.from, binding.to), binding.name, 'Unicode positions are UTF-16 offsets');
const literal = trace.expressions.find(item => program.slice(item.from, item.to) === '"🌱"');
assert.equal(literal?.value, '🌱');
assert.equal(literal?.type, 'string');
assert.equal(trace.outputs.find(item => item.name === 'thrust').value, 1, 'Actions show clamped command values');
assert.equal(trace.outputs.find(item => item.name === 'turn').value, -1);
assert.equal(trace.outputs.find(item => item.name === 'feed').from, null, 'Implicit defaults do not invent a source span');
assert.equal(trace.writes.find(item => item.name === 'ri001' && item.kind === 'write').applies, true);
assert.equal(trace.writes.find(item => item.name === 'ri001' && item.kind === 'clear').applies, false);
engine.getCellEvaluation(cellId);
assert.deepEqual(engine.getMemory(cellId), [], 'Inspecting writes never commits memory');
assert.deepEqual(engine.saveState(), before, 'Inspecting leaves world, RNG, registry and checkpoint bytes intact');

engine.tick(5);
const afterTicks = engine.saveState();
const current = engine.getCellEvaluation(cellId);
const bodyCurrent = engine.cellBody(cellId);
assert.equal(bodyCurrent.requested.find(item=>item.kind==='flagellum').count,2);
assert.equal(current.tick, 5);
assert.equal(current.bindings.find(item => item.name === 'count').value, 6, 'Preview reads current memory without incrementing it');
assert.equal(engine.getMemory(cellId).find(item => item.name === 'ri001').value, 5);
assert.deepEqual(engine.saveState(), afterTicks);
engine.restoreState(afterTicks);
assert.deepEqual(engine.getCellEvaluation(cellId), current, 'Restored programs retain correct source/value correspondence');
assert.deepEqual(engine.cellBody(cellId),bodyCurrent,'Restored bodies preserve exact organ identity, mass, damage and targets');
assert.throws(() => engine.getCellEvaluation(0), /living organism/);
assert.throws(() => engine.getCellEvaluation(0x1_0000_0000), /living organism/);
assert.throws(() => engine.getCellEvaluation(0xffff_ffff), /organism|cell|available|found/i);
assert.deepEqual(engine.saveState(), afterTicks, 'Failed inspection is also side-effect-free');

// A malformed boundary must not attach values from a different cell or source.
const reject = edit => { const copy = structuredClone(current); edit(copy); assert.throws(() => validateGenomeEvaluation(copy, cellId), /invalid program evaluation/); };
reject(copy => copy.cellId++);
reject(copy => copy.expressions[0].to = program.length + 1);
reject(copy => copy.expressions[0].value = { arbitrary: 'object' });
reject(copy => copy.signals[1].name = copy.signals[0].name);
reject(copy => copy.bindings[0].from++);
reject(copy => copy.writes[0].name = 'ri000');
reject(copy => copy.outputs[0].to = null);
reject(copy => copy.source = 'changed source');
assert.equal(validateGenomeEvaluation(current, cellId), current);

const conflict = engine.compile('ri001 = ri001 + 1; clear ri001; thrust=0;turn=0;divide=false;');
engine.resetEmpty(1, 0); engine.seed(conflict, 1);
const cleared = engine.getCellEvaluation(engine.snapshot().cells[0].id);
assert.equal(cleared.writes.find(item => item.kind === 'write').guard, true);
assert.equal(cleared.writes.find(item => item.kind === 'write').applies, false, 'Clear precedence is visible before committing');
assert.equal(cleared.writes.find(item => item.kind === 'clear').applies, true);

console.log('Program inspection passed: real WASM values, Unicode ranges, snapshots, register previews, persistence, and boundary validation.');
