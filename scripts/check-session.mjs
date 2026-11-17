import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
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
const settle = async () => { for (let index = 0; index < 8; index++) await Promise.resolve(); };

function setup() {
  const engine = {
    time: 0, broken: false,
    snapshot() { return { time: this.time, cells: [{ id: 1, generation: 0 }], food: [], births: 0, deaths: 0 }; },
    tick(steps) { this.time += steps; },
    saveState() { if (this.broken) throw new Error('Snapshot failed'); return encode({ time: this.time }); },
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
    createdAt: 1, updatedAt: 2, genomeNames: { 1: 'Forager' }, history: [],
    summary: { time: 0, population: 1, food: 0, births: 0, deaths: 0, generation: 0 },
  };
  const session = new SimulationSession(engine, metadata, storage, (status, error) => statuses.push({ status, error }), value => committed.push(structuredClone(value)));
  return { engine, session, metadata, storage, writes, committed, statuses };
}

// A later revision must never be marked saved by an older asynchronous write.
{
  const { engine, session, metadata, writes, statuses, committed } = setup();
  await session.checkpoint();
  assert.equal(writes.length, 0, 'A clean run does not create another checkpoint');
  metadata.genomeNames[1] = 'External mutation';
  assert.equal(session.metadata.genomeNames[1], 'Forager', 'Session metadata is owned');
  session.advance(3);
  const first = session.checkpoint();
  assert.equal(writes.length, 1);
  assert.equal(decode(writes[0].record.state).time, 3);
  assert.equal(writes[0].record.summary.time, 3);
  session.advance(2);
  session.metadata.genomeNames[1] = 'Changed after capture';
  session.touch();
  const coalesced = session.checkpoint();
  assert.equal(writes.length, 1, 'Overlapping autosave calls reuse the pending write');
  assert.equal(writes[0].record.genomeNames[1], 'Forager');
  assert.equal(session.savedRevision, 0);
  const flush = session.flush(); // Advancement is now suspended by the caller.
  writes[0].resolve();
  await first;
  await coalesced;
  await settle();
  assert.equal(writes.length, 2, 'Flush follows the old checkpoint with the latest revision');
  assert.equal(session.savedRevision, 1, 'The earlier commit covers only its captured revision');
  assert.equal(decode(writes[1].record.state).time, engine.time);
  assert.equal(writes[1].record.summary.time, engine.time);
  assert.equal(writes[1].record.genomeNames[1], 'Changed after capture');
  // A UI metadata change while the final checkpoint is pending must also commit
  // before ownership can be released, even though physical advancement is suspended.
  session.metadata.speed = 10;
  session.touch();
  writes[1].resolve();
  await settle();
  assert.equal(writes.length, 3, 'Flush drains a revision created during its final checkpoint');
  assert.equal(writes[2].record.speed, 10);
  assert.equal(decode(writes[2].record.state).time, engine.time);
  writes[2].resolve();
  await flush;
  assert.equal(session.savedRevision, session.revision);
  assert.equal(session.pending, null);
  assert.equal(committed.length, 3);
  assert.deepEqual(statuses.map(value => value.status), ['saving', 'saved', 'saving', 'saved', 'saving', 'saved']);
  assert.equal(Object.hasOwn(committed[0], 'state'), false, 'List metadata does not retain checkpoint bytes');
}

// Both synchronous validation failure and asynchronous commit failure remain dirty and retryable.
for (const failure of ['snapshot', 'validation', 'commit']) {
  const { engine, session, storage, writes, statuses, committed } = setup();
  session.advance(1);
  if (failure === 'snapshot') engine.broken = true;
  if (failure === 'validation') storage.synchronousFailure = true;
  const rejected = session.checkpoint();
  const checked = assert.rejects(rejected, failure === 'snapshot' ? /Snapshot failed/ : failure === 'validation' ? /Validation failed/ : /Commit failed/);
  if (failure === 'commit') writes[0].reject(new Error('Commit failed'));
  await checked;
  assert.equal(session.savedRevision, 0);
  assert.equal(session.pending, null);
  assert.equal(committed.length, 0);
  assert.equal(statuses.at(-1).status, 'error');
  engine.broken = false; storage.synchronousFailure = false;
  const retry = session.flush();
  writes.at(-1).resolve();
  await retry;
  assert.equal(session.savedRevision, session.revision);
  assert.equal(statuses.at(-1).status, 'saved');
}

// Closing while a transaction fails must reject; it must not imply the world is saved.
{
  const { session, writes } = setup();
  session.advance(1);
  const save = session.checkpoint();
  const saved = assert.rejects(save, /Storage unavailable/);
  const flush = assert.rejects(session.flush(), /Storage unavailable/);
  writes[0].reject(new Error('Storage unavailable'));
  await saved; await flush;
  assert.notEqual(session.savedRevision, session.revision);
}

// Rewinding the wall clock must not invalidate an otherwise valid simulation document.
{
  const { session, writes } = setup();
  session.metadata.updatedAt = Date.now() + 60000;
  session.touch();
  const save = session.checkpoint();
  assert.ok(writes[0].record.updatedAt >= session.metadata.updatedAt);
  writes[0].resolve(); await save;
}
{
  const { session } = setup();
  for (let index = 0; index < 730; index++) session.advance(1);
  assert.equal(session.metadata.history.length, 720);
  assert.equal(session.metadata.history[0].time, 11);
  assert.equal(session.metadata.history.at(-1).time, session.world.time);
}

// Exercise lock lifetime: claim resolves on acquisition, but the browser lock is held until release.
{
  const calls = [];
  let held = false;
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
  release(); await settle();
  assert.equal(held, false);
  const releaseAgain = await claimSimulation('simulation:lock-test');
  releaseAgain(); await settle();
  Object.defineProperty(globalThis, 'navigator', { value: {}, configurable: true });
  await assert.rejects(claimSimulation('simulation:lock-test'), /Web Locks/);
}

console.log('Session checks passed: coalesced autosaves, same-tick snapshots, revision isolation, final flush ordering, failed-save retries, synchronous validation errors, clock rollback, bounded history, and exclusive lock lifetime.');
