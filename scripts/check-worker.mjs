import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { Worker } from 'node:worker_threads';
import ts from 'typescript';
import { validateEngineArtifact } from './check-engine-artifact.mjs';

// Run the actual facade and dispatcher in separate JS threads, with the real
// compiled Rust module. Only browser transport/loading glue is adapted for Node.
const artifact = await validateEngineArtifact();
const contract = JSON.parse(await readFile(new URL('../src/engine-contract.json', import.meta.url), 'utf8'));
const temporary = await mkdtemp(join(tmpdir(), 'agar-worker-'));
const adapters = [];
const engines = [];
const compilerOptions = { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext };

try {
  for (const name of ['engine', 'simulation-protocol', 'simulation-worker-runtime', 'worker-engine']) {
    let source = await readFile(new URL(`../src/${name}.ts`, import.meta.url), 'utf8');
    if (name === 'engine') {
      source = source
        .replace("import engineUrl from '../engine/pkg/agar_engine.wasm?url';", `const engineUrl = ${JSON.stringify(`data:application/wasm;base64,${Buffer.from(artifact.bytes).toString('base64')}`)};`)
        .replace("import engineContract from './engine-contract.json';", `const engineContract = ${JSON.stringify(contract)};`);
    }
    let code = ts.transpileModule(source, { compilerOptions }).outputText;
    for (const dependency of ['engine', 'simulation-protocol', 'simulation-worker-runtime']) {
      code = code.replaceAll(`from './${dependency}'`, `from './${dependency}.mjs'`);
    }
    await writeFile(join(temporary, `${name}.mjs`), code);
  }
  await writeFile(join(temporary, 'thread.mjs'), `
    import assert from 'node:assert/strict';
    import { parentPort, workerData } from 'node:worker_threads';
    import { Engine } from './engine.mjs';
    import { createSimulationWorkerHandler } from './simulation-worker-runtime.mjs';
    const transfers = new Int32Array(workerData.transfers);
    const load = async () => {
      const engine = await Engine.load();
      if (workerData.partialFailure) {
        const tick = engine.tick.bind(engine);
        let fail = true;
        engine.tick = steps => {
          if (fail && steps > 2) {
            fail = false;
            tick(2);
            throw new Error('Injected allocation failure after two real ticks');
          }
          tick(steps);
        };
      }
      return engine;
    };
    const handle = createSimulationWorkerHandler((response, buffers) => {
      parentPort.postMessage(response, buffers);
      Atomics.add(transfers, 0, buffers.length);
      for (const buffer of buffers) {
        assert.equal(buffer.byteLength, 0, 'Returned buffers transfer ownership out of the worker');
        Atomics.add(transfers, 1, 1);
      }
    }, load);
    parentPort.on('message', request => handle(request));
  `);

  const { Engine } = await import(pathToFileURL(join(temporary, 'engine.mjs')).href);
  const { WorkerEngine } = await import(pathToFileURL(join(temporary, 'worker-engine.mjs')).href);

  class NodeWorkerAdapter {
    onmessage = null;
    onerror = null;
    onmessageerror = null;
    stopped = false;
    transfers = new Int32Array(new SharedArrayBuffer(8));
    constructor(partialFailure = false) {
      this.thread = new Worker(pathToFileURL(join(temporary, 'thread.mjs')), {
        workerData: { partialFailure, transfers: this.transfers.buffer },
      });
      this.thread.on('message', data => this.onmessage?.({ data }));
      this.thread.on('error', error => this.onerror?.({ message: error.message, preventDefault() {} }));
      this.thread.on('messageerror', error => this.onmessageerror?.({ data: error }));
      this.thread.on('exit', code => {
        if (!this.stopped) this.onerror?.({ message: `Simulation worker exited (${code})`, preventDefault() {} });
      });
      adapters.push(this);
    }
    postMessage(request, transfer) { this.thread.postMessage(request, transfer); }
    terminate() { this.stopped = true; this.termination ??= this.thread.terminate(); }
  }
  async function fresh(partialFailure = false) {
    const adapter = new NodeWorkerAdapter(partialFailure);
    const engine = await WorkerEngine.fromWorker(adapter);
    engines.push(engine);
    return { engine, adapter };
  }

  const source = [
    'ri001=ri001+1;', 'rs001="owned memory";',
    'target.photosynthetic.count=1;', 'target.photosynthetic.size=0.05;',
    'express.photosynthetic=if protein.photosynthetic+organelle.photosynthetic.mass<0.05 then 1 else 0;',
    'photosynthesize=1;', 'thrust=0;', 'turn=0;', 'divide=false;',
  ].join('\n');
  const { engine: remote, adapter } = await fresh();
  const reference = await Engine.load();
  const genome = await remote.compile(source);
  assert.equal(genome, reference.compile(source));

  // Issued without awaiting each other: the worker must retain invocation order.
  const pending = [
    remote.setEnvironment(0.5, 0.85, 5),
    remote.setFoodRate(0),
    remote.setMutationRate(0),
    remote.resetEmpty(171, 48),
    remote.seed(genome, 12),
    remote.advance(12, { metrics: true, environment: true }),
  ];
  reference.setEnvironment(0.5, 0.85, 5);
  reference.setFoodRate(0);
  reference.setMutationRate(0);
  reference.resetEmpty(171, 48);
  reference.seed(genome, 12);
  reference.tick(12);
  const initial = (await Promise.all(pending)).at(-1);
  assert.deepEqual(initial.frame, reference.renderSnapshot());
  assert.deepEqual(initial.metrics, reference.metricsSnapshot());
  assert.deepEqual(initial.environment, reference.environment());
  assert.equal(Object.hasOwn(initial.frame, 'environment'), false);
  assert.equal(Object.hasOwn(initial.frame.cells[0], 'organelles'), false);

  const full = reference.snapshot();
  for (let index = 0; index < full.cells.length; index++) {
    const cell = full.cells[index], visual = initial.frame.cells[index];
    for (const key of ['id', 'genomeId', 'x', 'y', 'vx', 'vy', 'angle', 'radius', 'energy', 'energyCapacity', 'gutEnergy', 'gutCapacity', 'integrity', 'photosynthesisRate', 'generation']) {
      assert.equal(visual[key], cell[key], `Render field ${key}`);
    }
    const family = kind => cell.organelles.find(organ => organ.kind === kind);
    assert.equal(visual.flagellumCount, family('flagellum').count);
    assert.equal(visual.flagellumCapacity, family('flagellum').capacity);
    assert.equal(visual.cortexMass, family('cortex').mass);
    assert.equal(visual.cortexCapacity, family('cortex').capacity);
    assert.equal(visual.photosyntheticCapacity, family('photosynthetic').capacity);
    assert.equal(visual.piercerCapacity, family('piercer').capacity);
  }
  assert.deepEqual(initial.frame.food, full.food);
  for (const key of ['mutatedBirths', 'mutationSkipped', 'genomeCount', 'genomeLimit', 'genomeBytes', 'genomeByteLimit', 'predationCount', 'attackCount', 'photosynthesisTotal', 'energyTotal', 'energyInputTotal', 'nutrientTotal', 'nutrientInputTotal', 'nutrientDissolvedTotal', 'nutrientFoodTotal', 'nutrientCellTotal']) {
    assert.equal(initial.metrics[key], full[key], `Metric ${key}`);
  }
  assert.equal(initial.metrics.population, full.cells.length);
  assert.equal(initial.metrics.food, full.food.length);
  assert.equal(initial.metrics.organicCounts.reduce((sum, count) => sum + count, 0), full.food.length);
  assert.deepEqual(initial.metrics.genomes, [{ id: genome, population: 12, maxGeneration: 0 }]);

  const id = full.cells[0].id;
  assert.deepEqual(await remote.cellSnapshot(id), full.cells[0]);
  assert.equal(await remote.cellSnapshot(0xffff_ffff), null);
  assert.deepEqual(await remote.cellBody(id), reference.cellBody(id));
  assert.deepEqual(await remote.getMemory(id), reference.getMemory(id));
  assert.deepEqual(await remote.getCellEvaluation(id), reference.getCellEvaluation(id));
  assert.deepEqual(await remote.getGenome(genome), reference.getGenome(genome));
  assert.deepEqual(await remote.getGenomeGraph(genome), reference.getGenomeGraph(genome));
  assert.deepEqual(await remote.saveState(), reference.saveState(), 'Presentation and inspection are read-only');

  const compactOnly = await remote.presentation();
  assert.equal(Object.hasOwn(compactOnly, 'metrics'), false);
  assert.equal(Object.hasOwn(compactOnly, 'environment'), false);

  // Checkpoint metadata and owned bytes must describe the point between the two
  // queued advances, not whichever tick is current when the response is read.
  const beforeCapture = remote.advance(7);
  const captured = remote.captureCheckpoint();
  const afterCapture = remote.advance(9);
  reference.tick(7);
  const expectedBytes = reference.saveState(), expectedSummary = reference.summary();
  reference.tick(9);
  const [beforeFrame, checkpoint, afterFrame] = await Promise.all([beforeCapture, captured, afterCapture]);
  assert.equal(beforeFrame.frame.tick, 19);
  assert.equal(afterFrame.frame.tick, 28);
  assert.deepEqual(checkpoint.state, expectedBytes);
  assert.deepEqual(checkpoint.summary, expectedSummary);
  assert.deepEqual(afterFrame.frame, reference.renderSnapshot());

  const savedCopy = checkpoint.state.slice();
  await remote.restoreState(checkpoint.state);
  assert.deepEqual(checkpoint.state, savedCopy, 'Restore transfers its own copy, preserving the caller\'s save');
  assert.deepEqual(await remote.summary(), expectedSummary);
  assert.deepEqual(await remote.saveState(), expectedBytes);

  // A rejected command does not block later commands or corrupt existing state.
  const rejected = assert.rejects(remote.compile('thrust=;turn=0;divide=false;'), /expression|expected|unexpected/i);
  const stillReadable = remote.presentation();
  await rejected;
  assert.equal((await stillReadable).frame.tick, 19);
  const invalidSave = Uint8Array.from([1, 2, 3]);
  await assert.rejects(remote.restoreState(invalidSave), /save|MiB|invalid/i);
  assert.deepEqual(invalidSave, Uint8Array.from([1, 2, 3]));
  assert.deepEqual(await remote.saveState(), expectedBytes);
  await assert.rejects(remote.advance(-1), /whole ticks/);
  await remote.advance(1);
  assert.equal((await remote.presentation()).frame.tick, 20);
  await remote.summary();
  assert.ok(Atomics.load(adapter.transfers, 0) > 0, 'Worker returned transferable buffers');
  assert.equal(Atomics.load(adapter.transfers, 1), Atomics.load(adapter.transfers, 0), 'Every transferred buffer detached at its sender');

  // Real Rust performs the first two ticks; only the subsequent failure is
  // injected so partial-batch reporting is deterministic and affordable to test.
  const { engine: failing } = await fresh(true);
  const failingId = await failing.compile(source);
  await failing.setFoodRate(0);
  await failing.resetEmpty(8, 0);
  await failing.seed(failingId, 2);
  const partial = await failing.advance(5, { metrics: true });
  assert.match(partial.error, /after two real ticks/);
  assert.equal(partial.frame.tick, 2);
  assert.equal(partial.metrics.tick, 2);
  assert.equal((await failing.captureCheckpoint()).summary.time, partial.frame.time);
  assert.equal((await failing.advance(3)).frame.tick, 5, 'A reported tick failure leaves later RPC usable');

  // Environment commands must use the same ordered real-worker path as ticks
  // and saved runs. This covers the ABI decoder as well as the Rust behavior.
  const { engine: habitat } = await fresh();
  await habitat.setFoodRate(0);
  await habitat.setEnvironment(0.5, 0, 0);
  assert.equal(await habitat.setWorldSize(2400, 1520), true);
  await habitat.setInitialNutrients(0.3);
  await habitat.resetEmpty(42, 0);
  const band = await habitat.addSource(6, 1200, 400, 0.6, 120, Math.PI / 2, 1200);
  const sink = await habitat.addSource(4, 300, 300, 1, 160);
  const wall = await habitat.addBarrier(800, 760, 500, 40, Math.PI / 4, false);
  const floater = await habitat.addBarrier(1200, 400, 100, 20, 0, true);
  assert.ok(band > 0 && sink > 0 && wall > 0 && floater > 0);
  let environment = await habitat.environment();
  assert.deepEqual([environment.width, environment.height, environment.columns, environment.rows], [2400, 1520, 120, 76]);
  assert.equal(environment.sources.find(item => item.id === band).length, 1200);
  assert.ok(Math.abs(environment.sources.find(item => item.id === band).angle - Math.PI / 2) < 1e-6);
  assert.equal(environment.barriers.find(item => item.id === floater).floating, true);
  const introduced = (await habitat.metricsSnapshot()).nutrientInputTotal;
  const transported = await habitat.advance(30, { metrics: true, environment: true });
  assert.ok(transported.environment.nutrientRemovedTotal > 0, 'A live absorber removes material');
  assert.ok(Math.abs(transported.metrics.nutrientTotal + transported.environment.nutrientRemovedTotal - introduced) < 0.001, 'Removal is explicit in the material balance');
  const moved = transported.environment.barriers.find(item => item.id === floater);
  assert.ok(Math.hypot(moved.x - 1200, moved.y - 400) > 0.01, 'A floating barrier follows its local current with global current strength zero');
  assert.equal(await habitat.updateSource(band, 6, 1200, 400, -0.4, 140, 0, 1600), true);
  assert.equal((await habitat.environment()).sources.find(item => item.id === band).strength < 0, true);
  const beforeResize = (await habitat.metricsSnapshot()).nutrientTotal;
  assert.equal(await habitat.setWorldSize(3600, 2280), true);
  assert.ok(Math.abs((await habitat.metricsSnapshot()).nutrientTotal - beforeResize) < 1e-6, 'Resizing redistributes the existing field without creating material');
  const settled = await habitat.saveState();
  assert.equal(await habitat.setWorldSize(3601, 2280), false);
  assert.equal(await habitat.updateSource(0xffff, 6, 0, 0, 1, 100, 0, 600), false);
  assert.deepEqual(await habitat.saveState(), settled, 'Invalid geometry commands are atomic');
  const later = await habitat.advance(12, { metrics: true, environment: true });
  await habitat.restoreState(settled);
  assert.deepEqual(await habitat.advance(12, { metrics: true, environment: true }), later, 'Resized worlds, sink ledgers, and moving barriers replay exactly after restore');
  assert.equal(await habitat.removeSource(sink), true);
  assert.equal(await habitat.removeBarrier(wall), true);
  assert.equal(await habitat.updateBarrier(floater, 900, 900, 180, 40, 0, false), true);
  assert.equal((await habitat.environment()).barriers[0].floating, false);
  await habitat.clearSources(); await habitat.clearBarriers();
  environment = await habitat.environment();
  assert.equal(environment.sources.length + environment.barriers.length, 0);

  // A substantial real tick batch runs while the main JS event loop continues.
  await remote.resetEmpty(62, 1500);
  await remote.seed(genome, 256);
  let heartbeats = 0;
  const timer = setInterval(() => { heartbeats++; }, 1);
  let advanced;
  try { advanced = await remote.advance(120); } finally { clearInterval(timer); }
  assert.equal(advanced.frame.tick, 120);
  assert.ok(heartbeats > 0, 'The main event loop remains responsive during real WASM simulation');

  const outstandingAdvance = assert.rejects(remote.advance(120), /closed/);
  const outstandingSave = assert.rejects(remote.captureCheckpoint(), /closed/);
  remote.dispose();
  await Promise.all([outstandingAdvance, outstandingSave]);
  await assert.rejects(remote.presentation(), /closed/);
  remote.dispose(); // Closing twice is harmless.

  console.log('Worker checks passed: real threaded WASM, ordered commands, compact/full projection parity, aggregate/detail inspection, transferred buffers, atomic checkpoints, restore ownership, recoverable command/partial-tick errors, responsive main loop, environment editing, large worlds, absorber accounting, floating barriers, deterministic replay, and disposal of pending work.');
} finally {
  for (const engine of engines) engine.dispose();
  for (const adapter of adapters) adapter.terminate();
  await Promise.allSettled(adapters.map(adapter => adapter.termination));
  await rm(temporary, { recursive: true, force: true });
}
