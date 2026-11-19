import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { advance, body, catalog, compile, evaluation, fresh, memory, restore, save, seed, snapshot } from './organelle-harness.mjs';

const { PRESETS, BUILTIN_ENVIRONMENTS } = await catalog();
const pool = BUILTIN_ENVIRONMENTS.find(item => item.id === 'builtin:environment:colony-pool').config;
const exchanges = ['energyNet', 'mineralNet', 'carbonNet'];

async function growPair(name, transform = source => source) {
  const preset = PRESETS.find(item => item.name === name);
  const example = await readFile(new URL('../examples/' + name.toLowerCase().replaceAll(' ', '-') + '.cell', import.meta.url), 'utf8');
  assert.equal(example.trimEnd(), preset.source, 'The downloadable example matches the playable genome');
  const api = await fresh({ ...pool, seed:42 });
  const id = compile(api, transform(preset.source));
  seed(api, id, 1, pool.width / 2, pool.height / 2);
  assert.equal(body(api, 1).cellular.assemblies.length, 0, 'Founder starts without free machinery');
  let state = snapshot(api);
  while (!state.births && state.ticks < 150 * 30) {
    assert.equal(state.population, 1, name + ' survives its initial development');
    state = advance(api, state.cells[0].area < 1.88 ? 30 : 1);
  }
  assert.equal(state.population, 2, name + ' reaches fission from one founder in the demo habitat');
  assert.equal(state.cells[1].parentId, state.cells[0].id);
  return { api, state };
}

const binding = (trace, name) => trace.bindings.find(item => item.name === name).value;
const activeMass = (inventory, name) => inventory.cellular.assemblies
  .filter(machine => machine.name === name && machine.complete)
  .reduce((sum, machine) => sum + machine.functionalMass * machine.activity, 0);

function closeJunctions(source) {
  let changed = 0;
  const control = source.replace(/(assembly Share(?:Energy|Mineral|Carbon): junction\([^)]*\) \{[^}]*?activity: )if paired_contact then 1 else 0/g,
    (_, prefix) => { changed++; return prefix + '0'; });
  assert.equal(changed, 3, 'Control closes all three junctions while retaining their physical recipes');
  return control;
}

async function nursery(open) {
  const { api, state:birth } = await growPair('Nurse rosette', open ? undefined : closeJunctions);
  const [adult, young] = birth.cells;
  const report = { birthSeconds:birth.seconds, pairedTicks:0, transferred:{energyNet:0,mineralNet:0,carbonNet:0}, energyExchanged:0,
    maternalSolarAtFission:activeMass(body(api, adult.id), 'Solar'), peakMaternalSolar:0, peakNurseFixation:0 };
  let confirmedRoles = false;
  let lastState = birth;
  for (let step = 0; step < 48 * 30; step++) {
    const state = advance(api, 1);
    assert.equal(state.population, 2);
    lastState = state;
    const mother = body(api, adult.id), daughter = body(api, young.id);
    const m = state.cells[0];
    if (mother.colony.neighbors < 1 || daughter.colony.neighbors < 1) continue;
    report.pairedTicks++;
    for (const resource of exchanges) {
      const gain = daughter.colony[resource], loss = mother.colony[resource];
      assert.ok(Math.abs(gain + loss) < 1e-9, resource + ' moved between cells');
      if (!open) assert.equal(gain, 0);
      report.transferred[resource] += gain;
      if (resource === 'energyNet') report.energyExchanged += Math.abs(gain);
    }
    const motherSolar = activeMass(mother, 'Solar'), daughterSolar = activeMass(daughter, 'Solar');
    if (daughterSolar > 0 && motherSolar > daughterSolar * 3) {
      report.peakNurseFixation = Math.max(report.peakNurseFixation, m.carbonFixationRate);
      report.peakMaternalSolar = Math.max(report.peakMaternalSolar, motherSolar);
      if (!confirmedRoles) {
        const motherTrace = evaluation(api, adult.id), childTrace = evaluation(api, young.id);
        confirmedRoles = binding(motherTrace, 'nursing') && binding(childTrace, 'developing');
        if (confirmedRoles) {
          assert.equal(motherTrace.outputs.find(item => item.name === 'activity.Envelope').value, 0);
          assert.equal(childTrace.outputs.find(item => item.name === 'activity.Solar').value, 0.05);
        }
      }
    }
  }
  assert.ok(confirmedRoles && report.pairedTicks > 30 * 40, 'Maternal and juvenile programs differentiate only after physical contact');
  assert.ok(report.peakNurseFixation > 0, 'Nurse fixes real inorganic carbon while the juvenile reduces harvesting');
  const juvenile = lastState.cells.find(cell => cell.id === young.id);
  report.daughterAt48s = { energy:juvenile.energy, organicCarbon:juvenile.organicCarbon, envelopeGrowth:juvenile.area - young.area };
  assert.ok(report.peakMaternalSolar > report.maternalSolarAtFission + 0.03, 'The mother completes an additional paid harvester while nursing');
  if (open) {
    assert.ok(report.transferred.carbonNet > 1 && report.transferred.mineralNet < -0.02,
      'The developing cell receives organic carbon and supplies minerals to its nurse');
    assert.ok(report.energyExchanged > 1, 'Energy flows as the cells’ reserve gradients change');
    assert.ok(report.daughterAt48s.envelopeGrowth > 0.3, 'The juvenile converts material into new envelope while attached');
  }
  while (!lastState.cells.some(cell => cell.parentId === young.id) && lastState.seconds < birth.seconds + 150) lastState = advance(api, 15);
  assert.ok(lastState.cells.some(cell => cell.parentId === young.id), 'The original daughter produces a granddaughter');
  report.daughterReproducedBySeconds = lastState.seconds - birth.seconds;
  return report;
}

async function swimmer(sender) {
  const { api, state:birth } = await growPair('Pulse swimmer', sender ? undefined : source => source.replace('signal rf021;', ''));
  const [adult, young] = birth.cells;
  const report = { birthSeconds:birth.seconds, adultPulses:[], daughterPulses:[], peakDaughterMotor:0, quietDaughterMotor:false };
  let prevAdult = false, prevYoung = false;
  const trajectory = [];
  for (let step = 1; step <= 18 * 30; step++) {
    const state = advance(api, 1);
    const values = id => Object.fromEntries(memory(api, id).map(item => [item.name, item.value]));
    const a = values(adult.id), d = values(young.id);
    const adultHigh = a.rf021 === 1, youngHigh = d.rf021 === 1;
    if (adultHigh && !prevAdult) report.adultPulses.push(step);
    if (youngHigh && !prevYoung) {
      report.daughterPulses.push(step);
      assert.equal(d.ri031, 180, 'Relay starts its refractory clock on the rising edge');
      assert.ok(state.cells.find(cell => cell.id === young.id).age < 40, 'Young relay cannot fire its own pacemaker');
    }
    if (youngHigh || (prevYoung && !youngHigh) || step % 30 === 0) {
      const motor = activeMass(body(api, young.id), 'PulseMotor');
      report.peakDaughterMotor = Math.max(report.peakDaughterMotor, motor);
      if (!youngHigh && motor === 0) report.quietDaughterMotor = true;
    }
    const child = state.cells.find(cell => cell.id === young.id);
    trajectory.push([child.vx, child.vy]);
    prevAdult = adultHigh; prevYoung = youngHigh;
  }
  assert.ok(report.adultPulses.length > 0, 'An attached mature cell initiates the pulse train');
  assert.ok(report.quietDaughterMotor);
  if (sender) {
    assert.ok(report.daughterPulses.length >= 2 && report.peakDaughterMotor > 0,
      'The young responder repeatedly activates paid, built propulsion');
    for (const step of report.daughterPulses) assert.ok(report.adultPulses.some(start => step > start && step <= start + 2),
      'A previous contact sample relays the pulse after its sender fires');
    for (let index = 1; index < report.daughterPulses.length; index++)
      assert.ok(report.daughterPulses[index] - report.daughterPulses[index - 1] > 180, 'Feedback cannot retrigger the refractory cell');
  } else {
    assert.deepEqual(report.daughterPulses, []);
    assert.equal(report.peakDaughterMotor, 0, 'A private pacemaker register cannot drive its daughter’s motor');
  }
  return { report, trajectory };
}

// In this controlled exchange assay the producer starts in mineral-depleted
// water. A later particle meal supplies the grazer with minerals it can return.
async function tradeWithProducer(openPorts) {
  const api = await fresh({width:400,height:400,ambientLight:.9,ambientTemperature:.18,initialNutrients:0,gases:{mode:'open',exchangeRate:.2}});
  const producer = PRESETS.find(preset => preset.name === 'Phototroph');
  const grazer = PRESETS.find(preset => preset.name === 'Forager');
  seed(api, compile(api,producer.source), 1, 200, 200);
  let closed = 0;
  const program = openPorts ? grazer.source : grazer.source.replace(/(assembly (?:Carbon|Mineral)Port: [^}]*?activity: )[^;]+;/g,
    (_, prefix) => { closed++; return prefix + '0;'; });
  if (!openPorts) assert.equal(closed,2);
  const id = compile(api,program);
  const host = advance(api,32*30).cells[0];
  // Replay the normal introduction jitter so the grazer's front receptor
  // faces the producer. Both still start from ordinary paid founder bodies.
  const before = save(api);
  seed(api,id,1,200,200);
  const trial = snapshot(api).cells[1];
  assert.equal(restore(api,before),1);
  const separation = host.radius + trial.radius + 0.12;
  seed(api,id,1,host.x - Math.cos(trial.angle)*separation - (trial.x-200),host.y - Math.sin(trial.angle)*separation - (trial.y-200));
  const report = {attachedTicks:0,grazerCarbonNet:0,grazerMineralNet:0};
  for (let tick=1; tick<=70*30; tick++) {
    const state = advance(api,1);
    const grazer = state.cells.find(cell => cell.id === trial.id);
    assert.ok(grazer, 'The grazer survives the full feeding interval');
    const transfers = state.cells.map(cell => body(api,cell.id).colony);
    const receiver = transfers[state.cells.indexOf(grazer)];
    if (receiver.neighbors) report.attachedTicks++;
    // Count the producer’s descendants if a natural division occurs.
    for (const resource of ['carbonNet','mineralNet']) {
      assert.ok(Math.abs(transfers.reduce((sum,flux) => sum + flux[resource],0)) < 1e-9);
    }
    report.grazerCarbonNet += receiver.carbonNet;
    report.grazerMineralNet += receiver.mineralNet;
    if (tick >= 24*30 && tick <= 45*30 && tick%30 === 0) api.add_food(grazer.x,grazer.y,4);
  }
  assert.ok(report.attachedTicks > 40*30, 'Both treatments sustain real adhesion');
  const inventory = body(api,trial.id).cellular.assemblies;
  assert.ok(['Dock','CarbonPort','MineralPort'].every(name => inventory.some(machine => machine.name===name && machine.functionalMass > 0)),
    'Contact and both exchange ports require completed machinery even in the closed control');
  return report;
}

const open = await nursery(true), closed = await nursery(false);
assert.ok(open.daughterAt48s.energy > closed.daughterAt48s.energy + 10, 'Open junctions protect the developing daughter’s reserve');
assert.ok(open.daughterReproducedBySeconds + 5 < closed.daughterReproducedBySeconds, 'Provisioning advances the daughter’s reproductive cycle');
console.log('Nurse rosette', JSON.stringify({ open, closed }));
const relay = await swimmer(true), silent = await swimmer(false);
const velocityDifference = Math.max(...relay.trajectory.map(([vx, vy], i) => Math.hypot(vx - silent.trajectory[i][0], vy - silent.trajectory[i][1])));
assert.ok(velocityDifference > 0.01, 'Relayed propulsion changes the daughter’s measured motion');
console.log('Pulse swimmer', JSON.stringify({ relay:relay.report, silent:silent.report, velocityDifference }));

const trading = await tradeWithProducer(true), isolated = await tradeWithProducer(false);
assert.ok(trading.grazerCarbonNet > 1 && trading.grazerMineralNet < -0.05, 'The producer supplies fixed carbon and receives mineral from a fed grazer');
assert.equal(isolated.grazerCarbonNet,0); assert.equal(isolated.grazerMineralNet,0);
console.log('Producer–grazer exchange', JSON.stringify({trading,isolated}));
