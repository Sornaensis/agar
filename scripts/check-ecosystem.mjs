import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { bytes, fresh, compile, seed, snapshot, advance, body, save, catalog } from './organelle-harness.mjs';

// Measure the actual shipped mixed scenario, without the supplied-food or
// stationary-organism shortcuts used by isolated pathway tests.
const options = { seeds: [42], duration: 600, mutation: undefined, output: null, wasm: null, assert: false };
for (let i = 2; i < process.argv.length; i++) {
  const key = process.argv[i];
  if (key === '--assert') { options.assert = true; continue; }
  const value = process.argv[++i];
  assert.ok(value && ['--seed', '--duration', '--mutation', '--output', '--wasm'].includes(key), `Unknown or missing argument: ${key}`);
  if (key === '--seed') options.seeds = value.split(',').map(Number);
  if (key === '--duration') options.duration = Number(value);
  if (key === '--mutation') options.mutation = Number(value);
  if (key === '--output') options.output = value;
  if (key === '--wasm') options.wasm = value;
}
assert.ok(options.seeds.every(n => Number.isInteger(n) && n >= 0 && n <= 0xffffffff));
assert.ok(Number.isSafeInteger(options.duration) && options.duration > 0);
assert.ok(!options.assert || options.duration >= 300, 'Ecological regression checks require at least 300 simulated seconds');
const binary = options.wasm ? await readFile(options.wasm) : bytes;
const customModule = options.wasm ? await WebAssembly.compile(binary) : undefined;
const { BUILTIN_ENVIRONMENTS, BUILTIN_GENOMES, BUILTIN_SIMULATIONS, simulationStartSettings } = await catalog();
const template = BUILTIN_SIMULATIONS.find(item => item.id === 'builtin:simulation:mixed-ecosystem');
const environment = BUILTIN_ENVIRONMENTS.find(item => item.id === template.environmentId);
const startSettings = simulationStartSettings();
options.mutation ??= startSettings.mutationRate;
assert.ok(options.mutation >= 0 && options.mutation <= 1);
const populations = template.populations.map(row => ({ ...BUILTIN_GENOMES.find(item => item.id === row.genomeId), ...row }));
assert.equal('foodRate' in environment.config,false, 'Habitats cannot provision organic particles');
const samplesAt = [...new Set([0, 15, 30, 60, 120, 180, 300, 600, 900, options.duration].filter(n => n <= options.duration))].sort((a, b) => a - b);
const study = {
  startedAt: new Date().toISOString(), wasmSha256: createHash('sha256').update(binary).digest('hex'), wasmBytes: binary.length,
  template, environment, populations, seeds: options.seeds, mutationRate: options.mutation,
  durationSeconds: options.duration, sampleSeconds: samplesAt, results: [],
  assertConsumers: options.assert,
  rateDefinitions: {
    upkeep: 'Body maintenance before temperature scaling; excludes the extra 0.15 ATP/s per area basal cost.',
    metabolism: 'Complete basal maintenance at local temperature; motor, weapon, construction and repair costs are additional.',
    digestionRate: 'Assimilated substrate potential per simulated second; usable energy requires oxygen-dependent respiration.',
  },
};
const round = n => Math.round(n * 1e6) / 1e6;
const sum = (rows, name) => rows.reduce((total, row) => total + row[name], 0);
const kindNames = ['exudate', 'tissue', 'waste'];
function closestDistance(cell, candidates) {
  const { width, height } = environment.config;
  let nearest = Infinity;
  for (const candidate of candidates) {
    const dx = Math.abs(cell.x - candidate.x), dy = Math.abs(cell.y - candidate.y);
    nearest = Math.min(nearest, Math.hypot(Math.min(dx, width - dx), Math.min(dy, height - dy)));
  }
  return Number.isFinite(nearest) ? round(nearest) : null;
}
for (const randomSeed of options.seeds) {
  const started = performance.now();
  const api = await fresh({ ...environment.config, ...startSettings, seed: randomSeed, mutationRate: options.mutation }, customModule);
  const names = new Map();
  for (const population of populations) {
    const genome = compile(api, population.source); names.set(genome, population.name); seed(api, genome, population.count);
  }
  const founding = [...new Set(names.values())];
  const lineage = genome => {
    if (names.has(genome)) return names.get(genome);
    const parent = api.genome_parent(genome); assert.ok(parent && parent !== genome);
    const name = lineage(parent); names.set(genome, name); return name;
  };
  const result = { seed: randomSeed, wallSeconds: 0, samples: [], observedDescendants: Object.fromEntries(founding.map(name => [name, 0])) };
  study.results.push(result);
  const descendants = new Map(founding.map(name => [name, new Set()]));
  const observeDescendants = state => {
    for (const cell of state.cells) if (cell.generation > 0) descendants.get(lineage(cell.genome)).add(cell.id);
    result.observedDescendants = Object.fromEntries([...descendants].map(([name, ids]) => [name, ids.size]));
  };
  let ticks = 0;
  for (const seconds of samplesAt) {
    let state;
    if (options.assert) {
      // Observe descendants every second so a birth need not coincide with the
      // sparse diagnostic samples. These remain observed IDs, not an exact
      // lifetime birth counter: an organism could be born and die between reads.
      while (ticks < seconds * 30) {
        const count = Math.min(30, seconds * 30 - ticks);
        state = advance(api, count); ticks += count; observeDescendants(state);
      }
      state ??= snapshot(api);
    } else {
      state = seconds === 0 ? snapshot(api) : advance(api, seconds * 30 - ticks);
      ticks = seconds * 30; observeDescendants(state);
    }
    const metricsPointer = api.metrics_snapshot_ptr();
    const metrics = new Float64Array(api.memory.buffer, metricsPointer, 51).slice();
    const carbonDelta = metrics[36] + metrics[49] - metrics[37];
    const oxygenDelta = metrics[44] + metrics[45] + metrics[48] + metrics[41] + metrics[42] - metrics[40] - metrics[47];
    // Historical comparison engines predate player withdrawals.
    const energyWithdrawn = api.energy_withdrawn_total?.() ?? 0;
    const mineralWithdrawn = api.mineral_withdrawn_total?.() ?? 0;
    const materialRemoved = api.nutrient_removed_total() + mineralWithdrawn;
    const accounting = {
      material: state.material, materialIntroduced: state.materialIntroduced, materialRemoved,
      materialDelta: state.material + materialRemoved - state.materialIntroduced,
      materialTolerance: Math.max(1e-7, state.materialIntroduced * 1e-10),
      energy: state.energy, energyWithdrawn, energyIntroduced: state.energyIntroduced,
      photosynthesis: state.photosynthesis, energyTolerance: 1e-7,
      energyExcess: state.energy + energyWithdrawn - state.energyIntroduced - state.photosynthesis,
      carbon: metrics[36], carbonIntroduced: metrics[37], carbonRemoved: metrics[49], carbonDelta,
      carbonTolerance: Math.max(1e-7, metrics[37] * 1e-10),
      oxygen: metrics[44] + metrics[45], oxygenIntroduced: metrics[47], oxygenRemoved: metrics[48], oxygenDelta,
      oxygenTolerance: Math.max(1e-7, (metrics[47] + metrics[40]) * 1e-10),
      carbonFixed: metrics[40], carbonRespired: metrics[41], carbonDecayed: metrics[42],
    };
    const materialAccounted = Math.abs(accounting.materialDelta) < accounting.materialTolerance;
    const energyAccounted = accounting.energy + accounting.energyWithdrawn <= accounting.energyIntroduced + accounting.photosynthesis + accounting.energyTolerance;
    const carbonAccounted = Math.abs(accounting.carbonDelta) < accounting.carbonTolerance;
    const oxygenAccounted = Math.abs(accounting.oxygenDelta) < accounting.oxygenTolerance;
    const producers = state.cells.filter(cell => lineage(cell.genome) === 'Phototroph');
    const lineages = founding.map(name => {
      const cells = state.cells.filter(cell => lineage(cell.genome) === name), machinery = new Map();
      for (const cell of cells) {
        const physical = body(api, cell.id);
        for (const machine of physical.cellular?.assemblies ?? []) {
          const entry = machinery.get(machine.name) ?? { name: machine.name, kind: machine.kind, instances: 0, complete: 0, active: 0, mass: 0 };
          entry.instances++; entry.complete += Number(machine.complete); entry.active += Number(machine.complete && machine.activity > 0 && machine.integrity > 0);
          entry.mass += machine.mass; machinery.set(machine.name, entry);
        }
      }
      const totalKeys = ['area', 'mass', 'energy', 'nutrients', 'gutEnergy', 'photosynthesisRate', 'digestionRate', 'growthRate', 'exudationRate', 'upkeep', 'metabolism', 'expressionRate', 'organicCarbon', 'internalOxygen', 'internalInorganicCarbon', 'respirationRate', 'carbonFixationRate'];
      return {
        name, count: cells.length, ...Object.fromEntries(totalKeys.map(key => [key, round(sum(cells, key))])),
        meanEnergyFraction: cells.length ? round(cells.reduce((total, cell) => total + cell.energy / cell.capacity, 0) / cells.length) : null,
        maxGeneration: cells.length ? Math.max(...cells.map(cell => cell.generation)) : null,
        gutByKind: Object.fromEntries(kindNames.map((kind, i) => [kind, round(cells.reduce((total, cell) => total + cell.gutEnergyByKind[i], 0))])),
        machinery: [...machinery.values()].map(entry => ({ ...entry, mass: round(entry.mass) })),
        consumers: name === 'Phototroph' ? undefined : cells.map(cell => ({
          id: cell.id, generation: cell.generation, x: round(cell.x), y: round(cell.y), angle: round(cell.angle),
          energy: round(cell.energy), capacity: round(cell.capacity), nutrients: round(cell.nutrients), area: round(cell.area),
          integrity: round(cell.integrity), gutEnergy: round(cell.gutEnergy), digestionRate: round(cell.digestionRate),
          organicCarbon: round(cell.organicCarbon), internalOxygen: round(cell.internalOxygen), respirationRate: round(cell.respirationRate),
          speed: round(Math.hypot(cell.vx, cell.vy)), upkeep: round(cell.upkeep), metabolism: round(cell.metabolism),
          closestParticle: closestDistance(cell, state.food),
          closestProducer: closestDistance(cell, producers),
        })),
      };
    });
    const sample = {
      seconds, population: state.population, births: state.births, deaths: state.deaths, particlesEaten: state.eaten,
      attacks: state.attacks, predations: state.predations, mutations: state.mutations,
      energy: round(state.energy), energyIntroduced: round(state.energyIntroduced), photosynthesis: round(state.photosynthesis),
      mineralError: accounting.materialDelta, accounting,
      organic: Object.fromEntries(kindNames.map((kind, i) => {
        const particles = state.food.filter(food => food.kind === i);
        return [kind, { count: particles.length, energy: round(sum(particles, 'energy')), minerals: round(sum(particles, 'minerals')) }];
      })), lineages,
    };
    result.samples.push(sample); result.wallSeconds = round((performance.now() - started) / 1000);
    if (!materialAccounted || !energyAccounted || !carbonAccounted || !oxygenAccounted) {
      // Preserve the exact failing world before throwing. A large-world error
      // must be inspectable and resumable, not just an unlabelled failed check.
      const outputBase = options.output ? resolve(options.output.replace(/\.json$/i, ''))
        : fileURLToPath(new URL('../experiments/ecosystem', import.meta.url));
      const stem = `${outputBase}.seed-${randomSeed}.tick-${state.ticks}.accounting-failure`;
      const checkpoint = `${stem}.bin`, details = `${stem}.json`;
      await writeFile(checkpoint, save(api));
      await writeFile(details, JSON.stringify({ seed: randomSeed, wasmSha256: study.wasmSha256, accounting, snapshot: state }, null, 2) + '\n');
      result.accountingFailure = { seconds: state.seconds, ticks: state.ticks, ...accounting, checkpoint, details };
    }
    console.log(JSON.stringify({ seed: randomSeed, seconds, population: sample.population, births: sample.births, deaths: sample.deaths,
      eaten: sample.particlesEaten, attacks: sample.attacks, predations: sample.predations, organic: sample.organic,
      lineages: lineages.map(({ name, count, meanEnergyFraction, gutEnergy, digestionRate, maxGeneration }) => ({ name, count, meanEnergyFraction, gutEnergy, digestionRate, maxGeneration })), wallSeconds: result.wallSeconds }));
    if (options.output) await writeFile(options.output, JSON.stringify(study, null, 2) + '\n');
    assert.ok(materialAccounted, `Material accounting at ${state.seconds}s: delta ${accounting.materialDelta}, introduced ${accounting.materialIntroduced}, tolerance ${accounting.materialTolerance}`);
    assert.ok(energyAccounted, `Energy accounting at ${state.seconds}s: excess ${accounting.energyExcess}, tolerance ${accounting.energyTolerance}`);
    assert.ok(carbonAccounted, `Carbon accounting at ${state.seconds}s: delta ${accounting.carbonDelta}, tolerance ${accounting.carbonTolerance}`);
    assert.ok(oxygenAccounted, `Oxygen accounting at ${state.seconds}s: delta ${accounting.oxygenDelta}, tolerance ${accounting.oxygenTolerance}`);
  }
  if (options.assert) {
    const last = result.samples.at(-1);
    result.assertions = {
      noExternalOrganicFood: !('add_food' in api) && !('set_food_rate' in api),
      producerLineageReproducesAcrossGenerations: last.lineages.find(item => item.name === 'Phototroph').maxGeneration >= 3,
      foragerSurvives: last.lineages.find(item => item.name === 'Forager').count > 0,
      predatorSurvives: last.lineages.find(item => item.name === 'Predator').count > 0,
      foragerReproduces: last.lineages.find(item => item.name === 'Forager').maxGeneration >= 2,
      predatorReproduces: last.lineages.find(item => item.name === 'Predator').maxGeneration >= 2,
      attacksOccur: last.attacks > 0,
      predationOccurs: last.predations > 0,
    };
    if (options.output) await writeFile(options.output, JSON.stringify(study, null, 2) + '\n');
  }
}
if (!options.output && !options.assert) console.log(JSON.stringify(study));
if (options.assert) {
  const failures = study.results.flatMap(result => Object.entries(result.assertions).filter(([, passed]) => !passed).map(([name]) => `seed ${result.seed}: ${name}`));
  assert.deepEqual(failures, [], `Mixed ecosystem regression failures: ${failures.join(', ')}`);
  console.log(`Mixed ecosystem checks passed: living producers reach generation three, consumers survive and reproduce, physical attacks and predation occur, no supplied food, ${options.duration} seconds across ${options.seeds.length} seed(s).`);
}
