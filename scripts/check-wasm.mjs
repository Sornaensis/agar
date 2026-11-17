import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { checkEngineLoader } from './check-engine-loader.mjs';

const file = new URL('../engine/pkg/agar_engine.wasm', import.meta.url);
const bytes = await readFile(file);
const { instance } = await WebAssembly.instantiate(bytes, {});
const api = instance.exports;
for (const name of ['alloc', 'dealloc', 'compile_genome', 'error_ptr', 'error_len', 'reset', 'seed_cells', 'add_food', 'set_food_rate', 'tick', 'snapshot_ptr', 'snapshot_len', 'abi_version', 'mutate_genome', 'genome_source_ptr', 'genome_source_len', 'genome_parent', 'genome_mutation_kind', 'set_mutation_rate', 'mutation_rate', 'mutation_count', 'mutation_skipped_count', 'genome_count', 'genome_limit', 'genome_bytes', 'genome_byte_limit', 'cell_memory_ptr', 'cell_memory_len', 'set_environment', 'add_source', 'remove_source', 'clear_sources', 'environment_snapshot_ptr', 'environment_snapshot_len', 'predation_count', 'attack_count', 'photosynthesis_total']) {
  assert.equal(typeof api[name], 'function', `Missing WASM export: ${name}`);
}
assert.ok(api.memory instanceof WebAssembly.Memory);
assert.equal(typeof api.genome_graph_ptr, 'function');
assert.equal(typeof api.genome_graph_len, 'function');
assert.equal(api.abi_version(), 7);
assert.equal(typeof api.cell_evaluation_ptr, 'function');
assert.equal(typeof api.cell_evaluation_len, 'function');
const HEADER_SIZE = 12;
const CELL_STRIDE = 63;
const FOOD_STRIDE = 4;
const ENV_HEADER = 7;
const PATCH_STRIDE = 5;
const DT = 1 / 30;
for (const name of ['set_initial_nutrients', 'add_nutrients', 'nutrient_total', 'nutrient_input_total', 'nutrient_dissolved_total', 'nutrient_food_total', 'nutrient_cell_total']) {
  assert.equal(typeof api[name], 'function', `Missing nutrient export: ${name}`);
}

function compile(source) {
  const encoded = new TextEncoder().encode(source);
  const pointer = api.alloc(encoded.length);
  try {
    new Uint8Array(api.memory.buffer, pointer, encoded.length).set(encoded);
    return api.compile_genome(pointer, encoded.length);
  } finally {
    api.dealloc(pointer, encoded.length);
  }
}

function snapshot() { return snapshotOf(api); }
function close(actual, expected, tolerance, message) {
  assert.ok(Math.abs(actual - expected) <= tolerance, `${message}: expected ${expected}, got ${actual}`);
}
function cellsOf(state) {
  return Array.from({ length: state[2] }, (_, index) => {
    const n = HEADER_SIZE + index * CELL_STRIDE;
    return { id:state[n], genome:state[n+1], x:state[n+2], y:state[n+3], vx:state[n+4], vy:state[n+5], energy:state[n+7],
      generation:state[n+9], parent:state[n+10], radius:state[n+11], foodDistance:state[n+13],
      neighborDistance:state[n+16], size:state[n+18], capacity:state[n+19], metabolism:state[n+20], memoryUsed:state[n+21], memoryCost:state[n+22], light:state[n+23], temperature:state[n+25], flowX:state[n+27], flowY:state[n+28], integrity:state[n+38], photoRate:state[n+39],
      nutrientStored:state[n+40], nutrientCapacity:state[n+41], nutrientLevel:state[n+42], nutrientAngle:state[n+43], uptakeRate:state[n+44],
      body:[...state.slice(n+45,n+50),state[n+53]], senseRange:state[n+50], remodelingRate:state[n+51], bodyUpkeep:state[n+52],
      gutEnergy:state[n+54], gutCapacity:state[n+55], gutNutrients:state[n+56], digestionRate:state[n+57], growthRate:state[n+58], birthSize:state[n+59], maturity:state[n+60], lifespan:state[n+61], exudationRate:state[n+62] };
  });
}
function advance(module, steps) {
  while (steps > 0) { const chunk = Math.min(steps, 120); assert.equal(module.tick(chunk), 1, errorOf(module)); steps -= chunk; }
  return snapshotOf(module);
}
function normalizedGenomes(state) {
  const result = state.slice();
  for (let n = HEADER_SIZE; n < HEADER_SIZE + state[2] * CELL_STRIDE; n += CELL_STRIDE) result[n+1] = 0;
  return result;
}
async function worldFor(text, seed = 31415, initialNutrients) {
  const module = await fresh();
  const id = compileIn(module, text);
  assert.ok(id, `Growth test program compiles: ${errorOf(module)}`);
  if (initialNutrients !== undefined) module.set_initial_nutrients(initialNutrients);
  module.reset(seed, id);
  module.set_mutation_rate(0);
  module.set_food_rate(0);
  return module;
}

const source = `# Pure expressions sampled once per simulation tick.
let wander = sin(age * 0.7 + id) * 0.6;
turn = if food.visible then clamp(food.angle * 1.8, -1, 1) else wander;
thrust = if food.visible then 0.8 else 0.45;
grow = if size < 2 && energy > 70 * size then 1 else 0;
divide = energy > 125 * size && size >= 2 && age > 5;`;
const genome = compile(source);
assert.ok(genome > 0, 'Valid reactive genome compiles');
api.reset(1337, genome);
api.set_mutation_rate(0);
const initial = snapshot();
assert.equal(initial[7], 1200);
assert.equal(initial[8], 760);
assert.ok(initial[2] > 0 && initial[3] > 0, 'World starts playable');
api.tick(90);
const firstRun = snapshot();
assert.equal(firstRun[0], 90);
assert.ok(Math.abs(firstRun[1] - 3) < 0.001, 'Fixed 30 Hz simulation');
assert.notDeepEqual(firstRun, initial, 'WASM actually advances the world');

api.reset(1337, genome);
api.set_mutation_rate(0);
api.tick(90);
assert.deepEqual(snapshot(), firstRun, 'Seeded simulation is deterministic');

const beforeInvalid = snapshot();
assert.equal(compile('thrust = unknown_signal; turn = 0; divide = false;'), 0, 'Unknown sensors are rejected');
const error = new TextDecoder().decode(new Uint8Array(api.memory.buffer, api.error_ptr(), api.error_len()));
assert.ok(error.length > 0, 'Compile errors include a readable explanation');
assert.deepEqual(snapshot(), beforeInvalid, 'Invalid source does not mutate the running world');
assert.equal(compile('thrust = true; turn = 0; divide = false;'), 0, 'Output types are checked');
assert.equal(compile('thrust = 0; turn = 0; divide = false; grow = true;'), 0, 'Growth must be a numeric output');

assert.ok(api.seed_cells(genome, 3, 600, 380) > 0, 'Compiled genomes can seed new cells');
assert.ok(api.add_food(600, 380, 5) > 0, 'User can add food');
// Sustained external input funds a heterotroph's slower growth cycle.
api.set_food_rate(6);
for (let step = 0; step < 40; step += 1) {
  api.tick(120);
  snapshot();
}
const final = snapshot();
assert.ok(final[6] > 0, 'Cells actually eat');
assert.ok(final[4] > 0, 'Cells actually reproduce');
assert.ok(final[5] > 0, 'Cells actually die');
console.log(`WASM integration passed: fixed ticks, deterministic seeds, typed programs, finite state, ${Math.round(final[4])} births, ${Math.round(final[5])} deaths, ${Math.round(final[6])} meals.`);

// Exercise mutations through the actual boundary used by the TypeScript UI.
function sourceOf(module, id) {
  const ptr = module.genome_source_ptr(id);
  const len = module.genome_source_len();
  assert.ok(ptr && len, `Genome ${id} must expose editable source`);
  return new TextDecoder().decode(new Uint8Array(module.memory.buffer, ptr, len));
}
function errorOf(module) {
  return new TextDecoder().decode(new Uint8Array(module.memory.buffer, module.error_ptr(), module.error_len()));
}
function snapshotOf(module) {
  const ptr = module.snapshot_ptr();
  const values = new Float32Array(module.memory.buffer, ptr, module.snapshot_len()).slice();
  assert.equal(values.length, HEADER_SIZE + values[2] * CELL_STRIDE + values[3] * FOOD_STRIDE, 'ABI7 snapshot layout');
  assert.ok(values.every(Number.isFinite), 'Simulation state must remain finite');
  assert.equal(values[9], 0, 'Former population-limit slot is reserved, with no population ceiling');
  assert.ok(Number.isSafeInteger(values[2]) && values[2] >= 0, 'Population is a nonnegative count');
  assert.ok(values[3] <= values[10], 'Organic particle budget');
  for (const cell of cellsOf(values)) {
    assert.ok(cell.x >= 0 && cell.x < values[7], 'Cell x wraps');
    assert.ok(cell.y >= 0 && cell.y < values[8], 'Cell y wraps');
    assert.ok(cell.energy > 0, 'Dead cells must be removed');
    assert.ok(cell.size >= 0.5 && cell.size <= 4, 'Biomass stays inside physical size bounds');
    close(cell.radius, 7 * Math.sqrt(cell.size), 0.00002, 'Radius follows 2D area');
    close(cell.capacity, 200 * cell.size, 0.0003, 'Energy capacity scales with biomass');
    const assigned = cell.body.reduce((sum, value) => sum + value, 0);
    assert.ok(cell.body.every(value => value >= 0 && value <= 1) && assigned <= 1.000001, 'Actual capabilities share one body budget');
    close(cell.bodyUpkeep, (0.30 * (assigned - cell.body[5]) + 0.80 * cell.body[5]) * cell.size, 0.000002, 'Specialized body upkeep is exposed');
    close(cell.metabolism, (0.35 * cell.size + cell.bodyUpkeep) * (0.5 + cell.temperature), 0.000003, 'Basal metabolism includes body investment and local temperature');
    close(cell.nutrientCapacity, 2 * cell.size, 0.000002, 'Free mineral storage scales with biomass');
    assert.ok(cell.nutrientStored >= 0 && cell.nutrientStored <= cell.nutrientCapacity + 0.000002, 'Nutrient reserves remain bounded');
    close(cell.senseRange, 40 + 400 * cell.body[4], 0.00004, 'Sensing follows actual investment');
    assert.ok(cell.nutrientLevel >= 0 && cell.uptakeRate >= 0 && cell.remodelingRate >= 0);
    assert.ok(cell.energy <= cell.capacity + 0.0003, 'Feeding respects size-dependent capacity');
    assert.ok(cell.gutEnergy >= 0 && cell.gutEnergy <= cell.gutCapacity + 0.0001, 'Gut contents respect capacity');
    assert.ok(cell.gutNutrients >= 0 && cell.digestionRate >= 0 && cell.growthRate >= 0 && cell.exudationRate >= 0, 'Trophic state remains nonnegative');
    assert.ok(cell.birthSize >= 0.5 && cell.birthSize <= 4 && cell.maturity >= 0 && cell.maturity <= 1 && cell.lifespan > 0, 'Lifecycle signals remain bounded');
  }
  return values;
}
function compileIn(module, text) {
  const encoded = new TextEncoder().encode(text);
  const ptr = module.alloc(encoded.length);
  assert.ok(ptr, 'Source stays within the byte allocation limit');
  try {
    new Uint8Array(module.memory.buffer, ptr, encoded.length).set(encoded);
    return module.compile_genome(ptr, encoded.length);
  } finally { module.dealloc(ptr, encoded.length); }
}
async function fresh() {
  return (await WebAssembly.instantiate(bytes, {})).instance.exports;
}

// Three-output programs remain valid and behave exactly like explicit grow=0.
const legacySource = 'thrust = 0; turn = 0; divide = false;';
const control = await worldFor(legacySource);
const explicitZero = await worldFor(`${legacySource} grow = 0;`);
const negativeGrowth = await worldFor(`${legacySource} grow = -5;`);
const controlAfterThreeSeconds = advance(control, 90);
assert.deepEqual(advance(explicitZero, 90), controlAfterThreeSeconds, 'Legacy programs default to zero growth');
assert.deepEqual(advance(negativeGrowth, 90), controlAfterThreeSeconds, 'Negative growth requests clamp to zero');
assert.ok(cellsOf(controlAfterThreeSeconds).every(cell => cell.size === 1), 'A non-growing founder stays size1');

// Inspect one tick away from food/collisions to account for growth's actual
// energy expenditure, independently of energy gained from eating.
const growthSource = `${legacySource} grow = 1;`;
const growth = await worldFor(growthSource, 31415, 5);
const matchedControl = await worldFor(legacySource, 31415, 5);
const growthInitial = snapshotOf(growth);
const isolatedIds = cellsOf(growthInitial).filter(cell => cell.foodDistance > 20 && cell.neighborDistance > 20).map(cell => cell.id);
assert.ok(isolatedIds.length > 0, 'Growth cost comparison has isolated cells');
const firstGrowth = advance(growth, 1);
const firstControl = advance(matchedControl, 1);
const firstGrowthCells = new Map(cellsOf(firstGrowth).map(cell => [cell.id, cell]));
const firstControlCells = new Map(cellsOf(firstControl).map(cell => [cell.id, cell]));
for (const id of isolatedIds) {
  const grown = firstGrowthCells.get(id);
  const unchanged = firstControlCells.get(id);
  close(grown.size - unchanged.size, 0.05 * DT, 0.000001, 'Growth rate is bounded per fixed tick');
  close(unchanged.energy - grown.energy, 80 * (grown.size - unchanged.size), 0.00003, 'Growing pays80 energy per biomass');
}
const excessiveGrowth = await worldFor(`${legacySource} grow = 50;`, 31415, 5);
assert.deepEqual(advance(excessiveGrowth, 1), firstGrowth, 'Growth requests above1 clamp to1');

// A size-dependent signal stops growth without any stateful genome instruction.
const feedback = await worldFor(`${legacySource} grow = if size < 1.5 then 1 else 0; photosynthesize=1;`);
feedback.set_environment(0.5,1,0);
const settled = advance(feedback, 1800);
assert.ok(cellsOf(settled).every(cell => cell.size >= 1.5 - 0.000001 && cell.size <= 1.5 + 0.05 * Math.sqrt(1.5) * DT + 0.000001), 'The size signal stops growth at its threshold, within one tick');
const settledSizes = cellsOf(settled).map(cell => [cell.id, cell.size]);
const later = advance(feedback, 120);
assert.deepEqual(cellsOf(later).map(cell => [cell.id, cell.size]), settledSizes, 'Size feedback keeps later growth at zero');

// Grow for exactly one tick, then verify the increased basal cost in the next.
const pulse = await worldFor(`${legacySource} grow = if size < 1.001 then 1 else 0;`);
const pulseFirst = new Map(cellsOf(advance(pulse, 1)).map(cell => [cell.id, cell]));
const pulseSecond = new Map(cellsOf(advance(pulse, 1)).map(cell => [cell.id, cell]));
for (const id of isolatedIds) {
  const before = pulseFirst.get(id);
  const after = pulseSecond.get(id);
  assert.equal(after.size, before.size, 'The size guard prevents another growth payment');
  close(before.energy - after.energy, (0.65 * before.size + after.uptakeRate) * DT, 0.00003, 'Maintenance charges the grown size and actual mineral uptake');
}

// The enlarged capacity must govern feeding, not merely be displayed in the ABI.
feedback.set_environment(0.5,0,0);
const beforeFeeding = advance(feedback,1800);
const feedingTarget = cellsOf(beforeFeeding).find(cell => cell.neighborDistance > 40);
assert.ok(feedingTarget, 'Capacity check has an isolated target');
assert.ok(feedback.add_food(feedingTarget.x, feedingTarget.y, 1200) > 0);
const fed = cellsOf(advance(feedback, 2400)).find(cell => cell.id === feedingTarget.id);
assert.ok(fed.energy > 200, 'A grown cell can store more energy than a size1 founder');
close(fed.energy, fed.capacity, 0.0003, 'Abundant food fills the size-dependent energy capacity over time');

// Unconditional growth reaches the size bound when actually supplied. Keep
// this fixture isolated from the local light competition tested separately.
const abundantGrowth = await fresh();
const abundantGenome = compileIn(abundantGrowth, `${growthSource} photosynthesize=1;`);
abundantGrowth.set_initial_nutrients(5);
abundantGrowth.reset_empty(31415, 0);
abundantGrowth.set_food_rate(0);
abundantGrowth.set_environment(0.5, 1, 0);
assert.equal(abundantGrowth.seed_cells(abundantGenome, 1, 600, 380), 1);
const abundantCell = cellsOf(snapshotOf(abundantGrowth))[0];
assert.equal(abundantGrowth.add_food(abundantCell.x, abundantCell.y, 1200), 1200);
const capped = advance(abundantGrowth, 4200);
assert.equal(capped[2], 1, 'The supplied cell survives its growth cycle');
assert.ok(cellsOf(capped).every(cell => Math.abs(cell.size - 4) < 0.000001), 'Growth stops at size4');
console.log('Growth WASM checks passed: legacy programs, paid/clamped growth, reactive size feedback, scaled maintenance, real feeding capacity, and size bounds.');

// Find a mutation in each category that changes the grow signal's observed
// behavior, then verify that exporting/recompiling preserves the whole result.
const growthMutator = await fresh();
const growthMutationSource = `${legacySource} grow = if size > 0.5 && size < 2 then 0.8 else 0;`;
const growthParent = compileIn(growthMutator, growthMutationSource);
assert.ok(growthParent);
growthMutator.reset(443, growthParent);
growthMutator.set_mutation_rate(0);
growthMutator.set_food_rate(0);
const unmutatedSize = cellsOf(advance(growthMutator, 30))[0].size;
let growthVariants = 0;
for (const kind of [1, 2, 3]) {
  let found = false;
  for (let seed = 1; seed <= 48; seed++) {
    const id = growthMutator.mutate_genome(growthParent, seed, kind);
    assert.ok(id, `Growth mutation succeeds: ${errorOf(growthMutator)}`);
    if (id === growthParent) continue;
    const text = sourceOf(growthMutator, id);
    assert.match(text, /\bgrow\s*=/, 'Exported variant includes its growth output');
    growthMutator.reset(443, id);
    growthMutator.set_mutation_rate(0);
    growthMutator.set_food_rate(0);
    const changed = advance(growthMutator, 30);
    if (Math.abs(cellsOf(changed)[0].size - unmutatedSize) < 0.000001) continue;
    const roundTrip = await worldFor(text, 443);
    assert.deepEqual(normalizedGenomes(advance(roundTrip, 30)), normalizedGenomes(changed), 'Recompiled mutant preserves growth and other behavior');
    assert.equal(sourceOf(growthMutator, growthParent), growthMutationSource, 'Growth mutation preserves the parent source');
    found = true;
    growthVariants++;
    break;
  }
  assert.ok(found, `Mutation category${kind} can alter growth while keeping a valid genome`);
}

const validator = await fresh();
const beforeManual = snapshot();
const originalSource = sourceOf(api, genome);
let variants = 0;
for (const kind of [1, 2, 3]) {
  let changed = 0;
  for (let seed = 1; seed <= 40; seed++) {
    const id = api.mutate_genome(genome, seed, kind);
    assert.ok(id, `Kind ${kind} mutation does not fail: ${errorOf(api)}`);
    if (id === genome) continue;
    changed++;
    variants++;
    assert.equal(api.genome_parent(id), genome, 'Mutation records its actual parent genome');
    assert.equal(api.genome_mutation_kind(id), kind, 'Mutation metadata reports the applied category');
    const text = sourceOf(api, id);
    assert.ok(compileIn(validator, text), `Every exported mutant compiles: ${errorOf(validator)}`);
    assert.equal(api.mutate_genome(genome, seed, kind), id, 'Same mutation seed and parent reuse the same variant');
    assert.equal(sourceOf(api, genome), originalSource, 'Mutation never rewrites the parent');
  }
  assert.ok(changed > 0, `Category ${kind} actually produces mutations`);
}
assert.deepEqual(snapshot(), beforeManual, 'Manual variants do not alter the running world');
assert.equal(api.mutation_count(), 0, 'Manual variants do not count as mutated births');
assert.equal(api.mutate_genome(0, 1, 0), 0, 'Unknown parent rejected');
assert.equal(api.mutate_genome(genome, 1, 999), 0, 'Unknown mutation category rejected');
assert.deepEqual(snapshot(), beforeManual, 'Rejected mutation requests leave the world unchanged');

// Compare the exact first division step with mutation disabled/enabled. Only
// daughter genotype IDs and kin-dependent prey observations may differ;
// biology, energy, positions, and every other field must remain unchanged.
const ordinary = await fresh();
const evolving = await fresh();
const divisionSource = 'thrust=0;turn=0;divide=true;grow=if size<2 then 1 else 0;photosynthesize=1;feed=0;invest.propulsion=0;invest.photosynthesis=1;invest.digestion=0;invest.armor=0;invest.sensing=0;invest.attack=0;';
const ordinaryId = compileIn(ordinary, divisionSource);
const evolvingId = compileIn(evolving, divisionSource);
ordinary.reset(2026, ordinaryId);
evolving.reset(2026, evolvingId);
ordinary.set_mutation_rate(0);
evolving.set_mutation_rate(1);
ordinary.set_food_rate(0); evolving.set_food_rate(0);
ordinary.set_environment(0.5,1,0); evolving.set_environment(0.5,1,0);
const noDivision = await worldFor(divisionSource.replace("divide=true", "divide=false"), 2026);
noDivision.set_environment(0.5,1,0);
for (let i = 0; i < 1800 && snapshotOf(ordinary)[4] === 0; i++) {
  ordinary.tick(1);
  evolving.tick(1);
  noDivision.tick(1);
}
const baselineDivision = snapshotOf(ordinary);
const mutatedDivision = snapshotOf(evolving);
assert.ok(baselineDivision[4] > 0, 'The comparison includes actual divisions');
close(cellsOf(baselineDivision).reduce((sum, cell) => sum + cell.size, 0), cellsOf(snapshotOf(noDivision)).reduce((sum, cell) => sum + cell.size, 0), 0.00001, 'Division conserves the biomass grown before splitting');
for (const daughter of cellsOf(baselineDivision).filter(cell => cell.parent)) {
  const parent = cellsOf(baselineDivision).find(cell => cell.id === daughter.parent);
  assert.ok(parent, 'The dividing parent remains in the world');
  assert.ok(daughter.size >= 1 && daughter.size < 1.01, 'A founder doubles before producing a size1 daughter');
  assert.equal(daughter.birthSize, daughter.size, 'Daughter birth mass resets the next division target');
  assert.equal(parent.birthSize, parent.size, 'Retained parent also needs another doubling');
  assert.equal(parent.size, daughter.size, 'Division splits biomass equally');
  assert.equal(parent.energy, daughter.energy, 'Division splits remaining energy equally');
}
assert.equal(evolving.mutation_count(), mutatedDivision[4], 'At 100%, every eligible daughter mutates');
assert.equal(ordinary.mutation_count(), 0, 'At 0%, inheritance is exact');
assert.equal(evolving.mutation_skipped_count(), 0, 'Simple programs admit mutation without skipped births');
for (let offset = HEADER_SIZE; offset < HEADER_SIZE + mutatedDivision[2] * CELL_STRIDE; offset += CELL_STRIDE) {
  const genotype = mutatedDivision[offset + 1];
  const parentCell = mutatedDivision[offset + 10];
  if (parentCell) {
    assert.notEqual(genotype, evolvingId, 'Daughter receives a variant');
    assert.equal(evolving.genome_parent(genotype), evolvingId, 'First generation retains parent provenance');
    assert.ok(compileIn(validator, sourceOf(evolving, genotype)), 'Daughter program is valid source');
  } else assert.equal(genotype, evolvingId, 'Founders keep the original genotype');
  mutatedDivision[offset + 1] = baselineDivision[offset + 1];
  // A newborn with a different genome is now preferred over a kin target in
  // read-only sensing. Those derived observations are not division physics.
  for (let field = 29; field <= 33; field++) mutatedDivision[offset + field] = baselineDivision[offset + field];
}
assert.deepEqual(mutatedDivision, baselineDivision, 'Mutation does not alter division energy or physics');
assert.equal(sourceOf(evolving, evolvingId), divisionSource);
assert.ok(cellsOf(advance(ordinary, 300)).every(cell => cell.generation <= 1), 'Daughters cannot divide again without growing');

// Evolve through multiple generations, then reset and repeat with the existing
// immutable genome registry. Historical IDs must not change seeded replay.
const evolutionGenome = compile(divisionSource);
assert.ok(evolutionGenome);
api.set_environment(0.5,1,0);
api.reset(77, evolutionGenome);
api.set_mutation_rate(0.4);
for (let i = 0; i < 30; i++) api.tick(120);
const evolvedFirst = snapshot();
const mutatedBirths = api.mutation_count();
const skippedBirths = api.mutation_skipped_count();
assert.ok(mutatedBirths > 0, 'Automatic mutation is active in a growing, light-supplied population');
for (let offset = HEADER_SIZE; offset < HEADER_SIZE + evolvedFirst[2] * CELL_STRIDE; offset += CELL_STRIDE) {
  const text = sourceOf(api, evolvedFirst[offset + 1]);
  assert.ok(compileIn(validator, text), 'Every living evolved genome remains editable and valid');
}
api.reset(77, evolutionGenome);
api.set_mutation_rate(0.4);
for (let i = 0; i < 30; i++) api.tick(120);
assert.deepEqual(snapshot(), evolvedFirst, 'Evolution repeats with the same seed and settings');
assert.equal(api.mutation_count(), mutatedBirths);
assert.equal(api.mutation_skipped_count(), skippedBirths);
api.set_mutation_rate(-1);
assert.equal(api.mutation_rate(), 0);
api.set_mutation_rate(2);
assert.equal(api.mutation_rate(), 1);
api.set_mutation_rate(Number.NaN);
assert.equal(api.mutation_rate(), 0);

// Saturated registries must retain a valid parent and expose why no new variant
// could be kept, instead of crashing or silently returning an invalid ID.
const bounded = await fresh();
const boundedParent = compileIn(bounded, divisionSource);
const firstVariant = bounded.mutate_genome(boundedParent, 33, 1);
assert.notEqual(firstVariant, boundedParent);
const limit = bounded.genome_limit();
assert.ok(limit > 0 && limit <= 2048, 'Genome storage has an explicit finite bound');
for (let i = 0; bounded.genome_count() < limit && i < limit; i++) {
  assert.ok(compileIn(bounded, `thrust = 0; turn = ${i + 10}; divide = true;`));
}
assert.equal(bounded.genome_count(), limit);
assert.equal(bounded.mutate_genome(boundedParent, 33, 1), firstVariant, 'Existing variants remain reusable at the limit');
assert.equal(bounded.mutate_genome(boundedParent, 999, 1), boundedParent, 'Capacity fallback inherits a correct parent');
assert.ok(errorOf(bounded).length, 'Manual capacity fallback explains the skipped change');
assert.equal(bounded.genome_count(), limit);
assert.equal(sourceOf(bounded, boundedParent), divisionSource);
bounded.reset(100, boundedParent);
bounded.set_mutation_rate(1);
bounded.set_environment(0.5,1,0);
advance(bounded,1800);
assert.ok(bounded.mutation_skipped_count() > 0, 'Automatic capacity fallback is visible in telemetry');
assert.equal(bounded.genome_count(), limit);
assert.ok(snapshotOf(bounded).every(Number.isFinite));
console.log(`Mutation WASM checks passed: ${variants} typed manual variants, ${growthVariants} growth-changing round trips, immutable parents, daughter inheritance, biomass/energy accounting, reproducible evolution, and explicit capacity fallback.`);

function memoryOf(module, cellId) {
  const ptr = module.cell_memory_ptr(cellId);
  const length = module.cell_memory_len();
  const entries = JSON.parse(new TextDecoder().decode(new Uint8Array(module.memory.buffer, ptr, length)));
  assert.ok(Array.isArray(entries), 'Memory inspection returns entries');
  return Object.fromEntries(entries.map(entry => [entry.name, entry.value]));
}
function environmentOf(module) {
  const ptr = module.environment_snapshot_ptr();
  const data = new Float32Array(module.memory.buffer, ptr, module.environment_snapshot_len()).slice();
  assert.equal(data.length, ENV_HEADER + data[0] * data[1] * PATCH_STRIDE + data[5] * 6, 'Environment snapshot layout');
  assert.ok(data.every(Number.isFinite));
  assert.ok(data[5] <= 16, 'Source count is bounded');
  return data;
}
function resourcesOf(module) {
  const state = snapshotOf(module);
  let resources = cellsOf(state).reduce((sum, cell) => sum + cell.energy + cell.gutEnergy + 80 * cell.size, 0);
  for (let n = HEADER_SIZE + state[2] * CELL_STRIDE; n < state.length; n += FOOD_STRIDE) resources += state[n + 2];
  return resources;
}

// Registers are typed, genuinely per-cell, and use the old state throughout a
// tick. False/zero values remain allocated until explicitly cleared.
const registerSource = `${legacySource}
ri001 = ri001 + 1;
rf001 = energy;
rb001 = false;
rs001 = "forage";
rv001 = vec(1, 2);`;
const registers = await worldFor(registerSource);
const registerBaseline = await worldFor(legacySource);
const registerInitial = snapshotOf(registers);
const registerIsolated = cellsOf(registerInitial).filter(c => c.foodDistance > 20 && c.neighborDistance > 20).map(c => c.id);
registers.tick(1); registerBaseline.tick(1);
for (const cell of cellsOf(snapshotOf(registers))) {
  const memory = memoryOf(registers, cell.id);
  assert.equal(memory.ri001, 1);
  assert.equal(memory.rb001, false);
  assert.equal(memory.rs001, 'forage');
  assert.deepEqual(memory.rv001, [1, 2]);
  assert.equal(cell.memoryUsed, 5, 'Populated false still consumes a register');
  close(cell.memoryCost, 5 * 0.0025, 0.000001, 'Memory upkeep is exposed');
}
const registerFirst = new Map(cellsOf(snapshotOf(registers)).map(c => [c.id, c]));
const baselineFirst = new Map(cellsOf(snapshotOf(registerBaseline)).map(c => [c.id, c]));
for (const id of registerIsolated) close(registerFirst.get(id).energy, baselineFirst.get(id).energy, 0.00002, 'New register writes are charged beginning next tick');
registers.tick(1); registerBaseline.tick(1);
const registerSecond = new Map(cellsOf(snapshotOf(registers)).map(c => [c.id, c]));
const baselineSecond = new Map(cellsOf(snapshotOf(registerBaseline)).map(c => [c.id, c]));
for (const id of registerIsolated) close(baselineSecond.get(id).energy - registerSecond.get(id).energy, 5 * 0.0025 * DT, 0.00003, 'Actual cell energy pays populated-register upkeep');
assert.equal(memoryOf(registers, 1).ri001, 2);
const seededRegisterId = compileIn(registers, registerSource);
registers.seed_cells(seededRegisterId, 1, 800, 500);
const newbornIntroduction = cellsOf(snapshotOf(registers)).at(-1);
assert.deepEqual(memoryOf(registers, newbornIntroduction.id), {}, 'Introduced cells start with independent empty memory');
registers.tick(1);
assert.equal(memoryOf(registers, newbornIntroduction.id).ri001, 1);
assert.equal(memoryOf(registers, 1).ri001, 3, 'One cell does not overwrite another cell’s memory');

const forwardRegisters = await worldFor(`${legacySource} ri001 = ri002 + 1; ri002 = ri001 + 10;`);
const reverseRegisters = await worldFor(`${legacySource} ri002 = ri001 + 10; ri001 = ri002 + 1;`);
for (let tick = 0; tick < 4; tick++) {
  forwardRegisters.tick(1); reverseRegisters.tick(1);
  assert.deepEqual(snapshotOf(forwardRegisters), snapshotOf(reverseRegisters), 'Rule order does not affect cell behavior');
  assert.deepEqual(memoryOf(forwardRegisters, 1), memoryOf(reverseRegisters, 1), 'Writes commit simultaneously');
  if (tick === 0) assert.deepEqual(memoryOf(forwardRegisters, 1), { ri001:1, ri002:10 });
}
const clears = await worldFor(`${legacySource} rb001 = false; clear rb001 when age >= 1 / 30;`);
clears.tick(1);
assert.deepEqual(memoryOf(clears, 1), { rb001:false });
clears.tick(1);
assert.deepEqual(memoryOf(clears, 1), {}, 'Clear wins over a matching write and deallocates false');
assert.equal(cellsOf(snapshotOf(clears))[0].memoryUsed, 0);
const defaults = await worldFor(`${legacySource} ri001=ri999; rf001=rf999; rb001=rb999; rs001=rs999; rv001=rv999;`);
defaults.tick(1);
assert.deepEqual(memoryOf(defaults, 1), { ri001:0, rf001:0, rb001:false, rs001:'', rv001:[0,0] });
for (const invalid of ['ri000=1;', 'ri1000=1;', 'ri01=1;', 'rb001=1;', 'ri001=1.5;', 'rs001=true;', 'rv001=1;', 'ri001=1; ri001=2;']) {
  assert.equal(compileIn(registers, legacySource + invalid), 0, `Reject invalid register program: ${invalid}`);
}
const textValue = 'quoted "text" \\ path\n<cell>&';
const stringRegister = await worldFor(`${legacySource} rs001=${JSON.stringify(textValue)};`);
stringRegister.tick(1);
assert.equal(memoryOf(stringRegister, 1).rs001, textValue, 'String serialization safely round-trips escapes and markup');
assert.ok(compileIn(registers, `${legacySource} rs001="${'ä'.repeat(64)}";`));
assert.equal(compileIn(registers, `${legacySource} rs001="${'ä'.repeat(65)}";`), 0, 'String bound counts UTF-8 bytes');

// Exercise every legal address in every bank without allocating memory for
// unpopulated cells; this also exceeds the original source and AST limits.
const fullBank = await fresh();
const bankStatements = Array.from({length:999}, (_, i) => {
  const suffix = String(i + 1).padStart(3, '0');
  return `ri${suffix}=${i};rf${suffix}=${i}.5;rb${suffix}=false;rs${suffix}="x";rv${suffix}=vec(0,0);`;
}).join('\n');
const bankId = compileIn(fullBank, `${legacySource}\n${bankStatements}`);
assert.ok(bankId, `All 999 addresses per primitive fit the new genome limits: ${errorOf(fullBank)}`);
fullBank.set_food_rate(0);
fullBank.seed_cells(bankId, 1, 600, 380);
fullBank.tick(1);
const fullMemory = memoryOf(fullBank, 1);
assert.equal(Object.keys(fullMemory).length, 5 * 999);
assert.equal(cellsOf(snapshotOf(fullBank))[0].memoryUsed, 4995);
assert.equal(fullMemory.ri999, 998);
assert.equal(fullMemory.rf999, 998.5);
const exactBudget = `let unused=0;thrust=${Array(4095).fill('1').join('+')};turn=0;divide=false;`;
assert.ok(compileIn(fullBank, exactBudget), `8192 expression nodes are accepted: ${errorOf(fullBank)}`);
assert.equal(compileIn(fullBank, `thrust=${Array(4096).fill('1').join('+')};turn=0;divide=false;`), 0, '8193 expression nodes are rejected');
assert.ok(compileIn(fullBank, `#${'x'.repeat(20_000)}\n${legacySource}`), 'Source can exceed the former16KiB limit');
assert.equal(fullBank.alloc(262_145), 0, 'WASM allocation enforces the256KiB source boundary');
assert.ok(fullBank.genome_bytes() > 0 && fullBank.genome_bytes() <= fullBank.genome_byte_limit());

const inheritance = await worldFor(divisionSource + 'ri001=ri001+1;', 2026);
inheritance.set_environment(0.5,1,0);
for (let tick = 0; tick < 1800 && snapshotOf(inheritance)[4] === 0; tick++) inheritance.tick(1);
const inheritanceState = snapshotOf(inheritance);
assert.ok(inheritanceState[4] > 0);
for (const cell of cellsOf(inheritanceState).filter(c => c.parent)) {
  assert.deepEqual(memoryOf(inheritance, cell.id), {}, 'Daughters start with empty registers');
  assert.ok(memoryOf(inheritance, cell.parent).ri001 > 0, 'Parent retains its register state');
}
inheritance.reset(2026, 1);
assert.deepEqual(memoryOf(inheritance, 1), {}, 'Reset clears per-cell memory');

// Mutation must preserve future state transitions as well as immediate outputs.
const stateMutations = await fresh();
const stateParent = compileIn(stateMutations, `${legacySource}
ri001=ri001+1;rf001=energy.fraction;rb001=food.visible;
rs001=if rb001 then "seek" else "wait";rv001=vec(flow.x,flow.y);
clear ri001 when ri001 > 20;`);
assert.ok(stateParent);
let stateVariants = 0;
for (const kind of [1,2,3]) {
  let changes = 0;
  for (let seed = 1; seed <= 12; seed++) {
    const id = stateMutations.mutate_genome(stateParent, seed, kind);
    assert.ok(id, errorOf(stateMutations));
    if (id === stateParent) continue;
    changes++; stateVariants++;
    const text = sourceOf(stateMutations, id);
    const exported = await worldFor(text, 291);
    stateMutations.reset(291, id); stateMutations.set_mutation_rate(0); stateMutations.set_food_rate(0);
    for (let tick = 0; tick < 6; tick++) {
      stateMutations.tick(1); exported.tick(1);
      assert.deepEqual(normalizedGenomes(snapshotOf(stateMutations)), normalizedGenomes(snapshotOf(exported)), 'Exported mutant preserves actions and metabolic state');
      assert.deepEqual(memoryOf(stateMutations, 1), memoryOf(exported, 1), 'Exported mutant preserves next-state register updates');
    }
  }
  assert.ok(changes > 0, `Register program admits mutation category ${kind}`);
}
console.log(`Register WASM checks passed: all5 primitive banks/999 addresses, simultaneous writes, clear priority, upkeep, inheritance, 8192-node budget, and ${stateVariants} mutated transition round trips.`);

// Environment sources and settings persist across deterministic world resets.
const environment = await worldFor(legacySource);
environment.set_environment(0.5, 0, 12);
const lamp = environment.add_source(0, 600, 380, 1, 180);
const heater = environment.add_source(1, 300, 380, 1, 200);
const vortex = environment.add_source(2, 900, 380, 0.5, 200);
assert.ok(lamp && heater && vortex);
advance(environment, 60);
const environmentRun = environmentOf(environment);
const environmentWorld = snapshotOf(environment);
assert.equal(environmentRun[5], 3);
let brightest = 0, hottest = 0;
for (let i = ENV_HEADER; i < ENV_HEADER + environmentRun[0] * environmentRun[1] * PATCH_STRIDE; i += PATCH_STRIDE) {
  brightest = Math.max(brightest, environmentRun[i]);
  hottest = Math.max(hottest, environmentRun[i + 1]);
}
assert.ok(brightest > 0.5, 'Light sources illuminate the field');
assert.ok(hottest > 0.5, 'Heat persists and diffuses through the field');
environment.reset(31415, 1); environment.set_food_rate(0);
advance(environment, 60);
assert.deepEqual(environmentOf(environment), environmentRun, 'Environment replay is deterministic');
assert.deepEqual(snapshotOf(environment), environmentWorld, 'Configured environment participates in replay');
assert.equal(environment.remove_source(lamp), 1);
assert.equal(environment.remove_source(lamp), 0);
environment.clear_sources();
assert.equal(environmentOf(environment)[5], 0);
for (let i = 0; i < 16; i++) assert.ok(environment.add_source(0, i * 60, 100, 0.2, 100));
assert.equal(environment.add_source(0, 100, 100, 1, 100), 0, 'Source capacity is explicit');

const floatingFood = await fresh();
floatingFood.set_food_rate(0); floatingFood.set_environment(0.5, 0, 20);
floatingFood.add_food(600, 380, 1);
const stillFood = snapshotOf(floatingFood);
floatingFood.tick(30);
assert.notDeepEqual(snapshotOf(floatingFood).slice(HEADER_SIZE), stillFood.slice(HEADER_SIZE), 'Food is advected even without cells');
const photosynth = await worldFor(`${legacySource} photosynthesize=1;`);
const darkPhoto = await worldFor(`${legacySource} photosynthesize=1;`);
photosynth.set_environment(0.5, 1, 0);
const litState = advance(photosynth, 90);
const darkState = advance(darkPhoto, 90);
assert.ok(photosynth.photosynthesis_total() > 0);
assert.equal(darkPhoto.photosynthesis_total(), 0);
assert.ok(cellsOf(litState).reduce((s,c)=>s+c.energy,0) > cellsOf(darkState).reduce((s,c)=>s+c.energy,0), 'Photosynthesis actually changes energy');

const hunters = await fresh();
const preyId = compileIn(hunters, legacySource + 'feed=0;');
const hunterId = compileIn(hunters, 'thrust=0;turn=0;divide=false;bite=1;engulf=true;feed=1;invest.propulsion=0;invest.photosynthesis=0;invest.digestion=0.4;invest.armor=0.1;invest.sensing=0.1;invest.attack=0.4;');
assert.ok(preyId && hunterId);
hunters.set_food_rate(0); hunters.set_mutation_rate(0);
hunters.seed_cells(preyId, 14, 600, 380); hunters.seed_cells(hunterId, 14, 600, 380);
const resourcesBeforeHunt = resourcesOf(hunters);
advance(hunters, 900);
assert.ok(hunters.attack_count() > 0 && hunters.predation_count() > 0, 'Dedicated attack tissue can wound and kill prey through the browser API');
assert.ok(resourcesOf(hunters) <= resourcesBeforeHunt + 0.002, 'Growth, predation, gut processing, and remains cannot create resource energy');
const harmless = await fresh();
const harmlessId = compileIn(harmless, 'thrust=0;turn=0;divide=false;bite=1;engulf=true;');
harmless.set_food_rate(0); harmless.seed_cells(harmlessId, 28, 600, 380);
advance(harmless, 180);
assert.equal(harmless.attack_count(),0,'Biting cannot borrow attack capacity from digestion');
assert.equal(harmless.predation_count(),0,'Engulf output alone cannot bypass the attack requirement');
const fighters = await fresh();
const fighterId = compileIn(fighters, 'thrust=0;turn=0;divide=false;bite=1;defend=0.2;repair=0.1;invest.attack=0.4;');
assert.ok(fighterId);
fighters.set_food_rate(0); fighters.seed_cells(fighterId, 28, 600, 380);
advance(fighters, 180);
assert.ok(fighters.attack_count() > 0, 'Contact bite attacks resolve');
assert.ok(cellsOf(snapshotOf(fighters)).every(c => c.integrity > 0 && c.integrity <= 1), 'Dead or invalid membrane state is removed');
console.log('Environment WASM checks passed: editable bounded sources, thermal fields, current-driven food, photosynthesis, deterministic reset, predation accounting, and contact attacks.');

// The ABI exposes a conserved material ledger separately from dissipating
// energy. User additions count as imports; internal transfers never do.
function checkMinerals(module, label) {
  const water = module.nutrient_dissolved_total();
  const food = module.nutrient_food_total();
  const cells = module.nutrient_cell_total();
  const total = module.nutrient_total();
  assert.ok([water, food, cells, total].every(value => Number.isFinite(value) && value >= 0), label);
  close(total, water + food + cells, 1e-7, `${label}: compartments sum to the total`);
  close(total, module.nutrient_input_total(), 1e-6, `${label}: no minerals created or destroyed`);
  const state = snapshotOf(module);
  const cellMinerals = cellsOf(state).reduce((sum, cell) => sum + cell.size + cell.nutrientStored + cell.gutNutrients, 0);
  let foodMinerals = 0;
  for (let n = HEADER_SIZE + state[2] * CELL_STRIDE; n < state.length; n += FOOD_STRIDE) {
    assert.ok(state[n + 3] >= 0);
    foodMinerals += state[n + 3];
  }
  close(cellMinerals, cells, 0.0001, `${label}: packed cell minerals`);
  close(foodMinerals, food, 0.0001, `${label}: packed food minerals`);
}
const minerals = await worldFor(`${legacySource} grow=if size<2 then 1 else 0; photosynthesize=1;`);
close(minerals.nutrient_total(), 60 * 38 * 0.3 + 22 * 1.5 + 360 * 0.25, 1e-7, 'Initial mineral ledger includes water, cells and food');
checkMinerals(minerals, 'Initial ledger');
const materialBeforeRecipe = minerals.nutrient_total();
minerals.set_initial_nutrients(1);
assert.equal(minerals.nutrient_total(), materialBeforeRecipe, 'Initial-density setting affects the next reset only');
assert.equal(environmentOf(minerals)[6], 1);
assert.equal(minerals.add_nutrients(-1, -1, 12.5), 12.5, 'Uniform fertilizer returns the actual imported mass');
assert.equal(minerals.add_nutrients(600, 380, Number.NaN), 0, 'Invalid fertilizer cannot poison the ledger');
assert.equal(minerals.add_nutrients(600, 380, -3), 0);
const importedCells = minerals.seed_cells(1, 2, 600, 380);
const importedFood = minerals.add_food(600, 380, 3);
close(minerals.nutrient_total(), materialBeforeRecipe + 12.5 + 1.5 * importedCells + 0.25 * importedFood, 1e-7, 'Only actual introductions add mineral mass');
minerals.set_environment(0.5, 0.8, 24);
const spring = minerals.add_source(3, 500, 380, 0.4, 120);
assert.ok(spring, 'Nutrient springs are part of the source ABI');
assert.equal(minerals.add_source(4, 500, 380, 1, 120), 0, 'Unknown source kinds remain rejected');
const inputsBeforeSpring = minerals.nutrient_input_total();
advance(minerals, 360);
close(minerals.nutrient_input_total() - inputsBeforeSpring, 1.5 * 0.4 * 12, 0.000002, 'Spring strength sets total mineral input per second');
checkMinerals(minerals, 'Transport, uptake, growth and decay');
assert.equal(minerals.remove_source(spring), 1);
const inputsWithoutSpring = minerals.nutrient_input_total();
advance(minerals, 120);
assert.equal(minerals.nutrient_input_total(), inputsWithoutSpring, 'Removing a spring stops new mineral imports');
checkMinerals(minerals, 'Closed recycling after removing a spring');
minerals.reset(31415, 1);
close(minerals.nutrient_dissolved_total(), 60 * 38, 1e-7, 'Reset applies the next starting density');
close(minerals.nutrient_input_total(), 60 * 38 + 22 * 1.5 + 360 * 0.25, 1e-7, 'Reset restarts the input ledger');
checkMinerals(minerals, 'Reset ledger');

const mineralLimited = await worldFor(`${legacySource} absorb=0;grow=1;photosynthesize=1;feed=0;`, 31415, 0);
mineralLimited.set_environment(0.5,1,0);
const isolatedMineralCells = cellsOf(snapshotOf(mineralLimited)).filter(cell => cell.foodDistance > 30 && cell.neighborDistance > 40).map(cell => cell.id);
assert.ok(isolatedMineralCells.length);
advance(mineralLimited, 1200);
for (const cell of cellsOf(snapshotOf(mineralLimited)).filter(cell => isolatedMineralCells.includes(cell.id))) {
  close(cell.size, 1.5, 0.000002, 'Growth cannot exceed the available half-unit mineral reserve');
  close(cell.nutrientStored, 0, 0.000002, 'Growth binds the reserve into the body');
  assert.equal(cell.uptakeRate, 0, 'absorb=0 disables water uptake');
}
checkMinerals(mineralLimited, 'Nutrient-limited growth');

const bodyOutputs = (p, l, d, a, s, attack = 0) => `invest.propulsion=${p};invest.photosynthesis=${l};invest.digestion=${d};invest.armor=${a};invest.sensing=${s};invest.attack=${attack};`;
const motorText = 'thrust=1;turn=0;divide=false;absorb=0;rf001=body.propulsion;rf002=sense.range;';
const balancedMotor = await worldFor(motorText);
const investedMotor = await worldFor(motorText + bodyOutputs(1, 0, 0, 0, 0));
const motorIds = cellsOf(snapshotOf(balancedMotor)).filter(cell => cell.foodDistance > 30 && cell.neighborDistance > 40).map(cell => cell.id);
assert.ok(motorIds.length);
const balancedStep = new Map(cellsOf(advance(balancedMotor, 1)).map(cell => [cell.id, cell]));
const investedStep = new Map(cellsOf(advance(investedMotor, 1)).map(cell => [cell.id, cell]));
for (const id of motorIds) {
  const base = balancedStep.get(id), changed = investedStep.get(id);
  assert.ok(Math.hypot(changed.vx,changed.vy) > Math.hypot(base.vx,base.vy),'Founders start with the capabilities requested by their program');
  close(changed.body[0],1,0.000001,'A motor founder uses its normalized starting body');
  assert.equal(changed.remodelingRate,0,'Initial specialization is not a paid remodel');
  close(memoryOf(investedMotor,id).rf001,0.2,1e-12,'Initial register reads still use the pre-step body');
}
const changingMotor = await worldFor(motorText + bodyOutputs('if age < 1 / 30 then 0.2 else 1','if age < 1 / 30 then 0.2 else 0','if age < 1 / 30 then 0.2 else 0','if age < 1 / 30 then 0.2 else 0','if age < 1 / 30 then 0.2 else 0'));
advance(changingMotor,1);
balancedMotor.tick(1); changingMotor.tick(1);
const changedSecond = new Map(cellsOf(snapshotOf(changingMotor)).map(cell=>[cell.id,cell]));
for (const base of cellsOf(snapshotOf(balancedMotor)).filter(cell=>motorIds.includes(cell.id))) {
  const changed=changedSecond.get(base.id);
  assert.deepEqual([changed.x,changed.y,changed.vx,changed.vy],[base.x,base.y,base.vx,base.vy],'An existing cell cannot benefit from a remodel during the same tick');
  close(base.energy-changed.energy,12*0.08*DT,0.00003,'Changing the body pays remodeling energy');
  close(changed.body[0],0.2+0.08*DT,0.000001,'Later remodeling is rate limited');
  close(changed.remodelingRate,0.08,0.000001,'Remodeling telemetry reports actual biomass per second');
  close(memoryOf(changingMotor,base.id).rf001,0.2,1e-12,'Registers observe the old allocation');
}
balancedMotor.tick(1);changingMotor.tick(1);
const changedThird = new Map(cellsOf(snapshotOf(changingMotor)).map(cell=>[cell.id,cell]));
for (const base of cellsOf(snapshotOf(balancedMotor)).filter(cell=>motorIds.includes(cell.id))) {
  const changed=changedThird.get(base.id);
  assert.ok(Math.hypot(changed.vx,changed.vy)>Math.hypot(base.vx,base.vy),'Paid remodeling improves motion on the following tick');
  close(memoryOf(changingMotor,base.id).rf001,changedSecond.get(base.id).body[0],0.000001,'Next evaluation reads the completed allocation');
}
const normalizedBody = await worldFor(`${legacySource} absorb=0;${bodyOutputs(1, 1, 1, 1, 1)}`);
const balancedBody = await worldFor(`${legacySource} absorb=0;`);
assert.deepEqual(advance(normalizedBody, 60), advance(balancedBody, 60), 'Over-budget targets normalize to one shared body');
const coreBody = await worldFor(`${legacySource} absorb=0;${bodyOutputs(0, 0, 0, 0, 0)}`);
for (const cell of cellsOf(advance(coreBody, 390))) {
  assert.ok(cell.body.every(fraction => fraction < 0.000001), 'All-zero targets leave unspecialized core');
  close(cell.metabolism, 0.35 * cell.size, 0.000001, 'Core still has a positive basal cost');
  close(cell.senseRange, 40, 0.00002, 'Unspecialized cells retain only basic sensing');
}
checkMinerals(coreBody, 'Body remodeling preserves mineral mass');
console.log('Nutrient/body WASM checks passed: ABI7, compartment accounting, explicit inputs, spring/reset behavior, mineral-limited growth, founder specialization, paid delayed remodeling, frozen body sensors, shared targets, and core upkeep.');

// Structural inspection must expose the compiler's graph without executing it
// or creating same-tick feedback edges between register writes and reads.
function graphOf(module, id) {
  const ptr = module.genome_graph_ptr(id);
  const len = module.genome_graph_len();
  assert.ok(ptr && len, `Genome graph is available: ${errorOf(module)}`);
  const graph = JSON.parse(new TextDecoder().decode(new Uint8Array(module.memory.buffer, ptr, len)));
  assert.equal(graph.version, 1);
  assert.ok(graph.nodes.length <= 8192);
  const types = new Set(['integer', 'scalar', 'boolean', 'string', 'vec2']);
  for (const [id, node] of graph.nodes.entries()) {
    assert.equal(node.id, id, 'Compiler node IDs are stable topological indexes');
    assert.ok(types.has(node.type));
    assert.equal(typeof node.label, 'string');
    assert.ok(node.bindings.every(name => typeof name === 'string'));
    assert.ok(node.inputs.every(input => Number.isInteger(input) && input >= 0 && input < id), 'Edges preserve acyclic dependencies');
    if (node.kind === 'register' || node.kind === 'populated') assert.deepEqual(node.inputs, [], 'Old-memory reads are graph leaves');
  }
  assert.equal(new Set(graph.roots.map(root => root.id)).size, graph.roots.length);
  assert.equal(graph.roots.filter(root => root.kind === 'output').length, 17);
  const pending = [];
  for (const root of graph.roots) {
    assert.ok(types.has(root.type));
    for (const id of [root.value, root.guard]) {
      if (id === null) continue;
      assert.ok(Number.isInteger(id) && id >= 0 && id < graph.nodes.length);
      pending.push(id);
    }
    if (root.guard !== null) assert.equal(graph.nodes[root.guard].type, 'boolean');
  }
  const reachable = new Set();
  while (pending.length) {
    const id = pending.pop();
    if (reachable.has(id)) continue;
    reachable.add(id); pending.push(...graph.nodes[id].inputs);
  }
  for (const node of graph.nodes) assert.equal(node.reachable, reachable.has(node.id), 'Unused nodes remain visible in the budget without becoming output dependencies');
  return graph;
}

const graphText = '<cell data-x="quoted">&\\\n';
const graphWorld = await worldFor(`
let unused = 99;
let fuel = energy;
let alias = fuel;
ri001 = ri001 + 1 when !food.visible;
clear ri001 when food.visible;
rs001 = ${JSON.stringify(graphText)};
thrust = if rb001 then 0.8 else 0.2;
turn = clamp(alias - fuel, -1, 1);
divide = false;`);
const graphBefore = snapshotOf(graphWorld);
const graphCountBefore = graphWorld.genome_count();
const structuralGraph = graphOf(graphWorld, 1);
assert.deepEqual(snapshotOf(graphWorld), graphBefore, 'Inspecting a graph cannot advance or alter the world');
assert.deepEqual(memoryOf(graphWorld, 1), {}, 'Graph inspection never executes register writes');
assert.equal(graphWorld.genome_count(), graphCountBefore, 'Graph inspection does not register another genome');
assert.ok(structuralGraph.nodes.some(node => node.bindings.includes('unused') && !node.reachable));
assert.ok(structuralGraph.nodes.some(node => node.bindings.includes('fuel')));
assert.ok(structuralGraph.nodes.some(node => node.bindings.includes('alias')));
assert.ok(structuralGraph.nodes.some(node => node.type === 'string' && node.label.includes('<cell')));
const writeRoot = structuralGraph.roots.find(root => root.id === 'write:ri001');
const clearRoot = structuralGraph.roots.find(root => root.id === 'clear:ri001');
assert.ok(writeRoot && clearRoot && writeRoot.guard !== null && clearRoot.guard !== null);
assert.equal(clearRoot.value, null);
assert.equal(writeRoot.type, 'integer');
assert.equal(structuralGraph.roots.find(root => root.id === 'output:photosynthesize').default, '0');
assert.equal(structuralGraph.roots.find(root => root.id === 'output:engulf').default, 'false');
assert.equal(structuralGraph.roots.find(root => root.id === 'output:absorb').default, '1');
assert.equal(structuralGraph.roots.find(root => root.id === 'output:feed').default, '1');
assert.equal(structuralGraph.roots.find(root => root.id === 'output:invest.attack').default, '0');
for (const ability of ['propulsion', 'photosynthesis', 'digestion', 'armor', 'sensing']) {
  assert.equal(structuralGraph.roots.find(root => root.id === `output:invest.${ability}`).default, '0.2');
}
const choice = structuralGraph.nodes.find(node => node.kind === 'conditional');
assert.equal(structuralGraph.nodes[choice.inputs[0]].label, 'rb001', 'Conditional operands retain condition/then/else order');
assert.equal(structuralGraph.nodes[choice.inputs[1]].label, '0.8');
assert.equal(structuralGraph.nodes[choice.inputs[2]].label, '0.2');
assert.equal(graphOf(fullBank, compileIn(fullBank, exactBudget)).nodes.length, 8192);
assert.equal(graphOf(fullBank, bankId).roots.filter(root => root.kind === 'write').length, 4995);
const graphMutation = graphWorld.mutate_genome(1, 123, 3);
assert.ok(graphMutation);
graphOf(graphWorld, graphMutation);
assert.equal(graphWorld.genome_graph_ptr(0), 0);
assert.equal(graphWorld.genome_graph_len(), 0);
assert.match(errorOf(graphWorld), /unknown genome/i);
assert.deepEqual(snapshotOf(graphWorld), graphBefore, 'Invalid graph IDs leave the world intact');
console.log('Genome graph WASM checks passed: compiler dependencies, typed roots, register tick boundaries, escaping, unused nodes, full-size graphs, and non-destructive inspection.');

// Checkpoints retain f64 state and exact ASTs, unlike the rendering snapshots.
for (const name of ['reset_empty', 'alloc_state', 'save_state_ptr', 'save_state_len', 'restore_state']) {
  assert.equal(typeof api[name], 'function', `Missing checkpoint export: ${name}`);
}
function saveStateOf(module) {
  const ptr = module.save_state_ptr();
  const len = module.save_state_len();
  assert.ok(ptr && len >= 20 && len <= 128 * 1024 * 1024, errorOf(module));
  return new Uint8Array(module.memory.buffer, ptr, len).slice();
}
function restoreStateIn(module, state) {
  const ptr = module.alloc_state(state.length);
  assert.ok(ptr, 'Checkpoint buffer can be allocated');
  try {
    new Uint8Array(module.memory.buffer, ptr, state.length).set(state);
    return module.restore_state(ptr, state.length);
  } finally { module.dealloc(ptr, state.length); }
}
const checkpointWorld = await worldFor(`ri001=ri001+1;rf001=food.angle;rb001=food.visible;rs001="alive";rv001=vec(flow.x,flow.y);${source} photosynthesize=1;`);
checkpointWorld.set_environment(0.5, 0.9, 7);
checkpointWorld.add_source(1, 900, 300, -0.3, 140);
checkpointWorld.add_source(2, 200, 400, 0.5, 180);
checkpointWorld.add_source(3, 600, 400, 0.8, 210);
checkpointWorld.set_food_rate(30);
checkpointWorld.set_mutation_rate(1);
let checkpointMutant = 1;
for (let i = 1; i < 12; i++) checkpointMutant = checkpointWorld.mutate_genome(checkpointMutant, i, 2);
checkpointWorld.seed_cells(checkpointMutant, 2, -1, -1);
advance(checkpointWorld, 120);
const savedCheckpoint = saveStateOf(checkpointWorld);
const resumedWorld = await fresh();
assert.equal(restoreStateIn(resumedWorld, savedCheckpoint), 1, errorOf(resumedWorld));
assert.deepEqual(saveStateOf(resumedWorld), savedCheckpoint, 'Checkpoint bytes roundtrip exactly across WASM instances');
assert.deepEqual(graphOf(resumedWorld, checkpointMutant), graphOf(checkpointWorld, checkpointMutant), 'Mutated AST structure is preserved');
assert.deepEqual(advance(resumedWorld, 600), advance(checkpointWorld, 600), 'Restored world continues identically');
assert.deepEqual(saveStateOf(resumedWorld), saveStateOf(checkpointWorld), 'Hidden state, fields, registers, random sequence, and lineage continue identically');
const beforeBadCheckpoint = saveStateOf(resumedWorld);
const corruptedCheckpoint = savedCheckpoint.slice(); corruptedCheckpoint[corruptedCheckpoint.length - 1] ^= 0xff;
assert.equal(restoreStateIn(resumedWorld, corruptedCheckpoint), 0);
assert.match(errorOf(resumedWorld), /checksum/i);
assert.deepEqual(saveStateOf(resumedWorld), beforeBadCheckpoint, 'Invalid checkpoint leaves the active world intact');
assert.equal(resumedWorld.alloc_state(128 * 1024 * 1024 + 1), 0, 'Checkpoint allocation limit is enforced');
resumedWorld.reset_empty(42, 23);
assert.equal(snapshotOf(resumedWorld)[2], 0, 'Fresh worlds can start empty');
assert.equal(snapshotOf(resumedWorld)[3], 23, 'Requested starting food count is exact');
assert.equal(resumedWorld.seed_cells(1, 1, -1, -1), 1, 'Adding one organism creates exactly one cell');
assert.equal(snapshotOf(resumedWorld)[2], 1);
assert.equal(snapshotOf(resumedWorld)[11], 30, 'Empty reset preserves food supply');
console.log('Checkpoint WASM checks passed: exact cross-instance continuation, retained mutation ASTs, memory/fields/RNG/lineage, atomic corruption rejection, size bounds, empty reset, and single-organism addition.');

// Populations are constrained by resources and available memory, not a fixed
// engine/UI quota. Exercise the real f64 WASM input and checkpoint boundary.
const uncappedWorld = await fresh();
const uncappedGenome = compileIn(uncappedWorld, 'rf001=food.energy;rf002=food.nutrients;thrust=0;turn=0;grow=0;divide=false;');
assert.ok(uncappedGenome > 0, errorOf(uncappedWorld));
uncappedWorld.reset_empty(2026, 0);
uncappedWorld.set_food_rate(0);
assert.equal(uncappedWorld.seed_cells(uncappedGenome, 512, -1, -1), 512, 'Seeding is not truncated at the former384 limit');
assert.equal(advance(uncappedWorld, 60)[2], 512, 'A population above384 advances normally');
assert.equal(memoryOf(uncappedWorld, 1).rf001, 0, 'Invisible food has zero energy');
assert.equal(memoryOf(uncappedWorld, 1).rf002, 0, 'Invisible food has zero minerals');
const uncappedCheckpoint = saveStateOf(uncappedWorld);
for (const count of [-1, 0.5, NaN, Infinity, 2 ** 32]) {
  assert.equal(uncappedWorld.seed_cells(uncappedGenome, count, -1, -1), 0, `Invalid count ${count} is rejected before WASM integer coercion`);
  assert.ok(errorOf(uncappedWorld).length, 'Invalid counts report a visible reason');
  assert.deepEqual(saveStateOf(uncappedWorld), uncappedCheckpoint, 'A rejected count leaves the world unchanged');
}
const uncappedRestore = await fresh();
assert.equal(restoreStateIn(uncappedRestore, uncappedCheckpoint), 1, errorOf(uncappedRestore));
assert.equal(snapshotOf(uncappedRestore)[2], 512, 'Checkpoints retain all organisms above the former limit');
assert.deepEqual(advance(uncappedRestore, 120), advance(uncappedWorld, 120), 'Larger populations resume deterministically');
console.log('Population WASM checks passed:512 living cells, continued simulation, exact save/resume, food-value signals, and atomic invalid-count rejection.');

for (const name of ['analyze_genome', 'genome_analysis_len']) assert.equal(typeof api[name], 'function', `Missing analysis export: ${name}`);
function analyzeBytes(module, encoded) {
  const size = encoded.length > 262144 ? 0 : Math.max(1, encoded.length);
  const ptr = size ? module.alloc(size) : 0;
  if (size) assert.ok(ptr);
  try {
    if (ptr) new Uint8Array(module.memory.buffer, ptr, encoded.length).set(encoded);
    const result = module.analyze_genome(ptr, encoded.length);
    assert.ok(result);
    return JSON.parse(new TextDecoder().decode(new Uint8Array(module.memory.buffer, result, module.genome_analysis_len())));
  } finally { if (ptr) module.dealloc(ptr, size); }
}
const analyzeIn = (module, text) => analyzeBytes(module, new TextEncoder().encode(text));
const unicodeAnalysisSource = '# café 🌱\r\nlet label="é🌱";let heading=vec(flow.x,flow.y);let count=ri001+1;let ready=food.visible;let fuel=energy*0.5;thrust=fuel;turn=x(heading);divide=ready;';
const originalAnalysisState = saveStateOf(checkpointWorld);
const originalAnalysisCount = checkpointWorld.genome_count(), originalAnalysisBudget = checkpointWorld.genome_bytes();
checkpointWorld.genome_source_ptr(0);
const originalAnalysisError = errorOf(checkpointWorld);
const analysis = analyzeIn(checkpointWorld, unicodeAnalysisSource);
assert.equal(analysis.valid, true);
assert.deepEqual(analysis.diagnostics, []);
assert.equal(analysis.sourceBytes, Buffer.byteLength(unicodeAnalysisSource));
assert.deepEqual(analysis.bindings.map(binding => [binding.name, binding.type]), [['label','string'],['heading','vec2'],['count','integer'],['ready','boolean'],['fuel','scalar']]);
for (const binding of analysis.bindings) {
  assert.equal(binding.from, unicodeAnalysisSource.indexOf('let '+binding.name)+4);
  assert.equal(unicodeAnalysisSource.slice(binding.from,binding.to),binding.name, 'Declaration spans use UTF-16 after emoji/comments');
}
assert.ok(analysis.nodeCount > 0 && analysis.nodeCount <= 8192);
for (const text of ['', '# 🌱', 'let incomplete =', 'let label="é🌱";thrust=', 'thrust=0;turn=0;\r\n']) {
  const result = analyzeIn(checkpointWorld,text);
  assert.equal(result.valid,false);
  assert.equal(result.diagnostics.length,1);
  assert.equal(result.diagnostics[0].from,text.length);
  assert.equal(result.diagnostics[0].to,text.length);
  assert.equal(result.diagnostics[0].line,text.split('\n').length);
  assert.equal(result.diagnostics[0].column,text.length-text.lastIndexOf('\n'));
}
const invalidTypedSource = '# é🌱\r\nlet label="🌱";let prior=1;let wrong=prior+false;thrust=0;turn=0;divide=false;';
const invalidTypedAnalysis = analyzeIn(checkpointWorld,invalidTypedSource);
assert.deepEqual(invalidTypedAnalysis.bindings.map(binding=>binding.name),['label','prior']);
const typedError = invalidTypedAnalysis.diagnostics[0];
assert.equal(typedError.from,invalidTypedSource.indexOf('+'));
assert.equal(typedError.to,typedError.from+1);
assert.equal(typedError.column,typedError.from-invalidTypedSource.lastIndexOf('\n',typedError.from));
assert.equal(typedError.severity,'error');
assert.match(typedError.message,/cannot combine/);
for (const text of ['# 🌱\nλ', 'let text="é🌱', 'let text="\\🌱']) {
  const result=analyzeIn(checkpointWorld,text);
  assert.equal(result.valid,false);
  assert.equal(result.diagnostics[0].to,text.length,'Unicode lexical failures end at complete UTF-16 character boundaries');
}
assert.equal(analyzeBytes(checkpointWorld,new Uint8Array([0xff])).valid,false);
assert.equal(analyzeIn(checkpointWorld,' '.repeat(262145)).sourceBytes,262145);
assert.equal(errorOf(checkpointWorld),originalAnalysisError,'Analysis preserves the previous engine error');
assert.equal(checkpointWorld.genome_count(),originalAnalysisCount);
assert.equal(checkpointWorld.genome_bytes(),originalAnalysisBudget);
assert.deepEqual(saveStateOf(checkpointWorld),originalAnalysisState,'Valid/invalid analysis does not change any saved world, memory, RNG, or registry state');
const analysisContinuation=await fresh();
assert.equal(restoreStateIn(analysisContinuation,originalAnalysisState),1);
assert.deepEqual(advance(checkpointWorld,60),advance(analysisContinuation,60),'Analysis cannot alter subsequent simulated behavior');
assert.deepEqual(saveStateOf(checkpointWorld),saveStateOf(analysisContinuation));
const fullRegistryState=saveStateOf(bounded);
assert.equal(analyzeIn(bounded,unicodeAnalysisSource).valid,true,'Analysis works even when no new genome can be registered');
assert.deepEqual(saveStateOf(bounded),fullRegistryState);
assert.equal(analyzeIn(fullBank,exactBudget).nodeCount,8192,'Analysis accepts the full expression budget');
console.log('Genome analysis WASM checks passed: compiler types/errors, UTF-16 Unicode/EOF ranges, partial bindings, full budgets, and exact non-mutation of simulation/registry/error/RNG state.');

// Exercise build-contract guards and the real frontend loader independently.
await checkEngineLoader();
