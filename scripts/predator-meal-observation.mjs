// A stationary donor releases one finite packet while the hunter grows elsewhere.
// The same saved encounter can then be approached from different bearings.
import assert from 'node:assert/strict';
import { advance, body, compile, error, fresh, restore, save, snapshot } from './organelle-harness.mjs';
import {lyseDonor} from './feeding-culture.mjs';

const pulseDonor = releaseWhen => `cellular counts;
product Mount = anchor();
product Carrier = transporter();
gene mounts: Mount = if built.Outlet < 0.016 && pool.Mount < 10 then 1 else 0;
gene carriers: Carrier = if built.Outlet < 0.016 && pool.Carrier < 10 then 1 else 0;
assembly Outlet: export(carbon) {
  Mount: 40;
  Carrier: 160;
  start_when: built.Outlet < 0.016;
  activity: if ${releaseWhen} then 1 else 0;
}`;

async function develop(source, donorSource) {
  const api = await fresh({ seed:42, width:1200, height:760, ambientLight:0.8,
    ambientTemperature:0.2, initialNutrients:1, countChemistry:true,
    gases:{ mode:'open', exchangeRate:0.2 } });
  const donor = api.place_cell(compile(api, donorSource), 120, 150);
  const hunter = api.place_cell(compile(api, source), 780, 460);
  assert.ok(donor && hunter, error(api));
  const state = advance(api, 34 * 30);
  const cell = state.cells.find(c => c.id === hunter);
  assert.equal(state.population, 2);
  assert.equal(state.eaten, 0);
  assert.equal(cell.gutEnergy, 0);
  assert.ok(cell.gutCapacity > 0 && Math.hypot(cell.vx, cell.vy) > 1,
    'The approaching hunter has paid for working food storage and is swimming');
  assert.ok(body(api, hunter).cellular.assemblies.some(a => a.kind === 'import(particle)' && a.complete));
  return {api, cell, donor, state};
}

function separateDonor(api, donor, cell, state) {
  const packets = state.food.filter(p => Math.hypot(p.x - 120, p.y - 150) < 40);
  assert.equal(packets.length, 1);
  const [packet] = packets;
  assert.ok(packet.kind === 0 && packet.energy > 0.4);
  // Leave the food in its original position with no producer in sensing range.
  assert.equal(api.move_cell(donor, 1050, 650, 0), 1, error(api));
  return {api, cell, packet, world:save(api)};
}

async function prepare(source) {
  const {api, donor, cell, state} = await develop(source, pulseDonor('age > 32 && age < 33.5'));
  assert.equal(state.food.length, 1);
  return separateDonor(api, donor, cell, state);
}

// Prime reserves with real digested donor tissue. The player ATP control keeps
// energy above the old appetite threshold while the reserve and gut diverge.
// All interventions end before saving the starting state for the approach.
async function prepareFunded(source) {
  // Trigger the other donor from its restored ATP supply once this specific
  // hunter is ready. Equal emission time gives both genomes the same fresh dose.
  const controlledDonor = pulseDonor('age > 35 && energy.fraction > 0.98');
  const {api, cell, donor} = await develop(source, controlledDonor);
  let separated = false;
  for (let second = 0; second < 200; second++) {
    const state = snapshot(api);
    const hunter = state.cells.find(c => c.id === cell.id);
    assert.ok(hunter);
    if (second % 4 === 0) {
      assert.equal(api.set_cell_stores(cell.id, 1, hunter.capacity, 0), 1, error(api));
    }
    if (!separated && hunter.organicCarbon * 20 > hunter.capacity * 0.6) {
      separated = true;
      assert.equal(api.move_cell(cell.id, 650, 450, 0), 1, error(api));
    }
    if (!separated && second % 4 === 0) {
      lyseDonor(api, 360, 220);
      const tissue = snapshot(api).food.filter(p => p.kind === 1 && Math.hypot(p.x-360, p.y-220) < 22)
        .sort((a, b) => b.energy-a.energy)[0];
      assert.ok(tissue);
      const reach = hunter.radius + 0.7;
      assert.equal(api.move_cell(cell.id, tissue.x - reach * Math.cos(hunter.angle),
        tissue.y - reach * Math.sin(hunter.angle), 0), 1, error(api));
    } else if (!separated && second % 4 === 3) {
      assert.equal(api.move_cell(cell.id, 650, 450, 0), 1, error(api));
    }
    if (separated && hunter.energy > hunter.capacity * 0.7 &&
        hunter.organicCarbon * 20 > hunter.capacity * 0.4 && hunter.gutEnergy < 3) {
      const emitter = state.cells.find(c => c.id === donor);
      assert.ok(emitter);
      assert.equal(api.set_cell_stores(donor, 1, emitter.capacity, 0), 1, error(api));
      const supplied = advance(api, 45);
      const ready = supplied.cells.find(c => c.id === cell.id);
      assert.ok(ready.energy > ready.capacity * 0.7 && ready.organicCarbon * 20 > ready.capacity * 0.4 &&
        ready.gutEnergy < 3, 'The hunter remains funded when the new pulse separates');
      assert.equal(api.set_cell_stores(donor, 1, emitter.capacity * 0.7, 0), 1, error(api));
      return separateDonor(api, donor, ready, supplied);
    }
    advance(api, 30);
  }
  assert.fail('The funded hunter must retain usable reserves with room for a new meal');
}

function observe({api, cell, packet, world}, gap, degrees, ticks = 180) {
  assert.equal(restore(api, world), 1, error(api));
  const angle = cell.angle + degrees * Math.PI / 180;
  const distance = cell.radius + gap;
  assert.equal(api.move_cell(cell.id, packet.x - distance * Math.cos(angle),
    packet.y - distance * Math.sin(angle), 0), 1, error(api));
  let state = snapshot(api);
  const capturesAtStart = state.eaten;
  let firstIngestionTick = null, targetConsumedTick = null, assimilated = 0;
  let targetPresent = true, firstDivisionTick = null;
  for (let tick = 1; tick <= ticks; tick++) {
    const before = state;
    const captures = state.eaten;
    state = advance(api, 1);
    const hunter = state.cells.find(c => c.id === cell.id);
    assert.ok(hunter, 'The test hunter survives the encounter');
    if (state.population > 2) firstDivisionTick ??= tick;
    if (hunter.gutEnergyByKind[0] > 0) firstIngestionTick ??= tick;
    assimilated += hunter.digestionRate / 30;
    // A primed cell can still be digesting its earlier tissue. Attribute this
    // meal by the disappearing pulse and a capture on the very same tick.
    if (targetPresent && !state.food.some(p => p.kind === 0 && Math.hypot(p.x-packet.x, p.y-packet.y) < 5)) {
      targetPresent = false;
      const anotherFeeder = [...before.cells, ...state.cells].some(c => c.id !== hunter.id &&
        Math.hypot(c.x-packet.x, c.y-packet.y) < c.radius + 8);
      if (state.eaten > captures && !anotherFeeder &&
          Math.hypot(hunter.x-packet.x, hunter.y-packet.y) < hunter.radius + 8) {
        targetConsumedTick = tick;
      }
    }
  }
  assert.equal(state.attacks, 0);
  assert.ok(Math.abs(state.material - state.materialIntroduced) < 1e-7);
  assert.ok(state.energy <= state.energyIntroduced + state.photosynthesis + 1e-7);
  return {gap, degrees, firstIngestionTick, targetConsumedTick, targetPresent, firstDivisionTick, assimilated, particlesEaten:state.eaten-capturesAtStart};
}

export async function observePredatorMeals(source) {
  const fixture = await prepare(source);
  const trials = [];
  for (const gap of [6, 14]) {
    for (const angle of [-120,-90,-60,-30,0,30,60,90,120]) {
      trials.push(observe(fixture, gap, angle));
    }
  }
  return {packetEnergy:fixture.packet.energy, trials};
}

export async function observeFundedPredatorMeals(source) {
  const fixture = await prepareFunded(source);
  const {cell, packet} = fixture;
  // This replay lets the pulse decay in place while the hunter is out of reach.
  const targetOnly = (gap, degrees) => {
    const {targetConsumedTick, targetPresent, firstDivisionTick} = observe(fixture, gap, degrees, 16 * 30);
    return {gap, degrees, targetConsumedTick, targetPresent, firstDivisionTick};
  };
  const distant = targetOnly(280, 0);
  const trials = [-90,0,90].map(degrees => targetOnly(6, degrees));
  return {atp:cell.energy, atpCapacity:cell.capacity, organicCarbon:cell.organicCarbon,
    gutEnergy:cell.gutEnergy, gutCapacity:cell.gutCapacity, packetEnergy:packet.energy, horizonSeconds:16, distant, trials};
}

export async function checkPredatorMeals(source) {
  const measured = await observePredatorMeals(source);
  const ate = row => row.firstIngestionTick !== null && row.targetConsumedTick !== null && row.assimilated > 0.1 && row.particlesEaten > 0;
  const directAndFlank = measured.trials.filter(row => row.degrees === 0 || Math.abs(row.degrees) === 90);
  assert.ok(directAndFlank.every(ate),
    'The moving hunter turns toward and digests a single soluble packet ahead and on each flank at both distances');
  const closed = source.replace(/(assembly\s+\w+\s*:\s*import\(particle\)\s*\{[^}]*?activity:\s*)[^;]+/g, (_, start) => start + '0');
  assert.notEqual(closed, source, 'The negative control closes the authored particle inlet');
  const disabled = await prepare(closed);
  assert.equal(disabled.packet.energy, measured.packetEnergy);
  for (const angle of [0, 90]) {
    const control = observe(disabled, 6, angle);
    assert.equal(control.firstIngestionTick, null);
    assert.equal(control.assimilated, 0);
    assert.equal(control.particlesEaten, 0);
  }
  const funded = await observeFundedPredatorMeals(source);
  assert.ok(funded.distant.targetPresent && funded.distant.targetConsumedTick === null,
    'The detached pulse persists for the full observation without access by the hunter');
  assert.ok(funded.trials.every(row => row.targetConsumedTick !== null),
    'A hunter with high reserves and an almost-empty gut can collect a nearby pulse head-on and from either side');
  console.log(`Predator particle approaches: ${measured.trials.filter(ate).length}/${measured.trials.length} consumed and digested within six seconds, including both flanks. The primed adult collected all three offered packets in separate replays. Closed inlets consumed zero.`);
}
