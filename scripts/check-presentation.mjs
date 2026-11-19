import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import ts from 'typescript';

const options = { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext };
const compile = source => ts.transpileModule(source, { compilerOptions: options }).outputText;
const url = source => `data:text/javascript;base64,${Buffer.from(compile(source)).toString('base64')}`;
const settle = async () => { for (let i = 0; i < 16; i++) await Promise.resolve(); };

// Use the actual worker dispatcher with counters around deliberately expensive
// projections. Summary-only work must never touch those functions or buffers.
{
  const protocol = await readFile(new URL('../src/simulation-protocol.ts', import.meta.url), 'utf8');
  const runtime = (await readFile(new URL('../src/simulation-worker-runtime.ts', import.meta.url), 'utf8'))
    .replace("import { Engine } from './engine';", 'const Engine = {};')
    .replace("from './simulation-protocol'", `from '${url(protocol)}'`);
  const { createSimulationWorkerHandler } = await import(url(runtime));
  const calls = { frame: 0, fields: 0, metadata: 0, metrics: 0, status: 0 };
  const engine = {
    tickValue: 0, fail: false,
    summary() { return { time: this.tickValue / 30, population: 0, food: 0, births: 2, deaths: 2, generation: 1 }; },
    status() { calls.status++; return { tick: this.tickValue, summary: this.summary() }; },
    tick(steps) { this.tickValue += this.fail ? 2 : steps; if (this.fail) throw new Error('Partial tick failure'); },
    renderSnapshotData() { calls.frame++; const data = new Float64Array(12); data[0] = this.tickValue; data[1] = this.tickValue / 30; data[4] = 2; data[5] = 2; data[11] = 1; return data; },
    metricsSnapshotData() { calls.metrics++; return Float64Array.of(this.tickValue); },
    environmentMetadata() { calls.metadata++; return { sources: [], barriers: [], columns: 1, rows: 1 }; },
    environment() { calls.fields++; return { sources: [], barriers: [], columns: 1, rows: 1, samples: new Float32Array(7) }; },
  };
  const pending = new Map(); let id = 0;
  const handle = createSimulationWorkerHandler((reply, buffers) => {
    const copied = structuredClone(reply, { transfer: buffers });
    for (const buffer of buffers) assert.equal(buffer.byteLength, 0);
    pending.get(reply.id)({ reply: copied, transfers: buffers.length }); pending.delete(reply.id);
  }, async () => engine);
  const request = (method, args = []) => new Promise(resolve => { pending.set(++id, resolve); handle({ id, method, args }); });
  assert.equal((await request('init')).reply.ok, true);
  let response = await request('advance', [8, { frame: false }]);
  assert.equal(response.reply.value.tick, 8); assert.deepEqual(response.reply.value.summary, engine.summary());
  assert.equal(response.transfers, 0); assert.equal('frame' in response.reply.value, false);
  assert.deepEqual(calls, { frame: 0, fields: 0, metadata: 0, metrics: 0, status: 1 });
  response = await request('presentation', [{ frame: false, environment: 'metadata' }]);
  assert.equal(response.transfers, 0); assert.equal('samples' in response.reply.value.environment, false);
  assert.equal(calls.metadata, 1); assert.equal(calls.fields, 0); assert.equal(calls.frame, 0);
  engine.fail = true;
  response = await request('advance', [8, { frame: false, metrics: true }]);
  assert.equal(response.reply.value.tick, 10); assert.match(response.reply.value.error, /Partial/);
  assert.equal(calls.frame, 0); assert.equal(response.transfers, 1);
  response = await request('presentation', [{ metrics: true, environment: true }]);
  assert.equal(response.transfers, 3); assert.equal(calls.frame, 1); assert.equal(calls.fields, 1);
  assert.deepEqual(response.reply.value.summary, engine.summary());
}

const appSource = await readFile(new URL('../src/app.ts', import.meta.url), 'utf8');
const app = ts.createSourceFile('app.ts', appSource, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TS);
const appFunction = name => {
  const node = app.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
  assert.ok(node); return compile(node.getText(app));
};

// The actual inspector refresh paints once after fresh data, never once before
// and again after. Paused unchanged state and hidden documents do no work.
{
  const doc = { hidden: false }, content = { ownerDocument: doc };
  let reads = 0, paints = 0;
  const engine = { async cellSnapshot(id) { reads++; return { id, genomeId: 1 }; } };
  const session = { revision: 0, stateVersion: 1, tick: 10, world: { cells: [{ id: 1 }] }, inspect: read => read(engine) };
  const context = vm.createContext({ active: session, cellTool: null, selectedId: 1, inspectorView: 'overview', inspectorEpoch: 0,
    inspectorStamp: '', inspectorPending: false, cellInspection: null, inspectorProgram: null,
    $: id => id === 'inspector-content' ? content : null,
    paintInspector: () => paints++, renderInspector: () => assert.fail('Living cell should remain selected'),
    readableError: String,
  });
  vm.runInContext(appFunction('updateInspector'), context);
  context.updateInspector(); await settle(); assert.equal(reads, 1); assert.equal(paints, 1);
  context.updateInspector(); await settle(); assert.equal(reads, 1); assert.equal(paints, 1);
  session.revision++; context.updateInspector(); await settle(); assert.equal(reads, 1); assert.equal(paints, 1);
  session.stateVersion++; context.updateInspector(); await settle(); assert.equal(reads, 2); assert.equal(paints, 2);
  doc.hidden = true; session.stateVersion++; context.updateInspector(); await settle(); assert.equal(reads, 2); assert.equal(paints, 2);
  doc.hidden = false; context.updateInspector(); await settle(); assert.equal(reads, 3); assert.equal(paints, 3);
}

// The cellular Memory panel combines the actual private register read with
// physical regulator stock. Filtering and escaping apply to both inventories.
{
  const doc={hidden:false}, head={}, footer={}, previous={}, next={};
  const elements={
    'inspector-content':{ownerDocument:doc}, 'memory-values':{closest:()=>({querySelector:()=>head})},
    'memory-page':{closest:()=>footer}, 'memory-summary':{}, 'memory-filter':{setAttribute(){}},
  };
  const cell={id:1,genomeId:1,memoryCost:0.005};
  const engine={cellSnapshot:()=>cell,cellBody:()=>({cellular:{products:[
    {name:'Brake',role:'regulator',mass:0.08,concentration:0.04,halfLife:8},
    {name:'Fiber',role:'filament',mass:1},
  ]}}),getMemory:()=>[{name:'rf001',type:'scalar',value:0},{name:'rs001',type:'string',value:'<private>'}]};
  const session={stateVersion:1,tick:0,world:{cells:[cell]},inspect:read=>read(engine)};
  const context=vm.createContext({active:session,cellTool:null,selectedId:1,inspectorView:'memory',inspectorEpoch:0,
    inspectorStamp:'',inspectorPending:false,cellInspection:null,memoryFilter:'',memoryPage:0,
    $:id=>elements[id],queryUi:selector=>selector.includes('memory-next')?next:selector.includes('memory-previous')?previous:null,
    esc:value=>String(value).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;'),
    readableError:String,
  });
  for (const method of ['memoryValue','paintInspector','updateInspector']) vm.runInContext(appFunction(method),context);
  context.updateInspector(); await settle();
  const rows=elements['memory-values'];
  assert.match(rows.innerHTML,/rf001/); assert.match(rows.innerHTML,/&lt;private&gt;/);
  assert.match(rows.innerHTML,/Brake/); assert.doesNotMatch(rows.innerHTML,/Fiber/);
  assert.match(elements['memory-summary'].textContent,/2 populated registers.*1 regulatory product/);
  context.memoryFilter='regulator'; context.paintInspector();
  assert.match(rows.innerHTML,/Brake/); assert.doesNotMatch(rows.innerHTML,/rf001|rs001/);
  context.memoryFilter='rf001'; context.paintInspector();
  assert.match(rows.innerHTML,/rf001.*<code>0<\/code>/); assert.doesNotMatch(rows.innerHTML,/Brake/);
}

// Program traces carry the version actually read, not the version current when
// an asynchronous UI callback runs. Same-tick mutations cannot stamp old values
// as current; hidden/destroyed editors do not request or paint live values.
{
  let reads = 0, paints = 0, errors = 0, resolve;
  const editor = { visible: true, setEvaluation() { paints++; }, setEvaluationUnavailable() { errors++; } };
  const session = { stateVersion: 1, tick: 10, world: { tick: 10, cells: [{ id: 1 }] }, evaluation() { reads++; return new Promise(done => { resolve = done; }); } };
  const context = vm.createContext({ active: session, inspectorProgram: editor, expandedInspections: new Set(),
    inspectedRevisions: new WeakMap(), evaluatingEditors: new WeakSet(), readableError: String });
  vm.runInContext(appFunction('updateProgramEvaluation'), context);
  context.updateProgramEvaluation(editor, session, 1); assert.equal(reads, 1);
  session.stateVersion = 2; resolve({ evaluation: { tick: 10 }, stateVersion: 1 }); await settle();
  assert.equal(paints, 0); assert.equal(context.inspectedRevisions.has(editor), false);
  context.updateProgramEvaluation(editor, session, 1); assert.equal(reads, 2);
  resolve({ evaluation: { tick: 10 }, stateVersion: 2 }); await settle(); assert.equal(paints, 1);
  context.updateProgramEvaluation(editor, session, 1); assert.equal(reads, 2);
  editor.visible = false; session.stateVersion++; context.updateProgramEvaluation(editor, session, 1); assert.equal(reads, 2); assert.equal(errors, 0);
}

// Moving objects update metadata without reallocating/recoloring the unchanged
// field bitmap. A new sample buffer or selected field invalidates it once.
{
  const source = await readFile(new URL('../src/renderer.ts', import.meta.url), 'utf8');
  const parsed = ts.createSourceFile('renderer.ts', source, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TS);
  const declaration = parsed.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === 'Renderer');
  const method = declaration.members.find(node => node.name?.getText(parsed) === 'drawEnvironment');
  let allocations = 0, uploads = 0;
  const pixels = { createImageData(width, height) { allocations++; return { width, height, data: new Uint8ClampedArray(width * height * 4) }; }, putImageData() { uploads++; } };
  const canvas = { width: 0, height: 0, getContext: () => pixels };
  const context = vm.createContext({ document: { createElement: () => canvas }, clamp: (x, low, high) => Math.max(low, Math.min(high, x)) });
  vm.runInContext(compile(`class TestRenderer { ${method.getText(parsed)} }; globalThis.TestRenderer = TestRenderer;`), context);
  const renderer = new context.TestRenderer(); renderer.worldWidth = 20; renderer.worldHeight = 20;
  renderer.context = { save() {}, restore() {}, drawImage() {} };
  const environment = { columns: 1, rows: 1, samples: Float32Array.of(1, .5, 0, 0, 2, 3, 4) };
  renderer.drawEnvironment(environment, 'oxygen'); assert.equal(allocations, 1); assert.equal(uploads, 1);
  renderer.drawEnvironment({ ...environment, barriers: [{ x: 10 }] }, 'oxygen'); assert.equal(allocations, 1); assert.equal(uploads, 1);
  renderer.drawEnvironment({ ...environment, samples: environment.samples.slice() }, 'oxygen'); assert.equal(allocations, 1); assert.equal(uploads, 2);
  renderer.drawEnvironment(environment, 'carbon'); assert.equal(allocations, 1); assert.equal(uploads, 3);
  renderer.drawEnvironment({ columns: 2, rows: 1, samples: new Float32Array(0) }, 'carbon'); assert.equal(uploads, 3, 'Missing resized fields wait for a full projection');
}

console.log('Presentation: omitted render/grid projections, metadata without transfer buffers, partial progress, single visible inspector paint, exact trace version gating and reused field pixels passed.');
