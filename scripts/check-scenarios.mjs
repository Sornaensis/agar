import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { crownObservation, sampleCrown } from './crown-observation.mjs';
import { advance, body, bytes, catalog, compile, error, evaluation, fresh, memory, restore, save, seed, snapshot } from './organelle-harness.mjs';

// Each run advances the shipped scattered launch without interventions. Descendant
// counts include the distinct individuals observed at the chosen sample interval.
const { BUILTIN_ENVIRONMENTS, BUILTIN_GENOMES, BUILTIN_SIMULATIONS, simulationStartSettings } = await catalog();
const requested = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  const key = process.argv[index], value = process.argv[index + 1];
  assert.ok(['--scenario', '--seeds', '--output'].includes(key) && value, 'Expected --scenario id[,id] --seeds n[,n] or --output path');
  requested.set(key, value);
}
const chosen = requested.get('--scenario')?.split(',');
const templates = BUILTIN_SIMULATIONS.filter(item => !chosen || chosen.includes(item.id.split(':').at(-1)));
assert.ok(templates.length && (!chosen || templates.length === new Set(chosen).size), 'Choose a shipped scenario ID');
const seeds = requested.get('--seeds')?.split(',').map(Number);
assert.ok(!seeds || seeds.every(n => Number.isInteger(n) && n >= 0 && n <= 0xffffffff), 'Seeds must be uint32 values');
const hash = data => createHash('sha256').update(data).digest('hex');
const round = value => Number(value.toFixed(5));
const report = { wasmSha256: hash(bytes), ticksPerSecond:30,
  founderPlacement:'Seeded uniform scatter from the normal launch path',
  environments:Object.fromEntries(BUILTIN_ENVIRONMENTS
    .filter(habitat => templates.some(template => template.environmentId === habitat.id))
    .map(habitat => [habitat.id,habitat.config])), scenarios:[] };
const horizons = { 'mixed-ecosystem':600, 'hunting-ground':120, 'light-and-shade':150, 'nurse-rosette':210, 'pulse-swimmers':210, 'crown-rosette':360 };

const activeMass = (inventory, name) => inventory.cellular.assemblies
  .filter(machine => machine.name === name && machine.complete)
  .reduce((sum, machine) => sum + machine.functionalMass * machine.activity, 0);
const register = (api, cell, name) => memory(api, cell.id).find(slot => slot.name === name)?.value;
const memberCount = (cells, lineages, lineage) => cells.filter(cell => lineages.get(cell.genome) === lineage).length;

async function run(template, worldSeed) {
  const id = template.id.split(':').at(-1);
  const habitat = BUILTIN_ENVIRONMENTS.find(item => item.id === template.environmentId);
  const settings = simulationStartSettings();
  const api = await fresh({ ...habitat.config, ...settings, seed: worldSeed });
  const lineages = new Map(), cohorts = [], observedOffspring = {};
  for (const row of template.populations) {
    const genome = BUILTIN_GENOMES.find(item => item.id === row.genomeId);
    const genomeId = compile(api, genome.source);
    seed(api, genomeId, row.count);
    lineages.set(genomeId, genome.name);
    observedOffspring[genome.name] = new Set();
    cohorts.push({ name:genome.name, count:row.count, sourceSha256:hash(genome.source) });
  }
  let state = snapshot(api);
  assert.equal(state.population, cohorts.reduce((sum, row) => sum + row.count, 0));
  assert.equal(state.food.length, 0, 'Every scenario starts with living founders and no organic particles');
  const result = { id, seed:worldSeed, seconds:horizons[id], habitat:habitat.id, settings, cohorts, observations:{}, checks:{} };
  const obs = result.observations;
  const crown = id === 'crown-rosette' ? crownObservation() : null;
  const shade = new Set(), bright = new Set(), hungryTissue = new Set(), grazing = new Set();
  const rememberedSteering = new Set(), grazersReceivingCarbon = new Set();
  let peakGrazerCarbonTransfer = 0, peakGrazerMineralReturn = 0;
  const pulsed = new Set(), rested = new Set(), priorHigh = new Set(), pulsesWhileYoung = new Map();
  let firstKill = null, firstBirth = null, pairSamples = 0, observedNursing = false, carbonToYoung = 0;
  const firstPassage = new Map();
  const oldestOffspring = new Map();
  const secondsPerSample = id === 'pulse-swimmers' ? 0.2 : ['nurse-rosette','light-and-shade'].includes(id) ? 0.5 : 2;
  result.sampleSeconds = secondsPerSample;
  let atFiveMinutes;
  const lineage = gene => {
    if (!lineages.has(gene)) {
      const parent = api.genome_parent(gene);
      assert.ok(parent > 0 && parent < gene, 'Mutation has an existing parent genome');
      lineages.set(gene, lineage(parent));
    }
    return lineages.get(gene);
  };
  while (state.ticks < result.seconds * 30) {
    state = advance(api, Math.min(secondsPerSample * 30, result.seconds * 30 - state.ticks));
    if (settings.countChemistry) {
      const pointer=api.count_audit_ptr();
      assert.ok(pointer);
      const ledger=new BigUint64Array(api.memory.buffer,pointer,12);
      for(const i of [0,3,6]) assert.equal(ledger[i]+ledger[i+2],ledger[i+1],`${id} count ledger ${i} at tick ${state.ticks}`);
      assert.ok(ledger[9]+ledger[11]<=ledger[10],`${id} energy budget at tick ${state.ticks}`);
    }
    if (crown) sampleCrown(api,state,crown);
    if (firstBirth === null && state.births) firstBirth = state.seconds;
    if (firstKill === null && state.predations) firstKill = state.seconds;
    for (const cell of state.cells) {
      const family = lineage(cell.genome);
      if (cell.generation) {
        observedOffspring[family].add(cell.id);
        oldestOffspring.set(family,Math.max(oldestOffspring.get(family) ?? 0, cell.age));
      }
      if (family === 'Predator' && cell.gutEnergyByKind[1] > 0.01) hungryTissue.add(cell.id);
      if (family === 'Forager' && cell.gutEnergyByKind[0] > 0 && cell.gutEnergyByKind[1] === 0
          && cell.gutEnergyByKind[2] === 0 && cell.digestionRate > 0) grazing.add(cell.id);
      if (id === 'mixed-ecosystem' && family === 'Forager') {
        const { colony } = body(api,cell.id);
        if (colony.neighbors && colony.carbonNet > 0) {
          grazersReceivingCarbon.add(cell.id);
          peakGrazerCarbonTransfer = Math.max(peakGrazerCarbonTransfer,colony.carbonNet);
        }
        if (colony.neighbors) peakGrazerMineralReturn = Math.max(peakGrazerMineralReturn,-colony.mineralNet);
      }
      if (id === 'light-and-shade' && family === 'Mixotroph' && cell.age > 20) {
        if (cell.carbonFixationRate > 0 && !bright.has(cell.id) && activeMass(body(api,cell.id),'Canopy') > 0) bright.add(cell.id);
        if (cell.digestionRate > 0 && cell.carbonFixationRate === 0 && register(api,cell,'rb050') === true) shade.add(cell.id);
      }
      if (id === 'light-and-shade' && family === 'Rememberer' && !rememberedSteering.has(cell.id)) {
        const lastCue = register(api,cell,'rf011');
        if (typeof lastCue === 'number' && cell.age - lastCue > 0.07 && cell.age - lastCue < 1.5) {
          const trace = evaluation(api,cell.id);
          const read = name => trace.bindings.find(binding => binding.name === name)?.value;
          if (read('cue') === false && read('recent') === true && read('steer') === register(api,cell,'rf010')
              && Math.abs(read('steer')) > 0 && activeMass(body(api,cell.id),'LeftMotor') > 0) rememberedSteering.add(cell.id);
        }
      }
      if (id === 'nurse-rosette' && cell.generation === 1 && cell.age < 50 && !observedNursing) {
        const parent = state.cells.find(candidate => candidate.id === cell.parentId);
        if (!parent) continue;
        const adult = body(api,parent.id), young = body(api,cell.id);
        if (adult.colony.neighborIds.includes(cell.id) && young.colony.neighborIds.includes(parent.id)) {
          pairSamples++;
          carbonToYoung = Math.max(carbonToYoung, young.colony.carbonNet);
          if (young.colony.carbonNet > 0 && adult.colony.carbonNet < 0
              && activeMass(adult,'Solar') > 3 * activeMass(young,'Solar')
              && parent.carbonFixationRate > cell.carbonFixationRate && cell.growthRate > 0) observedNursing = true;
        }
      }
      if (id === 'pulse-swimmers' && cell.generation > 0 && cell.age < 40) {
        const high = register(api,cell,'rf021') === 1;
        if (high && !priorHigh.has(cell.id)) {
          const parent = state.cells.find(candidate => candidate.id === cell.parentId);
          const inventory = body(api,cell.id);
          if (parent && register(api,parent,'rf021') === 1 && inventory.colony.neighborIds.includes(parent.id)
              && activeMass(inventory,'PulseMotor') > 0) {
            pulsed.add(cell.id);
            pulsesWhileYoung.set(cell.id,(pulsesWhileYoung.get(cell.id) ?? 0) + 1);
          }
        }
        if (!high && pulsed.has(cell.id) && !rested.has(cell.id)) {
          const inventory = body(api,cell.id);
          if (activeMass(inventory,'PulseMotor') === 0) rested.add(cell.id);
        }
        if (high) priorHigh.add(cell.id); else priorHigh.delete(cell.id);
      }
    }
    if (id === 'mixed-ecosystem' && state.ticks === 30 * 300) {
      atFiveMinutes = { second:state.seconds, population:Object.fromEntries(cohorts.map(({name}) =>
        [name,memberCount(state.cells,lineages,name)])), kills:state.predations };
    }
    for (const [name,ids] of Object.entries(observedOffspring)) if (ids.size && !firstPassage.has(name)) firstPassage.set(name,state.seconds);
  }
  obs.alive = Object.fromEntries(cohorts.map(({name}) => [name, memberCount(state.cells,lineages,name)]));
  obs.observedDescendants = Object.fromEntries(Object.entries(observedOffspring).map(([name,ids]) => [name,ids.size]));
  obs.firstObservedBirth = firstBirth;
  obs.firstObservedKill = firstKill;
  obs.kills = state.predations;
  obs.attacks = state.attacks;
  obs.foodCaptures = state.eaten;
  if (id === 'hunting-ground' || id === 'mixed-ecosystem') {
    obs.huntersObservedWithTissue = hungryTissue.size;
    result.checks.naturalHunt = state.predations > 0 && hungryTissue.size > 0 && obs.alive.Predator > 0;
    result.checks.photosynthesis = state.photosynthesis > 0;
  }
  if (id === 'mixed-ecosystem') {
    obs.firstDescendantSeconds = Object.fromEntries(firstPassage);
    obs.oldestObservedDescendantSeconds = Object.fromEntries(cohorts.map(({name}) => [name,round(oldestOffspring.get(name) ?? 0)]));
    obs.descendantsAliveAtEnd = Object.fromEntries(cohorts.map(({name}) => [name,
      state.cells.filter(cell => cell.generation > 0 && lineage(cell.genome) === name).length]));
    obs.producerGenerationAtEnd = state.cells.reduce((deepest, cell) =>
      lineage(cell.genome) === 'Phototroph' ? Math.max(deepest, cell.generation) : deepest, 0);
    obs.atFiveMinutes = atFiveMinutes;
    obs.grazersObservedDigestingProducerFood = grazing.size;
    obs.grazersReceivingCarbonAtJunctions = grazersReceivingCarbon.size;
    obs.peakCarbonTransferredIntoGrazerPerTick = peakGrazerCarbonTransfer;
    obs.peakMineralTransferredOutOfGrazerPerTick = peakGrazerMineralReturn;
    result.checks.noSuppliedOrganicFood = state.photosynthesis > 0 && !('add_food' in api) && !('set_food_rate' in api);
    result.checks.grazing = grazing.size > 0;
    result.checks.carbonReceivedThroughJunctions = grazersReceivingCarbon.size > 0;
    result.checks.tenMinuteCoexistence = Object.values(obs.alive).every(n => n > 0);
    result.checks.allLineagesReproduced = Object.values(obs.observedDescendants).every(n => n > 0);
    result.checks.producersRenewAcrossGenerations = obs.producerGenerationAtEnd >= 3;
  }
  if (id === 'light-and-shade') {
    obs.cellsObservedFixingCarbonWithCanopy = bright.size;
    obs.cellsObservedDigestingInShade = shade.size;
    obs.cellsObservedInBothModes = [...bright].filter(cell => shade.has(cell)).length;
    obs.rememberersObservedSteeringAfterCueLoss = rememberedSteering.size;
    result.checks.switchingFeedingModes = obs.cellsObservedInBothModes > 0 && state.eaten > 0;
    result.checks.rememberedSteering = rememberedSteering.size > 0;
  }
  if (id === 'nurse-rosette') {
    obs.peakObservedCarbonToJuvenile = round(carbonToYoung);
    obs.attachmentSamplesUntilNursing = pairSamples;
    result.checks.parentFeedsAttachedGrowingDaughter = observedNursing;
  }
  if (crown) {
    Object.assign(obs,crown);
    result.checks.edgeBuildsHarvesterBesideSignalingCenter = Boolean(crown.firstSpecialization);
  }
  if (id === 'pulse-swimmers') {
    obs.juvenilesWithMotorPulses = pulsed.size;
    obs.juvenilesWithSubsequentMotorRest = rested.size;
    obs.peakPulsesInOneJuvenile = Math.max(0,...pulsesWhileYoung.values());
    result.checks.juvenileRelaysAndRests = [...rested].some(cell => pulsesWhileYoung.get(cell) >= 2);
  }
  if (settings.countChemistry) {
    const checkpoint=save(api), resumed=await fresh();
    assert.equal(restore(resumed,checkpoint),1,error(resumed));
    assert.deepEqual(save(resumed),checkpoint,'Populated endpoint keeps exact inventories and program state');
    result.checks.exactResourceBudgets = true;
    result.checks.endpointSaveRestore = true;
  }
  result.passed = Object.values(result.checks).every(Boolean);
  console.log(JSON.stringify({scenario:id,seed:worldSeed,observations:obs,checks:result.checks}));
  return result;
}
for (const template of templates) {
  const runs = [];
  for (const worldSeed of seeds ?? [template.seed]) runs.push(await run(template,worldSeed));
  const { id, habitat, seconds, sampleSeconds, settings, cohorts } = runs[0];
  report.scenarios.push({ id, habitat, seconds, sampleSeconds, settings, cohorts,
    runs:runs.map(({seed, observations, checks, passed}) => ({seed, observations, checks, passed})) });
}
report.passed = report.scenarios.every(scenario => scenario.runs.every(run => run.passed));
if (requested.has('--output')) await writeFile(requested.get('--output'), JSON.stringify(report,null,2) + '\n');
assert.ok(report.passed,'One or more shipped scenarios did not demonstrate its advertised behavior');
