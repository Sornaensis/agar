import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

async function moduleUrl(relative, imports = {}) {
  let code = ts.transpileModule(await readFile(new URL(relative, import.meta.url), 'utf8'), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  }).outputText;
  for (const [name,url] of Object.entries(imports)) code = code.replaceAll(`from '${name}'`, `from ${JSON.stringify(url)}`);
  return `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`;
}
const organellesUrl = await moduleUrl('../src/organelles.ts');
const { ORGANELLE_CATALOG, STARTER_BODIES } = await import(organellesUrl);
const language = await import(await moduleUrl('../src/genome-language.ts', { './organelles': organellesUrl }));
const { GENOME_SYMBOLS, BUILTIN_FUNCTIONS, BUILTIN_SIGNALS, BUILTIN_OUTPUTS, LANGUAGE_KEYWORDS,
  REGISTER_FAMILIES, lookupGenomeSymbol, registerSymbol, registerCompletions, searchGenomeSymbols,
  tokenizeGenome, identifierAt, symbolAt, genomeCompletionContext, genomeCompletions,
  genomeSignalCatalog, genomeSymbolDocumentation, unwrapGenomeSelection } = language;
const rust = await readFile(new URL('../engine/src/genome.rs', import.meta.url), 'utf8');
const sorted = values => [...values].sort();
const names = symbols => symbols.map(item => item.name);
const section = (start, end) => {
  const from = rust.indexOf(start);
  assert.ok(from >= 0, `Rust authority section exists: ${start}`);
  const to = rust.indexOf(end, from);
  assert.ok(to > from, `Rust authority section ends: ${end}`);
  return rust.slice(from, to);
};
const namedVariants = value => [...value.matchAll(/Self::(\w+)\s*=>\s*"([^"]+)"/g)].map(match => [match[1], match[2]]);

// Reading the Rust tables makes a newly added/renamed/removed builtin fail this check.
const sensorNames = namedVariants(section('impl Sensor {', '    fn ty(self)'));
const functionNames = namedVariants(section('impl Function {', '    fn arity(self)'));
assert.deepEqual(sorted(names(BUILTIN_SIGNALS)), sorted(sensorNames.map(([, name]) => name)), 'Sensor catalog matches Rust');
assert.deepEqual(sorted(names(BUILTIN_FUNCTIONS).filter(name => name !== 'populated')), sorted(functionNames.map(([, name]) => name)), 'Function catalog matches Rust');
assert.ok(rust.includes('if name == "populated"'), 'The special populated(register) form exists in the Rust parser');
const rustOutputNames = [...section('const OUTPUT_NAMES:', '];').matchAll(/"([^"]+)"/g)].map(match => match[1]);
assert.deepEqual(names(BUILTIN_OUTPUTS), rustOutputNames, 'Output names and order match Rust');
assert.deepEqual(sorted(names(LANGUAGE_KEYWORDS)), sorted(['let', 'if', 'then', 'else', 'when', 'clear', 'true', 'false']));
assert.equal(new Set(names(GENOME_SYMBOLS)).size, GENOME_SYMBOLS.length, 'The symbol catalog has no duplicate names');
assert.ok(GENOME_SYMBOLS.length < 300, 'Registers are resolved dynamically instead of expanding the base catalog');
for (const item of GENOME_SYMBOLS) {
  assert.ok(item.signature.includes(item.name), `Signature names ${item.name}`);
  assert.ok(item.description.length > 12, `${item.name} has useful help`);
  assert.ok(Object.isFrozen(item), 'Catalog entries cannot be accidentally changed by an editor');
}

const familyTypes = { ri: 'integer', rf: 'scalar', rb: 'boolean', rs: 'string', rv: 'vec2' };
assert.deepEqual(Object.fromEntries(REGISTER_FAMILIES.map(item => [item.prefix, item.type])), familyTypes);
for (const [prefix, type] of Object.entries(familyTypes)) {
  assert.equal(registerSymbol(prefix + '001').type, type);
  assert.equal(registerSymbol(prefix + '999').type, type);
  assert.equal(registerSymbol(prefix + '000'), undefined);
  assert.equal(registerSymbol(prefix + '1000'), undefined);
  assert.equal(registerSymbol(prefix + '1'), undefined);
  assert.equal(registerSymbol(prefix + '01'), undefined);
}
for (const name of ['RI001', 'ra001', 'ri00x', 'ri001more', 'rf001.x', 'rv-01']) assert.equal(registerSymbol(name), undefined);
assert.deepEqual(names(registerCompletions('ri99', '', 12)), Array.from({ length: 10 }, (_, i) => `ri99${i}`));
assert.deepEqual(names(registerCompletions('ri999')), ['ri999']);
assert.deepEqual(registerCompletions('ri000'), []);
assert.deepEqual(registerCompletions('ri1000'), []);
assert.equal(registerCompletions('r').length, 12);
assert.deepEqual(names(registerCompletions('r', '', 5)), ['ri001', 'rf001', 'rb001', 'rs001', 'rv001']);
assert.equal(registerCompletions('rf', '', 5000).length, 100, 'Public completion requests remain bounded');
assert.deepEqual(registerCompletions('rf', '', -1), []);
assert.deepEqual(registerCompletions('rf', '', Infinity), []);

const references = String.raw`# rf999 light.bearing sin(ri001)
rs001 = "rf888 # rb001 \"rv999\"";
rf040 = sin(light.bearing) + rf003;
clear rf040 when !populated(rf003);`;
const referenceTokens = tokenizeGenome(references);
assert.deepEqual(referenceTokens.filter(token => token.kind === 'register').map(token => token.text), ['rs001', 'rf040', 'rf003', 'rf040', 'rf003']);
assert.deepEqual(names(registerCompletions('rf', references, 4)), ['rf040', 'rf003', 'rf001', 'rf002'], 'Real references lead suggestions; comments and string content are excluded');
for (const hidden of ['rf999', 'rf888', 'rb001', 'rv999']) {
  assert.equal(symbolAt(references, references.indexOf(hidden) + 2), null, `${hidden} inside a comment/string is not a symbol`);
  assert.equal(identifierAt(references, references.indexOf(hidden) + 2), null);
}
assert.equal(symbolAt(references, references.lastIndexOf('light.bearing') + 6).symbol.name, 'light.bearing');
assert.equal(symbolAt(references, references.lastIndexOf('rf003') + 3).symbol.type, 'scalar');

const arithmetic = tokenizeGenome('let a = -.25 + 1e-3 * +2.0E+2 / 10; let b = 1.;');
assert.deepEqual(arithmetic.filter(token => token.kind === 'number').map(token => token.text), ['.25', '1e-3', '2.0E+2', '10', '1.']);
assert.equal(arithmetic.find(token => token.text === '-').kind, 'operator', 'Unary signs are operator tokens, not part of numeric literals');
assert.equal(arithmetic.find(token => token.text === 'let').kind, 'keyword');
const typed = tokenizeGenome('ri001 rf001 rb001 rs001 rv001 true false object.valid build.optical populated unknown ri000 ri12 ri1000 rf001.extra');
assert.deepEqual(typed.filter(token => token.kind === 'register').map(token => token.type), ['integer', 'scalar', 'boolean', 'string', 'vec2']);
assert.equal(typed.find(token => token.text === 'true').kind, 'boolean');
assert.equal(typed.find(token => token.text === 'object.valid').type, 'boolean');
assert.equal(typed.find(token => token.text === 'build.optical').kind, 'output');
assert.equal(typed.find(token => token.text === 'populated').kind, 'function');
assert.ok(typed.filter(token => ['unknown', 'ri000', 'ri12', 'ri1000', 'rf001.extra'].includes(token.text)).every(token => token.kind === 'identifier'));
assert.deepEqual(tokenizeGenome('a==b && !c || d<=e; f!=g >= h;').filter(token => token.kind === 'operator').map(token => token.text), ['==', '&&', '!', '||', '<=', '!=', '>=']);
assert.equal(tokenizeGenome('rf001 = "unfinished # ri001')[2].kind, 'string');
assert.equal(tokenizeGenome('rs001 = "unfinished\nrf001 = 2;').find(token => token.text === 'rf001').kind, 'register', 'A physical newline recovers an incomplete string for the editor; Rust diagnoses the invalid string');

const unicode = 'rs001 = "海🌱"; # 🌱\nrf001 = .5;';
const unicodeToken = tokenizeGenome(unicode).find(token => token.text === 'rf001');
assert.equal(unicodeToken.from, unicode.indexOf('rf001'), 'Token positions are UTF-16 offsets, not UTF-8 byte positions');
assert.equal(unicodeToken.to, unicodeToken.from + 5);
for (const token of tokenizeGenome(unicode)) assert.equal(unicode.slice(token.from, token.to), token.text);
assert.deepEqual(identifierAt('light.bearing', 13), { from: 0, to: 13, text: 'light.bearing' });
assert.equal(identifierAt('light.bearing', -1), null);
assert.equal(identifierAt('light.bearing', 15), null);
assert.equal(symbolAt('unknown', 4), null);
const locals = [{ name: 'remembered', type: 'boolean', from: 4, to: 14 }];
assert.equal(lookupGenomeSymbol('remembered', locals).type, 'boolean');
assert.equal(symbolAt('remembered', 4, locals).symbol.category, 'binding');
assert.equal(searchGenomeSymbols('remembered', locals)[0].name, 'remembered');
assert.equal(searchGenomeSymbols('light.bearing')[0].name, 'light.bearing');
assert.deepEqual(searchGenomeSymbols('nutrient.', [], 5).map(item => item.name).sort(), ['nutrient.capacity', 'nutrient.fraction', 'nutrient.stored']);
assert.equal(searchGenomeSymbols('ri999')[0].name, 'ri999');

// Editing incomplete source must separate writable statements from readable expressions.
const atEnd = (program, prefix = '', bindings = []) => genomeCompletions(program, program.length, prefix, bindings);
for (const empty of ['', '   ', '# a complete comment\n', 'thrust = 0;\n\n']) {
  assert.equal(genomeCompletionContext(empty, empty.length).kind, 'statement');
  const completions = atEnd(empty);
  for (const output of BUILTIN_OUTPUTS) assert.ok(completions.some(item => item.name === output.name && item.insertText === `${output.name} = `), `Blank statement offers ${output.name}`);
  for (const family of REGISTER_FAMILIES) assert.ok(completions.some(item => item.name === `${family.prefix}001` && item.insertText.endsWith(' = ')), `Blank statement offers ${family.type} writes`);
  assert.ok(!completions.some(item => item.category === 'signal' || item.category === 'function'), 'Blank statements do not advertise read-only values as write targets');
}
for (const expression of ['thrust = ', 'let target = ', 'divide = if ', 'turn = clamp(', 'let x = (', 'rb001 = true when ']) {
  assert.equal(genomeCompletionContext(expression, expression.length).kind, 'expression');
  assert.ok(atEnd(expression).length > 0, `Blank expression has completions: ${expression}`);
  assert.ok(atEnd(expression).every(item => item.category !== 'output' && !['let', 'clear'].includes(item.name)), 'Expression completions never include write-only commands');
}
assert.ok(atEnd('thrust = ').some(item => item.name === 'energy.fraction'));
assert.ok(!atEnd('thrust = ').some(item => item.name === 'object.valid'), 'Numeric assignment omits bare booleans');
assert.ok(atEnd('divide = ').some(item => item.name === 'object.valid'));
assert.ok(atEnd('divide = ').some(item => item.name === 'energy'), 'Boolean expressions can begin a numeric comparison');
assert.ok(atEnd('rs001 = ').some(item => item.name === 'rs001'));
assert.ok(atEnd('rs001 = ').some(item => item.name === 'object.token'), 'String assignments offer opaque observation tokens');
assert.ok(atEnd('rs001 = ').filter(item => item.category === 'signal').every(item => item.type === 'string'), 'String assignments omit incompatible sensor types');
assert.ok(atEnd('rv001 = ').some(item => item.name === 'vec'));
assert.ok(!atEnd('rv001 = ').some(item => item.name === 'light.bearing'));
assert.ok(atEnd('let a = dot(').some(item => item.name === 'rv001'));
assert.ok(!atEnd('let a = dot(').some(item => item.name === 'energy'));
assert.ok(atEnd('let a = sin(').some(item => item.name === 'energy'));
assert.ok(!atEnd('let a = sin(').some(item => item.name === 'rb001'));
for (const address of ['clear ', 'clear ri', 'let remembered = populated(', 'let remembered = populated(r']) {
  const prefix = address.endsWith('ri') ? 'ri' : address.endsWith('(r') ? 'r' : '';
  assert.equal(genomeCompletionContext(address, address.length).kind, 'register');
  assert.ok(atEnd(address, prefix).every(item => item.category === 'register' && !item.insertText));
}
assert.deepEqual(atEnd('# food.'), []);
assert.deepEqual(atEnd('rs001 = "food.'), []);
assert.deepEqual(atEnd('let '), [], 'Declaration names do not show builtin reads or writes');
assert.ok(atEnd('let ok = remembered && ', '', locals).some(item => item.name === 'remembered'));
assert.deepEqual(names(atEnd('th', 'th')), ['thrust']);
assert.ok(atEnd('thrust = light.', 'light.').every(item => item.name.startsWith('light.') && item.category === 'signal'));
assert.ok(atEnd('target.flagellum.count = ').some(item => item.name === 'organelle.flagellum.count'));
assert.ok(atEnd('target.flagellum.count = ').filter(item => item.category === 'signal').every(item => item.type === 'integer'));
assert.ok(atEnd('scan.target = ').some(item => item.name === 'contact.token'));
for (const name of ['food.visible', 'prey.attack', 'threat.visible', 'neighbor.angle', 'size', 'invest.sensing']) assert.equal(lookupGenomeSymbol(name), undefined);
for (const [selection, expected] of [
  [' (energy * 0.1) ', 'energy * 0.1'], ['((energy * 0.1))', 'energy * 0.1'],
  ['(\"(🌱)\")', '\"(🌱)\"'], ['(a) + (b)', '(a) + (b)'], ['(energy * 0.1', '(energy * 0.1'],
  ['(energy # context\n * 0.1)', 'energy # context\n * 0.1'],
]) {
  const range = unwrapGenomeSelection(selection, 0, selection.length);
  assert.equal(selection.slice(range.from, range.to), expected, `Only balanced enclosing parentheses are unwrapped: ${selection}`);
}
for (const direction of ['read', 'write']) {
  const catalog = genomeSignalCatalog(direction);
  assert.ok(catalog.every(item => item.category === (direction === 'read' ? 'signal' : 'output') || item.category === 'register'));
  assert.equal(catalog.filter(item => item.category === 'register').length, 5);
  for (const item of catalog) {
    const docs = genomeSymbolDocumentation(item);
    assert.ok(docs.example.length > 4 && docs.notes.length > 0, `${item.name} has a reference example and operational notes`);
  }
}

// Check actual accepted expressions and result types against the shipped Rust WASM.
const { instance } = await WebAssembly.instantiate(await readFile(new URL('../engine/pkg/agar_engine.wasm', import.meta.url)), {});
const api = instance.exports;
const decode = (ptr, len) => new TextDecoder().decode(new Uint8Array(api.memory.buffer, ptr, len));
function compileProgram(program, valid = true) {
  const bytes = new TextEncoder().encode(program), ptr = api.alloc(bytes.length);
  assert.ok(ptr);
  try {
    new Uint8Array(api.memory.buffer, ptr, bytes.length).set(bytes);
    const id = api.compile_genome(ptr, bytes.length);
    if (!valid) { assert.equal(id, 0, `Expected invalid expression: ${program}`); return null; }
    assert.ok(id, decode(api.error_ptr(), api.error_len()));
    const graphPtr = api.genome_graph_ptr(id);
    return JSON.parse(decode(graphPtr, api.genome_graph_len()));
  } finally { api.dealloc(ptr, bytes.length); }
}
const required = 'thrust = 0; turn = 0; divide = false;';
for (const item of [...genomeSignalCatalog('read'), ...genomeSignalCatalog('write')]) {
  const example = genomeSymbolDocumentation(item).example;
  const missing = ['thrust', 'turn', 'divide'].filter(name => !new RegExp(`(?:^|;)\\s*${name}\\s*=`).test(example));
  compileProgram(`${example}\n${missing.map(name => `${name} = ${name === 'divide' ? 'false' : '0'};`).join('\n')}`);
}
for (const sensor of BUILTIN_SIGNALS) {
  const graph = compileProgram(`let probe = ${sensor.name}; ${required}`);
  assert.equal(graph.nodes.find(node => node.kind === 'sensor' && node.label === sensor.name).type, sensor.type, `${sensor.name} has the documented primitive type`);
}
const roots = compileProgram(required).roots;
for (const item of BUILTIN_OUTPUTS) assert.equal(roots.find(root => root.kind === 'output' && root.name === item.name).type, item.type);
assert.equal(roots.find(root => root.name === 'feed').default, '1');
assert.equal(roots.find(root => root.name === 'build.optical').default, '0');
assert.equal(roots.find(root => root.name === 'target.flagellum.count').default, '0');
compileProgram(`${required} feed = true;`, false);
compileProgram(`${required} build.piercer = false;`, false);
compileProgram(`${required} target.flagellum.count = 2.0;`, false);
compileProgram(`${required} target.flagellum.count = 2; scan.target = object.token;`);
compileProgram(`${required} feed = 1 - gut.fraction; build.piercer = energy.fraction;`);
compileProgram(`rf001 = object.area; rs001 = object.token; rv001 = object.offset; ${required}`);
compileProgram(`ri001 = object.area; ${required}`, false);
compileProgram(`object.area = 1; ${required}`, false);
assert.equal(BUILTIN_SIGNALS.length, 144);
assert.equal(BUILTIN_OUTPUTS.length, 78);
const schemaPointer = api.language_schema_ptr();
const schema = JSON.parse(decode(schemaPointer,api.language_schema_len()));
assert.equal(schema.version,3);
assert.deepEqual(STARTER_BODIES.map(body=>body.id),['bare','producer','scavenger','hunter','mixed']);
for (const organ of ORGANELLE_CATALOG) {
  assert.equal(schema.outputs.find(output=>output.name===`target.${organ.kind}.size`).default,organ.minimumMass,`${organ.kind} minimum mass documentation matches Rust`);
}
assert.deepEqual(schema.signals, BUILTIN_SIGNALS.map(item => ({name:item.name,type:item.type})), 'Editor sensor names/types/order agree with authoritative Rust metadata');
assert.deepEqual(schema.outputs.map(item => ({name:item.name,type:item.type})), BUILTIN_OUTPUTS.map(item => ({name:item.name,type:item.type})), 'Editor write names/types/order agree with Rust metadata');
const functionCases = [
  ['sin(1)', 'scalar'], ['cos(1)', 'scalar'], ['abs(1)', 'integer'], ['abs(1.5)', 'scalar'],
  ['min(1, 2)', 'integer'], ['min(1, 2.0)', 'scalar'], ['max(1, 2)', 'integer'], ['max(1.0, 2)', 'scalar'],
  ['clamp(1, 0, 2)', 'integer'], ['clamp(1, 2.0, 0)', 'scalar'], ['int(1.5)', 'integer'], ['float(1)', 'scalar'],
  ['vec(1, 2.5)', 'vec2'], ['length(vec(1, 2))', 'scalar'], ['length("海🌱")', 'integer'],
  ['dot(vec(1, 2), vec(3, 4))', 'scalar'], ['normalize(vec(0, 0))', 'vec2'], ['angle(vec(1, 0))', 'scalar'],
  ['x(vec(1, 0))', 'scalar'], ['y(vec(1, 0))', 'scalar'], ['sqrt(1)', 'scalar'], ['sign(1)', 'scalar'],
  ['atan2(0, 1)', 'scalar'], ['lerp(1, 2, 3)', 'scalar'], ['noise(1, 0.5)', 'scalar'],
  ['identity(1)', 'integer'], ['identity(1.0)', 'scalar'], ['identity(true)', 'boolean'], ['identity("x")', 'string'], ['identity(vec(1, 2))', 'vec2'],
  ...REGISTER_FAMILIES.map(item => [`populated(${item.prefix}001)`, 'boolean']),
];
const checkedFunctions = new Set();
for (const [expression, type] of functionCases) {
  const name = expression.slice(0, expression.indexOf('('));
  const graph = compileProgram(`let probe = ${expression}; ${required}`);
  const result = graph.nodes.find(node => node.bindings.includes('probe'));
  assert.equal(result.type, type, expression);
  const documented = lookupGenomeSymbol(name).type;
  assert.ok(documented === 'T' || documented.split(' | ').includes(type), `${expression} agrees with its documented result types`);
  checkedFunctions.add(name);
}
assert.deepEqual(sorted(checkedFunctions), sorted(names(BUILTIN_FUNCTIONS)), 'Every builtin has at least one real compiler check');
for (const expression of ['sqrt(true)', 'length(1)', 'vec(1, true)', 'populated(ri001 + 1)', 'identity(1, 2)']) compileProgram(`let probe = ${expression}; ${required}`, false);
for (const program of [
  `rf001 = 1; ${required}`,
  `ri001 = int(1.5); ${required}`,
  `let probe = if true then 1 else 2.0; ${required}`,
]) compileProgram(program);
compileProgram(`ri001 = 1.5; ${required}`, false);

console.log(`Genome language checks passed: ${BUILTIN_SIGNALS.length} signals, ${BUILTIN_FUNCTIONS.length} functions, ${BUILTIN_OUTPUTS.length} outputs; typed registers, lexical isolation, cursor ranges, and compiler result types.`);
