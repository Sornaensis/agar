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
    countAudit() { return undefined; },
    mineralAbsorbedTotal() { return 0; },
    energyLoss() { return undefined; },
    ecologySnapshotData() { return Float64Array.of(0, 0); },
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
  assert.equal(calls.frame, 0); assert.equal(response.transfers, 2);
  response = await request('presentation', [{ metrics: true, environment: true }]);
  assert.equal(response.transfers, 4); assert.equal(calls.frame, 1); assert.equal(calls.fields, 1);
  assert.deepEqual(response.reply.value.summary, engine.summary());
}

const appSource = await readFile(new URL('../src/app.ts', import.meta.url), 'utf8');
const app = ts.createSourceFile('app.ts', appSource, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TS);
const appFunction = name => {
  const node = app.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
  assert.ok(node); return compile(node.getText(app));
};

// Exercise exact u64 decoding and rendering above JavaScript's safe integer
// range. A one-count imbalance or energy dissipation remains observable.
{
  const source = await readFile(new URL('../src/engine.ts',import.meta.url),'utf8');
  const ast = ts.createSourceFile('engine.ts',source,ts.ScriptTarget.ES2022,true);
  const fn = ast.statements.find(node=>ts.isFunctionDeclaration(node) && node.name?.text==='decodeCountAudit');
  const {decodeCountAudit} = await import(url(fn.getText(ast)));
  const census = ast.statements.find(node=>ts.isFunctionDeclaration(node) && node.name?.text==='decodeEcologySnapshot');
  const {decodeEcologySnapshot} = await import(url(census.getText(ast)));
  const row = new Array(19).fill(0);
  row[0] = 4; row[9] = -1;
  row[1] = 7;
  const second = [...row]; second[0] = 9;
  const legacy = decodeEcologySnapshot(Float64Array.of(0, 2, ...row, ...second));
  const extended = decodeEcologySnapshot(Float64Array.of(0, 2, ...row, 3, ...second, 1));
  assert.equal(legacy.lineages[1].lowAtpWithReserveFuel, undefined);
  assert.deepEqual(extended.lineages, legacy.lineages.map((record, index) =>
    ({...record,lowAtpWithReserveFuel:[3,1][index]})));
  assert.throws(() => decodeEcologySnapshot(Float64Array.of(0, 1, ...row, 1, 2)), /invalid layout/);
  const large = 9007199254740993n;
  const audit = decodeCountAudit(BigUint64Array.of(large,large+1n,1n,large,large,0n,large,large+3n,3n,large,large+8n,7n));
  assert.equal(audit.carbon.held,'9007199254740993');
  assert.throws(()=>decodeCountAudit(new BigUint64Array(11)),/four budgets/);
  const elements = new Map();
  const context = vm.createContext({COUNT_SCALE:10000,metricDefinitions:[],clock:String, renderEcology() {}, renderBudgetStatus() {},
    active:{world:{environment:{nutrientRemovedTotal:0}},metadata:{history:[]}},
    $:id=>{if(!elements.has(id)) elements.set(id,{});return elements.get(id);}
  });
  vm.runInContext(appFunction('renderMetrics'),context);
  const metrics = new Proxy({countAudit:audit,flow:{production:.0001},organicCounts:[0,0,0],trophic:{}},{get:(obj,key)=>obj[key]??0});
  context.renderMetrics(metrics);
  assert.equal(elements.get('count-oxygen-held').textContent,'9007199254740993');
  assert.equal(elements.get('count-oxygen-difference').textContent,'0');
  assert.equal(elements.get('count-energy-difference').textContent,'1');
  assert.equal(elements.get('dissipated-energy').textContent,'1');
  assert.equal(elements.get('introduced-material').textContent,'9007199254740994');
  assert.equal(elements.get('flow-production').textContent,'1 energy counts/s');

  // Reconcile independent energy loss counters at values larger than f64
  // can distinguish. Fabrication may recover a prepaid fraction of a count.
  const ecologySource = await readFile(new URL('../src/ecology-metrics.ts', import.meta.url), 'utf8');
  const ecology = ts.createSourceFile('ecology-metrics.ts', ecologySource, ts.ScriptTarget.ES2022, true);
  const budget = ecology.statements.find(n => ts.isFunctionDeclaration(n) && n.name?.text === 'renderBudgetStatus');
  const labels = ecology.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations[0].name.getText(ecology) === 'energyLabels');
  const recorded = ecology.statements.find(n => ts.isFunctionDeclaration(n) && n.name?.text === 'recordedEnergyLoss');
  const {renderBudgetStatus, recordedEnergyLoss} = await import(url('const count = String;\n'
    + [labels,recorded,budget].map(n => n.getText(ecology)).join('\n')));
  const status = {classList:{toggle(){}}};
  const target = {querySelector: selector => selector === '[data-budget-status]' ? status : {}};
  const losses = { opening:'0', maintenance:'2', fabrication:'-1', respiration:'0',
    decomposition:'0', feedingAndDivision:'0', overflow:'0' };
  renderBudgetStatus(target, {countAudit:audit, energyLoss:losses}, 0);
  assert.equal(status.textContent, 'Minerals, carbon, oxygen and energy balance.');
  renderBudgetStatus(target, {countAudit:{...audit,carbon:{...audit.carbon,held:String(large-1n)}},
    energyLoss:{...losses,maintenance:'1'}}, 0);
  assert.equal(status.textContent, 'Carbon difference 1 · Energy difference 1');
  context.recordedEnergyLoss = recordedEnergyLoss;
  metrics.energyLoss = {...losses, maintenance:'1'};
  context.renderMetrics(metrics);
  assert.equal(elements.get('count-energy-difference').textContent, '1', 'Raw energy remainder stays visible');
  assert.equal(elements.get('dissipated-energy').textContent, '0', 'Measured loss is independent of the remainder');
  assert.equal(elements.get('dissipated-energy-label').textContent, 'Dissipated energy (measured)');
  const continuous = new Proxy({nutrientInputTotal:20,nutrientTotal:19}, {get:(target, key)=>target[key] ?? 0});
  renderBudgetStatus(target, continuous, 0);
  assert.equal(status.textContent, 'Minerals difference 1.00', 'Supply minus inventory has the same sign in both arithmetic modes');
}

// The actual inspector refresh paints once after fresh data, never once before
// and again after. Paused unchanged state and hidden documents do no work.
{
  let showFood = false, bodyReads = 0;
  const doc = { hidden: false }, content = { ownerDocument: doc, querySelector(selector) {
    return selector === '[data-food-potential][open]' && showFood ? {} : null;
  } };
  let reads = 0, paints = 0;
  const engine = { async cellSnapshot(id) { reads++; return { id, genomeId: 1 }; }, async cellBody() { bodyReads++; return {}; } };
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
  assert.equal(bodyReads, 0, 'Overview only requests a body when food potential is open');
  showFood = true; context.updateInspector(); await settle(); assert.equal(bodyReads, 1);
  context.updateInspector(); await settle(); assert.equal(bodyReads, 1);
  showFood = false; context.updateInspector(); await settle(); assert.equal(bodyReads, 1);
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
