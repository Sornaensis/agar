import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { setImmediate as nextTurn } from 'node:timers/promises';
import ts from 'typescript';

const source = await readFile(new URL('../src/session.ts', import.meta.url), 'utf8');
const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { SimulationSession, claimSimulation } = await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
const encode = value => new TextEncoder().encode(JSON.stringify(value));
const decode = bytes => JSON.parse(new TextDecoder().decode(bytes));
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
async function until(predicate, label) {
  for (let attempt = 0; attempt < 50 && !predicate(); attempt++) await nextTurn();
  assert.ok(predicate(), label);
}

function setup() {
  const engine = {
    time: 0, foodRate: 0, broken: false, disposed: false, gate: null, partialFailure: false, events: [],
    summary() { return { time: this.time, population: 1, food: 0, births: 0, deaths: 0, generation: 0 }; },
    async presentation() {
      return { frame: { ...this.summary(), tick: this.time * 30, cells: [{ id: 1, generation: 0 }], food: [], foodRate: this.foodRate },
        metrics: { tick: this.time * 30, population: 1 }, environment: { ambientTemperature: .5, ambientLight: .2, currentStrength: 0 } };
    },
    async advance(steps) {
      this.events.push(['advance', steps]);
      if (this.gate) { const gate = this.gate; this.gate = null; await gate.promise; }
      this.time += this.partialFailure ? 2 : steps;
      const result = await this.presentation();
      if (this.partialFailure) result.error = 'Organism allocation failed';
      this.events.push(['advanced', this.time]);
      return result;
    },
    async captureCheckpoint() {
      this.events.push(['capture', this.time]);
      if (this.broken) throw new Error('Snapshot failed');
      return { state: encode({ time: this.time, foodRate: this.foodRate }), summary: this.summary() };
    },
    async setFoodRate(value) { this.foodRate = value; },
    dispose() { this.disposed = true; },
  };
  const writes = [], committed = [], statuses = [];
  const storage = { synchronousFailure: false, putSimulation(record) {
    if (this.synchronousFailure) throw new Error('Validation failed');
    const commit = deferred();
    writes.push({ record: structuredClone(record), ...commit });
    return commit.promise;
  } };
  const metadata = {
    id: 'simulation:test', name: 'Test', environmentName: 'Still water', seed: 42, paused: true, speed: 1,
    createdAt: 1, updatedAt: 2, genomeNames: { 1: 'Forager' }, history: [], summary: engine.summary(),
  };
  const initial = { frame: { ...engine.summary(), tick: 0, cells: [{ id: 1, generation: 0 }], food: [] }, metrics: {}, environment: {} };
  const session = new SimulationSession(engine, metadata, initial, storage,
    (status, error) => statuses.push({ status, error }), value => committed.push(structuredClone(value)));
  return { engine, session, metadata, storage, writes, committed, statuses };
}

// Capture is queued behind outstanding advancement, and before later commands.
{
  const { engine, session, writes } = setup();
  const gate = engine.gate = deferred();
  const advance = session.advance(3), saved = session.checkpoint(), later = session.advance(2);
  assert.equal(session.busy, true);
  await nextTurn();
  assert.deepEqual(engine.events, [['advance', 3]]);
  assert.equal(writes.length, 0);
  gate.resolve();
  await advance; await later;
  await until(() => writes.length === 1, 'Queued checkpoint reaches storage');
  assert.deepEqual(engine.events, [['advance', 3], ['advanced', 3], ['capture', 3], ['advance', 2], ['advanced', 5]]);
  assert.equal(decode(writes[0].record.state).time, 3);
  assert.equal(writes[0].record.summary.time, 3);
  writes[0].resolve(); await saved;
  assert.equal(session.savedRevision, 1);
  assert.equal(session.revision, 2);
}

// A later revision must never be marked saved by an older asynchronous write.
{
  const { engine, session, metadata, writes, statuses, committed } = setup();
  await session.checkpoint();
  assert.equal(writes.length, 0, 'A clean run does not create another checkpoint');
  metadata.genomeNames[1] = 'External mutation';
  assert.equal(session.metadata.genomeNames[1], 'Forager', 'Session metadata is owned');
  await session.advance(3);
  const first = session.checkpoint();
  await until(() => writes.length === 1, 'First save starts');
  await session.advance(2);
  session.metadata.genomeNames[1] = 'Changed after capture'; session.touch();
  const coalesced = session.checkpoint();
  assert.equal(coalesced, first, 'Overlapping autosaves reuse one promise');
  assert.equal(writes[0].record.genomeNames[1], 'Forager');
  assert.equal(session.savedRevision, 0);
  const flush = session.flush();
  assert.equal(session.busy, true, 'The scheduler cannot advance during flush');
  writes[0].resolve(); await first;
  await until(() => writes.length === 2, 'Flush captures the newest revision');
  assert.equal(session.savedRevision, 1);
  assert.equal(decode(writes[1].record.state).time, engine.time);
  assert.equal(writes[1].record.summary.time, engine.time);
  assert.equal(writes[1].record.genomeNames[1], 'Changed after capture');
  session.metadata.speed = 10; session.touch();
  writes[1].resolve();
  await until(() => writes.length === 3, 'Flush drains metadata edited during the final write');
  assert.equal(writes[2].record.speed, 10);
  writes[2].resolve(); await flush;
  assert.equal(session.savedRevision, session.revision);
  assert.equal(session.pending, null);
  assert.equal(session.busy, false);
  assert.equal(committed.length, 3);
  assert.deepEqual(statuses.map(value => value.status), ['saving', 'saved', 'saving', 'saved', 'saving', 'saved']);
  assert.equal(Object.hasOwn(committed[0], 'state'), false, 'List metadata does not retain checkpoint bytes');
}

// Related inspector reads remain on one tick even if a new batch is queued.
{
  const { engine, session } = setup();
  const gate = deferred();
  const detail = session.inspect(async current => {
    const summaryTime = current.time;
    await gate.promise;
    return { summaryTime, bodyTime: current.time };
  });
  const advance = session.advance(3);
  await nextTurn(); assert.equal(engine.time, 0);
  gate.resolve();
  assert.deepEqual(await detail, { summaryTime: 0, bodyTime: 0 });
  await advance; assert.equal(engine.time, 3);
  assert.equal(session.revision, 1, 'Inspection does not dirty a run');
}

// Mutations, their metadata, and their derived presentation precede capture.
{
  const { engine, session, writes } = setup();
  const gate = deferred();
  const mutation = session.mutate(async current => {
    await gate.promise; await current.setFoodRate(12);
    session.metadata.genomeNames[2] = 'New arrival';
    return 2;
  });
  const save = session.checkpoint();
  gate.resolve(); assert.equal(await mutation, 2);
  await until(() => writes.length === 1, 'Mutation checkpoint starts');
  assert.equal(session.world.foodRate, 12);
  assert.equal(decode(writes[0].record.state).foodRate, 12);
  assert.equal(writes[0].record.genomeNames[2], 'New arrival');
  writes[0].resolve(); await save;
  await assert.rejects(session.mutate(async current => {
    await current.setFoodRate(9); throw new Error('Mutation partially failed');
  }), /partially failed/);
  assert.equal(session.world.foodRate, engine.foodRate);
  assert.equal(session.revision, 2, 'Partial mutations remain dirty and visible');
  await session.advance(1);
  assert.equal(session.world.time, 1, 'A failed command does not poison later work');
}

// A failed batch can still complete ticks; their actual state must be saved.
{
  const { engine, session, writes } = setup();
  engine.partialFailure = true;
  await assert.rejects(session.advance(5), /allocation failed/);
  assert.equal(session.world.time, 2);
  assert.equal(session.metadata.summary.time, 2);
  assert.notEqual(session.savedRevision, session.revision);
  const saved = session.checkpoint();
  await until(() => writes.length === 1, 'Partial batch can be saved');
  assert.equal(writes[0].record.summary.time, decode(writes[0].record.state).time);
  writes[0].resolve(); await saved;
}

// Capture, validation and storage errors all remain dirty and retryable.
for (const failure of ['snapshot', 'validation', 'commit']) {
  const { engine, session, storage, writes, statuses, committed } = setup();
  await session.advance(1);
  if (failure === 'snapshot') engine.broken = true;
  if (failure === 'validation') storage.synchronousFailure = true;
  const rejected = session.checkpoint();
  const checked = assert.rejects(rejected, failure === 'snapshot' ? /Snapshot failed/ : failure === 'validation' ? /Validation failed/ : /Commit failed/);
  if (failure === 'commit') {
    await until(() => writes.length === 1, 'Failing commit starts');
    writes[0].reject(new Error('Commit failed'));
  }
  await checked;
  assert.equal(session.savedRevision, 0); assert.equal(session.pending, null);
  assert.equal(committed.length, 0); assert.equal(statuses.at(-1).status, 'error');
  engine.broken = false; storage.synchronousFailure = false;
  const count = writes.length, retry = session.flush();
  await until(() => writes.length > count, 'Failed save can be retried');
  writes.at(-1).resolve(); await retry;
  assert.equal(session.savedRevision, session.revision);
  assert.equal(statuses.at(-1).status, 'saved');
}

// Closing while a transaction fails must reject without declaring success.
{
  const { session, writes } = setup();
  await session.advance(1);
  const saved = assert.rejects(session.checkpoint(), /Storage unavailable/);
  const flush = assert.rejects(session.flush(), /Storage unavailable/);
  await until(() => writes.length === 1, 'Closing checkpoint starts');
  writes[0].reject(new Error('Storage unavailable'));
  await saved; await flush;
  assert.notEqual(session.savedRevision, session.revision);
}

// A clock rollback cannot invalidate the record; history remains bounded.
{
  const { session, writes } = setup();
  session.metadata.updatedAt = Date.now() + 60000; session.touch();
  const save = session.checkpoint();
  await until(() => writes.length === 1, 'Clock rollback checkpoint starts');
  assert.ok(writes[0].record.updatedAt >= session.metadata.updatedAt);
  writes[0].resolve(); await save;
  for (let index = 0; index < 730; index++) await session.advance(1);
  assert.equal(session.metadata.history.length, 720);
  assert.equal(session.metadata.history[0].time, 11);
  assert.equal(session.metadata.history.at(-1).time, session.world.time);
}

// Disposing a run releases ownership once and rejects new commands.
{
  const { engine, session } = setup();
  let released = 0; session.release = () => { released++; };
  await session.flush(); session.dispose(); session.dispose();
  assert.equal(engine.disposed, true); assert.equal(released, 1);
  await assert.rejects(session.advance(1), /closed/);
  await assert.rejects(session.inspect(async () => 1), /closed/);
}

// A browser lock lives from acquisition until explicit release.
{
  const calls = []; let held = false;
  const locks = { request(name, options, callback) {
    calls.push({ name, options });
    if (held) return Promise.resolve(callback(null));
    held = true;
    return Promise.resolve(callback({ name })).finally(() => { held = false; });
  } };
  Object.defineProperty(globalThis, 'navigator', { value: { locks }, configurable: true });
  const release = await claimSimulation('simulation:lock-test');
  assert.equal(held, true);
  await assert.rejects(claimSimulation('simulation:lock-test'), /another tab/);
  assert.equal(calls[0].name, 'cellarium:simulation:simulation:lock-test');
  assert.deepEqual(calls[0].options, { mode: 'exclusive', ifAvailable: true });
  release(); await nextTurn(); assert.equal(held, false);
  const releaseAgain = await claimSimulation('simulation:lock-test'); releaseAgain(); await nextTurn();
  Object.defineProperty(globalThis, 'navigator', { value: {}, configurable: true });
  await assert.rejects(claimSimulation('simulation:lock-test'), /Web Locks/);
}

console.log('Session checks passed: ordered commands and same-tick inspection/capture; coalesced autosaves; revision isolation; final flush; partial failures and retries; bounded history; disposal and exclusive lock lifetime.');
