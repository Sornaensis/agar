import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { parseArgs } from 'node:util';

// Exercise the actual player, worker, autosave and canvas on a released build.
// Run against a local production preview, with Playwright available separately.
const { values } = parseArgs({ options: {
  checkpoint: { type: 'string' }, seconds: { type: 'string', default: '10' },
  speed: { type: 'string', default: '10' }, samples: { type: 'string', default: '3' },
  renderer: { type: 'string', default: 'auto' },
} });
const seconds = Number(values.seconds), speed = Number(values.speed), samples = Number(values.samples);
assert.ok(seconds >= 1 && [1, 2, 5, 10].includes(speed) && Number.isSafeInteger(samples) && samples > 0);
assert.ok(['auto', 'canvas2d', 'webgl2'].includes(values.renderer));
const base = process.env.UI_TEST_URL || 'http://127.0.0.1:4173/';
const checkpoint = values.checkpoint && await readFile(values.checkpoint);
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_PATH,
  args: ['--no-sandbox', '--disable-dev-shm-usage', '--js-flags=--no-liftoff'] });

function observe(renderer) {
  const meter = window.playerMeter = {
    ticks: 0, published: 0, lastFrame: -1, population: 0, particles: 0,
    gpuFrames: 0, canvasFrames: 0, gpuDraws: 0, device: null,
  };
  let lastGpuTick = -1, lastCanvasTick = -1;
  window.Worker = class extends Worker {
    pending = new Map();
    constructor(...args) {
      super(...args);
      let previousTick;
      this.addEventListener('message', ({ data }) => {
        const method = this.pending.get(data.id);
        this.pending.delete(data.id);
        const value = data.value;
        if (value?.frame) meter.lastFrame = value.frame[0];
        if (method === 'advance' && data.ok) {
          if (previousTick !== undefined) meter.ticks += value.tick - previousTick;
          if (value.frame) meter.published++;
          meter.population = value.summary.population;
          meter.particles = value.summary.food;
        }
        if (Number.isSafeInteger(value?.tick)) previousTick = value.tick;
      });
    }
    postMessage(message, ...args) {
      this.pending.set(message.id, message.method);
      super.postMessage(message, ...args);
    }
  };

  const seenContexts = new WeakSet();
  const getContext = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function(type, ...args) {
    if (type === 'webgl2' && renderer === 'canvas2d') return null;
    if (type === 'webgl2' && renderer === 'webgl2') args[0] = {...args[0], failIfMajorPerformanceCaveat:false};
    const ctx = getContext.call(this, type, ...args);
    if (type === 'webgl2' && ctx && !seenContexts.has(ctx)) {
      seenContexts.add(ctx);
      const debug = ctx.getExtension('WEBGL_debug_renderer_info');
      const rendererKey = debug?.UNMASKED_RENDERER_WEBGL;
      meter.device = ctx.getParameter(rendererKey ?? ctx.RENDERER);
      if (renderer === 'webgl2' && rendererKey !== undefined) {
        // Exercise direct drawing even on software-only benchmark hosts. Report the actual device above.
        const getParameter = ctx.getParameter.bind(ctx);
        ctx.getParameter = key => key === rendererKey ? 'Forced WebGL benchmark' : getParameter(key);
      }
    }
    return ctx;
  };
  const gl = window.WebGL2RenderingContext?.prototype;
  if (gl) {
    const drawTargets = new WeakMap();
    const bind = gl.bindFramebuffer;
    gl.bindFramebuffer = function(target, buffer) {
      if (target === this.FRAMEBUFFER || target === this.DRAW_FRAMEBUFFER) drawTargets.set(this, buffer);
      return bind.call(this, target, buffer);
    };
    for (const method of ['drawArrays', 'drawElements', 'drawArraysInstanced', 'drawElementsInstanced']) {
      const draw = gl[method];
      gl[method] = function(...args) {
        const result = draw.apply(this, args);
        // Multiple batches and camera redraws can represent the same simulation frame.
        const count = method === 'drawElements' ? args[1] : method === 'drawArrays' ? args[2] : args.at(-1);
        if (count > 0 && this.canvas.isConnected && this.canvas.classList.contains('gpu-world-entities') && !drawTargets.get(this)) {
          meter.gpuDraws++;
          if (meter.lastFrame !== lastGpuTick) { lastGpuTick = meter.lastFrame; meter.gpuFrames++; }
        }
        return result;
      };
    }
  }
  const fillRect = CanvasRenderingContext2D.prototype.fillRect;
  CanvasRenderingContext2D.prototype.fillRect = function(x, y, width, height) {
    if (x === 0 && y === 0 && this.canvas.id === 'world' && lastCanvasTick !== meter.lastFrame) {
      lastCanvasTick = meter.lastFrame; meter.canvasFrames++;
    }
    return fillRect.call(this, x, y, width, height);
  };
}

async function run() {
  const context = await browser.newContext({ viewport: { width: 1366, height: 900 }, deviceScaleFactor: 1 });
  try {
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(observe, values.renderer);
    await page.goto(base);
    await page.waitForFunction(() => document.querySelector('#save-state')?.textContent === 'Saved');
    if (checkpoint) {
      await page.route('**/__bench_state', route => route.fulfill({ body: checkpoint }));
      await page.evaluate(async speed => {
        const state = new Uint8Array(await (await fetch('./__bench_state')).arrayBuffer());
        const id = 'simulation:' + crypto.randomUUID(), now = Date.now();
        const metadata = { id, name: 'Performance sample', createdAt: now, updatedAt: now,
          environmentName: 'Springwater basin', seed: 7, speed, paused: true, genomeNames: {}, history: [],
          summary: { time: 0, population: 0, food: 0, births: 0, deaths: 0, generation: 0 } };
        await new Promise((resolve, reject) => {
          const open = indexedDB.open('cellarium');
          open.onerror = () => reject(open.error);
          open.onsuccess = () => {
            const db = open.result, tx = db.transaction(['simulations','simulationMetadata'], 'readwrite');
            tx.objectStore('simulations').put({ ...metadata, state });
            tx.objectStore('simulationMetadata').put(metadata);
            tx.oncomplete = () => { db.close(); resolve(); };
            tx.onerror = () => reject(tx.error);
          };
        });
        localStorage.setItem('cellarium.preferences.v1', JSON.stringify({ activeSimulationId: id }));
      }, speed);
      await page.reload();
      await page.getByRole('button', { name: 'Play simulation' }).click();
    } else {
      await page.locator('[data-action="start-template"]').first().click();
      await page.getByRole('button', { name: /Start simulation/ }).click();
      await page.locator('#playback-speed').selectOption(String(speed));
    }
    await page.waitForFunction(() => window.playerMeter.ticks >= 60, undefined, { timeout: 120000 });
    const backend = await page.locator('#world').evaluate(canvas => canvas.dataset.renderBackend || 'canvas2d');
    if (values.renderer !== 'auto') assert.equal(backend, values.renderer);
    if (backend === 'webgl2') await page.waitForFunction(() => window.playerMeter.gpuFrames > 0);
    const result = await page.evaluate(async ({ seconds, backend }) => {
      const meter = window.playerMeter;
      const frameCounter = backend === 'webgl2' ? 'gpuFrames' : 'canvasFrames';
      const ticks = meter.ticks, frames = meter[frameCounter], draws = meter.gpuDraws, published = meter.published;
      const population = meter.population, particles = meter.particles, start = performance.now();
      await new Promise(resolve => setTimeout(resolve, seconds * 1000));
      const elapsed = (performance.now() - start) / 1000;
      return { seconds: elapsed, backend, device: meter.device, startingPopulation: population, startingParticles: particles,
        ticksPerSecond: (meter.ticks - ticks) / elapsed, uniqueFramesPerSecond: (meter[frameCounter] - frames) / elapsed,
        gpuDrawsPerSecond: (meter.gpuDraws - draws) / elapsed,
        publishedPerSecond: (meter.published - published) / elapsed };
    }, { seconds, backend });
    assert.equal(await page.locator('#world').evaluate(canvas => canvas.dataset.renderBackend || 'canvas2d'), backend);
    assert.deepEqual(errors, []);
    assert.ok(result.ticksPerSecond > 0 && result.uniqueFramesPerSecond > 0);
    return result;
  } finally { await context.close(); }
}
try {
  const runs = [];
  for (let i = 0; i < samples; i++) runs.push(await run());
  console.log(JSON.stringify({ browser: browser.version(), wasmTier: 'optimizing', warmupTicks: 60,
    requestedRenderer: values.renderer, speed, viewport: [1366, 900], devicePixelRatio: 1, runs }, null, 2));
} finally { await browser.close(); }
