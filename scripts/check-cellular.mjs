import {prepareBloom, placeByTissue} from './feeding-culture.mjs';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fresh as newWorld, compile, seed, snapshot, advance, graph, body, evaluation, save, restore, source, error, catalog } from './organelle-harness.mjs';

const { PRESETS, CELLULAR_FAMILY_PRESETS, MECHANISM_PRESETS_V6 } = await catalog();
const countChemistry = !process.argv.includes('--continuous');
const fresh = config => newWorld({ ...config, countChemistry });

const conserved = state => {
  assert.ok(Math.abs(state.material - state.materialIntroduced) < Math.max(1e-7, state.materialIntroduced * 1e-10), 'Material transfers remain conserved');
  assert.ok(state.energy <= state.energyIntroduced + state.photosynthesis + 1e-7, 'Fabrication and metabolism cannot create energy');
};

function assertIngredientSupply(program, name) {
  // An imported blueprint can be intentionally incomplete, but every stock used
  // in these authored organisms has a synthesis gene, including finishing inputs.
  const declarations = program.replace(/#[^\n]*/g,'');
  const supplied = new Set([...declarations.matchAll(/\bgene\s+\w+\s*:\s*(\w+)\s*=/g)].map(match=>match[1]));
  const ingredients = [...declarations.matchAll(/\bassembly\s+\w+[^{}]*\{([^}]*)\}/g)]
    .flatMap(match=>[...match[1].matchAll(/\b(\w+)\s*:\s*[-+.]?\d/g)].map(part=>part[1]))
    .filter(part=>!['at','direction','activity','recycle','start_when'].includes(part));
  for (const input of declarations.matchAll(/=\s*adhesin\(\s*(\w+)\s*\)/g)) ingredients.push(input[1]);
  for (const ingredient of ingredients) assert.ok(supplied.has(ingredient), name+' supplies '+ingredient);
}
for (const preset of [...CELLULAR_FAMILY_PRESETS, ...MECHANISM_PRESETS_V6]) {
  assertIngredientSupply(preset.source, preset.name);
}

// Every authored example must compile, including files users can import directly.
const playable = new Map(PRESETS.map(preset => [preset.name.toLowerCase().replaceAll(' ', '-'), preset]));
const importable = [...playable.keys(), 'solar','regulated-motor','separate-digestion','local-control','colony','conserver','archive/drifter','archive/giant'];
for (const name of importable) {
  const api = await fresh({ ambientLight: 0.85, initialNutrients: 3 });
  const example = await readFile(new URL(`../examples/${name}.cell`, import.meta.url), 'utf8');
  if (playable.has(name)) assert.equal(example.trimEnd(), playable.get(name).source, name+' example matches its live catalog program');
  assertIngredientSupply(example, name);
  const genome = compile(api, example);
  if (name === 'solar' || name === 'regulated-motor') {
    seed(api, genome, 1);
    conserved(advance(api, 1200));
    assert.equal(body(api, 1).cellular.assemblies.filter(machine => machine.complete).length, 1,
      `${name} keeps its stock buffer small enough to avoid an unintended second complete machine`);
  }
}

// Follow the same paid machinery across a bright-to-shade-to-bright change.
// The middle light level must retain either state, depending on the preceding reading.
{
  const api = await fresh({width:400,height:400,ambientLight:0.85,initialNutrients:3,gases:{mode:'open',exchangeRate:.5}});
  seed(api, compile(api, playable.get('mixotroph').source), 1, 200, 200);
  const required = ['Solar','Canopy','Gut','Capture','ExudateEnzyme','LeftMotor','RightMotor'];
  let state = snapshot(api);
  const inventory = () => body(api, 1).cellular.assemblies;
  while (state.seconds < 35 && !required.every(name => inventory().some(machine => machine.complete && machine.name === name))) {
    state = advance(api, 30);
  }
  const machines = inventory();
  assert.ok(required.every(name => machines.some(machine => machine.name === name && machine.functionalMass > 0)));
  function observe(light, expectedShade) {
    api.set_environment(.5,light,0);
    state = advance(api, 2);
    const reading = evaluation(api,1);
    assert.equal(reading.bindings.find(item => item.name === 'shade_feed').value, expectedShade);
    assert.ok(Math.abs(reading.signals.find(item => item.name === 'sensed.Canopy.level').value-light)<.001);
    assert.equal(state.cells[0].carbonFixationRate > 0, !expectedShade);
    return inventory();
  }
  observe(.45,false);
  const swimming = observe(.15,true);
  assert.ok(swimming.some(machine => machine.kind === 'propel()' && machine.activity > 0 && machine.functionalMass > 0));
  observe(.45,true);
  const beforeFeeding = state.eaten;
  placeByTissue(api,state.cells[0],0);
  let digested = 0;
  for (let second=0; second<25 && digested===0; second++) {
    state = advance(api,30);
    digested += state.cells[0].digestionRate;
    assert.equal(state.cells[0].carbonFixationRate,0);
  }
  assert.ok(state.eaten > beforeFeeding && digested > 0, 'In shade the same organism captures and digests actual particles');
  const resting = observe(.85,false);
  assert.ok(resting.filter(machine => machine.kind === 'propel()' || machine.kind==='import(particle)').every(machine => machine.activity === 0));
  const beforeResting = state.eaten;
  assert.ok(state.food.some(p=>p.energy>0),'Available tissue remains beside the bright cell');
  state = advance(api, 60);
  assert.equal(state.eaten,beforeResting,'Bright cells leave available particles in the water');
  assert.ok(state.cells[0].carbonFixationRate > 0);
  observe(.45,false);
  conserved(state);
}

// Exercise the shipped local-control program through real sensing and paid
// remodeling. Compilation and a successful birth alone do not prove these work.
{
  const api = await fresh({ ambientLight: 0.85, initialNutrients: 3 });
  const genome = compile(api, await readFile(new URL('../examples/local-control.cell', import.meta.url), 'utf8'));
  seed(api, genome, 1, 600, 380);
  let state = advance(api, 60 * 30);
  conserved(state);
  const built = body(api, 1).cellular;
  const oldMotors = built.assemblies.filter(machine => machine.kind === 'propel()');
  assert.equal(oldMotors.length, 2);
  assert.ok(oldMotors.every(machine => machine.complete));
  assert.ok(built.assemblies.some(machine => machine.kind === 'recycle()' && machine.complete));
  const read = name => evaluation(api, 1).signals.find(signal => signal.name === name)?.value;
  assert.equal(read('sensed.LeftEye.valid'), true);
  assert.ok(Math.abs(read('sensed.LeftEye.level') - 0.85) < 0.001);

  const cell = state.cells[0], left = cell.angle + Math.PI / 2;
  api.set_environment(0.5, 0.35, 0);
  assert.ok(api.add_source(0, cell.x + 35 * Math.cos(left), cell.y + 35 * Math.sin(left), 0.4, 35));
  state = advance(api, 1);
  assert.ok(read('sensed.LeftEye.level') > read('sensed.RightEye.level'), 'Opposite receptors sample different physical locations');
  api.clear_sources();

  api.set_environment(0.5, 0.1, 0);
  let recovered = 0;
  for (let second = 0; second < 15; second++) {
    state = advance(api, 30);
    conserved(state);
    recovered += body(api, 1).cellular.recipes.filter(recipe => recipe.kind === 'propel()').reduce((sum, recipe) => sum + recipe.recycleRate, 0);
  }
  const shaded = body(api, 1).cellular;
  assert.ok(shaded.products.find(product => product.name === 'Brake').concentration > 0.001);
  assert.ok(shaded.recipes.filter(recipe => recipe.kind === 'propel()').every(recipe => !recipe.startAllowed));
  assert.equal(shaded.assemblies.filter(machine => machine.kind === 'propel()').length, 0, 'Paid teardown finishes without microscopic residual objects');
  assert.ok(recovered > 0, 'The complete recycler pays for actual material recovery');

  api.set_environment(0.5, 0.85, 0);
  state = advance(api, 40 * 30);
  conserved(state);
  const regrown = body(api, 1).cellular.assemblies.filter(machine => machine.kind === 'propel()' && machine.complete);
  assert.equal(regrown.length, 2, 'The visible program regrows both motors when the material brake decays');
  assert.ok(regrown.every(machine => !oldMotors.some(old => old.id === machine.id)), 'Regrowth constructs new physical instances');
  const checkpoint = save(api), resumed = await fresh();
  assert.equal(restore(resumed, checkpoint), 1, error(resumed));
  advance(api, 60); advance(resumed, 60);
  assert.deepEqual(save(api), save(resumed), 'Named sensing, shared stock, controls and remodeling resume exactly');
}

// Follow a real division with identical resources, seed and physical recipes.
// Removing only the contact predicate isolates adhesion from ordinary drag.
const colonySource = await readFile(new URL('../examples/colony.cell', import.meta.url), 'utf8');
const pairedGaps = [];
for (const adhesive of [true, false]) {
  const api = await fresh({ width:400, height:400, ambientLight:0.85, initialNutrients:3,
    gases:{ mode:'open', exchangeRate:0.5 } });
  const genome = compile(api, adhesive ? colonySource : colonySource.replace(/engage_when:[^;]+;/, ''));
  seed(api, genome, 1, 200, 200);
  let state = snapshot(api);
  while (!state.births && state.seconds < 150) state = advance(api, state.cells[0].area < 1.9 ? 30 : 1);
  assert.equal(state.population, 2);
  for (const cell of state.cells) {
    assert.ok(body(api, cell.id).cellular.assemblies.some(machine => machine.kind === 'adhere()' && machine.functionalMass > 0));
  }
  const checkpoint = save(api), resumed = await fresh();
  assert.equal(restore(resumed, checkpoint), 1, error(resumed));
  state = advance(api, 150);
  advance(resumed, 150);
  assert.deepEqual(save(resumed), save(api), 'Both newly separated and adhering daughters replay exactly');
  const [a, b] = state.cells;
  const delta = n => n - Math.round(n / 400) * 400;
  const gap = Math.hypot(delta(a.x - b.x), delta(a.y - b.y)) - a.radius - b.radius;
  pairedGaps.push(gap);
  assert.ok(adhesive ? gap < 0.05 : gap > 3);
  for (const cell of state.cells) {
    const paired = evaluation(api, cell.id).bindings.find(binding => binding.name === 'paired_contact');
    assert.equal(paired.value, adhesive, 'Flank receptors read the other daughter while it remains attached');
  }
  conserved(state);
}
console.log(`Five seconds after division, colony surface gaps with and without adhesion were ${pairedGaps.map(gap => gap.toFixed(3)).join(' and ')} world units.`);

const cycles = [];
for (const preset of PRESETS) {
  const producer = ['Phototroph', 'Nurse rosette', 'Pulse swimmer', 'Crown rosette', 'Mixotroph'].includes(preset.name);
  const api = await fresh({width:400,height:400,ambientLight:.85,ambientTemperature:producer ? 0.5 : 0.2,initialNutrients:3,
    gases:{mode:'open',initialOxygen:producer ? 1 : 3,oxygenEquilibrium:producer ? 1 : 3,exchangeRate:.5}});
  const genome = compile(api,preset.source);
  assert.equal(graph(api,genome).mode,'cellular');
  const site = producer ? {x:200,y:200} : prepareBloom(api,playable.get('phototroph').source);
  const founder = api.place_cell(genome,site.x,site.y);
  assert.ok(founder,error(api));
  const startTick=snapshot(api).ticks;
  const initial = body(api, founder);
  assert.equal(initial.organs.length, 1);
  assert.equal(initial.organs[0].kind, 'core');
  assert.equal(initial.cellular.assemblies.length, 0);
  assert.ok(initial.cellular.products.every(product => product.mass === 0));
  const beforeInspection = save(api);
  const inspected = evaluation(api, founder);
  assert.equal(inspected.mode, 'cellular');
  assert.ok(!inspected.signals.some(signal => ['id', 'time', 'body.division_ready'].includes(signal.name)));
  assert.deepEqual(save(api), beforeInspection, 'Cellular inspection must not initialize or mutate state');

  let state = advance(api, 1);
  const first = body(api, founder).cellular;
  assert.equal(first.assemblies.length, 0, 'Newly expressed material cannot install in the same tick');
  const made = first.products.reduce((sum, product) => sum + product.mass, 0);
  assert.ok(made > 0 && made * 0.6 <= 0.022 / 30 + 1e-10, 'All genes share the same paid core budget');
  assert.ok(state.cells[0].energy < 68 && state.cells[0].nutrients < 0.6);

  // The consumer has one finite producer-derived patch and receives no later inputs.
  const cycleSeconds = ({ Predator:900, Rememberer:210 })[preset.name] ?? 150;
  let previous;
  while (!state.births && state.ticks-startTick < cycleSeconds * 30) {
    const parent = state.cells.find(cell => cell.id === founder);
    assert.ok(parent, `${preset.name} must survive development in its supplied habitat`);
    const steps = parent.area >= 1.9 ? 1 : 30;
    previous = steps === 1 ? body(api, founder).cellular : undefined;
    state = advance(api, steps);
    conserved(state);
  }
  assert.equal(state.births, 1, `${preset.name} assembles and funds a complete reproductive cycle within ${cycleSeconds} seconds in its supplied habitat`);
  assert.equal(state.population, 2);
  assert.ok(previous, 'Observe the actual fission tick');
  const daughter = state.cells.find(cell => cell.generation === 1);
  assert.ok(daughter && daughter.parentId === founder);
  const inherited = [body(api, founder).cellular, body(api, daughter.id).cellular];
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
  if (preset.name === 'Forager' || preset.name === 'Predator') {
    const essential = ['Gut', 'ExudateEnzyme', 'Capture', 'LeftMotor', 'RightMotor', 'Vision', 'Touch',
      ...(preset.name === 'Predator' ? ['TissueEnzyme', 'Piercer', 'LeftHeat', 'RightHeat'] : ['Translation'])];
    for (const name of essential) {
      assert.ok(machines.filter(machine => machine.name === name && machine.functionalMass > 0).length >= 2,
        `${preset.name} paid for duplicate ${name} machinery before division`);
      for (const cell of inherited) {
        assert.ok(cell.assemblies.some(machine => machine.name === name && machine.functionalMass > 0),
          `${preset.name} gives each daughter an existing working ${name}, including each motor orientation`);
      }
    }
  }
  if (producer) {
    assert.ok(state.photosynthesis > 0);
    assert.equal(state.eaten, 0);
  }
  if (!producer) assert.ok(state.eaten>0 && state.cells.every(cell=>cell.carbonFixationRate===0));

  const firstDivisionSeconds = (state.ticks-startTick)/30;
  const bytes = save(api), resumed = await fresh();
  assert.equal(new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(8, true), 19);
  assert.equal(restore(resumed, bytes), 1, error(resumed));
  assert.deepEqual(save(resumed), bytes);
  advance(api, 90); advance(resumed, 90);
  assert.deepEqual(save(resumed), save(api), 'Products, progress, inherited recipes and subsequent dynamics resume exactly');
  if (['Phototroph', 'Mixotroph', 'Forager'].includes(preset.name)) {
    const daughter = state.cells.find(cell => cell.parentId === founder);
    assert.ok(daughter, 'Follow the first daughter after she inherits her own machinery');
    const hasGranddaughter = () => state.cells.some(cell => cell.parentId === daughter.id);
    while (!hasGranddaughter() && state.ticks - startTick < 300 * 30) {
      state = advance(api, 60);
      conserved(state);
    }
    assert.ok(hasGranddaughter(), `The first ${preset.name} daughter must grow and divide in the same habitat as her founder`);
  }
  for (const kind of [4, 5, 6]) {
    const mutant = api.mutate_genome(genome, 17, kind);
    assert.ok(mutant !== genome && mutant > 0, error(api));
    assert.equal(api.genome_mutation_kind(mutant), kind);
    compile(api, source(api, mutant));
    assert.equal(api.mutate_genome(genome, 17, kind), mutant);
  }
  cycles.push({ name: preset.name, suppliedSubstrate: producer ? 'light' : 'heat-killed producer bloom',
    firstDivisionSeconds: Math.round(firstDivisionSeconds * 100) / 100, timeLimitSeconds: cycleSeconds });
}

console.log(`Cellular checks passed in ${countChemistry ? 'counted' : 'continuous'} chemistry: ${PRESETS.length} authored genomes, ${importable.length} importable examples, local receptor separation, bright/shade motor recycling and regrowth, paid development, frozen inspection, resource conservation, whole-machine fission with functioning consumer daughters, gene/placement mutations, and exact v17 continuation.`);
console.log(JSON.stringify({ conditions: 'One founder in lit water. Consumer trials start beside a finite patch of tissue and exudate from a bloom of 25 Phototrophs grown for 60 seconds and killed by thermal stress. No subsequent inputs.', cycles }));
