import assert from 'node:assert/strict';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({
  headless: true, executablePath: process.env.CHROMIUM_PATH,
  args: ['--no-sandbox', '--disable-dev-shm-usage']
});
const url = process.env.UI_TEST_URL || 'http://127.0.0.1:4173/';

function observe(mode) {
  const probe = window.probe = { frame: null, draws: 0, copies: 0, readbacks: 0, device: null };
  const contexts = new WeakSet(), getContext = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function(type, ...args) {
    if (type === 'webgl2' && mode === 'canvas2d') return null;
    if (type === 'webgl2' && mode === 'webgl2') args[0] = { ...args[0], failIfMajorPerformanceCaveat: false };
    const context = getContext.call(this, type, ...args);
    if (context && type === 'webgl2' && !contexts.has(this)) {
      contexts.add(this);
      const debug = context.getExtension('WEBGL_debug_renderer_info');
      const rendererKey = debug?.UNMASKED_RENDERER_WEBGL;
      probe.device = context.getParameter(rendererKey ?? context.RENDERER);
      if (mode === 'webgl2' && rendererKey !== undefined) {
        const getParameter = context.getParameter.bind(context);
        context.getParameter = key => key === rendererKey ? 'Forced WebGL test' : getParameter(key);
      }
    }
    return context;
  };
  const drawImage = CanvasRenderingContext2D.prototype.drawImage;
  CanvasRenderingContext2D.prototype.drawImage = function(source, ...args) {
    if (contexts.has(source)) probe.copies++;
    return drawImage.call(this, source, ...args);
  };
  const gl = window.WebGL2RenderingContext?.prototype;
  if (gl) {
    for (const name of ['drawArrays', 'drawElements', 'drawArraysInstanced', 'drawElementsInstanced', 'readPixels']) {
      const original = gl[name];
      gl[name] = function(...args) {
        if (this.canvas.isConnected && this.canvas.classList.contains('gpu-world-entities')) {
          if (name === 'readPixels') probe.readbacks++;
          else {
            const count = name === 'drawArrays' ? args[2] : name === 'drawElements' ? args[1] : args.at(-1);
            if (count > 0 && !this.getParameter(this.DRAW_FRAMEBUFFER_BINDING)) probe.draws++;
          }
        }
        return original.apply(this, args);
      };
    }
  }
  window.Worker = class extends Worker {
    constructor(...args) {
      super(...args);
      this.addEventListener('message', ({ data }) => { if (data.value?.frame) probe.frame = data.value.frame; });
    }
  };
}

// Use the worker's real compact frame to locate entities independently of either renderer.
function visibleEntities(waitForSample = false) {
  const canvas = document.querySelector('#world'), rect = canvas.getBoundingClientRect();
  const frame = window.probe.frame;
  if (!frame) return null;
  const width = frame[7], height = frame[8];
  const camera = window.probe.camera ?? { x: width / 2, y: height / 2, zoom: 1 };
  const scale = Math.min(rect.width / width, rect.height / height) * camera.zoom;
  function point(x, y) {
    return {
      x: Math.round(rect.x + rect.width / 2 + (x - camera.x) * scale),
      y: Math.round(rect.y + rect.height / 2 + (y - camera.y) * scale),
    };
  }
  const visible = ({ x, y }) => x > 5 && y > 5 && x < innerWidth - 5 && y < innerHeight - 5 && document.elementFromPoint(x, y) === canvas;
  const cells = [], food = [];
  for (let i = 0, n = 12; i < frame[2]; i++, n += 21) {
    const p = { ...point(frame[n + 2], frame[n + 3]), id: frame[n], radius: frame[n + 7] * scale };
    if (visible(p)) cells.push(p);
  }
  for (let i = 0, n = 12 + 21 * frame[2]; i < frame[3]; i++, n += 6) {
    const p = point(frame[n], frame[n + 1]);
    // A particle under an organism cannot establish that particles themselves were rendered.
    if (visible(p) && cells.every(c => Math.hypot(c.x - p.x, c.y - p.y) > c.radius * 2 + 4)) food.push(p);
  }
  if (waitForSample && (cells.length < 15 || food.length < 12)) return null;
  return { cells, food };
}

async function checkPixels(page, reference, other, entities) {
  const observed = await page.evaluate(async ({ images, entities }) => {
    const [before, after] = await Promise.all(images.map(async base64 => {
      const image = await createImageBitmap(await (await fetch('data:image/png;base64,' + base64)).blob());
      const canvas = new OffscreenCanvas(image.width, image.height), ctx = canvas.getContext('2d');
      ctx.drawImage(image, 0, 0); image.close();
      return ctx.getImageData(0, 0, canvas.width, canvas.height);
    }));
    function changed({ x, y }) {
      for (let dy = -2; dy <= 2; dy++) {
        for (let dx = -2; dx <= 2; dx++) {
          const first = 4 * ((y + dy) * before.width + x + dx);
          for (let i = first; i < first + 3; i++) {
            if (Math.abs(before.data[i] - after.data[i]) >= 12) return true;
          }
        }
      }
      return false;
    }
    return Object.fromEntries(Object.entries(entities).map(([key, points]) => [key, points.filter(changed).length / points.length]));
  }, { images: [reference, other].map(image => image.toString('base64')), entities });
  for (const [kind, ratio] of Object.entries(observed)) {
    assert.ok(ratio > .78, kind + ' pixels present at ' + (ratio * 100).toFixed(1) + '% of sampled positions');
  }
}

async function run(mode) {
  const context = await browser.newContext({ viewport: { width: 1200, height: 900 }, deviceScaleFactor: 2 });
  const page = await context.newPage(), errors = [];
  page.on('pageerror', error => errors.push(error.message));
  try {
    await page.addInitScript(observe, mode);
    await page.goto(url);
    await page.locator('[data-action="start-template"]').first().click();
    await page.getByRole('button', { name: /Start simulation/ }).click();
    const world = page.locator('#world'), gpu = page.locator('canvas.gpu-world-entities');
    await world.waitFor();
    const backend = await world.getAttribute('data-render-backend');
    if (mode === 'auto') {
      assert.ok(['webgl2', 'canvas2d'].includes(backend));
      const device = await page.evaluate(() => window.probe.device);
      if (!device || /swiftshader|llvmpipe|softpipe|basic render driver|software rasterizer/i.test(device)) {
        assert.equal(backend, 'canvas2d');
      }
      if (backend === 'webgl2') await page.waitForFunction(() => window.probe.draws > 0);
      assert.deepEqual(errors, []);
      console.log('Automatic renderer selected ' + backend + '.');
      return;
    }
    assert.equal(backend, mode);
    await page.locator('#playback-speed').selectOption('10');
    await world.evaluate(c => c.addEventListener('camerachange', e => { window.probe.camera = e.detail; }));
    await page.setViewportSize({ width: 1350, height: 910 });
    await page.locator('[data-camera="player:in"]').click();
    await page.locator('#player-overlay').selectOption('oxygen');
    // Wait for separated food in this camera view. Newly secreted particles can still overlap their producers.
    await page.waitForFunction(visibleEntities, true, { timeout: 90000, polling: 250 });
    await page.getByRole('button', { name: 'Pause simulation' }).click();
    await page.waitForFunction(() => document.getElementById('save-state')?.textContent === 'Saved');
    const entities = await page.evaluate(visibleEntities, false);
    assert.ok(entities.cells.length >= 10 && entities.food.length >= 5, 'Expected visible isolated entities for visual checks');
    if (mode === 'webgl2') {
      const surface = await gpu.evaluate(c => {
        const world = document.querySelector('#world'), rect = c.getBoundingClientRect(), r = world.getBoundingClientRect();
        return {
          fits: c.width === world.width && c.height === world.height
            && ['x', 'y', 'width', 'height'].every(key => Math.abs(rect[key] - r[key]) < 1),
          pointerEvents: getComputedStyle(c).pointerEvents, error: c.getContext('webgl2').getError()
        };
      });
      assert.deepEqual(surface, { fits: true, pointerEvents: 'none', error: 0 });
      const snap = () => page.screenshot({ scale: 'css', animations: 'disabled' });
      const direct = await snap();
      await gpu.evaluate(c => { c.style.visibility = 'hidden'; });
      const background = await snap();
      await gpu.evaluate(c => {
        c.style.visibility = '';
        window.probe.loss = c.getContext('webgl2').getExtension('WEBGL_lose_context');
        window.probe.loss.loseContext();
      });
      await page.waitForFunction(() => document.querySelector('#world').dataset.renderBackend === 'canvas2d');
      const fallback = await snap();
      await checkPixels(page, background, direct, entities);
      await checkPixels(page, background, fallback, entities);
      const previous = await page.evaluate(() => window.probe.draws);
      await page.evaluate(() => window.probe.loss.restoreContext());
      await page.waitForFunction(before => document.querySelector('#world').dataset.renderBackend === 'webgl2' && window.probe.draws > before, previous);
      await checkPixels(page, background, await snap(), entities);
    }
    const target = entities.cells[0];
    await page.mouse.click(target.x, target.y);
    await page.locator('#cell-inspector .organism-title .muted').filter({ hasText: 'Organism #' + target.id + ' ·' }).waitFor();
    const counters = await page.evaluate(() => ({ draws: window.probe.draws, copies: window.probe.copies, readbacks: window.probe.readbacks }));
    assert.equal(counters.draws > 0, mode === 'webgl2');
    assert.equal(counters.copies, 0); assert.equal(counters.readbacks, 0);
    assert.deepEqual(errors, []);
    console.log(mode === 'canvas2d' ? 'Canvas 2D startup fallback and picking passed.' : 'Visible GPU pixels, resize, zoom, picking, layers and context recovery passed.');
  } finally { await context.close(); }
}
try {
  await run('webgl2');
  await run('canvas2d');
  await run('auto');
} finally {
  await browser.close();
}
