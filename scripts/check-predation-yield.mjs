// Use the same paid Phototroph and Predator on both sides of the accounting comparison.
// --baseline path/to/abi16.wasm runs both versions and verifies the paired outcome.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { advance, body, catalog, compile, error, restore, save, snapshot, source as genomeSource } from './organelle-harness.mjs';

const args = new Map();
for (let i = 2; i < process.argv.length; i += 2) {
  assert.ok(['--wasm', '--baseline', '--output'].includes(process.argv[i]) && process.argv[i + 1]);
  args.set(process.argv[i], process.argv[i + 1]);
}
async function run(file) {
  const bytes = await readFile(file);
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const api = instance.exports;
  const abi = api.abi_version();
  assert.ok([16,17,18,19,20,21,22].includes(abi));
  const sha = data => createHash('sha256').update(data).digest('hex');
  const { PRESETS } = await catalog();
  const ok = (status, reason) => assert.ok(status, reason + ' ' + error(api));
  const close = (actual, expected, epsilon = 1e-6) => assert.ok(Math.abs(actual - expected) <= epsilon, `${actual} != ${expected}`);
  const sum = (array, fn) => array.reduce((total, item) => total + fn(item), 0);
  const tissue = state => sum(state.food.filter(f => f.kind === 1), f => f.energy);
  const carbon = () => {
    const ptr = api.metrics_snapshot_ptr();
    return new Float64Array(api.memory.buffer,ptr,37)[36];
  };
  const structure = cell => cell.mass - cell.nutrients - cell.gutNutrients;
  const foodPotential = cell => abi >= 17 ? body(api, cell.id).foodPotential : null;
  const live = (state, id) => state.cells.find(c => c.id === id);

  ok(api.set_world_size(800,480), 'habitat');
  api.set_environment(.5,.85,0);
  api.set_initial_nutrients(3); api.set_mutation_rate(0);
  if (abi < 18) api.set_food_rate(0);
  ok(api.set_initial_gases(1,1), 'initial gases');
  ok(api.set_gas_exchange(1,.5,1,1), 'open reservoir');
  ok(abi < 18 ? api.reset_empty(42,0) : api.reset_empty(42), 'reset');
  // Historical comparisons start both engines with exactly the same two
  // compiled programs, before the introduction of the first founder. The
  // shared producer disables docking so differences in adhesive fabrication
  // cannot change its development during the cross-version comparison.
  // Single-engine runs measure the current catalog.
  let photo, predator;
  if (args.has('--baseline') || abi < 20) {
    ok(restore(api, await readFile(new URL('../engine/fixtures/predation-assay-v12.bin',import.meta.url))), 'restore paired assay');
    assert.equal(api.genome_count(),2);
    [photo,predator] = [1,2];
  } else {
    photo = compile(api,PRESETS.find(p=>p.name==='Phototroph').source);
    predator = compile(api,PRESETS.find(p=>p.name==='Predator').source);
  }
  const grownId = api.place_cell(photo,100,100);
  ok(grownId,'seed Phototroph');
  let state = snapshot(api);
  const report = {
    abi, wasmSha256:sha(bytes),
    sources:{ Phototroph:sha(genomeSource(api,photo)), Predator:sha(genomeSource(api,predator)) },
    setup:{seed:42,width:800,height:480,initialMineralsPerPatch:3,temperature:.5,
      light:.85,oxygenEquilibrium:1,inorganicCarbonEquilibrium:1,reservoirExchangePerSecond:.5,
      mutationRate:0,initialFood:0,organicInputPerSecond:0,encounterSeconds:60},
    lysis:{}, encounters:{},
  };

  function measureLysis() {
    const cp = save(api), cell = live(state,grownId), initialC = carbon();
    const beforeTissue = tissue(state), basis = 20 * (structure(cell) + cell.organicCarbon);
    const potential = foodPotential(cell);
    // One living cell; subtract all other measured stocks from total system energy.
    const fabricated = state.energy - sum(state.food, f => f.energy) - cell.energy - cell.gutEnergy - basis;
    if (potential) close(potential.body + potential.reserve - basis, fabricated);
    ok(api.set_cell_stores(cell.id, 1, 0, 0), 'controlled ATP withdrawal and immediate lysis');
    const after = snapshot(api), released = tissue(after) - beforeTissue;
    close(carbon(),initialC); close(after.material,state.material);
    close(released, abi === 16 ? basis : potential.total - cell.energy);
    close(state.energy - after.energy - cell.energy, abi === 16 ? fabricated : 0);
    const observation = { age:cell.age,area:cell.area,structuralCarbon:structure(cell),reserveCarbon:cell.organicCarbon,
      atpWithdrawn:cell.energy,retainedFabrication:fabricated, releasedTissue:released,
      energyLostBeyondWithdrawal:state.energy - after.energy - cell.energy };
    ok(restore(api,cp),'restore pre-lysis state');
    return observation;
  }
  report.lysis.newborn = measureLysis();
  while (state.cells[0].area < 1.22 && state.seconds < 130) state = advance(api,3);
  assert.ok(state.cells[0].area >= 1.22);
  report.lysis.grown = measureLysis();
  assert.ok(report.lysis.grown.releasedTissue > report.lysis.newborn.releasedTissue);
  const hunterId = api.place_cell(predator,550,300);
  ok(hunterId,'seed Predator');
  const machinery = ['Gut','TissueEnzyme','Piercer','Vision','Touch','Capture'];
  while (true) {
    state = advance(api,3);
    const hunter = live(state,hunterId);
    assert.ok(hunter && hunter.age < 30,'natural predator development');
    const assemblies = body(api,hunterId).cellular.assemblies;
    if (machinery.every(name => assemblies.some(a => a.name === name && a.complete))) break;
  }
  const newbornId = api.place_cell(photo,250,100);
  ok(newbornId,'seed identical newborn Phototroph');
  const start = save(api);
  const materialFractions = new DataView(start.buffer,start.byteOffset).getUint32(8,true) >= 20;

  function runEncounter(name, preyId, carcassOnly = false) {
    ok(restore(api,start),'restore common encounter');
    const before = snapshot(api), hunter = live(before,hunterId), prey = live(before,preyId);
    if (prey) {
      const reach = hunter.radius + prey.radius + 0.15;
      ok(api.move_cell(preyId,hunter.x + Math.cos(hunter.angle) * reach,hunter.y + Math.sin(hunter.angle) * reach,0),'one-time placement at contact');
    }
    // These walls isolate background algae and naturally secreted particles.
    for (const x of [300,775]) ok(api.add_barrier(x,240,480,10,Math.PI/2,0),'isolation wall');
    let providedTissue = 0;
    if (carcassOnly) {
      ok(api.set_cell_stores(preyId,1,0,0),'withdraw prey ATP before feeding');
      const packets = snapshot(api).food.filter(f => f.kind === 1);
      assert.ok(packets.length);
      providedTissue = sum(packets,f => f.energy);
      // Put the consumer once at the actual carcass. All harvesting, digestion
      // and respiration after this controlled first contact remain autonomous.
      const intake = body(api,hunter.id).cellular.assemblies.find(a => a.kind === 'import(particle)' && a.complete);
      const angle = hunter.angle + (intake?.at ?? 0);
      const offset = materialFractions ? hunter.radius : 0;
      ok(api.move_cell(hunterId,packets[0].x - Math.cos(angle)*offset,packets[0].y - Math.sin(angle)*offset,0),'first contact at the intake with a real carcass packet');
    }
    let current = snapshot(api), previous = live(current,hunterId), previousPrey = live(current,preyId);
    let previousPreyPotential = previousPrey && foodPotential(previousPrey);
    const initialATP = previous.energy;
    let observedTissueIn = 0, observedTissueOut = 0, digested = 0, respired = 0, grossATP = 0;
    let peakGutTissue = 0, otherGut = 0, firstHit = null, kill = null;
    const offspring = new Set();
    const firstAge = previous.age;
    for (let tick = 1; tick <= 1800; tick++) {
      // Respiration occurs after passive gas exchange but before capture, digestion,
      // fixation or construction. Use the pre-tick reserve's energy/carbon ratio.
      const density = previous.organicCarbon > 0
        ? (abi === 16 ? 20 : foodPotential(previous).reserve / previous.organicCarbon) : 0;
      current = advance(api,1);
      const h = live(current,hunterId);
      assert.ok(h,'Hunter survives the observation window');
      for (const c of current.cells) if (c.genome === predator && c.generation > 0) offspring.add(c.id);
      const target = live(current,preyId);
      if (current.attacks > 0 && firstHit === null) firstHit = tick / 30;
      const delta = h.gutEnergyByKind[1] - previous.gutEnergyByKind[1];
      observedTissueIn += Math.max(0,delta);
      observedTissueOut += Math.max(0,-delta);
      peakGutTissue = Math.max(peakGutTissue,h.gutEnergyByKind[1]);
      otherGut = Math.max(otherGut,h.gutEnergyByKind[0]+h.gutEnergyByKind[2]);
      digested += h.digestionRate / 30;
      respired += h.respirationRate / 30;
      grossATP += .8 * density * h.respirationRate / 30;
      if (previousPrey && !target && !carcassOnly && !kill) {
        kill = {seconds:tick/30, preHitBodyCarbon:structure(previousPrey),preHitReserveCarbon:previousPrey.organicCarbon,
          preHitATP:previousPrey.energy, preHitFoodPotential:previousPreyPotential?.total ?? null,
          tissueReleased:tissue(current),tissueInHunter:h.gutEnergyByKind[1]};
        const estimate = abi === 16 ? 20 * (kill.preHitBodyCarbon + kill.preHitReserveCarbon) : previousPreyPotential.total;
        close(kill.tissueReleased,estimate,1);
        close(kill.tissueInHunter,0);
      }
      previousPrey = target;
      previousPreyPotential = target && foodPotential(target);
      previous = h;
    }
    const family = current.cells.filter(c => c.genome === predator);
    const result = {initialATP,finalATP:previous.energy,initialHunterAge:firstAge,
      hunterAreaAtEnd:previous.area,hunterOffspring:offspring.size,hunterFamilyATPAtEnd:sum(family,c => c.energy),
      preyAgeAtPlacement:prey?.age ?? null,preyAreaAtPlacement:prey?.area ?? null,
      firstHit,kill,predations:current.predations,providedTissue,
      observedTissueIn,observedTissueOut,peakGutTissue,peakOtherGutFractions:otherGut,
      assimilatedFoodEnergy:digested,respiredCarbon:respired,grossRespiratoryATP:grossATP,tissueLeft:tissue(current)};
    if (!prey) {
      assert.equal(current.predations,0); close(peakGutTissue,0); close(digested,0);
    } else {
      assert.ok(observedTissueIn > 0 && observedTissueOut > 0 && digested > 0 && grossATP > 0);
      if (carcassOnly) {
        assert.equal(current.predations,0);
        if (materialFractions) assert.ok(otherGut > 0, 'Both reserve carbon and structural carbon enter the gut from a real carcass');
        else close(otherGut,0);
      }
      else { assert.ok(firstHit !== null && kill && current.predations === 1); }
    }
    report.encounters[name] = result;
  }
  runEncounter('fasting',0);
  runEncounter('newborn',newbornId);
  runEncounter('grown',grownId);
  runEncounter('carcassOnly',grownId,true);
  assert.ok(report.encounters.grown.kill.tissueReleased > report.encounters.newborn.kill.tissueReleased);
  assert.ok(report.encounters.carcassOnly.grossRespiratoryATP > report.encounters.fasting.grossRespiratoryATP);
  report.notes = [
    'Neutral lysis removes all free ATP first. The energy-loss figure excludes that recorded withdrawal.',
    'Live prey and the hunter develop from ordinary founders. One placement establishes first contact. A kill and subsequent pickup are separate events.',
    'Tissue-gut increases and decreases are changes in the founding hunter. Capture, digestion, spill and fission can affect that gut. Assimilated food energy includes all substrates. Live prey may leak exudate.',
    'The isolated carcass has its ATP removed first. Fasting and carcass-only arms measure gross respiratory ATP including the hunter’s original reserves.',
  ];
  return report;
}

const current = await run(args.get('--wasm') ?? new URL('../engine/pkg/agar_engine.wasm', import.meta.url));
let output = current;
if (args.has('--baseline')) {
  const before = await run(args.get('--baseline'));
  assert.equal(before.abi,16);
  assert.ok(current.abi >= 17);
  assert.deepEqual(current.sources,before.sources);
  for (const name of ['newborn','grown']) {
    const old = before.encounters[name], now = current.encounters[name];
    assert.equal(now.kill.seconds,old.kill.seconds,'Compare the same completed hunt');
    for (const field of ['preHitBodyCarbon','preHitReserveCarbon','preHitATP']) {
      assert.ok(Math.abs(now.kill[field] - old.kill[field]) < 1e-8, 'Matched prey ' + field);
    }
    assert.ok(now.kill.tissueReleased > old.kill.tissueReleased);
    assert.ok(now.grossRespiratoryATP > old.grossRespiratoryATP);
  }
  assert.ok(current.encounters.carcassOnly.grossRespiratoryATP > before.encounters.carcassOnly.grossRespiratoryATP);
  assert.ok(Math.abs(current.encounters.fasting.grossRespiratoryATP
    - before.encounters.fasting.grossRespiratoryATP) < 1e-8, 'Matched fasting respiration');
  output = {before,after:current};
}
const json = JSON.stringify(output,null,2) + '\n';
if (args.has('--output')) await writeFile(args.get('--output'),json);
else process.stdout.write(json);
