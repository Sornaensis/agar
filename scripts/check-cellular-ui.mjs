import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

async function moduleUrl(relative, imports = {}) {
  let code = ts.transpileModule(await readFile(new URL(relative,import.meta.url),'utf8'),{
    compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext},
  }).outputText;
  for (const [name,url] of Object.entries(imports)) code = code.replaceAll(`from '${name}'`,`from ${JSON.stringify(url)}`);
  return `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`;
}
const organelles = await moduleUrl('../src/organelles.ts');
const { MECHANISM_CATALOG } = await import(organelles);
const language = await import(await moduleUrl('../src/genome-language.ts',{'./organelles':organelles}));
const ui = await moduleUrl('../src/ui.ts');
const { renderCellularBody } = await import(await moduleUrl('../src/cellular-inspector.ts',{'./organelles':organelles,'./ui':ui}));
const contract = JSON.parse(await readFile(new URL('../src/engine-contract.json',import.meta.url),'utf8'));
const engineSource = (await readFile(new URL('../src/engine.ts',import.meta.url),'utf8'))
  .replace("import engineUrl from '../engine/pkg/agar_engine.wasm?url';",'const engineUrl="https://agar.invalid/cellular-ui.wasm";')
  .replace("import engineContract from './engine-contract.json';",`const engineContract=${JSON.stringify(contract)};`);
const engineCode = ts.transpileModule(engineSource,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText;
const { Engine,validateBodyInspection,validateGenomeEvaluation } = await import(`data:text/javascript;base64,${Buffer.from(engineCode).toString('base64')}`);
const originalFetch = globalThis.fetch, bytes = await readFile(new URL('../engine/pkg/agar_engine.wasm',import.meta.url));
globalThis.fetch = async()=>new Response(bytes);
let engine;
try { engine = await Engine.load(); } finally { globalThis.fetch = originalFetch; }

const source = `# café 🌱 — physical products\ncellular;
product Anchor = anchor();
product Rotor = motor();
product Tail = filament();
product Memory = regulator(8);
gene anchors: Anchor = if pool.Anchor < 0.005 then 0.5 else 0;
gene drive: Rotor = if pool.Rotor < 0.01 then 1 else 0;
gene filament_supply: Tail = if pool.Tail < 0.025 then 1 else 0;
gene remember: Memory = if energy > 60 then 0.1 else 0;
assembly Swim: propel() {
  Anchor: 0.005; Rotor: 0.01; Tail: 0.025;
  activity: if pool.Memory > 0.001 then 0.6 else 0.2;
  at: 1.570796; direction: 0;
}`;
const names = values=>values.map(value=>value.name);
const complete = (text,prefix='')=>language.genomeCompletions(text,text.length,prefix);
assert.equal(language.isCellularSource(source),true);
assert.equal(language.isCellularSource('# cellular;\nthrust=0;'),false);
assert.equal(language.isCellularSource('let x="cellular;";'),false);
assert.equal(language.tokenizeGenome(source).some(token=>token.kind==='invalid'),false);
assert.deepEqual(names(language.cellularDeclarations(source).products),['Anchor','Rotor','Tail','Memory']);
assert.deepEqual(names(language.cellularDeclarations(source+'\n# product Hidden = motor();').products),['Anchor','Rotor','Tail','Memory']);
assert.deepEqual(names(complete('cellular;\n')),['product','gene','assembly','fn','let']);
assert.deepEqual(names(complete('cellular; product X = ')),[...language.CELLULAR_PRODUCT_ROLES]);
assert.deepEqual(names(complete(source+'\ngene new_gene: ')),['Anchor','Rotor','Tail','Memory']);
assert.ok(names(complete(source+'\nassembly Other: ')).includes('replicator'),'Legacy recipe remains available');
for (const name of ['store(food)','digest(tissue)','translate()','propel()']) assert.ok(names(complete(source+'\nassembly Other: ')).includes(name));
assert.deepEqual(names(complete(source+'\nassembly Other: store(')),['energy','mineral','food']);
assert.deepEqual(names(complete(source+'\nassembly Other: digest(ti','ti')),['tissue']);
assert.deepEqual(names(complete(source+'\nassembly Other: propel() ')),[]);
assert.equal(language.cellularDeclarations(source).assemblies[0].kind,'propel()');
assert.equal(language.cellularDeclarations('cellular; assembly Vessel: store( food ) {').assemblies[0].kind,'store(food)');
assert.ok(names(complete(source+'\nassembly Other: flagellum {\n')).includes('Rotor'));
assert.ok(complete(source+'\nassembly Other: flagellum {\n').find(item=>item.name==='Rotor').insertText.endsWith(': '));
const expressions = complete(source+'\nlet next = ');
for (const name of ['pool.Memory','concentration.Memory','built.Swim','energy','light.valid','activate','repress']) assert.ok(names(expressions).includes(name),`Cellular expression offers ${name}`);
for (const name of ['thrust','divide','grow','id','time','protein.flagellum','organelle.core.count','body.division_ready','rf001','populated']) assert.ok(!names(expressions).includes(name),`Cellular completion excludes ${name}`);
assert.ok(names(complete(source+'\ngene sensed: Rotor = if ')).includes('light.valid'),'A numeric gene can use boolean sensor conditions');
assert.deepEqual(complete(source+'\n# pool.'),[]);
assert.ok(language.lookupCellularSymbol('built.Swim',source).description.includes('damage'));
assert.ok(!language.genomeSignalCatalog('write',source).some(item=>item.category==='output'||item.category==='register'));

const schema = engine.languageSchema();
assert.equal(schema.cellular.marker,'cellular;');
assert.deepEqual(schema.cellular.reads,language.CELLULAR_READ_SIGNALS.map(item=>({name:item.name,type:item.type})));
assert.deepEqual(names(schema.cellular.products),[...language.CELLULAR_PRODUCT_ROLES]);
assert.deepEqual(names(schema.cellular.assemblies),names(language.CELLULAR_ASSEMBLY_KINDS));
assert.deepEqual(names(schema.cellular.mechanisms),names(language.CELLULAR_MECHANISMS));
assert.equal(schema.cellular.mechanisms.find(item=>item.name==='store(food)').target,'food');
assert.equal(schema.cellular.mechanisms.find(item=>item.name==='propel()').target,null);
for (const mechanism of MECHANISM_CATALOG) {
  const actual=schema.cellular.mechanisms.find(item=>item.name===mechanism.kind);
  assert.deepEqual(actual.requiredRoles,mechanism.roles,`${mechanism.kind} documents authoritative material roles`);
  assert.equal(actual.constructor,mechanism.constructor);
  assert.equal(actual.target,mechanism.target);
  assert.deepEqual(actual.receptorFields.map(({name,type})=>({name,type})),(language.CELLULAR_RECEPTOR_FIELDS[mechanism.kind]??[]).map(({name,type})=>({name,type})),`${mechanism.kind} only offers its own typed readings`);
  const symbol=language.CELLULAR_MECHANISMS.find(item=>item.name===mechanism.kind);
  engine.compile('cellular;\n'+language.genomeSymbolDocumentation(symbol).example);
}
const genome = engine.compile(source);
engine.resetEmpty(42,0); engine.seed(genome,1);
const id = engine.snapshot().cells[0].id;
const evaluation = engine.getCellEvaluation(id);
assert.equal(evaluation.mode,'cellular');
assert.ok(evaluation.outputs.every(output=>/^(gene|activity|start_when|recycle)\./.test(output.name)));
assert.equal(evaluation.outputs.find(output=>output.name==='gene.drive').value,1);
assert.ok(evaluation.signals.some(signal=>signal.name==='pool.Memory'&&signal.value===0));
assert.equal(evaluation.writes.length,0);
assert.ok(evaluation.expressions.every(expression=>source.slice(expression.from,expression.to).length>0));
const badEvaluation = structuredClone(evaluation); badEvaluation.outputs[0].name='thrust';
assert.throws(()=>validateGenomeEvaluation(badEvaluation,id));

engine.tick(2);
const body = engine.cellBody(id);
assert.equal(body.cellular.products.length,4);
assert.equal(body.cellular.recipes[0].components.length,3);
assert.equal(body.cellular.recipes[0].kind,'propel()');
assert.ok(body.cellular.products.some(product=>product.expressionRate>0));
assert.ok(body.cellular.assemblies.length>0,'An actual incomplete machine is inspectable');
assert.equal(body.cellular.assemblies[0].complete,false);
assert.ok(body.cellular.assemblies[0].components.some(part=>part.mass>0));
const html = renderCellularBody(body,new Set(['cellular-products']),true);
assert.ok(html.includes('Living composition')&&html.includes('Products &amp; expression')===false);
assert.ok(html.includes('Waiting for')&&html.includes('Assembling'));
assert.ok(html.includes('t½ 8s')&&html.includes('90°'));
assert.ok(html.includes('propel()')&&html.includes('Propulsion'));
assert.ok(html.includes('Translation work'));
assert.ok(html.includes('New starts')&&html.includes('Recycle request')&&html.includes('Recycling'));
assert.ok(html.includes('data-kind="cellular-products" open'));
const untrusted = structuredClone(body); untrusted.cellular.assemblies[0].bottleneck='<svg onload=alert(1)>';
assert.ok(renderCellularBody(untrusted).includes('&lt;svg onload=alert(1)&gt;'));
assert.ok(!renderCellularBody(untrusted).includes('<svg onload'));
for (const change of [
  value=>{value.cellular.products[0].mass=-1;},
  value=>{value.cellular.assemblies[0].components[0].mass=NaN;},
  value=>{value.cellular.assemblies[0].direction=Infinity;},
  value=>{value.cellular.assemblies.push(structuredClone(value.cellular.assemblies[0]));},
  value=>{value.cellular.replication.progress=2;},
  value=>{value.cellular.recipes[0].kind='digest(unknown)';},
  value=>{value.cellular.recipes[0].startAllowed=1;},
  value=>{value.cellular.recipes[0].recycleRequested=2;},
  value=>{value.cellular.recipes[0].recycleRate=-1;},
]) { const invalid=structuredClone(body);change(invalid);assert.throws(()=>validateBodyInspection(invalid,id)); }
assert.equal(renderCellularBody({...body,cellular:null}),'');

const controlled = `cellular;
product Mount = anchor(); product Detector = receptor(); product Pigment = pigment();
product Rotor = motor(); product Tail = filament(); product Brake = regulator(8);
fn calm(value, threshold) = repress(value, threshold);
gene braking: Brake = if sensed.Eye.valid && sensed.Eye.level < 0.3 then 0.2 else 0;
assembly Eye: sense(light) { Mount: 0.002; Detector: 0.006; Pigment: 0.002; }
assembly Swim: propel() { Mount: 0.005; Rotor: 0.01; Tail: 0.01;
  start_when: concentration.Brake < 0.001;
  activity: calm(concentration.Brake, 0.02);
  recycle: activate(concentration.Brake, 0.005);
}`;
assert.deepEqual(language.cellularDeclarations(controlled).functions.map(fn=>[fn.name,...fn.parameters]),[['calm','value','threshold']]);
assert.ok(language.tokenizeGenome(controlled).some(token=>token.text==='calm'&&token.kind==='function'));
assert.equal(language.lookupCellularSymbol('sensed.Eye.valid',controlled).type,'boolean');
assert.equal(language.lookupCellularSymbol('sensed.Eye.level',controlled).type,'scalar');
assert.equal(language.lookupCellularSymbol('sensed.Eye.organic',controlled),undefined);
assert.ok(names(complete(controlled+'\nlet measurement = sensed.Eye.','sensed.Eye.')).includes('sensed.Eye.level'));
assert.ok(!names(complete(controlled+'\nlet measurement = sensed.Eye.','sensed.Eye.')).includes('sensed.Eye.organic'));
assert.ok(names(complete(controlled+'\nassembly Other: propel() {\n')).includes('start_when'));
assert.ok(names(complete(controlled+'\nassembly Other: propel() {\n')).includes('recycle'));
assert.equal(language.genomeCompletionContext(controlled+'\nassembly Other: propel() { start_when: ',(controlled+'\nassembly Other: propel() { start_when: ').length).expected,'boolean');
assert.ok(names(complete(controlled+'\nlet response = ')).includes('calm'));
assert.ok(names(complete('cellular; fn sample(value, threshold) = ')).includes('value'));
assert.ok(!names(complete('cellular; fn sample(value, threshold) = ')).includes('sample'));
assert.ok(!names(complete(controlled+'\nlet outside = ')).includes('threshold'));
assert.equal(schema.cellular.userFunctions.recursive,false);
assert.equal(schema.cellular.userFunctions.maxParameters,8);
assert.equal(schema.cellular.recipeControls.find(item=>item.name==='start_when').type,'boolean');
const controlGenome=engine.compile(controlled);
engine.resetEmpty(52,0); engine.seed(controlGenome,1);
const controlId=engine.snapshot().cells[0].id;
const controlEvaluation=engine.getCellEvaluation(controlId);
assert.equal(controlEvaluation.outputs.find(item=>item.name==='start_when.Swim').value,true);
assert.equal(controlEvaluation.outputs.find(item=>item.name==='activity.Swim').value,1);
assert.equal(controlEvaluation.outputs.find(item=>item.name==='recycle.Swim').value,0);
assert.equal(controlEvaluation.signals.find(item=>item.name==='sensed.Eye.valid').value,false);
assert.equal(controlEvaluation.signals.find(item=>item.name==='sensed.Eye.level').value,0);
assert.ok(controlEvaluation.expressions.some((item,index)=>item.id!==index),'Authored function calls retain graph IDs after ambiguous inline internals are omitted');
assert.ok(controlEvaluation.expressions.some(item=>controlled.slice(item.from,item.to)==='calm(concentration.Brake, 0.02)'&&item.value===1),'Function call has one actual value at its invocation');
const repeatedId=structuredClone(controlEvaluation);
repeatedId.expressions[1].id=repeatedId.expressions[0].id;
assert.throws(()=>validateGenomeEvaluation(repeatedId,controlId));
const badControl=structuredClone(controlEvaluation);
badControl.outputs.find(item=>item.name==='start_when.Swim').type='scalar';
badControl.outputs.find(item=>item.name==='start_when.Swim').value=1;
assert.throws(()=>validateGenomeEvaluation(badControl,controlId));

console.log('Cellular UI checks passed: mechanism/receptor schema parity, typed local readings and concentrations, pure function scope, initiation/recycling controls, live values, physical inventory validation and escaped rendering.');
