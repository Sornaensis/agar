import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import ts from 'typescript';

const outputFlag = process.argv.indexOf('--output');
const output = resolve(outputFlag >= 0 ? process.argv[outputFlag + 1] : 'experiments/presentation-performance.json');
const source = await readFile(new URL('../src/periodic-visibility.ts', import.meta.url), 'utf8');
const cameraSource = await readFile(new URL('../src/world-camera.ts', import.meta.url), 'utf8');
const load = async source => {
  const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  return import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
};
const { PeriodicVisibility } = await load(source);
const { WorldCamera } = await load(cameraSource);

// Exact delivered baseline renderer helper, before this optimization. Keeping
// the divisions, nested arrays and filter reproduces the old per-entity work.
function previousWrappedPositions(x, y, margin) {
  const xs = [x];
  const ys = [y];
  if (x < margin) xs.push(x + this.worldWidth);
  if (x > this.worldWidth - margin) xs.push(x - this.worldWidth);
  if (y < margin) ys.push(y + this.worldHeight);
  if (y > this.worldHeight - margin) ys.push(y - this.worldHeight);
  const left = -this.offsetX / this.scale - margin, right = (this.cssWidth - this.offsetX) / this.scale + margin;
  const top = -this.offsetY / this.scale - margin, bottom = (this.cssHeight - this.offsetY) / this.scale + margin;
  return xs.flatMap(px => ys.map(py => ({ x: px, y: py }))).filter(point => point.x >= left && point.x <= right && point.y >= top && point.y <= bottom);
}
const camera = new WorldCamera(), visitor = new PeriodicVisibility();
let seed = 42;
const random = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 2 ** 32; };
function view(width, height, zoom, corner = false) {
  camera.setWorld(width, height); camera.setViewport(1000, 760); camera.fit(); camera.zoomAt(zoom);
  if (corner) camera.pan(1e8, 1e8);
  Object.assign(visitor, { width, height, left: -camera.offsetX / camera.scale, right: (1000 - camera.offsetX) / camera.scale,
    top: -camera.offsetY / camera.scale, bottom: (760 - camera.offsetY) / camera.scale });
  return { worldWidth: width, worldHeight: height, cssWidth: 1000, cssHeight: 760, offsetX: camera.offsetX, offsetY: camera.offsetY, scale: camera.scale };
}

let exactCases = 0;
for (const [width, height] of [[400, 400], [1200, 760], [4800, 3040]]) for (const zoom of [.25, 1, 24]) for (const corner of [false, true]) {
  const baseline = view(width, height, zoom, corner);
  const check = (x, y, margin) => {
    const expected = previousWrappedPositions.call(baseline, x, y, margin).flatMap(point => [point.x, point.y]);
    const count = visitor.collect(x, y, margin);
    assert.deepEqual([...visitor.points.subarray(0, count)], expected); exactCases++;
  };
  for (let i = 0; i < 300; i++) check(random() * width, random() * height, random() * width * 1.2);
  for (const x of [-0, 0, width / 2, width]) for (const y of [-0, 0, height / 2, height]) for (const margin of [0, 5, width / 2, 6000]) check(x, y, margin);
}

seed = 42;
const cells = 10000, particles = 100000;
const entities = new Float64Array((cells + particles) * 3);
for (let index = 0; index < cells + particles; index++) {
  entities[index * 3] = random() * 4800; entities[index * 3 + 1] = random() * 3040; entities[index * 3 + 2] = index < cells ? 24 : 5;
}
const rows = [], median = values => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
for (const zoom of [1, 24]) {
  const baseline = view(4800, 3040, zoom);
  const previous = () => {
    let images = 0, checksum = 0;
    for (let index = 0; index < entities.length; index += 3) {
      const points = previousWrappedPositions.call(baseline, entities[index], entities[index + 1], entities[index + 2]);
      images += points.length;
      for (const point of points) checksum += point.x * .25 + point.y * .5;
    }
    return { images, checksum };
  };
  const optimized = () => {
    let images = 0, checksum = 0;
    for (let index = 0; index < entities.length; index += 3) {
      const count = visitor.collect(entities[index], entities[index + 1], entities[index + 2]);
      images += count / 2;
      for (let point = 0; point < count; point += 2) checksum += visitor.points[point] * .25 + visitor.points[point + 1] * .5;
    }
    return { images, checksum };
  };
  const expected = previous(); assert.deepEqual(optimized(), expected);
  for (let i = 0; i < 3; i++) { previous(); optimized(); }
  const before = [], after = [];
  const sample = (run, target) => {
    const start = performance.now();
    for (let i = 0; i < 3; i++) assert.deepEqual(run(), expected);
    target.push((performance.now() - start) / 3);
  };
  for (let i = 0; i < 5; i++) {
    if (i % 2) { sample(optimized, after); sample(previous, before); }
    else { sample(previous, before); sample(optimized, after); }
  }
  rows.push({ zoom, entities: cells + particles, ...expected, previousMedianMs: median(before), optimizedMedianMs: median(after), previousSamplesMs: before, optimizedSamplesMs: after });
}
const report = {
  measuredAt: new Date().toISOString(), node: process.version, execArgv: process.execArgv,
  productionSource: 'src/periodic-visibility.ts', productionSha256: createHash('sha256').update(source).digest('hex'),
  baseline: 'Previous renderer wrappedPositions implementation, preserved verbatim in this harness',
  baselineSha256: createHash('sha256').update(previousWrappedPositions.toString()).digest('hex'),
  exactCases, world: { width: 4800, height: 3040 }, cells, particles, rows,
  limits: 'Node/V8 helper traversal only; no Canvas drawing, browser layout, GPU or whole-application FPS is measured. Both paths assert identical ordered periodic images and full-traversal counts/checksums.',
};
await mkdir(dirname(output), { recursive: true }); await writeFile(output, JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify({ output, exactCases, rows }, null, 2));
