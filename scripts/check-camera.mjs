import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

// Transpile the pure camera module without needing a browser or generated build.
const source = await readFile(new URL('../src/world-camera.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { WorldCamera } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);
const near = (actual, expected, message) => assert.ok(Math.abs(actual - expected) < 1e-8, `${message}: ${actual} != ${expected}`);

const camera = new WorldCamera();
camera.setViewport(1000, 760);
near(camera.point(500, 380).x, 600, 'Initial world center x');
near(camera.point(500, 380).y, 380, 'Initial world center y');
near(camera.point(camera.offsetX, camera.offsetY).x, 0, 'Letterbox origin x');
near(camera.point(camera.offsetX, camera.offsetY).y, 0, 'Letterbox origin y');

camera.zoomAt(3, 650, 400);
const anchored = camera.point(650, 400);
camera.zoomAt(1.2, 650, 400);
near(camera.point(650, 400).x, anchored.x, 'Cursor-anchored zoom x');
near(camera.point(650, 400).y, anchored.y, 'Cursor-anchored zoom y');
const before = { x: camera.x, y: camera.y, scale: camera.scale };
camera.pan(20, -10);
near(camera.x, before.x - 20 / before.scale, 'Pan world distance x');
near(camera.y, before.y + 10 / before.scale, 'Pan world distance y');

camera.setWorld(4800, 3040);
near(camera.zoom, 1, 'Resized world fits');
near(camera.x, 2400, 'Large-world center x');
near(camera.y, 1520, 'Large-world center y');
camera.zoomAt(2);
camera.setViewport(700, 500);
near(camera.zoom, 2, 'Viewport resize preserves zoom');
near(camera.x, 2400, 'Viewport resize preserves center x');
near(camera.y, 1520, 'Viewport resize preserves center y');

camera.pan(1e10, 1e10);
near(camera.point(0, 0).x, 0, 'Pan clamps at left edge');
near(camera.point(0, 0).y, 0, 'Pan clamps at top edge');
camera.pan(-1e10, -1e10);
near(camera.point(700, 500).x, 4800, 'Pan clamps at right edge');
near(camera.point(700, 500).y, 3040, 'Pan clamps at bottom edge');
camera.zoomAt(1e20);
near(camera.zoom, 24, 'Zoom upper bound');
camera.zoomAt(1e-20);
near(camera.zoom, .25, 'Zoom lower bound');
near(camera.x, 2400, 'Zooming out centers a completely visible world');
for (const value of [NaN, Infinity, -1, 0]) camera.zoomAt(value);
near(camera.zoom, .25, 'Invalid zoom does not corrupt camera');
camera.fit();
near(camera.zoom, 1, 'Fit restores zoom');
near(camera.point(350, 250).x, 2400, 'Fit restores center');
const visibilitySource = await readFile(new URL('../src/periodic-visibility.ts', import.meta.url), 'utf8');
const visibilityCode = ts.transpileModule(visibilitySource, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { PeriodicVisibility } = await import(`data:text/javascript;base64,${Buffer.from(visibilityCode).toString('base64')}`);
const visibility = new PeriodicVisibility(), buffer = visibility.points;
// Exact oracle is the former renderer helper, including image/painter order.
function previous(x, y, margin) {
  const xs = [x], ys = [y];
  if (x < margin) xs.push(x + visibility.width);
  if (x > visibility.width - margin) xs.push(x - visibility.width);
  if (y < margin) ys.push(y + visibility.height);
  if (y > visibility.height - margin) ys.push(y - visibility.height);
  return xs.flatMap(px => ys.map(py => ({ x: px, y: py })))
    .filter(point => point.x >= visibility.left - margin && point.x <= visibility.right + margin && point.y >= visibility.top - margin && point.y <= visibility.bottom + margin)
    .flatMap(point => [point.x, point.y]);
}
let seed = 42, cases = 0;
const random = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 2 ** 32; };
for (const [width, height] of [[400, 400], [1200, 760], [4800, 3040]]) {
  visibility.width = width; visibility.height = height;
  for (const zoom of [.25, 1, 24]) {
    camera.setWorld(width, height); camera.setViewport(1000, 760); camera.fit(); camera.zoomAt(zoom);
    visibility.left = -camera.offsetX / camera.scale; visibility.right = (1000 - camera.offsetX) / camera.scale;
    visibility.top = -camera.offsetY / camera.scale; visibility.bottom = (760 - camera.offsetY) / camera.scale;
    const check = (x, y, margin) => {
      const count = visibility.collect(x, y, margin);
      assert.deepEqual([...visibility.points.subarray(0, count)], previous(x, y, margin));
      assert.equal(visibility.points, buffer, 'Every entity uses the same fixed storage'); cases++;
    };
    for (let i = 0; i < 300; i++) check(random() * width, random() * height, random() * width * 1.2);
    for (const x of [-0, 0, width / 2, width]) for (const y of [-0, 0, height / 2, height]) for (const margin of [0, 5, width / 2, 6000]) check(x, y, margin);
  }
}
console.log(`Camera: fit, anchoring, pan, resize and bounds; ${cases} allocation-free torus image cases match exact ordered seam/viewport behavior.`);
