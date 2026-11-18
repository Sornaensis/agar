import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fresh, compile, seed, snapshot, advance, graph, body, evaluation, save, restore, source, error, catalog } from './organelle-harness.mjs';

const { PRESETS } = await catalog();
const conserved = state => {
  assert.ok(Math.abs(state.material - state.materialIntroduced) < Math.max(1e-7, state.materialIntroduced * 1e-10), 'Material transfers remain conserved');
  assert.ok(state.energy <= state.energyIntroduced + state.photosynthesis + 1e-7, 'Fabrication and metabolism cannot create energy');
};

// Every authored example must compile, including files users can import directly.
for (const name of ['solar', 'regulated-motor', 'phototroph', 'forager', 'rememberer', 'separate-digestion']) {
  const api = await fresh({ ambientLight: 0.85, initialNutrients: 3 });
  const genome = compile(api, await readFile(new URL(`../examples/${name}.cell`, import.meta.url), 'utf8'));
  if (name === 'solar' || name === 'regulated-motor') {
    seed(api, genome, 1);
    conserved(advance(api, 1200));
    assert.equal(body(api, 1).cellular.assemblies.filter(machine => machine.complete).length, 1,
      `${name} keeps its stock buffer small enough to avoid an unintended second complete machine`);
  }
}

const cycles = [];
for (const preset of PRESETS) {
  const api = await fresh({ ambientLight: 0.85, initialNutrients: 3 });
  const genome = compile(api, preset.source);
  assert.equal(graph(api, genome).mode, 'cellular');
  seed(api, genome, 1, 600, 380);
  const initial = body(api, 1);
  assert.equal(initial.organs.length, 1);
  assert.equal(initial.organs[0].kind, 'core');
  assert.equal(initial.cellular.assemblies.length, 0);
  assert.ok(initial.cellular.products.every(product => product.mass === 0));
  const beforeInspection = save(api);
  const inspected = evaluation(api, 1);
  assert.equal(inspected.mode, 'cellular');
  assert.ok(!inspected.signals.some(signal => ['id', 'time', 'body.division_ready'].includes(signal.name)));
  assert.deepEqual(save(api), beforeInspection, 'Cellular inspection must not initialize or mutate state');

  let state = advance(api, 1);
  const first = body(api, 1).cellular;
  assert.equal(first.assemblies.length, 0, 'Newly expressed material cannot install in the same tick');
  const made = first.products.reduce((sum, product) => sum + product.mass, 0);
  assert.ok(made > 0 && made * 0.6 <= 0.022 / 30 + 1e-10, 'All genes share the same paid core budget');
  assert.ok(state.cells[0].energy < 68 && state.cells[0].nutrients < 0.6);

  let nextFood = 1, previous;
  while (!state.births && state.ticks < 150 * 30) {
    const parent = state.cells.find(cell => cell.id === 1);
    assert.ok(parent, `${preset.name} must survive development in its supplied habitat`);
    if (preset.name !== 'Phototroph' && state.ticks >= nextFood) {
      assert.equal(api.add_food(parent.x, parent.y, 4), 4);
      nextFood = state.ticks + 30;
    }
    const steps = parent.area >= 1.9 ? 1 : 30;
    previous = steps === 1 ? body(api, 1).cellular : undefined;
    state = advance(api, steps);
    conserved(state);
  }
  assert.equal(state.births, 1, `${preset.name} assembles and funds a complete reproductive cycle`);
  assert.equal(state.population, 2);
  assert.ok(previous, 'Observe the actual fission tick');
  const daughter = state.cells.find(cell => cell.generation === 1);
  assert.ok(daughter && daughter.parentId === 1);
  const inherited = [body(api, 1).cellular, body(api, daughter.id).cellular];
  const machines = inherited.flatMap(cell => cell.assemblies);
  assert.equal(new Set(machines.map(machine => machine.id)).size, machines.length, 'Fission assigns actual instances without cloning IDs');
  for (const machine of previous.assemblies) {
    const copy = machines.find(item => item.id === machine.id);
    assert.ok(copy, 'All unfinished and complete machines survive partition');
    assert.equal(copy.name, machine.name);
    assert.equal(copy.kind, machine.kind);
    assert.ok(copy.mass + 1e-10 >= machine.mass, 'Partition preserves installed material');
  }
  for (const cell of inherited) {
    assert.equal(cell.constriction.progress, 0);
    assert.equal(cell.replication.mass, 0);
    assert.ok(cell.replication.genomeMass > 0, 'Paid copied material is inherited instead of disappearing');
  }
  if (preset.name === 'Phototroph') {
    assert.ok(state.photosynthesis > 0);
    assert.equal(state.eaten, 0);
  }
  if (preset.name === 'Forager') assert.equal(state.photosynthesis, 0);

  const bytes = save(api), resumed = await fresh();
  assert.equal(new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(8, true), 6);
  assert.equal(restore(resumed, bytes), 1, error(resumed));
  assert.deepEqual(save(resumed), bytes);
  advance(api, 90); advance(resumed, 90);
  assert.deepEqual(save(resumed), save(api), 'Products, progress, inherited recipes and subsequent dynamics resume exactly');
  for (const kind of [4, 5]) {
    const mutant = api.mutate_genome(genome, 17, kind);
    assert.ok(mutant !== genome && mutant > 0, error(api));
    assert.equal(api.genome_mutation_kind(mutant), kind);
    compile(api, source(api, mutant));
    assert.equal(api.mutate_genome(genome, 17, kind), mutant);
  }
  cycles.push({ name: preset.name, firstDivisionSeconds: Math.round(state.seconds * 100) / 100 });
}

console.log('Cellular checks passed: 8 authored genomes, 6 importable examples, paid development, frozen inspection, resource conservation, physical fission, gene mutations, and exact v6 continuation.');
console.log(JSON.stringify({ conditions: 'One founder; 0.85 light; 3 initial minerals per patch; 4 supplied particles/s for non-phototrophs.', cycles }));
