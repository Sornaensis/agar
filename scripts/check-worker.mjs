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
    remote.setMutationRate(0),
    remote.resetEmpty(171),
    remote.seed(genome, 12),
    remote.advance(12, { metrics: true, environment: true }),
  ];
  reference.setEnvironment(0.5, 0.85, 5);
  reference.setMutationRate(0);
  reference.resetEmpty(171);
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

  {
    const { engine: lineage } = await fresh();
    await lineage.resetEmpty(37);
    assert.deepEqual(await lineage.getGenomeLineage(), []);
    const rootSource = 'thrust=.25;turn=0;divide=false;';
    const root = await lineage.compile(rootSource);
    assert.equal(await lineage.compile(rootSource), root);
    const first = (await lineage.mutate(root, 57, 1)).id;
    const grandchild = (await lineage.mutate(first, 7, 1)).id;
    const sibling = (await lineage.mutate(root, 31, 2)).id;
    assert.deepEqual([root, first, grandchild, sibling], [1, 2, 3, 4]);
    assert.equal((await lineage.mutate(root, 57, 1)).id, first, 'Replaying a mutation reuses its recorded edge');
    assert.equal((await lineage.mutate(root, 57, 4)).id, root, 'An inapplicable edit adds no lineage edge');
    const independent = await lineage.compile((await lineage.getGenome(first)).source);
    assert.equal(independent, 5, 'Compiling identical derived text directly creates an independent root');
    await lineage.seed(grandchild, 1);
    const tree = [
      { id: root, parentId: 0, mutationKind: 0 },
      { id: first, parentId: root, mutationKind: 1 },
      { id: grandchild, parentId: first, mutationKind: 1 },
      { id: sibling, parentId: root, mutationKind: 2 },
      { id: independent, parentId: 0, mutationKind: 0 },
    ];
    const savedTree = await lineage.saveState();
    assert.deepEqual(await lineage.getGenomeLineage(), tree, 'Include the unoccupied ancestor, sibling and independent root');
    assert.deepEqual(await lineage.getGenomeLineage(first), tree.slice(2));
    assert.deepEqual(await lineage.getGenomeLineage(independent), []);
    await assert.rejects(lineage.getGenomeLineage(-1), /offset/);
    assert.deepEqual(await lineage.saveState(), savedTree, 'Reading registry metadata preserves simulation state');
    await lineage.restoreState(new Uint8Array(await readFile(new URL('../engine/fixtures/organelle-v3.bin', import.meta.url))));
    assert.deepEqual(await lineage.getGenomeLineage(), [tree[0]], 'Oldest supported saves retain registered roots');
    await assert.rejects(lineage.getGenomeLineage(independent), /offset/);
    await lineage.restoreState(savedTree);
    assert.deepEqual(await lineage.getGenomeLineage(), tree, 'Restoring brings back dormant branches');
  }

  const compactOnly = await remote.presentation();
  assert.equal(Object.hasOwn(compactOnly, 'metrics'), false);
  assert.equal(Object.hasOwn(compactOnly, 'environment'), false);
  assert.equal(compactOnly.tick, compactOnly.frame.tick);
  assert.deepEqual(compactOnly.summary, reference.summary());

  // Hidden stepping sends exact state metadata without exporting entity arrays
  // or field grids. A read-only full refresh later catches the player up.
  const summaryOnly = await remote.advance(3, { frame: false }); reference.tick(3);
  assert.equal(Object.hasOwn(summaryOnly, 'frame'), false);
  assert.equal(Object.hasOwn(summaryOnly, 'metrics'), false);
  assert.equal(Object.hasOwn(summaryOnly, 'environment'), false);
  assert.equal(summaryOnly.tick, 15); assert.deepEqual(summaryOnly.summary, reference.summary());
  const metadataOnly = await remote.presentation({ frame: false, environment: 'metadata' });
  const { samples: _samples, ...expectedMetadata } = reference.environment();
  assert.deepEqual(metadataOnly.environment, expectedMetadata);
  assert.equal(Object.hasOwn(metadataOnly.environment, 'samples'), false);
  assert.deepEqual(await remote.environmentMetadata(), expectedMetadata);
  assert.deepEqual((await remote.presentation({ environment: true })).frame, reference.renderSnapshot());

  // Checkpoint metadata and owned bytes must describe the point between the two
  // queued advances, not whichever tick is current when the response is read.
  const beforeCapture = remote.advance(7);
  const captured = remote.captureCheckpoint();
  const afterCapture = remote.advance(9);
  reference.tick(7);
  const expectedBytes = reference.saveState(), expectedSummary = reference.summary();
  reference.tick(9);
  const [beforeFrame, checkpoint, afterFrame] = await Promise.all([beforeCapture, captured, afterCapture]);
  assert.equal(beforeFrame.frame.tick, 22);
  assert.equal(afterFrame.frame.tick, 31);
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
  assert.equal((await stillReadable).frame.tick, 22);
  const invalidSave = Uint8Array.from([1, 2, 3]);
  await assert.rejects(remote.restoreState(invalidSave), /save|MiB|invalid/i);
  assert.deepEqual(invalidSave, Uint8Array.from([1, 2, 3]));
  assert.deepEqual(await remote.saveState(), expectedBytes);
  await assert.rejects(remote.advance(-1), /whole ticks/);
  await remote.advance(1);
  assert.equal((await remote.presentation()).frame.tick, 23);
  await remote.summary();
  assert.ok(Atomics.load(adapter.transfers, 0) > 0, 'Worker returned transferable buffers');
  assert.equal(Atomics.load(adapter.transfers, 1), Atomics.load(adapter.transfers, 0), 'Every transferred buffer detached at its sender');

  // Real Rust performs the first two ticks; only the subsequent failure is
  // injected so partial-batch reporting is deterministic and affordable to test.
  const { engine: failing } = await fresh(true);
  const failingId = await failing.compile(source);
  await failing.resetEmpty(8);
  await failing.seed(failingId, 2);
  const partial = await failing.advance(5, { frame: false, metrics: true });
  assert.match(partial.error, /after two real ticks/);
  assert.equal(Object.hasOwn(partial, 'frame'), false); assert.equal(partial.tick, 2);
  assert.equal(partial.metrics.tick, 2);
  assert.equal((await failing.captureCheckpoint()).summary.time, partial.summary.time);
  assert.equal((await failing.advance(3)).frame.tick, 5, 'A reported tick failure leaves later RPC usable');

  // Environment commands must use the same ordered real-worker path as ticks
  // and saved runs. This covers the ABI decoder as well as the Rust behavior.
  {
    // Direct manipulation has its own exact placement path. Commands target the
    // immutable organism ID, even if another organism was added more recently.
    const { engine: edit } = await fresh();
    const founderGenome = await edit.compile(source);
    await edit.resetEmpty(81);
    const founder = await edit.placeCell(founderGenome, -0.625, 98.125);
    const other = await edit.placeCell(founderGenome, 200, 220);
    assert.notEqual(founder, other);
    const first = await edit.cellSnapshot(founder);
    assert.deepEqual([first.x, first.y, first.age, first.parentId], [1199.375, 98.125, 0, 0]);
    assert.equal(await edit.moveCell(founder, 100.0625, 200.25, true), 1);
    const moved = await edit.cellSnapshot(founder);
    assert.deepEqual([moved.x, moved.y, moved.vx, moved.vy], [100.0625,200.25,first.vx,first.vy]);
    assert.equal((await edit.cellSnapshot(other)).x,200);
    await edit.addBarrier(340, 440, 80, 20, 0, false);
    const intact = await edit.saveState();
    for (const operation of [
      () => edit.placeCell(founderGenome, 200, 220), () => edit.moveCell(founder, 340, 440),
      () => edit.moveCell(0x100000001, 100, 200), () => edit.placeCell(founderGenome, Infinity, 20),
      () => edit.setCellStores(founder, 1, moved.nutrientCapacity+1),
      () => edit.setCellStores(founder, NaN, null),
    ]) {
      await assert.rejects(operation());
      assert.deepEqual(await edit.saveState(), intact, 'Rejected edits preserve genome state, RNG and accounting');
    }
    const original = await edit.metricsSnapshot();
    await edit.setCellStores(founder, first.energy+1, null);
    await edit.setCellStores(founder, first.energy-1, first.nutrientStored-0.25);
    const edited = await edit.metricsSnapshot();
    assert.equal(edited.energyInputTotal,original.energyInputTotal+1);
    assert.equal(edited.energyWithdrawnTotal,2);
    assert.equal(edited.mineralWithdrawnTotal,0.25);
    assert.ok(Math.abs(edited.nutrientTotal+edited.mineralWithdrawnTotal-edited.nutrientInputTotal)<1e-7);
    assert.equal((await edit.snapshot()).mineralWithdrawnTotal,0.25);
    const editedState = await edit.saveState();
    await edit.tick(1);
    const next = await edit.saveState();
    await edit.restoreState(editedState);
    assert.deepEqual(await edit.metricsSnapshot(),edited);
    await edit.tick(1);
    assert.deepEqual(await edit.saveState(),next);
    await edit.setCellStores(founder, 0, null);
    assert.equal(await edit.cellSnapshot(founder),null);
    assert.equal((await edit.metricsSnapshot()).deaths,1);
    assert.ok((await edit.snapshot()).food.some(particle=>particle.kind===1));
    assert.ok(await edit.cellSnapshot(other));
    const afterDeath = await edit.saveState();
    await edit.restoreState(afterDeath);
    assert.deepEqual(await edit.saveState(),afterDeath);

    // A saved attached pair has actual mature adhesive machinery and registers.
    await edit.restoreState(new Uint8Array(await readFile(new URL('../engine/fixtures/attached-v11.bin', import.meta.url))));
    const pair = (await edit.snapshot()).cells;
    assert.equal(pair.length,2);
    assert.equal((await edit.cellBody(pair[0].id)).colony.neighbors,1);
    assert.equal(await edit.moveCell(pair[0].id,25,50,true),2);
    const transported = (await edit.snapshot()).cells;
    assert.equal(transported[0].x,25);
    assert.equal(transported[0].y,50);
    for (const axis of ['x','y']) assert.ok(Math.abs(transported[1][axis]-transported[0][axis]-
      (pair[1][axis]-pair[0][axis]))<1e-9);
    assert.equal(await edit.moveCell(pair[0].id,280,140,false),1);
    assert.deepEqual((await edit.snapshot()).cells[1],transported[1]);
    assert.equal((await edit.cellBody(pair[1].id)).colony.neighbors,0);
    assert.equal(await edit.moveCell(pair[0].id,290,150,true),1);
  }

  const { engine: habitat } = await fresh();
  await habitat.setEnvironment(0.5, 0, 0);
  assert.equal(await habitat.setWorldSize(2400, 1520), true);
  await habitat.setInitialNutrients(0.3);
  await habitat.resetEmpty(42);
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
  const sourceConfig = [{kind: 5, x: 400.25, y: 500.75, strength: .3, radius: 100, angle: .7, length: 1100},
    {kind: 8, x: 700, y: 400, strength: 1, radius: 80, angle: 0, length: 600}];
  assert.equal(await habitat.replaceSources(sourceConfig), true);
  const packedMetadata = await habitat.environmentMetadata();
  assert.equal(packedMetadata.sources.length, 2);
  for (let index = 0; index < sourceConfig.length; index++) for (const [key, value] of Object.entries(sourceConfig[index])) {
    assert.equal(packedMetadata.sources[index][key], Math.fround(value), `Packed source ${index} ${key} preserves scalar ABI normalization`);
  }
  const beforeInvalid = await habitat.saveState();
  assert.equal(await habitat.replaceSources([sourceConfig[0], {...sourceConfig[1], kind: 99}]), false);
  assert.deepEqual(await habitat.saveState(), beforeInvalid, 'Invalid packed source replacement is atomic');
  assert.equal(await habitat.replaceSources([]), true);
  assert.equal((await habitat.environmentMetadata()).sources.length, 0);

  const { engine: chemistry } = await fresh();
  await chemistry.setEnvironment(.5,0,0);
  assert.equal(await chemistry.setInitialGases(0,2),true);
  const sealed={mode:'sealed',exchangeRate:.02,oxygenEquilibrium:1,carbonEquilibrium:1};
  assert.equal(await chemistry.setGasExchange(sealed),true);
  await chemistry.resetEmpty(901);
  let chemicals=await chemistry.metricsSnapshot();
  assert.equal(chemicals.oxygenDissolvedTotal,0);
  assert.equal(chemicals.inorganicCarbonTotal,4560);
  const gasEnvironment=await chemistry.environment();
  assert.equal(gasEnvironment.samples.length,gasEnvironment.columns*gasEnvironment.rows*7);
  assert.equal(gasEnvironment.gases.initialOxygen,0);
  assert.equal(gasEnvironment.gases.initialCarbon,2);
  assert.equal(gasEnvironment.samples[5],0);assert.equal(gasEnvironment.samples[6],2);
  const checkChemicalBudget=metrics=>{
    assert.ok(Math.abs(metrics.carbonTotal+metrics.carbonRemovedTotal-metrics.carbonInputTotal)<1e-7,'Every external carbon transfer reconciles in the Float64 metrics');
    assert.ok(Math.abs(metrics.oxygenDissolvedTotal+metrics.oxygenCellTotal+metrics.oxygenRemovedTotal+metrics.carbonRespiredTotal+metrics.decayRespiredTotal-metrics.carbonFixedTotal-metrics.oxygenInputTotal)<1e-7,'Oxygen accounting includes metabolic stoichiometry');
  };
  const oxygenEmitter=await chemistry.addSource(8,300,300,1,120);
  const carbonAbsorber=await chemistry.addSource(11,600,380,.5,120);
  assert.ok(oxygenEmitter&&carbonAbsorber);
  const gasStep=await chemistry.advance(30,{metrics:true,environment:true});
  assert.ok(gasStep.metrics.oxygenDissolvedTotal>0);
  assert.ok(gasStep.metrics.carbonRemovedTotal>0);checkChemicalBudget(gasStep.metrics);
  const gasSave=await chemistry.saveState();
  assert.equal(await chemistry.setInitialGases(NaN,1),false);
  assert.equal(await chemistry.setGasExchange({...sealed,exchangeRate:-1}),false);
  assert.equal(await chemistry.setGasExchange({...sealed,mode:'unexpected'}),false);
  assert.deepEqual(await chemistry.saveState(),gasSave,'Invalid gas settings leave both fields and config unchanged');
  assert.equal(await chemistry.removeSource(oxygenEmitter),true);
  assert.equal(await chemistry.removeSource(carbonAbsorber),true);
  assert.ok(await chemistry.addSource(12,600,380,1,180));
  const beforeAerator=await chemistry.metricsSnapshot();
  chemicals=(await chemistry.advance(30,{metrics:true})).metrics;
  assert.ok(chemicals.oxygenInputTotal>beforeAerator.oxygenInputTotal,'A local aerator admits oxygen even when the rest of the surface is sealed');
  assert.ok(chemicals.carbonRemovedTotal>beforeAerator.carbonRemovedTotal,'Air exchange can release excess inorganic carbon');checkChemicalBudget(chemicals);
  const beforeGasResize=chemicals;
  assert.equal(await chemistry.setWorldSize(2400,1520),true);
  chemicals=await chemistry.metricsSnapshot();
  assert.ok(Math.abs(chemicals.carbonTotal-beforeGasResize.carbonTotal)<1e-7);
  assert.ok(Math.abs(chemicals.oxygenDissolvedTotal-beforeGasResize.oxygenDissolvedTotal)<1e-7);
  assert.equal(await chemistry.setGasExchange({...sealed,mode:'open'}),true);
  const gasCheckpoint=await chemistry.saveState();
  const gasContinuation=await chemistry.advance(12,{metrics:true,environment:true});
  checkChemicalBudget(gasContinuation.metrics);
  await chemistry.restoreState(gasCheckpoint);
  assert.deepEqual(await chemistry.advance(12,{metrics:true,environment:true}),gasContinuation,'Gas pools, external ledgers and boundary configuration replay exactly');
  const presentBeforeConfig=(await chemistry.metricsSnapshot()).oxygenDissolvedTotal;
  assert.equal(await chemistry.setInitialGases(12,12),true);
  assert.equal((await chemistry.metricsSnapshot()).oxygenDissolvedTotal,presentBeforeConfig,'Starting gas settings do not reset a live field');

  // All three supplied chemicals are dissolved. A lit, uninhabited pond
  // accumulates exactly the requested imports and produces no meals.
  const { engine: pond } = await fresh();
  assert.equal(await pond.setWorldSize(400,400),true);
  await pond.clearSources(); await pond.setEnvironment(.5,1,0);
  await pond.setInitialNutrients(0);
  assert.equal(await pond.setInitialGases(0,0),true);
  assert.equal(await pond.setGasExchange(sealed),true);
  await pond.resetEmpty(454);
  for (const kind of [3,8,10]) assert.ok(await pond.addSource(kind,200,200,1,100));
  const afterLight = (await pond.advance(120,{metrics:true})).metrics;
  assert.equal(afterLight.food,0);
  assert.equal(afterLight.carbonFixedTotal,0);
  for (const key of ['nutrientDissolvedTotal','oxygenDissolvedTotal','inorganicCarbonTotal']) {
    assert.ok(Math.abs(afterLight[key]-6)<1e-8, key);
  }
  await pond.clearSources();
  for (const kind of [4,9,11]) assert.ok(await pond.addSource(kind,200,200,.25,100));
  const drained = (await pond.advance(30,{metrics:true})).metrics;
  for (const key of ['nutrientDissolvedTotal','oxygenDissolvedTotal','inorganicCarbonTotal']) {
    assert.ok(Math.abs(afterLight[key]-drained[key]-.375)<1e-8, key);
  }
  assert.equal(drained.food,0);
  checkChemicalBudget(drained);

  const priorEnvironment=new Uint8Array(await readFile(new URL('../engine/fixtures/environment-v8.bin',import.meta.url)));
  const priorMetadata=JSON.parse(await readFile(new URL('../engine/fixtures/environment-v8.json',import.meta.url),'utf8'));
  await chemistry.restoreState(priorEnvironment);
  const migratedGas=await chemistry.presentation({metrics:true,environment:true});
  assert.equal(migratedGas.frame.tick,priorMetadata.tick);
  assert.equal(migratedGas.environment.barriers.length,2);
  assert.equal(migratedGas.environment.sources.length,2);
  assert.equal(migratedGas.environment.barriers.filter(item=>item.floating).length,1);
  assert.ok(Math.abs(migratedGas.metrics.nutrientTotal-priorMetadata.material)<1e-8);
  assert.ok(Math.abs(migratedGas.environment.nutrientRemovedTotal-priorMetadata.materialRemoved)<1e-6);
  assert.ok(migratedGas.metrics.legacyCarbonImported>0,'Legacy organic inventory is explicitly counted at migration');
  checkChemicalBudget(migratedGas.metrics);
  const upgradedGas=await chemistry.saveState();
  assert.equal(new DataView(upgradedGas.buffer,upgradedGas.byteOffset,upgradedGas.byteLength).getUint32(8,true),14);
  const migratedFuture=await chemistry.advance(12,{metrics:true,environment:true});
  await chemistry.restoreState(upgradedGas);
  assert.deepEqual(await chemistry.advance(12,{metrics:true,environment:true}),migratedFuture);

  // A substantial real tick batch runs while the main JS event loop continues.
  await remote.resetEmpty(62);
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

  console.log('Worker checks passed: threaded WASM, ordered commands, compact/full parity, transfer ownership, checkpoints, partial errors, responsive main loop, environment editing, gas sources/sinks/aerators, conservative resize, carbon/oxygen budgets, v8 migration, deterministic replay, and disposal.');
} finally {
  for (const engine of engines) engine.dispose();
  for (const adapter of adapters) adapter.terminate();
  await Promise.allSettled(adapters.map(adapter => adapter.termination));
  await rm(temporary, { recursive: true, force: true });
}
