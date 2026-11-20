import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

const source = await readFile(new URL('../src/simulation-loop.ts', import.meta.url), 'utf8');
const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { SimulationLoop } = await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
const settle = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };

class Clock {
  now = 0;
  sequence = 0;
  tasks = new Map();
  window(name, hidden = false) {
    const clock = this;
    return { name, document: { hidden },
      setTimeout(callback, delay) { const id = ++clock.sequence; clock.tasks.set(id, { callback, due: clock.now + delay, owner: this }); return id; },
      clearTimeout(id) { clock.tasks.delete(id); },
    };
  }
  async advance(milliseconds) {
    const end = this.now + milliseconds;
    let turns = 0;
    for (;;) {
      await settle();
      const next = [...this.tasks.entries()].sort((a, b) => a[1].due - b[1].due || a[0] - b[0])[0];
      if (!next || next[1].due > end || this.now > end) break;
      assert.ok(++turns < 100000, 'No synchronous/busy-spin timer loop');
      this.tasks.delete(next[0]); this.now = Math.max(this.now, next[1].due); next[1].callback();
    }
    this.now = Math.max(this.now, end); await settle();
  }
}

// There are no animation callbacks at all. Fast hidden runs can still reach
// their requested rate rather than inheriting the old 100ms × eight-tick cap.
for (const speed of [1, 2, 5, 10]) {
  const clock = new Clock(), host = clock.window('hidden-player', true), target = {};
  let ticks = 0, inFlight = 0, maximum = 0, paused = false;
  const batches = [];
  const loop = new SimulationLoop(() => host, () => paused ? null : { target, speed, busy: false }, async (current, steps) => {
    assert.equal(current, target); inFlight++; maximum = Math.max(maximum, inFlight); batches.push(steps); ticks += steps;
    await Promise.resolve(); inFlight--;
  }, (_target, error) => { throw error; }, () => clock.now);
  loop.start(); await clock.advance(10000);
  assert.equal(maximum, 1); assert.ok(batches.every(steps => steps >= 1 && steps <= 8));
  assert.ok(ticks >= 30 * speed * 10 - 9 && ticks <= 30 * speed * 10, `${speed}×: ${ticks} ticks in 10s`);
  if (speed === 10) assert.ok(ticks / 10 > 80, 'Hidden 10× work is independent of the display fallback');
  paused = true; loop.reset(); const before = ticks; await clock.advance(60000); assert.equal(ticks, before);
  paused = false; loop.reset(); await clock.advance(100); assert.ok(ticks > before && ticks - before <= 30 * speed / 10);
  loop.stop(); assert.equal(clock.tasks.size, 0);
}

// A visible display can request an earned tick before the timer's full batch
// deadline. The physics clock still bounds elapsed time at each refresh rate.
for (const [speed, refreshRate] of [[1, 60], [2, 60], [5, 120]]) {
  const clock = new Clock(), host = clock.window('visible'), target = {};
  let needsFrame = true, published = 0, ticks = 0;
  const loop = new SimulationLoop(() => host, () => ({ target, speed, busy: false }), async (_target, steps) => {
    ticks += steps;
    if (needsFrame) { published++; needsFrame = false; }
  }, (_target, error) => { throw error; }, () => clock.now);
  loop.start();
  for (let frame = 1; frame <= refreshRate * 2; frame++) {
    await clock.advance(frame * 1000 / refreshRate - clock.now);
    needsFrame = true;
    loop.refresh(true);
  }
  loop.stop();
  assert.ok(Math.abs(ticks - 60 * speed) <= 2, `Wall-time physics at ${speed}×: ${ticks}`);
  assert.ok(published >= 1.8 * Math.min(30 * speed, refreshRate), `${published} publications at ${refreshRate} Hz / ${speed}×`);
  assert.ok(published <= refreshRate * 2 + 1);
}

// A slow tick is indivisible, but timing feedback prevents queuing an eight-tick
// block once the measured work exceeds the responsiveness budget.
{
  const clock = new Clock(), host = clock.window('visible-player'), target = {}, batches = [];
  const loop = new SimulationLoop(() => host, () => ({ target, speed: 10, busy: false }), async (_target, steps) => {
    batches.push(steps); clock.now += 40 * steps;
  }, (_target, error) => { throw error; }, () => clock.now);
  loop.start(); await clock.advance(600); loop.stop();
  assert.ok(batches.length >= 10); assert.ok(batches.every(steps => steps === 1), 'Start cautiously and adapt to expensive ticks');
}

// The accelerated player can finish a partial worker slice. A delayed
// response must not force every subsequent command down to one tick.
{
  const clock = new Clock(), host = clock.window('accelerated'), target = {};
  const calls = [];
  let paused = false;
  const loop = new SimulationLoop(() => host, () => paused ? null : { target, speed: 10, busy: false, workerSliceMs: 50 },
    async (_target, requested, _now, slice) => {
      assert.equal(slice, 50);
      const completed = Math.min(requested, 3);
      calls.push({ requested, completed });
      clock.now += 20 * completed;
      if (calls.length === 8) paused = true;
      return completed;
    }, (_target, error) => { throw error; }, () => clock.now);
  loop.start(); await clock.advance(1500);
  assert.equal(calls.length, 8, 'Pausing between worker replies wins before the next command');
  assert.ok(calls.some(({ requested, completed }) => requested > completed));
  assert.ok(calls.slice(1).every(({ requested, completed }) => requested <= 8 && completed > 1));
  loop.stop();
}

// In-flight work finishes once, then explicit pause/queued edits win before the
// next command. Resuming rebases time instead of catching up a paused interval.
{
  const clock = new Clock(), host = clock.window('player'), target = {};
  let paused = false, busy = false, resolve, calls = 0;
  const loop = new SimulationLoop(() => host, () => paused ? null : { target, speed: 10, busy }, () => {
    calls++; return new Promise(done => { resolve = done; });
  }, (_target, error) => { throw error; }, () => clock.now);
  loop.start(); await clock.advance(100); assert.equal(calls, 1); assert.equal(clock.tasks.size, 0);
  loop.refresh(); await clock.advance(1000); assert.equal(calls, 1);
  paused = true; loop.reset(); resolve(); await settle(); await clock.advance(1000); assert.equal(calls, 1);
  paused = false; busy = true; loop.reset(); await clock.advance(100); assert.equal(calls, 1);
  busy = false; await clock.advance(100); assert.equal(calls, 2);
  loop.stop(); resolve(); await settle(); assert.equal(clock.tasks.size, 0, 'Stopped in-flight completion cannot restart work');
}

// Switching window ownership cancels stale callbacks. A suspended clock allows
// only one bounded batch, and closing/stopping leaves no timer in either host.
{
  const clock = new Clock(), opener = clock.window('opener'), popup = clock.window('popup', true), target = {};
  let host = popup, ticks = 0;
  const loop = new SimulationLoop(() => host, () => ({ target, speed: 10, busy: false }), async (_target, steps) => { ticks += steps; }, (_target, error) => { throw error; }, () => clock.now);
  loop.start(); await clock.advance(100);
  const stale = [...clock.tasks.values()][0].callback, before = ticks;
  host = opener; loop.refresh(); stale(); await settle(); assert.equal(ticks, before);
  assert.ok([...clock.tasks.values()].every(task => task.owner === opener));
  clock.now += 3600000; await clock.advance(0);
  assert.ok(ticks - before <= 8, 'No offline catch-up after a suspended timer');
  const stopped = [...clock.tasks.values()][0].callback;
  loop.stop(); stopped(); await settle(); assert.equal(clock.tasks.size, 0);
}

console.log('Simulation loop: 1×/2×/5×/10× without rAF, hidden >80 ticks/s, adaptive ≤8-tick batches, one in flight, pause/command fairness, no offline catch-up and window/stop cleanup passed.');
