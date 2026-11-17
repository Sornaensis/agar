import assert from 'node:assert/strict';
import { fresh, compile, seed, snapshot, advance, evaluation, save, restore, catalog } from './organelle-harness.mjs';

// The actual authored program and shipped Rust engine are the test subjects.
// Placement uses a portable checkpoint plus public seed calls; no engine state
// or signals are patched, and the program sees no object classification.
const { PRESETS } = await catalog();
const forager = PRESETS.find(item => item.name === 'Forager');
const api = await fresh({ seed: 42, ambientLight: 0.8, initialNutrients: 1 });
const genome = compile(api, forager.source);
const obstacleGenome = compile(api, 'thrust=0;turn=0;divide=false;feed=0;absorb=0;');
seed(api, genome, 1, 600, 380);
// The program first builds its own movement, vision and contact hardware using
// the universal newborn's reserves. There is no food or body injection here.
let developed = snapshot(api);
for (let tick = 0; tick < 600; tick += 5) {
  const cell = developed.cells[0];
  assert.ok(cell, 'The shared newborn survives its initial construction');
  if (['digestive', 'phagocyte', 'flagellum', 'optical', 'mechanoreceptor'].every(kind => cell.organs[kind].capacity >= 1)) break;
  developed = advance(api, 5);
}
assert.ok(['digestive', 'phagocyte', 'flagellum', 'optical', 'mechanoreceptor'].every(kind => developed.cells[0].organs[kind].capacity >= 1), 'The authored forager constructs all hardware used by the fixture');
assert.equal(developed.births, 0);
const initial = snapshot(api).cells[0];
const wrap = (value, extent) => ((value % extent) + extent) % extent;
const delta = (value, extent) => value - Math.round(value / extent) * extent;
const distance = (a, b) => Math.hypot(delta(a.x - b.x,1200),delta(a.y - b.y,760));

// Measure the deterministic public seeding offset, restore, then choose a
// center that puts a resistant cell just against the scavenger's front surface.
const beforeObstacle = save(api);
seed(api, obstacleGenome, 1, initial.x, initial.y);
const trialObstacle = snapshot(api).cells.find(cell => cell.genome === obstacleGenome);
const offset = { x:delta(trialObstacle.x-initial.x,1200), y:delta(trialObstacle.y-initial.y,760) };
assert.equal(restore(api,beforeObstacle),1);
const contactDistance = initial.radius + trialObstacle.radius - 0.1;
seed(api, obstacleGenome, 1,
  wrap(initial.x+Math.cos(initial.angle)*contactDistance-offset.x,1200),
  wrap(initial.y+Math.sin(initial.angle)*contactDistance-offset.y,760));
const first = snapshot(api);
const obstacleId = first.cells.find(cell => cell.genome === obstacleGenome).id;
const trace = evaluation(api,initial.id);
const signal = name => trace.signals.find(item=>item.name===name).value;
const output = name => trace.outputs.find(item=>item.name===name).value;
assert.equal(signal('contact.available'),true);
assert.equal(signal('contact.valid'),true,'The fixture begins with a real sensed contact');
assert.ok(signal('contact.resistance')>=0.12,'The surface resists capture');
assert.ok(signal('object.area')>=0.35,'The optical return is larger than the tentative particle heuristic');
assert.ok(output('thrust')>=0.5,'The authored forager actively leaves a resistant contact');
assert.ok(Math.abs(output('turn'))>=0.5,'It steers around the surface instead of remaining pointed into it');
assert.equal(first.food.length,0,'No organic particles can explain the initial departure');

const separated = advance(api,300);
const moving = separated.cells.find(cell=>cell.id===initial.id);
const obstacle = separated.cells.find(cell=>cell.id===obstacleId);
assert.ok(moving&&obstacle,'Both physical cells remain alive');
const clearance = distance(moving,obstacle)-moving.radius-obstacle.radius;
assert.ok(clearance>8,`The scavenger leaves the unconsumable surface (clearance ${clearance.toFixed(3)})`);
assert.equal(separated.predations,0); assert.equal(separated.attacks,0);

// A visible patch ahead can subsequently be approached and processed. Its
// particles expose only geometry/appearance and soft mechanical resistance.
assert.equal(api.add_food(wrap(moving.x+Math.cos(moving.angle)*20,1200),wrap(moving.y+Math.sin(moving.angle)*20,760),64),64);
let bestGut=0, usableDigestion=0, sawSmallObject=false, sawSoftContact=false;
for(let tick=0; tick<600; tick+=10){
  const current = advance(api,10), cell=current.cells.find(item=>item.id===initial.id);
  assert.ok(cell,'The tested scavenger survives the short collection fixture');
  bestGut=Math.max(bestGut,cell.gutEnergy); usableDigestion+=cell.digestionRate/3;
  const reading=evaluation(api,initial.id),values=Object.fromEntries(reading.signals.map(item=>[item.name,item.value]));
  sawSmallObject ||= values['object.valid']&&values['object.area']<0.35;
  sawSoftContact ||= values['contact.valid']&&values['contact.resistance']<0.12;
}
const final=snapshot(api);
assert.ok(sawSmallObject,'Built optics can observe a particle-sized return');
assert.ok(sawSoftContact,'Built mechanoreceptors detect soft particle contact');
assert.ok(bestGut>1&&usableDigestion>1,'The escaping scavenger subsequently captures and digests organic particles');
assert.ok(final.eaten>0,'Particles are actually consumed, not merely inspected');
assert.ok(Math.abs(final.material-final.materialIntroduced)<1e-7,'Navigation and feeding preserve the material ledger');
console.log(`Preset navigation passed: real resistant contact escaped by ${clearance.toFixed(1)} units, then particles collected (gut peak ${bestGut.toFixed(1)}, sampled digestion ${usableDigestion.toFixed(1)}).`);
