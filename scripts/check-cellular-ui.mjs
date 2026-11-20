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
const { Engine,decodeRenderSnapshot,validateBodyInspection,validateGenomeEvaluation } = await import(`data:text/javascript;base64,${Buffer.from(engineCode).toString('base64')}`);
// Contact links can bridge two colonies while leaving each colony's membership intact.
{
  const base=new Float64Array(12+5*21);
  base[2]=5; base[7]=400; base[8]=400; base[9]=1;
  for (const [row,[id,x]] of [[8,3],[2,397],[5,12],[13,9],[21,11]].entries()) {
    base.set([id,1,x,110,0,0,0,7],12+row*21);
  }
  const wire=edges=>Float64Array.of(...base,4,0,...edges.flat());
  const raw=wire([[2,8,1],[5,8,0],[5,13,1],[8,13,2]]);
  const projected=decodeRenderSnapshot(raw);
  const peers=id=>projected.cells.find(c=>c.id===id);
  assert.deepEqual(peers(8).neighborIds,[2,5,13]);
  assert.equal(peers(2).attachedNeighbors,1);
  assert.deepEqual(peers(8).colonyNeighborIds,[2]);
  assert.deepEqual(peers(8).feedingNeighborIds,[5]);
  assert.deepEqual(peers(8).contactNeighborIds,[13]);
  assert.equal(peers(2).colonySize,2);
  assert.equal(peers(13).colonySize,2);
  assert.equal(peers(21).colonySize,undefined);
  assert.equal(peers(21).contactSignal,0);
  const docked=decodeRenderSnapshot(wire([[2,8,0]]));
  assert.equal(docked.cells.find(c=>c.id===2).colonySize,undefined);
  assert.equal(docked.bonds[0][2],'feeding');
  for (const edges of [[[8,2,1]],[[2,2,1]],[[2,99,1]],[[2,8,0],[2,8,1]],[[5,8,0],[2,8,1]],[[2,8,3]]]) {
    assert.throws(()=>decodeRenderSnapshot(wire(edges)),/bond/);
  }
  for (const badCount of [-1,0.5,6]) {
    const bad=raw.slice(); bad[9]=badCount; assert.throws(()=>decodeRenderSnapshot(bad),/layout/);
  }
  assert.throws(()=>decodeRenderSnapshot(raw.slice(0,-1)),/layout/);
  const viewModule=await moduleUrl('../src/periodic-visibility.ts');
  const { PeriodicVisibility }=await import(viewModule);
  const { Renderer }=await import(await moduleUrl('../src/renderer.ts',{
    './organelles':organelles,'./world-camera':await moduleUrl('../src/world-camera.ts'),'./periodic-visibility':viewModule,
  }));
  const visibility=new PeriodicVisibility();
  visibility.width=visibility.right=400;visibility.height=visibility.bottom=400;
  const segments=[],rings=[];
  const context={save(){},restore(){},stroke(){},beginPath(){},moveTo(x,y){segments.push([x,y]);},lineTo(x,y){segments.at(-1).push(x,y);},arc(x,y,r){rings.push({x,y,r,color:this.strokeStyle});}};
  Renderer.prototype.drawBonds.call({context,visibility},{...projected,bonds:[projected.bonds[0]]});
  assert.equal(segments.length,2,'Both images of a cross-seam bond are drawn');
  assert.ok(segments.some(([x])=>Math.abs(x)<3) && segments.some(([x])=>Math.abs(x-400)<3));
  assert.ok(segments.every(([xa,ya,xb,yb])=>Math.hypot(xa-xb,ya-yb)<3),'Neither segment crosses the middle of the world');
  Renderer.prototype.drawBonds.call({context,visibility},docked);
  assert.equal(rings.length,2,'Both images of a feeding link across the seam are rings');
  assert.ok(rings.some(({x})=>x===0) && rings.some(({x})=>x===400));
}
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
const statements = complete('cellular;\n');
for (const name of ['product','gene','assembly','fn','let','clear','publish','signal','ri001','rf001','rb001','rs001','rv001']) {
  assert.ok(names(statements).includes(name), 'Cellular statement offers '+name);
}
assert.equal(statements.find(item=>item.name==='rb001').insertText,'rb001 = ');
assert.ok(!names(statements).includes('thrust'));
for (const keyword of ['publish','signal']) {
  const addresses=complete('cellular; '+keyword+' ');
  assert.ok(addresses.length && addresses.every(item=>item.category==='register' && item.type==='scalar'));
  assert.ok(names(complete('cellular; '+keyword+' rf21','rf21')).includes('rf210'));
  assert.deepEqual(complete('cellular; '+keyword+' rb','rb'),[]);
  assert.deepEqual(complete('cellular; '+keyword+' rf001 = '),[]);
}
const occupiedIntegers=Array.from({length:45},(_,i)=>'ri'+String(i+1).padStart(3,'0')+'=1;').join(' ');
assert.ok(names(complete('cellular; '+occupiedIntegers+' publish r','r')).includes('rf001'));
for (const rhs of ['clear ', 'let used = populated(', 'fn test(x) = populated(']) {
  const suggestions=complete('cellular; '+rhs);
  assert.ok(suggestions.length && suggestions.every(item=>item.category==='register'));
}
for (const [part,expected] of [['rb001 = ','boolean'],['rs001 = ','string'],['rv001 = ','vec2'],['rf002 = 1 when ','boolean'],['clear rf001 when ','boolean']]) {
  const code='cellular; '+part;
  assert.equal(language.genomeCompletionContext(code,code.length).expected,expected);
}
assert.deepEqual(names(complete('cellular; product X = ')),[...language.CELLULAR_PRODUCT_ROLES]);
assert.deepEqual(names(complete('cellular; product Coat = scaffold(); product Wall = membrane(); product Glue = adhesin(')), ['Coat']);
assert.deepEqual(names(complete(source+'\ngene new_gene: ')),['Anchor','Rotor','Tail','Memory']);
assert.ok(!names(complete(source+'\nassembly Other: ')).includes('replicator'),'New recipe completion offers explicit mechanisms');
assert.equal(language.lookupCellularSymbol('replicator',source).category,'keyword','The editor can still explain an existing combined recipe');
for (const name of ['store(food)','digest(tissue)','translate()','propel()']) assert.ok(names(complete(source+'\nassembly Other: ')).includes(name));
assert.deepEqual(names(complete(source+'\nassembly Other: store(')),['energy','mineral','food']);
assert.deepEqual(names(complete(source+'\nassembly Other: digest(ti','ti')),['tissue']);
assert.deepEqual(names(complete(source+'\nassembly Other: propel() ')),[]);
assert.equal(language.cellularDeclarations(source).assemblies[0].kind,'propel()');
assert.equal(language.cellularDeclarations('cellular; assembly Vessel: store( food ) {').assemblies[0].kind,'store(food)');
assert.ok(names(complete(source+'\nassembly Other: flagellum {\n')).includes('Rotor'));
assert.ok(complete(source+'\nassembly Other: flagellum {\n').find(item=>item.name==='Rotor').insertText.endsWith(': '));
const expressions = complete(source+'\nlet next = ');
for (const name of ['pool.Memory','concentration.Memory','built.Swim','energy','light.valid','activate','repress','rf001','rs001','populated','memory.used','memory.cost']) assert.ok(names(expressions).includes(name),`Cellular expression offers ${name}`);
for (const name of ['thrust','divide','grow','id','time','protein.flagellum','organelle.core.count','body.division_ready']) assert.ok(!names(expressions).includes(name),`Cellular completion excludes ${name}`);
assert.ok(names(complete(source+'\ngene sensed: Rotor = if ')).includes('light.valid'),'A numeric gene can use boolean sensor conditions');
assert.deepEqual(complete(source+'\n# pool.'),[]);
assert.ok(language.lookupCellularSymbol('built.Swim',source).description.includes('damage'));
const writes=language.genomeSignalCatalog('write',source);
assert.ok(!writes.some(item=>item.category==='output'));
assert.deepEqual(writes.filter(item=>item.category==='register').map(item=>item.name),['ri001','rf001','rb001','rs001','rv001']);

const schema = engine.languageSchema();
assert.equal(schema.cellular.marker,'cellular;');
assert.deepEqual(schema.cellular.registers,{prefixes:['ri','rf','rb','rs','rv'],addressesPerType:999,upkeepPerSecond:0.0025,clearWins:true});
assert.deepEqual(schema.cellular.contactMessages,{
  declarations:['publish','signal'],registerType:'scalar',receptor:'sense(contact)',
  validField:'message_valid',valueField:'message',signalValidField:'signal_valid',signalValueField:'signal',bondedField:'bonded',peerFields:language.CONTACT_FIELDS.map(({name,type})=>({name,type})),initializerLimit:1e12,
});
assert.deepEqual(schema.cellular.reads,language.CELLULAR_READ_SIGNALS.map(item=>({name:item.name,type:item.type})));
assert.deepEqual(names(schema.cellular.products),[...language.CELLULAR_PRODUCT_ROLES]);
assert.deepEqual(schema.cellular.products.find(p=>p.name==='adhesin').arguments, ['precursor']);
const adhesin=language.CELLULAR_CONSTRUCTORS.find(p=>p.name==='adhesin');
engine.compile('cellular;\n'+language.genomeSymbolDocumentation(adhesin).example);
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
const counted = "cellular counts; product Skin=membrane(); assembly Tank:store(energy){ Skin:200; } let available=energy; let material=pool.Skin; let activity_now=built.Tank; ri001=energy;";
assert.equal(language.isCellularSource(counted),true);
assert.equal(language.isCellularSource("cellularcounts;"),false);
assert.equal(language.usesCountUnits(source),false);
assert.equal(language.usesCountUnits(counted),true);
assert.deepEqual([...schema.cellular.counts.integerSignals].sort(),[...language.COUNT_RESOURCE_SIGNALS].sort());
assert.equal(schema.cellular.counts.perUnit,10000);
for (const [name,type] of [['energy','count'],['pool.Skin','count'],['built.Tank','scalar'],['energy.fraction','scalar']]) {
  assert.equal(language.genomeSignalCatalog('read',counted).find(s=>s.name===name).type,type);
}
for (const sym of language.CELLULAR_MECHANISMS) engine.compile('cellular counts;\n'+language.genomeSymbolDocumentation(sym,true).example);
const countedId=engine.compile(counted);
for (const countChemistry of [false,true]) {
  engine.resetEmpty(42);
  engine.setCountChemistry(countChemistry);
  engine.seed(countedId,1,120,100);
  const countedTrace=engine.getCellEvaluation(1);
  assert.equal(countedTrace.bindings.find(b=>b.name==='available').value,Math.round(engine.cellSnapshot(1).energy*10000));
  assert.equal(countedTrace.bindings.find(b=>b.name==='available').type,'count');
  assert.equal(countedTrace.bindings.find(b=>b.name==='material').type,'count');
  assert.equal(countedTrace.bindings.find(b=>b.name==='activity_now').type,'scalar');
}
engine.resetEmpty(42);
engine.setCountChemistry(false);
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
const html = renderCellularBody(body,new Set(['cellular-products']));
assert.ok(html.includes('Cell machinery') && html.includes('Products & expression'));
assert.doesNotMatch(html,/data-kind="cellular-assembly-[^"]+" open/);
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

const contactSource=`# café 🌱 — stable UTF-16 ranges
cellular;
publish rf001 = 0;
signal rf003 = -2;
product Mount = anchor(); product Port = receptor(); product Fiber = filament();
assembly Touch: sense(contact) { Mount: 0.01; Port: 0.01; Fiber: 0.01; }
product Catalyst = enzyme();
let ready = energy > 0;
let have_signal = peer.signal_valid;
assembly Needle: pierce() { Mount: 0.02; Fiber: 0.02; Catalyst: 0.02; engage_when: have_signal && peer.signal >= 2; }
ri001 = ri001 + 1;
rf002 = rf001 when populated(rf001);
rb001 = sensed.Touch.message_valid;
rs001 = "private";
rv001 = vec(1 + sensed.Touch.message, -1);
clear rf001 when ri001 >= 1;`;
assert.equal(language.lookupCellularSymbol('sensed.Touch.bonded',contactSource).type,'boolean');
assert.equal(language.lookupCellularSymbol('sensed.Eye.bonded',controlled),undefined);
for (const kind of language.CONTACT_RECIPE_KINDS) {
  const block = 'cellular; assembly Tip: ' + kind + ' { ';
  assert.ok(names(complete(block)).includes('engage_when'));
  assert.deepEqual(names(complete(block + 'engage_when: peer.', 'peer.')).sort(), names(language.CONTACT_FIELDS).sort());
  assert.deepEqual(names(complete(block + 'activity: peer.', 'peer.')), []);
  assert.deepEqual(names(complete(block + 'engage_when: energy', 'energy')), []);
}
assert.ok(!names(complete('cellular; assembly Eye: sense(optical) { ')).includes('engage_when'));
for (const old of ['avoid_message','adhere_message']) {
  assert.ok(!names(statements).includes(old));
  assert.equal(engine.analyze('cellular; ' + old + ' rf001;').valid,false);
}
assert.ok(!names(complete('cellular; assembly New: ')).includes('digestive'));
assert.equal(language.symbolAt('cellular; let x = feed;',18),null);
assert.equal(language.lookupCellularSymbol('sensed.Touch.message_valid',contactSource).type,'boolean');
assert.equal(language.lookupCellularSymbol('sensed.Touch.message',contactSource).type,'scalar');
assert.equal(language.lookupCellularSymbol('sensed.Touch.message',contactSource.replace('sense(contact)','mechanoreceptor')).type,'scalar');
assert.equal(language.lookupCellularSymbol('sensed.Eye.message',controlled),undefined);
for (const keyword of schema.cellular.contactMessages.declarations) {
  const symbol=language.lookupCellularSymbol(keyword,contactSource);
  engine.compile('cellular;\n'+language.genomeSymbolDocumentation(symbol).example);
}
const aliases = engine.analyze(contactSource).bindings;
assert.equal(aliases.find(item=>item.name==='ready').contactSafe,false);
assert.equal(aliases.find(item=>item.name==='have_signal').perContact,true);
const guardCursor = contactSource.indexOf('engage_when: ') + 'engage_when: '.length;
const allowedAliases = names(language.genomeCompletions(contactSource, guardCursor, '', aliases));
assert.ok(allowedAliases.includes('have_signal') && !allowedAliases.includes('ready'));
const ordinaryProgram = contactSource + '\nrb007 = ';
const normalAliases = names(language.genomeCompletions(ordinaryProgram, ordinaryProgram.length, '', aliases));
assert.ok(!normalAliases.includes('have_signal') && normalAliases.includes('ready'));
const contactId=engine.compile(contactSource);
engine.resetEmpty(72,0); engine.seed(contactId,1);
const founder=engine.snapshot().cells[0].id;
const memory=()=>Object.fromEntries(engine.getMemory(founder).map(item=>[item.name,item.value]));
assert.deepEqual(memory(),{rf001:0,rf003:-2},'Only the explicitly initialized register starts occupied');
assert.equal(engine.renderSnapshot().cells[0].contactSignal,-2,'Compact player frames expose the current payload');
const frame=engine.renderSnapshotData();
for (const [offset, bad] of [[0,-1], [0,1], [0,0.5], [1,NaN]]) {
  const corrupt=frame.slice();
  corrupt[corrupt.length-2+offset]=bad;
  assert.throws(()=>decodeRenderSnapshot(corrupt),/signal/);
}
const inventory=engine.cellBody(founder);
assert.deepEqual(inventory.colony.neighborIds,[]);
assert.equal(engine.renderSnapshot().bonds.length,0,'Publishing a value never invents a bond');
const blueprint={...inventory,cellular:{...inventory.cellular,assemblies:[],
  recipes:inventory.cellular.recipes.map(r=>({...r,name:'Bus',kind:'junction(signal)'}))}};
const planMarkup=renderCellularBody(blueprint);
assert.ok(planMarkup.includes('Unbuilt recipes'));
assert.ok(!planMarkup.includes('data-kind="colony-reception"')&&!planMarkup.includes('class="colony-exchange"'),
  'An unbuilt junction has neither a live receiver nor membership');
const signedExchanges={...inventory,colony:{neighbors:3,neighborIds:[7,9,11],colonyNeighborIds:[7],feedingNeighborIds:[9],contactNeighborIds:[11],energyNet:0.25,mineralNet:-0.001,carbonNet:0}};
assert.doesNotThrow(()=>validateBodyInspection(signedExchanges,founder));
const exchangePanel=renderCellularBody(signedExchanges);
for (const label of ['Resource exchange','Colony bonds · #7','Feeding links · #9','Contact links · #11','+0.25','−0.001']) assert.ok(exchangePanel.includes(label));
assert.throws(()=>validateBodyInspection({...inventory,colony:{...signedExchanges.colony,carbonNet:Infinity}},founder));
for (const neighborIds of [[7],[9,7,11],[7,7,11],[founder,9,11]]) {
  assert.throws(()=>validateBodyInspection({...inventory,colony:{...signedExchanges.colony,neighborIds}},founder));
}
const checkpoint=engine.saveState();
const preview=engine.getCellEvaluation(founder);
assert.deepEqual(preview.contactMessages,{publish:'rf001',initial:0,signal:'rf003',signalInitial:-2});
assert.deepEqual(preview.contactMessageDeclarations.map(item=>[item.kind,item.value,contactSource.slice(item.from,item.to)]),[
  ['publish',0,'publish rf001 = 0;'],['signal',-2,'signal rf003 = -2;'],
]);
assert.equal(preview.bindings.find(b=>b.name==='have_signal').value, null);
assert.equal(preview.bindings.find(b=>b.name==='have_signal').perContact, true);
assert.equal(preview.outputs.find(row=>row.name==='engage_when.Needle').perContact,true);
assert.deepEqual(preview.signals.filter(item=>item.name.startsWith('sensed.Touch.message')).map(item=>[item.name,item.value]),[
  ['sensed.Touch.message_valid',false],['sensed.Touch.message',0],
]);
// Exercise the editor's actual cursor and selection lookup against this real
// frozen trace without requiring a DOM or starting a second simulation.
const editorFile=ts.createSourceFile('program-editor.ts',await readFile(new URL('../src/program-editor.ts',import.meta.url),'utf8'),ts.ScriptTarget.ES2022,true,ts.ScriptKind.TS);
const editorClass=editorFile.statements.find(node=>ts.isClassDeclaration(node) && node.name?.text==='ProgramEditor');
const methods=['evaluatedPart','tokenAt'].map(name=>editorClass.members.find(node=>node.name?.getText(editorFile)===name).getText(editorFile));
const fragment=ts.transpileModule('class EditorProbe {'+methods.join('\n')+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const EditorProbe=new Function('cellularDeclarations','unwrapGenomeSelection',fragment+';return EditorProbe;')(language.cellularDeclarations,language.unwrapGenomeSelection);
const editor=new EditorProbe(); editor.evaluation=preview; editor.source=contactSource; editor.tokens=language.tokenizeGenome(contactSource); editor.codeTokens=editor.tokens.filter(token=>token.kind!=='comment');
for (const declaration of preview.contactMessageDeclarations) {
  const hovered=editor.evaluatedPart(declaration.from+1,declaration.from+1);
  const selected=editor.evaluatedPart(declaration.from,declaration.to);
  assert.equal(hovered.value,declaration.value);
  assert.deepEqual(selected,hovered);
  assert.equal(selected.to,declaration.to);
}
assert.ok(preview.writes.find(item=>item.name==='rf002').applies);
assert.equal(preview.writes.find(item=>item.name==='rf001').applies,false);
assert.deepEqual(engine.saveState(),checkpoint,'Register/message previews are read-only');
for (const corrupt of [
  trace=>{trace.outputs.find(row=>row.perContact).value=true;},
  trace=>{trace.contactMessages.publish='rb001';},
  trace=>{trace.contactMessages.initial=Infinity;},
  trace=>{trace.contactMessages.signal=trace.contactMessages.publish;},
  trace=>{trace.contactMessages.signalInitial=NaN;},
  trace=>{trace.contactMessageDeclarations[0].value='0';},
  trace=>{trace.contactMessageDeclarations[1].to=contactSource.length+1;},
  trace=>{trace.contactMessageDeclarations[0].from=0;},
  trace=>{trace.contactMessageDeclarations.pop();},
]) {
  const invalid=structuredClone(preview); corrupt(invalid);
  assert.throws(()=>validateGenomeEvaluation(invalid,founder));
}
engine.tick(1);
assert.deepEqual(memory(),{ri001:1,rf001:0,rf002:0,rf003:-2,rb001:false,rs001:'private',rv001:[1,-1]});
assert.equal(engine.getCellEvaluation(founder).outputs.find(item=>item.name==='engage_when.Needle').value,null);
engine.tick(1);
assert.equal(memory().rf001,undefined);
assert.equal(engine.getCellEvaluation(founder).contactMessageDeclarations.find(item=>item.kind==='publish').value,null);

// Ledger and inspector agree on the yield of an exact, ordinary founder.
engine.resetEmpty(19,0);
const subject = engine.placeCell(engine.compile(source),200,200);
const inspected = engine.cellBody(subject);
const potential = inspected.foodPotential, live = engine.cellSnapshot(subject);
const close = (actual,expected) => assert.ok(Math.abs(actual-expected) < 1e-8,`${actual} ≠ ${expected}`);
close(potential.atp,live.energy); close(potential.gut,live.gutEnergy);
close(potential.total,potential.body+potential.reserve+potential.gut+potential.atp);
assert.ok(potential.body > 0 && potential.reserve > 0);
assert.throws(() => validateBodyInspection({...inspected,foodPotential:{...potential,total:potential.total+1}},subject));
const alive = engine.metricsSnapshot();
close(alive.energyTotal,potential.total);
engine.setCellStores(subject,0,null);
const remains = engine.metricsSnapshot();
close(remains.energyWithdrawnTotal,potential.atp);
close(remains.packetEnergyTotal,potential.total-potential.atp);
close(remains.packetEnergyTotal,engine.renderSnapshot().food.reduce((sum,packet)=>sum+packet.energy,0));
close(remains.energyTotal+remains.energyWithdrawnTotal,alive.energyTotal);

// Exercise the same live native projections used by the renderer and inspector.
// Grow a paid mother–daughter bond; then sever it during a paused edit.
const nurse=engine.compile(await readFile(new URL('../examples/nurse-rosette.cell',import.meta.url),'utf8'));
engine.clearSources(); engine.clearBarriers();
assert.ok(engine.setWorldSize(400,400));
engine.setEnvironment(.5,.85,0); engine.setInitialNutrients(3); engine.setMutationRate(0);
assert.ok(engine.setInitialGases(1,1));
assert.ok(engine.setGasExchange({mode:'open',exchangeRate:.02,oxygenEquilibrium:1,carbonEquilibrium:1}));
engine.resetEmpty(42);engine.seed(nurse,1,200,200);
let family=engine.renderSnapshot();
assert.equal(family.bonds.length,0);
while (!family.bonds.length && family.time<150) { engine.tick(30); family=engine.renderSnapshot(); }
assert.equal(family.bonds.length,1,'An actual fission establishes an observable bond');
assert.equal(family.bonds[0][2],'colony');
const [[mother,daughter]]=family.bonds;
assert.equal(engine.cellSnapshot(daughter.id).parentId,mother.id);
assert.equal(mother.colonySize,2);
const motherBody=engine.cellBody(mother.id);
assert.equal(motherBody.cellular.products.find(p=>p.name==='Glue').precursor,'CoatPrecursor');
assert.match(renderCellularBody(motherBody,new Set(['cellular-products'])),/adhesin from CoatPrecursor/);
assert.deepEqual(motherBody.colony.neighborIds,[daughter.id]);
assert.deepEqual(motherBody.colony.colonyNeighborIds,[daughter.id]);
assert.deepEqual(motherBody.colony.feedingNeighborIds,[]);
const attached=engine.saveState();
assert.equal(engine.moveCell(daughter.id,35,35),1);
assert.equal(engine.renderSnapshot().bonds.length,0,'Single-cell relocation immediately removes the edge on the paused frame');
assert.deepEqual(engine.cellBody(mother.id).colony.neighborIds,[]);
engine.restoreState(attached);
assert.deepEqual(engine.renderSnapshot().bonds.map(([a,b])=>[a.id,b.id]),[[mother.id,daughter.id]]);
assert.deepEqual(engine.cellBody(daughter.id).colony.neighborIds,[mother.id]);

console.log('Cellular UI checks passed for register and message completions, authoritative mechanism/receptor schema, pure functions, live UTF-16 declaration spans, frozen memory/write previews, physical inventory and escaped rendering.');
