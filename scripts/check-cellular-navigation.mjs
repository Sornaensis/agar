import {placeByTissue} from './feeding-culture.mjs';
import assert from 'node:assert/strict';
import { fresh, compile, seed, snapshot, advance, body, evaluation, save, restore, catalog } from './organelle-harness.mjs';

// Exercise the current, unchanged authored genomes through paid construction and
// actual receptor inputs. Checkpoint restore only replays the seed RNG to place
// an introduced target; it never injects machinery, energy, or sensor values.
const { PRESETS } = await catalog();
const wrap = (x, extent) => ((x % extent) + extent) % extent;
const delta = (x, extent) => x - Math.round(x / extent) * extent;
const conserved = state => {
  assert.ok(Math.abs(state.material - state.materialIntroduced) < 1e-7, 'Material remains accounted for');
  assert.ok(state.energy <= state.energyIntroduced + state.photosynthesis + 1e-7, 'Food transfers do not create energy');
};
const values = (api, id) => {
  const result = evaluation(api, id);
  return Object.fromEntries([...result.bindings, ...result.signals].map(entry => [entry.name, entry.value]));
};
async function developed(name, required) {
  // Navigation/meal fixtures need oxygen as well as the supplied organic food.
  const api = await fresh({ seed: 42, ambientLight: 0.8, initialNutrients: 0.3,
    gases: { mode:'open', exchangeRate:.5, oxygenEquilibrium:1, carbonEquilibrium:1 } });
  const genome = compile(api, PRESETS.find(preset => preset.name === name).source);
  seed(api, genome, 1, 600, 380);
  let state = snapshot(api);
  // The polar hunter funds its first ATP compartment alongside the front hunting gear.
  const startupWindow = name === 'Predator' ? 25 : 20;
  while (state.seconds < startupWindow && !required.every(name => body(api, 1).cellular.assemblies.some(machine => machine.name === name && machine.complete))) {
    state = advance(api, 5);
    assert.ok(state.cells.some(cell => cell.id === 1), `${name} survives its paid startup`);
  }
  assert.ok(required.every(name => body(api, 1).cellular.assemblies.some(machine => machine.name === name && machine.complete)), `${name} equips its feeding and navigation machinery before needing its first meal`);
  assert.equal(state.cells[0].area, 1, 'Hunting does not require prior growth');
  assert.ok(state.cells[0].energy > 30, 'Startup retains a maintenance and search reserve');
  assert.equal(state.eaten, 0);
  assert.equal(state.food.length, 0);
  assert.equal(state.photosynthesis, 0);
  assert.equal(state.births, 0);
  conserved(state);
  return { api, genome, state };
}
function placeAhead(api, targetGenome, cell, separation) {
  const known = new Set(snapshot(api).cells.map(other => other.id));
  const checkpoint = save(api);
  seed(api, targetGenome, 1, cell.x, cell.y);
  const trial = snapshot(api).cells.find(other => !known.has(other.id));
  const jitter = { x: delta(trial.x - cell.x, 1200), y: delta(trial.y - cell.y, 760) };
  assert.equal(restore(api, checkpoint), 1);
  seed(api, targetGenome, 1,
    wrap(cell.x + Math.cos(cell.angle) * separation - jitter.x, 1200),
    wrap(cell.y + Math.sin(cell.angle) * separation - jitter.y, 760));
  return snapshot(api).cells.find(other => !known.has(other.id));
}

const forager = await developed('Forager', ['Gut', 'ExudateEnzyme', 'Capture', 'LeftMotor', 'RightMotor', 'Vision', 'Touch']);
const obstacleGenome = compile(forager.api, 'thrust=0;turn=0;divide=false;feed=0;absorb=0;');
const start = forager.state.cells[0];
const obstacle = placeAhead(forager.api, obstacleGenome, start, start.radius + 7 - 0.1);
const contact = values(forager.api, 1);
assert.equal(contact['sensed.Touch.valid'], true);
assert.ok(contact['sensed.Touch.resistance'] >= 0.12);
assert.equal(contact.blocked, true);
assert.ok(contact.move >= 0.5 && Math.abs(contact.steer) >= 0.5, 'A resistant front contact causes active escape');
let followed = snapshot(forager.api), frontCleared = false, surfaceTravel = 0;
let previousBearing = Math.atan2(delta(start.y - obstacle.y, 760), delta(start.x - obstacle.x, 1200));
for (let tick = 0; tick < 300; tick += 5) {
  followed = advance(forager.api, 5);
  const cell = followed.cells.find(cell => cell.id === 1), target = followed.cells.find(cell => cell.id === obstacle.id);
  assert.ok(cell && target);
  const bearing = Math.atan2(delta(cell.y - target.y, 760), delta(cell.x - target.x, 1200));
  surfaceTravel += Math.atan2(Math.sin(bearing - previousBearing), Math.cos(bearing - previousBearing));
  previousBearing = bearing;
  frontCleared ||= !values(forager.api, 1)['sensed.Touch.valid'];
}
assert.ok(frontCleared && Math.abs(surfaceTravel) > 1, 'The forager clears its front contact and travels around the surface instead of pushing into it');
const moving = followed.cells.find(cell => cell.id === 1);
assert.equal(followed.attacks, 0);
assert.equal(followed.predations, 0);
placeByTissue(forager.api,moving,5);
let bestGut = 0, digestion = 0, sawParticle = false;
for (let tick = 0; tick < 600; tick += 10) {
  const state = advance(forager.api, 10), cell = state.cells.find(cell => cell.id === 1);
  assert.ok(cell);
  const reading = values(forager.api, 1);
  sawParticle ||= reading['sensed.Vision.valid'] && reading['sensed.Vision.area'] < 0.35;
  bestGut = Math.max(bestGut, cell.gutEnergy);
  digestion += cell.digestionRate / 3;
  conserved(state);
}
assert.ok(sawParticle && bestGut > 1 && digestion > 1, 'After following the surface, the unchanged genome captures and digests real particles');

const predator = await developed('Predator', ['Gut', 'Capture', 'TissueEnzyme', 'LeftMotor', 'RightMotor', 'Vision', 'Touch', 'Piercer']);
const preyGenome = compile(predator.api, 'thrust=0;turn=0;divide=false;feed=0;absorb=0;');
const hunter = predator.state.cells[0];
// Neither an unequipped Predator nor a register-only mimic has a display patch.
const markedGenome = compile(predator.api, 'cellular; publish rf099 = 17;');
const encounterStart = save(predator.api);
for (const visitor of [predator.genome, markedGenome]) {
  placeAhead(predator.api, visitor, hunter, hunter.radius + 7 - 0.1);
  const read = values(predator.api, hunter.id);
  assert.equal(read['sensed.Touch.valid'], true);
  assert.equal(read['sensed.Touch.message_valid'], false);
  assert.equal(read.peer, false);
  assert.equal(restore(predator.api, encounterStart), 1);
}
const prey = placeAhead(predator.api, preyGenome, hunter, hunter.radius + 7 - 0.1);
assert.equal(values(predator.api, 1)['sensed.Touch.valid'], true);
let tissuePeak = 0, usefulTissue = 0, killed = false;
for (let tick = 0; tick < 1200; tick += 5) {
  const state = advance(predator.api, 5), cell = state.cells.find(cell => cell.id === 1);
  assert.ok(cell, 'The hunting founder survives the encounter');
  // This closed encounter begins with no food. Both soluble reserves and structural
  // tissue in the resulting meal came from the same contacted prey.
  tissuePeak = Math.max(tissuePeak, cell.gutEnergy);
  if (cell.gutEnergy > 0) usefulTissue += cell.digestionRate / 6;
  killed ||= !state.cells.some(cell => cell.id === prey.id);
  conserved(state);
  if (killed && tissuePeak > 1 && usefulTissue > 1) break;
}
const hunted = snapshot(predator.api);
assert.ok(hunted.attacks > 0 && killed, 'The newborn-built piercer kills a real contacted prey cell');
assert.ok(tissuePeak > 1 && usefulTissue > 1, 'The separate capture/storage/digestion path consumes attack-derived tissue');
assert.equal(hunted.photosynthesis, 0);

// Contact receptors reach farther than particle capture. A soft particle at
// that boundary must cause a continued approach rather than an attack or stop.
{
  const fixture = await developed('Predator', ['Gut', 'ExudateEnzyme', 'Capture', 'TissueEnzyme', 'LeftMotor', 'RightMotor', 'Vision', 'Touch', 'Piercer']);
  const api = fixture.api, ready = advance(api, 1), cell = ready.cells[0];
  const particle = placeByTissue(api, cell, 3.35);
  Object.assign(cell,snapshot(api).cells.find(c=>c.id===cell.id));
  const separation = Math.hypot(delta(particle.x - cell.x, 1200), delta(particle.y - cell.y, 760));
  assert.ok(separation > cell.radius + 2.5, 'The introduced particle initially lies outside capture range');
  const sensed = values(api, 1);
  assert.equal(sensed['sensed.Touch.valid'], true);
  assert.ok(sensed['sensed.Touch.resistance'] < 0.1);
  assert.equal(sensed.firm_contact, false);
  assert.ok(sensed.move > 0, 'A soft contact does not stop forward propulsion');
  let captured = null, digested = 0;
  for (let tick = 0; tick < 5 * 30; tick++) {
    const state = advance(api, 1);
    assert.equal(state.population, 1);
    if (state.eaten > 0 && captured === null) captured = state.seconds - ready.seconds;
    digested += state.cells[0].digestionRate / 30;
    assert.equal(state.attacks, 0);
    conserved(state);
    if (captured !== null && digested > 0.3) break;
  }
  assert.ok(captured !== null && digested > 0.3, 'The paid hunter approaches, captures and digests the sensed soft particle');
  console.log(`Predator soft-contact approach passed: a particle beyond capture range is captured after ${captured.toFixed(2)} seconds without an attack.`);
}

// A late carcass remains usable while the scavenger retains ATP and enough
// organic carbon to build its missing enzyme. ATP alone cannot pay that mass.
// Wait for the measured low-ATP state, then place the cell beside a physically
// lysed donor. The front intake must capture that real, mixed-composition meal.
const late = await developed('Forager', ['Gut', 'ExudateEnzyme', 'Capture', 'LeftMotor', 'RightMotor', 'Vision', 'Touch']);
let route = snapshot(late.api);
while (route.cells[0].energy > 22) route = advance(late.api, 1);
const arrival = route.cells[0], arrivalTick = route.ticks;
assert.ok(arrival.organicCarbon > 0.08, 'The encounter retains a real biosynthetic carbon reserve');
placeByTissue(late.api,arrival,2.4);
let lowMeal = false, lateDigestion = 0, lateComplete = false;
for (let tick = arrivalTick; tick < arrivalTick + 600; tick++) {
  const state = advance(late.api, 1), cell = state.cells.find(cell => cell.id === 1);
  assert.ok(cell, 'The carbon-solvent scavenger processes its captured meal before starving');
  const enzyme = body(late.api, 1).cellular.assemblies.find(machine => machine.name === 'TissueEnzyme');
  lowMeal ||= cell.energy < 30 && cell.organicCarbon > 0.08 && cell.gutEnergyByKind[1] > 0 && !enzyme?.complete;
  if (lowMeal && cell.gutEnergyByKind[1] > 0) lateDigestion += cell.digestionRate / 30;
  lateComplete ||= enzyme?.complete ?? false;
  conserved(state);
  if (lowMeal && lateComplete && lateDigestion > 1) break;
}
assert.ok(lowMeal, 'The fixture reaches a captured tissue meal below thirty ATP, with carbon but without its enzyme');
assert.ok(lateComplete && lateDigestion > 1, 'Paid enzyme construction remains possible from actual available energy, independent of storage capacity');
console.log(`Current cellular navigation passed: Forager travels ${Math.abs(surfaceTravel).toFixed(1)} radians around a resistant surface and captures particles (gut ${bestGut.toFixed(1)}, digestion ${digestion.toFixed(1)}); Predator equips before growth, attacks ${hunted.attacks} times and digests killed prey (tissue ${tissuePeak.toFixed(1)}, digestion ${usefulTissue.toFixed(1)}).`);
console.log(`Late tissue recovery passed: the current Forager pays carbon and ATP for its missing enzyme and assimilates ${lateDigestion.toFixed(2)} chemical-potential units from a naturally produced carcass.`);

// Modest real damage must trigger paid maintenance, not a second complete body
// plan. Heating the live water exercises damage, expression, installation and
// repair without injecting damage flags or modifying checkpoint contents.
const maintenance = [];
for (const name of ['Forager', 'Predator']) {
  const required = ['Gut', 'ExudateEnzyme', 'Capture', 'LeftMotor', 'RightMotor', 'Vision', 'Touch',
    ...(name === 'Predator' ? ['TissueEnzyme', 'Piercer'] : [])];
  const fixture = await developed(name, required), api = fixture.api;
  advance(api, Math.max(0, (name === 'Predator' ? 16 : 12) * 30 - fixture.state.ticks));
  const initial = snapshot(api), originals = body(api, 1).cellular.assemblies.filter(machine => required.includes(machine.name));
  assert.equal(originals.length, required.length);
  assert.ok(originals.every(machine => machine.complete && machine.integrity === 1));
  assert.ok(!body(api, 1).cellular.assemblies.some(machine => machine.name === 'Repair'), 'Maintenance does not consume the healthy startup reserve');

  api.set_environment(1, 0.8, 0);
  const stressed = advance(api, 30);
  api.set_environment(0.5, 0.8, 0);
  assert.ok(stressed.cells[0].integrity < 0.96 && stressed.cells[0].integrity > 0.94, 'One hot second damages the actual organism');
  assert.equal(values(api, 1).powered, true, 'Modest wear does not revoke a working feeding pathway');
  assert.equal(values(api, 1).equipped, true, 'Modest wear does not revoke working navigation and hunting');
  let repairedMass = 0, state = stressed;
  for (let tick = 0; tick < 25 * 30; tick++) {
    state = advance(api, 1);
    assert.equal(state.population, 1, 'The consumer pays for maintenance from its finite starting energy');
    const view = body(api, 1);
    repairedMass += view.repairRate / 30;
    for (const original of originals) {
      const copies = view.cellular.assemblies.filter(machine => machine.name === original.name);
      assert.equal(copies.length, 1, 'Repairable wear does not initiate redundant whole mechanisms');
      assert.equal(copies[0].id, original.id, 'Repair preserves the actual original machine');
      assert.equal(copies[0].mass, original.mass, 'Repair does not create installed material');
    }
    conserved(state);
  }
  const repaired = body(api, 1).cellular.assemblies;
  assert.ok(originals.every(original => repaired.find(machine => machine.id === original.id).integrity > 0.999), 'The original machinery regains its working integrity');
  assert.ok(repaired.some(machine => machine.name === 'Repair' && machine.complete), 'The authored repair mechanism is physically synthesized and installed');
  assert.ok(state.cells[0].integrity > 0.999 && repairedMass > 0.005);
  assert.ok(state.energy < initial.energy - 2, 'Construction and repair consume accounted energy even when respiration replenishes ATP');
  assert.equal(state.materialIntroduced, initial.materialIntroduced);
  assert.equal(state.energyIntroduced, initial.energyIntroduced);
  assert.equal(state.photosynthesis, 0);
  assert.equal(state.eaten, 0);
  assert.equal(state.births, 0);
  maintenance.push({ name, repairedMass, remainingEnergy: state.cells[0].energy });
}
console.log(`Paid maintenance passed after real thermal stress: ${JSON.stringify(maintenance)}.`);

// Damage can also arrive before navigation is complete. Crossing the working
// capacity threshold must not switch off expression of its own repair pathway.
{
  const api = await fresh({ seed: 42, ambientLight: 0.8, initialNutrients: 0.3 });
  seed(api, compile(api, PRESETS.find(preset => preset.name === 'Forager').source), 1, 600, 380);
  advance(api, 6 * 30);
  assert.ok(!body(api, 1).cellular.assemblies.some(machine => machine.name === 'Repair'));
  api.set_environment(1, 0.8, 0);
  let damaged = snapshot(api), requestedDuringDevelopment = false, partialDuringDevelopment = false;
  for (let tick = 0; tick < 3 * 30; tick++) {
    damaged = advance(api, 1);
    const reading = values(api, 1);
    requestedDuringDevelopment ||= !reading.powered && !reading.equipped && reading.need_Repair;
    partialDuringDevelopment ||= body(api, 1).cellular.assemblies.some(machine => machine.name === 'Repair' && !machine.complete);
  }
  api.set_environment(0.5, 0.8, 0);
  assert.ok(damaged.cells[0].integrity < 0.9);
  assert.equal(values(api, 1).powered, false);
  assert.equal(values(api, 1).equipped, false);
  assert.ok(requestedDuringDevelopment, 'Damage cannot revoke a solvent organism’s maintenance request');
  assert.ok(partialDuringDevelopment, 'Maintenance passes through real paid construction while development is interrupted');
  let repairWorked = false;
  for (let tick = 0; tick < 10 * 30; tick++) {
    const state = advance(api, 1);
    assert.equal(state.population, 1);
    repairWorked ||= body(api, 1).repairRate > 0;
    conserved(state);
  }
  assert.ok(body(api, 1).cellular.assemblies.some(machine => machine.name === 'Repair' && machine.complete));
  assert.ok(repairWorked, 'Paid repair completes despite interrupted feeding and navigation development');
  const final = snapshot(api);
  assert.equal(final.energyIntroduced, damaged.energyIntroduced);
  assert.equal(final.eaten, 0);
  assert.equal(final.photosynthesis, 0);
}
console.log('Early damage recovery passed: maintenance completes while damaged developmental prerequisites recover.');
