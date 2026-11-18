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
console.log('Camera: fit, cursor anchoring, pan, bounds, viewport resize, large worlds and invalid-input checks passed.');
