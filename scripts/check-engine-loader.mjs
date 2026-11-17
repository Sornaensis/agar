import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile, rm, truncate } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import { validateEngineArtifact } from './check-engine-artifact.mjs';

function unsigned(value) {
  const bytes = [];
  do { const byte = value & 0x7f; value >>>= 7; bytes.push(byte | (value ? 0x80 : 0)); } while (value);
  return bytes;
}
function readUnsigned(bytes, cursor) {
  let value = 0, shift = 0, byte;
  do {
    assert.ok(cursor.offset < bytes.length && shift < 35, 'Bounded WASM integer');
    byte = bytes[cursor.offset++]; value |= (byte & 0x7f) << shift; shift += 7;
  } while (byte & 0x80);
  return value >>> 0;
}

// Keep real compiled engine code and edit only its export table. These are
// genuine instantiable WASM binaries, not JavaScript mock export objects.
function changeExports(bytes, change) {
  const cursor = { offset: 8 }, chunks = [bytes.subarray(0, 8)];
  let found = false;
  while (cursor.offset < bytes.length) {
    const start = cursor.offset, section = bytes[cursor.offset++];
    const length = readUnsigned(bytes, cursor), end = cursor.offset + length;
    assert.ok(end <= bytes.length);
    if (section === 7) {
      const count = readUnsigned(bytes, cursor), entries = [];
      for (let i = 0; i < count; i++) {
        const size = readUnsigned(bytes, cursor);
        const name = new TextDecoder().decode(bytes.subarray(cursor.offset, cursor.offset + size));
        cursor.offset += size;
        const kind = bytes[cursor.offset++], index = readUnsigned(bytes, cursor);
        entries.push({ name, kind, index });
      }
      assert.equal(cursor.offset, end);
      const updated = change(entries), payload = [...unsigned(updated.length)];
      for (const entry of updated) {
        const name = new TextEncoder().encode(entry.name);
        payload.push(...unsigned(name.length), ...name, entry.kind, ...unsigned(entry.index));
      }
      chunks.push(Uint8Array.from([7, ...unsigned(payload.length), ...payload]));
      found = true;
    } else chunks.push(bytes.subarray(start, end));
    cursor.offset = end;
  }
  assert.ok(found);
  return Buffer.concat(chunks);
}

async function engineClass(assetUrl, contract) {
  const source = await readFile(new URL('../src/engine.ts', import.meta.url), 'utf8');
  assert.ok(source.includes("import engineUrl from '../engine/pkg/agar_engine.wasm?url';"));
  assert.ok(source.includes("import engineContract from './engine-contract.json';"));
  const transformed = source
    .replace("import engineUrl from '../engine/pkg/agar_engine.wasm?url';", `const engineUrl = ${JSON.stringify(assetUrl)};`)
    .replace("import engineContract from './engine-contract.json';", `const engineContract = ${JSON.stringify(contract)};`);
  const code = ts.transpileModule(transformed, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  return (await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`)).Engine;
}

export async function checkEngineLoader() {
  const artifact = await validateEngineArtifact();
  const contract = JSON.parse(await readFile(new URL('../src/engine-contract.json', import.meta.url), 'utf8'));
  assert.equal(artifact.abi, contract.abi);
  const bytes = artifact.bytes;
  const legacy = changeExports(bytes, entries => entries.filter(entry => !['analyze_genome', 'genome_analysis_len'].includes(entry.name)));
  const wrongFunction = changeExports(bytes, entries => entries.map(entry => entry.name === 'analyze_genome'
    ? { ...entry, kind: 2, index: entries.find(item => item.name === contract.memory).index } : entry));
  const wrongMemory = changeExports(bytes, entries => entries.map(entry => entry.name === contract.memory
    ? { ...entry, kind: 0, index: entries.find(item => item.name === 'abi_version').index } : entry));
  const wrongAbi = changeExports(bytes, entries => entries.map(entry => entry.name === 'abi_version'
    ? { ...entry, index: entries.find(item => item.name === 'genome_limit').index } : entry));
  const legacyInstance = (await WebAssembly.instantiate(legacy, {})).instance;
  assert.equal(legacyInstance.exports.abi_version(), 4, 'Regression fixture is an actual ABI4 engine');
  assert.equal(legacyInstance.exports.analyze_genome, undefined);
  const temporary = await mkdtemp(join(tmpdir(), 'agar-engine-check-'));
  try {
    for (const [name, data, expected] of [
      ['older-abi4.wasm', legacy, /Missing engine exports: analyze_genome, genome_analysis_len/],
      ['wrong-function.wasm', wrongFunction, /analyze_genome is memory, expected function/],
      ['wrong-memory.wasm', wrongMemory, /memory is function, expected memory/],
      ['wrong-abi.wasm', wrongAbi, /Expected engine ABI 4, received 2048/],
      ['invalid.wasm', new Uint8Array(8), /not a valid WebAssembly module/],
    ]) {
      const path = join(temporary, name); await writeFile(path, data);
      await assert.rejects(validateEngineArtifact(path), error => {
        assert.match(error.message, expected);
        assert.match(error.message, /npm run build:wasm/);
        return true;
      });
    }
    await assert.rejects(validateEngineArtifact(join(temporary, 'missing.wasm')), /could not be read.*npm run build:wasm/);
    const oversized = join(temporary, 'oversized.wasm'); await writeFile(oversized, ''); await truncate(oversized, 17 * 1024 * 1024);
    await assert.rejects(validateEngineArtifact(oversized), /16 MiB/);
  } finally { await rm(temporary, { recursive: true, force: true }); }

  const oldUrl = 'https://example.github.io/example-repository/wasm/cellarium_engine.wasm';
  const currentUrl = `https://example.github.io/example-repository/assets/agar_engine-${artifact.sha256.slice(0, 8)}.wasm`;
  const OldEngine = await engineClass(oldUrl, contract), Engine = await engineClass(currentUrl, contract);
  const originalFetch = globalThis.fetch, originalCompile = WebAssembly.compile;
  const requests = [], failures = [() => new Response('', { status: 503 }), ...[new Uint8Array([0, 1, 2, 3]), legacy, wrongFunction, wrongMemory, wrongAbi].map(data => () => new Response(data))];
  let compilations = 0;
  try {
    globalThis.fetch = async url => {
      requests.push(url);
      // A browser may retain this fixed legacy URL indefinitely. The current
      // build must request its separate fingerprinted asset instead.
      if (url === oldUrl) return new Response(legacy);
      assert.equal(url, currentUrl);
      return failures.length ? failures.shift()() : new Response(bytes);
    };
    WebAssembly.compile = value => { compilations++; return originalCompile(value); };
    await assert.rejects(OldEngine.load(), /different builds.*Missing engine exports: analyze_genome, genome_analysis_len/);
    await assert.rejects(Engine.load(), /HTTP 503/);
    await assert.rejects(Engine.load(), WebAssembly.CompileError);
    await assert.rejects(Engine.load(), /different builds.*Missing engine exports: analyze_genome, genome_analysis_len/);
    await assert.rejects(Engine.load(), /Missing engine exports: analyze_genome/);
    await assert.rejects(Engine.load(), /Missing engine exports: memory/);
    await assert.rejects(Engine.load(), /Expected engine ABI 4, received 2048/);
    const requestsBefore = requests.length, compilationsBefore = compilations;
    const [editorA, editorB, run] = await Promise.all([Engine.load(), Engine.load(), Engine.load()]);
    assert.equal(requests.length, requestsBefore + 1, 'Concurrent loads share one fetch after retrying failures');
    assert.equal(compilations, compilationsBefore + 1, 'Concurrent loads share one successful compilation');
    assert.equal(requests.filter(url => url === oldUrl).length, 1, 'Current loader never revisits the cached legacy path');
    const untouched = editorB.saveState();
    const id = editorA.compile('ri001=ri001+1;thrust=0.5;turn=0;divide=false;');
    editorA.resetEmpty(123, 0); assert.equal(editorA.seed(id, 1), 1); editorA.tick(3);
    assert.deepEqual(editorB.saveState(), untouched, 'Instances retain independent genomes, memory, and worlds');
    run.restoreState(editorA.saveState()); run.tick(10); editorA.tick(10);
    assert.deepEqual(run.saveState(), editorA.saveState(), 'Typed checkpoints continue across independent instances');
    const beforeAnalysis = editorA.saveState();
    const unicode = '# é🌱\nlet note="🌱";let count=ri001+1;thrust=0;turn=0;divide=false;';
    const analysis = editorA.analyze(unicode);
    assert.ok(analysis.valid);
    assert.deepEqual(analysis.bindings.map(binding => [unicode.slice(binding.from, binding.to), binding.type]), [['note', 'string'], ['count', 'integer']]);
    assert.equal(editorA.analyze('let unfinished =').valid, false);
    assert.equal(editorA.analyze('').diagnostics[0].from, 0);
    assert.equal(editorA.analyze('é'.repeat(131073)).sourceBytes, 262146);
    assert.deepEqual(editorA.saveState(), beforeAnalysis);
    await Engine.load();
    assert.equal(requests.length, requestsBefore + 1, 'Later loads reuse compiled code');
    assert.equal(compilations, compilationsBefore + 1);
  } finally {
    globalThis.fetch = originalFetch; WebAssembly.compile = originalCompile;
  }
  console.log('Engine artifact/loader checks passed: real old ABI4 regression, export types/memory/ABI guards, bounded input, fingerprinted cache isolation, retry/shared compilation, and independent state.');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await checkEngineLoader();
