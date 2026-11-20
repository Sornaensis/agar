import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile, rm, truncate } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
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
  assert.equal(legacyInstance.exports.abi_version(), contract.abi, 'Missing-export fixture retains real compiled engine code');
  assert.equal(legacyInstance.exports.analyze_genome, undefined);
  const temporary = await mkdtemp(join(tmpdir(), 'agar-engine-check-'));
  try {
    for (const [name, data, expected] of [
      ['missing-analysis.wasm', legacy, /Missing engine exports: analyze_genome, genome_analysis_len/],
      ['wrong-function.wasm', wrongFunction, /analyze_genome is memory, expected function/],
      ['wrong-memory.wasm', wrongMemory, /memory is function, expected memory/],
      ['wrong-abi.wasm', wrongAbi, new RegExp(`Expected engine ABI ${contract.abi}, received 2048`)],
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
    await assert.rejects(Engine.load(), new RegExp(`Expected engine ABI ${contract.abi}, received 2048`));
    const requestsBefore = requests.length, compilationsBefore = compilations;
    const [editorA, editorB, run] = await Promise.all([Engine.load(), Engine.load(), Engine.load()]);
    assert.equal(requests.length, requestsBefore + 1, 'Concurrent loads share one fetch after retrying failures');
    assert.equal(compilations, compilationsBefore + 1, 'Concurrent loads share one successful compilation');
    assert.equal(requests.filter(url => url === oldUrl).length, 1, 'Current loader never revisits the cached legacy path');
    const untouched = editorB.saveState();
    const id = editorA.compile('ri001=ri001+1;thrust=0.5;turn=0;divide=false;');
    editorA.resetEmpty(123); assert.equal(editorA.seed(id, 1), 1); editorA.tick(3);
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
    const populationProbe = await Engine.load();
    const probeGenome = populationProbe.compile('rf001=chemical.organic;rf002=chemical.mineral;thrust=0;turn=0;divide=false;');
    populationProbe.resetEmpty(81);
    const beforeInvalidSeed = populationProbe.saveState();
    for (const count of [-1, 0.5, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1, 0x100000000, 0xffffffff]) {
      assert.throws(() => populationProbe.seed(probeGenome, count), /count|identifier/i);
      assert.deepEqual(populationProbe.saveState(), beforeInvalidSeed, 'Rejected requests cannot partially seed or wrap their count');
    }
    assert.equal(populationProbe.seed(probeGenome, 0), 0);
    assert.equal(populationProbe.seed(probeGenome, 2049), 2049, 'There is no fixed 384 or 2048 population ceiling');
    assert.equal(populationProbe.snapshot().cells.length, 2049);
    assert.equal(Object.hasOwn(populationProbe.snapshot(), 'maxCells'), false);
    const populationState = populationProbe.saveState(), populationCopy = await Engine.load();
    populationCopy.restoreState(populationState);
    assert.equal(populationCopy.snapshot().cells.length, 2049);
    assert.deepEqual(populationCopy.saveState(), populationState);
    const organicProbe = await Engine.load();
    organicProbe.resetEmpty(71);
    assert.equal(organicProbe.snapshot().food.length, 0);
    for (const name of ['add_food','set_food_rate']) assert.equal(name in organicProbe.wasm, false);
    const organicHistory = await readFile(new URL('../engine/fixtures/organic-history-v13.bin', import.meta.url));
    assert.equal(organicHistory.readUInt32LE(8), 13);
    organicProbe.restoreState(organicHistory);
    const api = organicProbe.wasm;
    const packetStart = api.snapshot_ptr() + 12 * 8;
    const packets = new Uint8Array(api.memory.buffer, packetStart, (api.snapshot_len() - 12) * 8);
    assert.equal(createHash('sha256').update(packets).digest('hex'), 'cdbd87b3d8d4751a4adce408cb22361dc47cffd06c98cb384d1849703b4f540e');
    const organicSaved = organicProbe.saveState();
    const organicTotals = organicProbe.metricsSnapshot();
    assert.equal(organicTotals.food, 1401);
    organicProbe.tick(300);
    const continued = organicProbe.metricsSnapshot();
    for (const field of ['food','energyTotal','energyInputTotal','carbonTotal','nutrientTotal','nutrientInputTotal']) {
      assert.equal(continued[field], organicTotals[field], 'Existing particles persist without continuing the old external supply');
    }
    const future = organicProbe.saveState();
    organicProbe.restoreState(organicSaved); organicProbe.tick(300);
    assert.deepEqual(organicProbe.saveState(), future);
    const current=editorA.saveState();
    assert.equal(new DataView(current.buffer,current.byteOffset,current.byteLength).getUint32(8,true),20);
    for (const filename of ['trophic-v1.bin','trophic-v2.bin']) {
      const legacy=new Uint8Array(await readFile(new URL('../engine/fixtures/'+filename,import.meta.url)));
      assert.throws(()=>editorA.restoreState(legacy),/Unsupported simulation version.*preserved/);
      assert.deepEqual(editorA.saveState(),current,'Old body saves are rejected atomically, never silently converted');
    }
    // Check the genuine WASM32 lineage on its native pointer-width budget.
    // A single Nurse rosette grew in the ABI16 Colony pool (seed 42, tick 2250).
    const oldGrown = await readFile(new URL('../engine/fixtures/grown-v12.bin', import.meta.url));
    assert.equal(oldGrown.readUInt32LE(8),12);
    assert.equal(createHash('sha256').update(oldGrown).digest('hex'),'2563e1f2aa9c1c26af2f5ab99a28671e7330ee4c2005af3ab538351197f0d91f');
    const grown = await Engine.load(); grown.restoreState(oldGrown);
    const oldTotals = grown.metricsSnapshot();
    const close = (a,b) => assert.ok(Math.abs(a-b)<1e-8, `Expected ${a} to equal ${b}`);
    close(oldTotals.energyTotal,324.8172229953766);
    close(grown.cellBody(1).foodPotential.body,1.6401427086660116*20 + 33.74410098753904);
    close(grown.cellBody(1).foodPotential.atp,77.26168152533621);
    grown.setCellStores(1,0,null);
    const deadTotals = grown.metricsSnapshot();
    assert.equal(deadTotals.population,0);
    close(deadTotals.energyTotal + deadTotals.energyWithdrawnTotal,oldTotals.energyTotal);
    close(deadTotals.carbonTotal,oldTotals.carbonTotal);
    close(deadTotals.nutrientTotal,oldTotals.nutrientTotal);
    const carcass = grown.saveState(); grown.restoreState(carcass);
    assert.deepEqual(grown.saveState(),carcass);
    // Produced by the shipped ABI13 WASM (SHA-256 3ecb2671f2d07847a473ffa07817324aa1e258010364deaa00ce82db6e75a1e4).
    // This founder has a v9 cellular genome and no contact declarations or register state.
    const historical = await readFile(new URL('../engine/fixtures/contact-migration-v9.bin', import.meta.url));
    assert.equal(historical.readUInt32LE(8), 9);
    const migrated = await Engine.load();
    migrated.restoreState(historical);
    const upgraded = migrated.saveState();
    assert.equal(new DataView(upgraded.buffer, upgraded.byteOffset, upgraded.byteLength).getUint32(8,true), 20);
    // Recorded on the shipped ABI19 engine using this authentic v9 founder.
    // Compare biology independently of the current genome registry's memory budget.
    function physicalStateHash(instance) {
      const api = instance.wasm;
      const hash = createHash('sha256');
      for (const [read, len, width] of [['snapshot_ptr','snapshot_len',8],
        ['environment_snapshot_ptr','environment_snapshot_len',4], ['metrics_snapshot_ptr','presentation_len',8]]) {
        const pointer = api[read]();
        const bytes = new Uint8Array(api.memory.buffer, pointer, api[len]() * width);
        if (read === 'metrics_snapshot_ptr') {
          // One three-word recipe Vec and an optional contact guard pointer live in the WASM32 cellular definition.
          assert.equal(new DataView(bytes.buffer,bytes.byteOffset).getFloat64(13 * width,true),5543+12+4);
          hash.update(bytes.subarray(0, 13 * width));
          hash.update(bytes.subarray(14 * width));
        } else hash.update(bytes);
      }
      return hash.digest('hex');
    }
    assert.equal(physicalStateHash(migrated),'3dfd0bb386b35fec378afb1d0f9325adf4c68dcc6a36db93755a7868ef17a329');
    const replay = await Engine.load();
    replay.restoreState(upgraded);
    const importedTick = migrated.snapshot().tick;
    migrated.tick(60);
    replay.tick(60);
    assert.equal(migrated.snapshot().tick, importedTick+60);
    assert.deepEqual(replay.saveState(), migrated.saveState(), 'The migrated founder replays exactly');
    assert.equal(physicalStateHash(migrated),physicalStateHash(replay));
    assert.ok(Math.abs(migrated.wasm.nutrient_total()-migrated.wasm.nutrient_input_total()) < 1e-7);
    const energy = migrated.wasm.energy_total();
    assert.ok(energy > 0 && energy <= migrated.wasm.energy_input_total()+migrated.wasm.photosynthesis_total()+1e-7);
    const markerOnly = await Engine.load();
    markerOnly.restoreState(await readFile(new URL('../engine/fixtures/signal-migration-v10.bin', import.meta.url)));
    assert.deepEqual(markerOnly.getMemory(1).map(x => [x.name,x.value]), [['rf002',6]]);
    assert.deepEqual(markerOnly.getMemory(2).map(x => [x.name,x.value]), [['rf007',17]]);
    for (const id of [1,2]) {
      assert.equal(markerOnly.getCellEvaluation(id).contactMessages.signal, null);
      assert.deepEqual(markerOnly.cellBody(id).colony,{neighbors:0,neighborIds:[],colonyNeighborIds:[],feedingNeighborIds:[],contactNeighborIds:[],energyNet:0,mineralNet:0,carbonNet:0});
    }
    // ABI17's import-time physical projections, with retired rate slot 11 zeroed.
    for (const [read, len, width, expected] of [
      ['snapshot_ptr','snapshot_len',8,'e683c315c787695af922b6eb393f71eda7424d7af6e19eb26137629871040c86'],
      ['environment_snapshot_ptr','environment_snapshot_len',4,'f10db2a8b3fe4c5ba8f271ce188d31c17778b03427b676d2ae8596267c559d39'],
    ]) {
      const api = markerOnly.wasm, ptr = api[read]();
      assert.equal(createHash('sha256').update(new Uint8Array(api.memory.buffer,ptr,api[len]()*width)).digest('hex'),expected);
    }
    replay.restoreState(markerOnly.saveState());
    markerOnly.tick(60); replay.tick(60);
    assert.deepEqual(markerOnly.saveState(), replay.saveState());
    assert.equal(markerOnly.snapshot().food.length, 1, 'The old automatic six-particle/s supply stops at import.');

    const body=editorA.cellBody(editorA.snapshot().cells[0].id);
    assert.ok(body.organs.length>0);assert.ok(body.spaceUsed<=body.spaceCapacity);
    const schema=editorA.languageSchema();assert.equal(schema.signals.length,174);assert.equal(schema.outputs.length,81);
    assert.deepEqual(editorA.saveState(),current,'Schema and body inspection do not alter saved state');
    await Engine.load();
    assert.equal(requests.length, requestsBefore + 1, 'Later loads reuse compiled code');
    assert.equal(compilations, compilationsBefore + 1);
  } finally {
    globalThis.fetch = originalFetch; WebAssembly.compile = originalCompile;
  }
  console.log('Engine artifact/loader checks passed: export types/memory/ABI guards, bounded input, fingerprinted cache isolation, retry/shared compilation, independent state, version20 body/product/substrate/mechanism/control/environment/gas continuation and explicit atomic rejection of pre-organelle checkpoints.');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await checkEngineLoader();
