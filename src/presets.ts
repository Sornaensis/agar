export interface Preset { name: string; description: string; source: string }

type OrganTargets = ReadonlyArray<readonly [string, number]>;
type EnergyPath = 'light' | 'organic' | 'mixed';
type DigestionProfile = 'scavenger' | 'recycler' | 'hunter';

/** Expand the complete reactive development program into ordinary editable source. */
function program(organs: OrganTargets, path: EnergyPath, behavior: string, minimumBuildEnergy = 18,
  areaTarget = 'body.birth_area * 2', digestion: DigestionProfile = 'scavenger'): string {
  const powered = path === 'light' ? 'light_path' : path === 'organic' ? 'digest_path && capture_path'
    : 'light_path && digest_path && capture_path';
  const orientation = path === 'light' ? '' : ' && mobile && seeing && feeling';
  const first: Record<string, string> = {
    photosynthetic: '!light_path',
    digestive: '!digest_path',
    phagocyte: 'digest_path && !capture_path',
    metabolic: 'powered && !processing',
    transporter: 'powered && processing && !supplied',
    flagellum: path === 'light' ? 'false' : 'powered && processing && supplied && !mobile',
    optical: 'powered && processing && supplied && mobile && !seeing',
    mechanoreceptor: 'powered && processing && supplied && mobile && !feeling',
    reserve: 'established && core_ready && organelle.reserve.capacity < 1',
  };
  const paired = organs.map(([family]) => `organelle.${family}.count >= 2 && organelle.${family}.building < 0.000001`).join('\n              && ');
  const digestionProgram = path === 'light' ? 'digest.exudate = 0;\ndigest.tissue = 0;\ndigest.waste = 0;'
    : digestion === 'hunter' ? `# Spend most processing work on tissue; waste is a poor hunting return.
digest.exudate = 0.5;
digest.tissue = 1;
digest.waste = if gut.tissue.energy + gut.exudate.energy < 1 then 0.3 else 0.05;`
    : digestion === 'recycler' ? `# A drifting recycler keeps processing refractory material between encounters.
digest.exudate = 0.7;
digest.tissue = 0.4;
digest.waste = 1;`
    : `# Prefer easy energy; spend spare digestive work recovering waste.
digest.exudate = 1;
digest.tissue = 0.7;
digest.waste = if gut.exudate.energy + gut.tissue.energy < 1 then 1 else 0.1;`;
  return `# Every introduced cell has one core. Express an energy path before expanding.
# Protein synthesis and assembly are separate; unused stocks occupy room and decay.
let light_path = organelle.photosynthetic.capacity >= 1;
let digest_path = organelle.digestive.capacity >= 1;
let capture_path = organelle.phagocyte.capacity >= 1;
let powered = ${powered};
let processing = organelle.metabolic.capacity >= 1;
let supplied = organelle.transporter.capacity >= 1;
let mobile = organelle.flagellum.capacity >= 1;
let seeing = organelle.optical.capacity >= 1;
let feeling = organelle.mechanoreceptor.capacity >= 1;
let established = powered && processing && supplied${orientation};
# Absolute buffers keep storage growth from switching construction off.
let funded = energy > ${minimumBuildEnergy} && nutrient.stored > 0.002;
let area_target = ${areaTarget};
let core_ready = organelle.core.count >= 2 && organelle.core.building < 0.000001
                 && organelle.core.capacity + 0.000001 >= area_target;
let expanding = established && core_ready && body.area >= area_target * 0.75;
let copies = if expanding then 2 else 1;
let paired = ${paired};

# Duplicate the core, grow room, then prepare organs for both daughters.
target.core.count = 2;
target.core.size = max(0.04, area_target * 0.02);
let core_missing = max(0, max(0.08, area_target * 0.04) - organelle.core.mass);
express.core = if established && energy > 50 && nutrient.stored > 0.01
                 && protein.core < min(0.012, core_missing) then 1 else 0;
${organs.map(([family, mass]) => `target.${family}.count = copies;
target.${family}.size = ${mass};
let missing_${family} = max(0, ${mass} * copies - organelle.${family}.mass);
express.${family} = if funded && (${first[family] ?? 'false'} || expanding)
                   && protein.${family} < min(0.012, missing_${family}) then 1 else 0;`).join('\n')}

${behavior.trim()}
${digestionProgram}

absorb = if nutrient.fraction < 0.85 then 1 else 0;
repair = if integrity < 0.98 && energy > 40 then 0.4 else 0;
grow = if core_ready && body.area < area_target && energy > 90
          && nutrient.stored > 0.02 then 0.8 else 0;
divide = body.division_ready && body.area + 0.000001 >= area_target && paired && energy > 120;`;
}
const producer: OrganTargets = [['photosynthetic',0.12],['metabolic',0.04],['reserve',0.04],['transporter',0.025],['flagellum',0.025],['photoreceptor',0.01],['thermoreceptor',0.01],['chemoreceptor',0.015],['cortex',0.02]];
const scavenger: OrganTargets = [['digestive',0.08],['metabolic',0.04],['reserve',0.04],['transporter',0.02],['phagocyte',0.04],['flagellum',0.04],['optical',0.025],['chemoreceptor',0.015],['mechanoreceptor',0.01],['cortex',0.02]];
const hunter: OrganTargets = [['digestive',0.10],['metabolic',0.04],['reserve',0.04],['transporter',0.02],['phagocyte',0.035],['flagellum',0.05],['piercer',0.05],['optical',0.03],['mechanoreceptor',0.01],['thermoreceptor',0.01],['cortex',0.025]];
const mixed: OrganTargets = [['photosynthetic',0.08],['digestive',0.07],['metabolic',0.04],['reserve',0.04],['transporter',0.025],['phagocyte',0.035],['flagellum',0.035],['photoreceptor',0.01],['chemoreceptor',0.015],['optical',0.02],['mechanoreceptor',0.01]];

const surroundings = `# Small appearance and soft contact are useful clues, not guarantees of food.
let visible = object.available && object.valid;
let particle = visible && object.area < 0.35;
let sensed_contact = contact.available && contact.valid;
let blocked = sensed_contact && contact.resistance >= 0.12;
let soft_contact = sensed_contact && contact.resistance < 0.12;
let large_near = visible && !particle
                 && length(object.offset) < 7 * (sqrt(body.area) + sqrt(object.area)) + 2;
let obstacle = if blocked then angle(contact.offset) else angle(object.offset);
# Aim slightly beyond the tangent, so thrust carries us around and away.
let slide = if obstacle >= 0 then obstacle - 1.8 else obstacle + 1.8;`;
const forage = `${surroundings}
let seeking = gut.fraction < 0.55;
let cue = chemical.valid && chemical.organic > 0.001;
turn = if blocked || large_near then clamp(slide * 2, -1, 1)
       else if seeking && particle then clamp(angle(object.offset) * 1.8, -1, 1)
       else if seeking && cue then clamp(chemical.organic_bearing * 1.5, -1, 1)
       else sin(age * 0.65 + id) * 0.35;
thrust = if blocked || large_near then 0.65
         else if soft_contact || gut.fraction > 0.45 then 0
         else if seeking && particle then 0.5 else 0.28;
feed = 1;
engulf = gut.fraction < 0.65;
photosynthesize = 0;`;

/** Preserved examples for existing v3 programs and compatibility regression checks. */
export const LEGACY_PRESETS: Preset[] = [
  {
    name: 'Forager',
    description: 'Approach small visible objects and local organic cues; slide away from resistant surfaces.',
    source: program(scavenger, 'organic', `${forage}`),
  },
  {
    name: 'Drifter',
    description: 'Drift between encounters and reserve digestive work for slow, refractory waste.',
    source: program(scavenger.map(([kind,mass]) => [kind,kind === 'flagellum' ? 0.025 : mass] as const), 'organic', `${surroundings}
turn = if blocked || large_near then clamp(slide * 1.5, -1, 1)
       else if chemical.valid && chemical.organic > 0.001 && gut.fraction < 0.3 then clamp(chemical.organic_bearing, -1, 1)
       else sin(age * 0.4 + id) * 0.15;
thrust = if blocked || large_near then 0.45
         else if soft_contact || gut.fraction > 0.3 then 0 else 0.08;
feed = 1;
engulf = gut.fraction < 0.6;
photosynthesize = 0;`, 18, 'body.birth_area * 2', 'recycler'),
  },
  {
    name: 'Conserver',
    description: 'Feed opportunistically and preserve reserves before building more machinery.',
    source: program(scavenger, 'organic', `${surroundings}
let seeking = energy.fraction < 0.6 && gut.fraction < 0.3;
turn = if blocked || large_near then clamp(slide * 1.5, -1, 1)
       else if seeking && particle then clamp(angle(object.offset) * 1.5, -1, 1)
       else if seeking && chemical.valid && chemical.organic > 0.001 then clamp(chemical.organic_bearing, -1, 1)
       else sin(age * 0.3 + id) * 0.2;
thrust = if blocked || large_near then 0.5
         else if soft_contact || gut.fraction > 0.25 then 0
         else if seeking && particle then 0.35 else if seeking then 0.2 else 0.08;
feed = 1;
engulf = gut.fraction < 0.7;
photosynthesize = 0;`, 28),
  },
  {
    name: 'Giant',
    description: 'Grow a larger envelope, then fill it with digestive machinery and protection.',
    source: program(scavenger.map(([kind,mass]) => [kind,kind === 'digestive' ? 0.14 : kind === 'cortex' ? 0.04 : mass] as const), 'organic', `${forage}`, 18, 'max(3, body.birth_area * 2)'),
  },
  {
    name: 'Rememberer',
    description: 'Remember an observed object token and bearing; reacquisition still needs a physical sensor.',
    source: program(scavenger, 'organic', `${surroundings}
# All reads see old memory; remember only promising observations.
let observed = particle && !blocked;
ri001 = ri001 + 1 when !observed;
clear ri001 when observed;
rs001 = object.token when observed;
rf001 = angle(object.offset) when observed;
rv001 = object.offset when observed;
clear rs001 when blocked || (!observed && ri001 > 60);
clear rf001 when blocked || (!observed && ri001 > 60);
clear rv001 when blocked || (!observed && ri001 > 60);
rb001 = if energy.fraction < 0.35 then true
        else if energy.fraction > 0.75 then false else rb001;
scan.target = if populated(rs001) && !blocked then rs001 else "";
let remembered = populated(rf001) && ri001 < 60;
turn = if blocked || large_near then clamp(slide * 2, -1, 1)
       else if observed then clamp(angle(object.offset) * 1.8, -1, 1)
       else if remembered then clamp(rf001, -1, 1)
       else if chemical.valid && chemical.organic > 0.001 then clamp(chemical.organic_bearing, -1, 1)
       else sin(age * 0.6 + id) * 0.4;
thrust = if blocked || large_near then 0.65
         else if soft_contact || gut.fraction > 0.45 then 0
         else if observed && gut.fraction < 0.6 then 0.5
         else if rb001 then 0.3 else 0.2;
feed = 1;
engulf = gut.fraction < 0.6;
photosynthesize = 0;`),
  },
  {
    name: 'Phototroph',
    description: 'Express a light-harvesting pathway, then grow sensors and duplicate organs for division.',
    source: program(producer, 'light', `# Harvesting light and measuring light are separate capabilities.
let hot = heat.available && heat.valid && heat.value > 0.8;
let cool = if heat.bearing > 0 then heat.bearing - 3.141593
           else heat.bearing + 3.141593;
let needs_minerals = nutrient.fraction < 0.2;
turn = if hot then clamp(cool, -1, 1)
       else if needs_minerals && chemical.valid then clamp(chemical.mineral_bearing, -1, 1)
       else if light.available && light.valid then clamp(light.bearing, -1, 1) else 0;
thrust = if hot then 0.3 else if needs_minerals then 0.08
         else if light.valid && light.level < 0.3 then 0.07 else 0;
feed = 0;
photosynthesize = 1;`),
  },
  {
    name: 'Predator',
    description: 'Develop a piercer, approach visible objects and prioritize tissue digestion.',
    source: program(hunter, 'organic', `# Neither vision nor contact reveals another cell’s genome, diet or energy.
let hungry = energy.fraction < 0.7 && gut.fraction < 0.45;
let visible = object.available && object.valid;
let approachable = visible && object.area < body.area * 1.2;
let hot = heat.valid && heat.value > 0.82;
let cool = if heat.bearing > 0 then heat.bearing - 3.141593 else heat.bearing + 3.141593;
let touching = contact.available && contact.valid;
turn = if hot then clamp(cool, -1, 1)
       else if hungry && approachable then clamp(angle(object.offset) * 1.8, -1, 1)
       else sin(age * 0.5 + id) * 0.4;
thrust = if hot then 0.35 else if touching || gut.fraction > 0.45 then 0
         else if hungry && approachable then 0.5 else 0.07;
feed = 1;
photosynthesize = 0;
bite = if hungry && touching then 0.8 else 0;
engulf = hungry && touching;
defend = if touching then 0.2 else 0;`, 18, 'body.birth_area * 2', 'hunter'),
  },
  {
    name: 'Mixotroph',
    description: 'Share finite internal space between light harvesting and contact feeding.',
    source: program(mixed, 'mixed', `${surroundings}
let seek = particle && gut.fraction < 0.4 && energy.fraction < 0.75;
turn = if blocked || large_near then clamp(slide * 1.5, -1, 1)
       else if seek then clamp(angle(object.offset) * 1.5, -1, 1)
       else if light.valid then clamp(light.bearing, -1, 1) else 0;
thrust = if blocked || large_near then 0.45
         else if soft_contact || gut.fraction > 0.4 then 0
         else if seek then 0.3 else if light.valid && light.level < 0.3 then 0.1 else 0;
feed = 1;
engulf = gut.fraction < 0.6;
photosynthesize = 1;`),
  },
];

type CellularPart = readonly [role: string, mass: number];
interface CellularRecipe {
  name: string; kind: string; parts: readonly CellularPart[];
  activity?: string; at?: number | string; direction?: number; ready?: string; goal?: string; recycle?: string;
}

const sharedProduct: Record<string,string> = {
  scaffold:'Frame',anchor:'Mount',motor:'Rotor',filament:'Tail',pigment:'Pigment',enzyme:'Catalyst',
  membrane:'Wall',transporter:'Carrier',receptor:'Detector',replicase:'Replicase',constrictor:'Ring',
};

/** Explicit initiation lets independently regulated recipes share physical stocks. */
function controlledProgram(recipes: CellularRecipe[], logic: string, regulator = false): string {
  const roles = [...new Set(recipes.flatMap(recipe => recipe.parts.map(([role]) => role)))];
  return `# Shared products supply independently regulated physical recipes.
# Genes make stock; start_when permits new machines; activity runs built ones.
cellular;

${roles.map(role => `product ${sharedProduct[role]} = ${role}();`).join('\n')}
${regulator ? 'product Persistence = regulator(8);\n' : ''}
# Tolerate modest wear while repair restores working capacity. The allowance
# stays below one machine, even when a larger body needs many copies.
fn missing(installed, required, unit) = installed + 0.000001 < required - min(required * 0.1, unit * 0.2);
fn enough(installed, required) = installed + 0.000001 >= required * 0.9;
fn buffered(stock, effort) = if stock < 0.001 then effort else 0;

let funded = (energy > 10 || gut.energy > 0) && nutrient.stored > 0.001;
let expanding = body.area > body.birth_area * 1.6;
${logic.trim()}

${recipes.map(recipe => {
    const mass = recipe.parts.reduce((sum, [, value]) => sum + value, 0);
    const goal = recipe.goal ?? `(if expanding then ${amount(mass * 2)} else ${amount(mass)})`;
    return `let need_${recipe.name} = funded && (${recipe.ready ?? 'true'}) && missing(built.${recipe.name}, ${goal}, ${amount(mass)});`;
  }).join('\n')}

${roles.map(role => {
    const requests = recipes.flatMap(recipe => {
      const part = recipe.parts.find(([kind]) => kind === role);
      if (!part) return [];
      const largest = Math.max(...recipe.parts.map(([, value]) => value));
      return [`(if need_${recipe.name} then ${amount(part[1] / largest)} else 0)`];
    });
    return `gene supply_${sharedProduct[role]}: ${sharedProduct[role]} = buffered(pool.${sharedProduct[role]}, ${requests.join(' + ')});`;
  }).join('\n')}
${regulator ? '\n# Cue-driven material memory; growth dilutes its concentration.\ngene remember: Persistence = if sensed.LeftChemistry.valid && sensed.RightChemistry.valid\n  && concentration.Persistence < 0.008 then 0.15 * activate(sensed.LeftChemistry.organic + sensed.RightChemistry.organic, 0.002) else 0;\n' : ''}
${recipes.map(recipe => `assembly ${recipe.name}: ${recipe.kind} {
${recipe.parts.map(([role, value]) => `  ${sharedProduct[role]}: ${amount(value)};`).join('\n')}
  start_when: need_${recipe.name};
  activity: ${recipe.activity ?? '1'};${recipe.recycle === undefined ? '' : `\n  recycle: ${recipe.recycle};`}${recipe.at === undefined ? '' : `\n  at: ${placement(recipe.at)};`}${recipe.direction === undefined ? '' : `\n  direction: ${amount(recipe.direction)};`}
}`).join('\n\n')}`;
}
const amount = (value: number): string => String(Number(value.toFixed(6)));
const placement = (value: number | string): string => typeof value === 'string' ? value : amount(value);
const productName = (recipe: CellularRecipe, role: string): string => recipe.name + role[0].toUpperCase() + role.slice(1);

/** The expanded source is ordinary, inspectable cellular code, with no hidden behaviors. */
function cellularProgram(recipes: CellularRecipe[], logic: string, regulator = false, stockBuffer = 0.004): string {
  return `# Products are material stocks. Genes pay to synthesize them.
# Recipes install old stock into machines; activity uses completed machines.
cellular;

${recipes.flatMap(recipe => recipe.parts.map(([role]) => `product ${productName(recipe, role)} = ${role}();`)).join('\n')}
${regulator ? 'product Persistence = regulator(8);\n' : ''}
let funded = energy > 10 && nutrient.stored > 0.001;
let expanding = body.area > body.birth_area * 1.6;
${logic.trim()}
${regulator ? '\n# A physical, decaying memory of a recent organic cue.\ngene remember: Persistence = if chemical.valid && chemical.organic > 0.001 && pool.Persistence < 0.008 then 0.15 else 0;\n' : ''}
${recipes.map(recipe => {
    const mass = recipe.parts.reduce((sum, [, value]) => sum + value, 0), largest = Math.max(...recipe.parts.map(([, value]) => value));
    const goal = recipe.goal ?? `(if expanding then ${amount(mass * 2)} else ${amount(mass)})`;
    return `# ${recipe.name}: synthesize only while this recipe needs more installed material.
let need_${recipe.name} = funded && (${recipe.ready ?? 'true'}) && built.${recipe.name} + 0.000001 < ${goal};
${recipe.parts.map(([role, value]) => `gene make_${productName(recipe, role)}: ${productName(recipe, role)} = if need_${recipe.name} && pool.${productName(recipe, role)} < ${amount(Math.min(stockBuffer, value * 0.25))} then ${amount(value / largest)} else 0;`).join('\n')}
assembly ${recipe.name}: ${recipe.kind} {
${recipe.parts.map(([role, value]) => `  ${productName(recipe, role)}: ${amount(value)};`).join('\n')}
  activity: ${recipe.activity ?? '1'};${recipe.at === undefined ? '' : `\n  at: ${placement(recipe.at)};`}${recipe.direction === undefined ? '' : `\n  direction: ${amount(recipe.direction)};`}
}`;
  }).join('\n\n')}`;
}

function cellularFamilyOrganism(path: EnergyPath, behavior: string, options: { drift?: boolean; giant?: boolean; memory?: boolean; predator?: boolean; conserve?: boolean } = {}): string {
  const hasLight = path !== 'organic', hasGut = path !== 'light';
  const ready = hasLight && hasGut ? 'built.Solar + 0.000001 >= 0.08 && built.Gut + 0.000001 >= 0.08 && built.Capture + 0.000001 >= 0.04'
    : hasLight ? 'built.Solar + 0.000001 >= 0.12' : 'built.Gut + 0.000001 >= 0.08 && built.Capture + 0.000001 >= 0.04';
  const recipes: CellularRecipe[] = [];
  if (hasLight) recipes.push({ name: 'Solar', kind: 'photosynthetic', parts: [['anchor', hasGut ? 0.01 : 0.02], ['pigment', hasGut ? 0.06 : 0.08], ['enzyme', hasGut ? 0.01 : 0.02]], activity: '1' });
  if (hasGut) recipes.push(
    { name: 'Gut', kind: 'digestive', parts: [['membrane', 0.04], ['enzyme', 0.04]], activity: '1' },
    { name: 'Capture', kind: 'phagocyte', parts: [['anchor', 0.008], ['motor', 0.016], ['membrane', 0.016]], activity: 'if gut.fraction < 0.7 then 1 else 0', ready: 'built.Gut + 0.000001 >= 0.08' },
  );
  recipes.push(
    { name: 'Metabolism', kind: 'metabolic', parts: [['scaffold', 0.02], ['enzyme', 0.02]], ready: 'powered' },
    { name: 'Minerals', kind: 'transporter', parts: [['anchor', 0.005], ['transporter', 0.02]], ready: 'powered', activity: 'if nutrient.fraction < 0.9 then 1 else 0' },
    { name: 'Storage', kind: 'reserve', parts: [['membrane', 0.04]], ready: 'powered' },
  );
  if (hasGut || options.drift) recipes.push(
    { name: 'LeftMotor', kind: 'flagellum', parts: [['anchor', 0.005], ['motor', 0.01], ['filament', 0.01]], ready: 'powered', activity: 'clamp(move - steer * 0.5, 0, 1)', at: Math.PI / 2, direction: 0 },
    { name: 'RightMotor', kind: 'flagellum', parts: [['anchor', 0.005], ['motor', 0.01], ['filament', 0.01]], ready: 'powered', activity: 'clamp(move + steer * 0.5, 0, 1)', at: -Math.PI / 2, direction: 0 },
    { name: 'Vision', kind: 'optical', parts: [['anchor', 0.004], ['receptor', 0.012], ['pigment', 0.004]], ready: 'powered' },
    { name: 'Touch', kind: 'mechanoreceptor', parts: [['anchor', 0.002], ['receptor', 0.006], ['filament', 0.002]], ready: 'powered' },
    { name: 'Chemistry', kind: 'chemoreceptor', parts: [['anchor', 0.003], ['receptor', 0.012]], ready: 'powered' },
  );
  if (options.predator) recipes.push({ name: 'Piercer', kind: 'piercer', parts: [['anchor', 0.01], ['filament', 0.03], ['enzyme', 0.01]], ready: 'powered', activity: 'if contact.valid && gut.fraction < 0.5 then 0.8 else 0' });
  const growthArea = options.giant ? 'max(3, body.birth_area * 2)' : 'body.birth_area * 2';
  recipes.push(
    { name: 'Nucleus', kind: 'core', parts: [['scaffold', 0.03], ['enzyme', 0.03]], ready: `powered && energy > ${options.conserve ? 60 : 45}`, goal: options.giant ? 'max(0.24, body.birth_area * 0.12)' : 'max(0.12, body.birth_area * 0.12)' },
    { name: 'Envelope', kind: 'membrane', parts: [['membrane', 0.015], ['enzyme', 0.005]], ready: 'built.Nucleus + 0.000001 >= 0.12', activity: `if energy > 90 && body.area < ${growthArea} then 0.8 else 0` },
    { name: 'Replication', kind: 'replicator', parts: [['scaffold', 0.005], ['replicase', 0.015]], ready: 'built.Nucleus + 0.000001 >= 0.12', activity: 'if energy > 80 then 1 else 0' },
    { name: 'Division', kind: 'constrictor', parts: [['anchor', 0.004], ['motor', 0.008], ['constrictor', 0.008]], ready: 'expanding', activity: `if body.area + 0.000001 >= ${growthArea} && energy > 100 then 1 else 0` },
  );
  return cellularProgram(recipes, `let powered = ${ready};\n${behavior}`, options.memory);
}

const cellularForaging = `# Receptor readings describe physical surroundings, not hidden identities.
let seen = object.valid && object.area < 0.35;
let blocked = contact.valid && contact.resistance >= 0.12;
let obstacle = if contact.valid then angle(contact.offset) else 0;
let escape = if obstacle >= 0 then obstacle - 1.8 else obstacle + 1.8;
let seek = gut.fraction < 0.55;
let steer = if blocked then clamp(escape * 2, -1, 1)
  else if seek && seen then clamp(angle(object.offset) * 1.8, -1, 1)
  else if seek && chemical.valid then clamp(chemical.organic_bearing, -1, 1)
  else sin(age * 0.65) * 0.35;
let move = if blocked then 0.65 else if gut.fraction > 0.45 then 0
  else if seek && seen then 0.5 else 0.28;`;

/** New runs use physical products, named genes and composition-based machinery. */
export const CELLULAR_FAMILY_PRESETS: Preset[] = [
  { name: 'Forager', description: 'Build a gut, capture machinery and two placed motors; steer from physical receptor readings.', source: cellularFamilyOrganism('organic', cellularForaging) },
  { name: 'Drifter', description: 'Use quiet paired motors and opportunistic contact feeding; most motion comes from the water.', source: cellularFamilyOrganism('organic', 'let steer = sin(age * 0.4) * 0.12;\nlet move = if gut.fraction < 0.3 then 0.08 else 0;', { drift: true }) },
  { name: 'Conserver', description: 'Express machinery in stages and preserve energy before replicating its physical core.', source: cellularFamilyOrganism('organic', 'let steer = if object.valid then clamp(angle(object.offset), -1, 1) else sin(age * 0.3) * 0.2;\nlet move = if energy.fraction < 0.6 && gut.fraction < 0.3 then 0.25 else 0.05;', { conserve: true }) },
  { name: 'Giant', description: 'Build extra core material and an active membrane machine to grow a larger envelope.', source: cellularFamilyOrganism('organic', cellularForaging, { giant: true }) },
  { name: 'Rememberer', description: 'A regulator with an eight-second half-life preserves a material memory of an organic cue.', source: cellularFamilyOrganism('organic', 'let steer = if chemical.valid then clamp(chemical.organic_bearing, -1, 1) else sin(age * 0.5) * 0.3;\nlet move = if gut.fraction > 0.5 then 0 else if pool.Persistence > 0.002 then 0.4 else 0.15;', { memory: true }) },
  { name: 'Phototroph', description: 'Assemble pigment, enzyme and anchors into a light harvester, then build growth and division machinery.', source: cellularFamilyOrganism('light', '# This stationary producer spends no material on locomotion.') },
  { name: 'Predator', description: 'Express a composed piercer and use contact activity to access tissue while paired motors approach objects.', source: cellularFamilyOrganism('organic', 'let hungry = energy.fraction < 0.7 && gut.fraction < 0.45;\nlet steer = if hungry && object.valid then clamp(angle(object.offset) * 1.8, -1, 1) else sin(age * 0.5) * 0.4;\nlet move = if contact.valid || gut.fraction > 0.45 then 0 else if hungry && object.valid then 0.5 else 0.07;', { predator: true }) },
  { name: 'Mixotroph', description: 'Share finite material and core work between a light harvester and composed feeding machinery.', source: cellularFamilyOrganism('mixed', cellularForaging) },
];


/** Each recipe installs one operation; no family grants additional hidden machinery. */
function cellularOrganism(path: EnergyPath, behavior: string, options: { drift?: boolean; giant?: boolean; memory?: boolean; predator?: boolean; conserve?: boolean; local?:boolean; restOnMeal?:boolean; colony?:boolean; multicellular?:'nursery' | 'relay'; scavenger?:boolean } = {}): string {
  const hasLight = path !== 'organic', hasGut = path !== 'light';
  const exudateMass = options.local && options.predator ? 0.025 : 0.04;
  const tissueMass = options.predator ? (options.local ? 0.08 : 0.04) : options.local && options.scavenger ? 0.04 : hasLight ? 0.01 : 0.02;
  const sufficient = (name: string, mass: number) => options.local
    ? `enough(built.${name}, ${amount(mass)})` : `built.${name} + 0.000001 >= ${amount(mass)}`;
  const foodReady = [sufficient('Gut', 0.06), sufficient('ExudateEnzyme', exudateMass), sufficient('Capture', 0.03)].join(' && ');
  const feedingReady = hasLight && hasGut ? `${sufficient('Solar', 0.08)} && ${foodReady}`
    : hasLight ? sufficient('Solar', 0.12) : foodReady;
  const ready = options.local ? `${feedingReady} && ${sufficient('Respiration', 0.04)}` : feedingReady;
  const recipes: CellularRecipe[] = [];
  if (options.local) recipes.push({ name:'Respiration', kind:'respire(oxygen)', parts:[['enzyme',0.025],['membrane',0.015]],
    activity:'if oxygen > 0 && energy.fraction < 0.98 && (carbon.organic > 0.2 * body.area || energy < 20) then 1 else 0' });
  const usesVision = !options.local || behavior.includes('sensed.Vision');
  const usesTouch = !options.local || behavior.includes('sensed.Touch') || options.predator;
  const usesChemistry = !options.local || behavior.includes('sensed.Chemistry') || options.memory;
  const equipped = (hasGut || options.drift) && !hasLight
    ? ['powered', sufficient('LeftMotor', 0.025), sufficient('RightMotor', 0.025),
      ...(usesVision ? [sufficient('Vision', 0.02)] : []),
      ...(usesTouch ? [sufficient('Touch', 0.01)] : []),
      ...(options.predator ? [sufficient('Piercer', 0.05), sufficient('TissueEnzyme', tissueMass)] : [])].join(' && ')
    : 'powered';
  // Consumers must acquire food before spending the search reserve on optional
  // capacity and reproductive support. These are ordinary authored conditions.
  const surplusReady = options.local && hasGut && !hasLight ? 'surplus' : 'powered';
  const nursery = options.multicellular === 'nursery';
  if (hasLight) {
    // A nurse can fit a smaller extra harvester after its envelope partitions.
    const parts: CellularPart[] = nursery ? [['anchor',0.01],['pigment',0.04],['enzyme',0.01]]
      : [['anchor',hasGut?0.01:0.02],['pigment',hasGut?0.06:0.08],['enzyme',hasGut?0.01:0.02]];
    recipes.push({ name:'Solar', kind:'harvest(light)', parts,
      ...(nursery ? {goal:'if nursing then 0.36 else if expanding then 0.24 else 0.12', activity:'if developing then 0.05 else 1'} : {}) });
  }
  if (options.colony) recipes.push(
    { name:'Adhesin', kind:'adhere()', parts:[['anchor',0.01],['membrane',0.01]], ready:'powered', activity:'if energy > 12 then 1 else 0' },
    ...['LeftJunction', 'RightJunction'].map((name, index) => ({
      name, kind:'sense(contact)', parts:[['anchor',0.002],['receptor',0.006],['filament',0.002]] as CellularPart[],
      at:index === 0 ? Math.PI/2 : -Math.PI/2, ready:'powered',
    })),
  );
  if (nursery) {
    for (const resource of ['energy','mineral','carbon']) recipes.push({
      name:`Share${resource[0].toUpperCase() + resource.slice(1)}`, kind:`junction(${resource})`,
      parts:[['membrane',0.012],['transporter',0.008]], ready:'powered', activity:'if paired_contact then 1 else 0',
    });
  }
  if (options.multicellular === 'relay') recipes.push({
    name:'PulseMotor', kind:'propel()', parts:[['anchor',0.005],['motor',0.01],['filament',0.01]],
    ready:'powered', activity:'if firing then 0.6 else 0', at:0, direction:0,
  });
  if (hasGut) recipes.push(
    { name:'Gut', kind:'store(food)', parts:[['membrane',0.06]] },
    { name:'ExudateEnzyme', kind:'digest(exudate)', parts:[['enzyme',exudateMass]], activity:'if gut.exudate.energy > 0 then 1 else 0' },
    { name:'Capture', kind:'import(particle)', parts:[['anchor',0.01],['motor',0.02]], activity:'if gut.fraction < 0.7 then 1 else 0', ready:sufficient('Gut', 0.06) },
    { name:'TissueEnzyme', kind:'digest(tissue)', parts:[['enzyme',tissueMass]], ready:options.local && !options.predator ? 'powered && gut.tissue.energy > 0' : 'powered', activity:'if gut.tissue.energy > 0 then 1 else 0' },
    { name:'WasteEnzyme', kind:'digest(waste)', parts:[['enzyme',options.drift?0.03:hasLight?0.01:0.015]], ready:options.local ? 'powered && gut.waste.energy > 0' : 'powered', activity:'if gut.waste.energy > 0 then 1 else 0' },
  );
  recipes.push(
    { name:'Translation', kind:'translate()', parts:[['scaffold',0.02],['enzyme',0.02]], ready:options.local && options.predator ? 'surplus && body.area > 1.2' : surplusReady },
    { name:'Minerals', kind:'import(mineral)', parts:[['anchor',0.005],['transporter',0.02]], ready:options.local && hasGut ? 'powered && (nutrient.stored < 0.15 || surplus)' : 'powered', activity:'if nutrient.fraction < 0.9 then 1 else 0' },
    { name:'EnergyStore', kind:'store(energy)', parts:[['membrane',options.local?0.04:0.08]], ready:surplusReady,
      ...(options.colony ? { goal:'if expanding || paired_contact then 0.08 else 0.04' } : {}) },
    { name:'MineralStore', kind:'store(mineral)', parts:[['membrane',0.02]], ready:options.local?`${surplusReady} && body.area > 1.2`:surplusReady },
  );
  const movementReady = hasLight && hasGut ? 'powered && body.area > 1.2' : 'powered';
  const restMotors = options.local && options.restOnMeal ? `if ${options.scavenger ? 'meal > 2 && !injured' : 'gut.energy > 2'} then 0 else ` : '';
  if (hasGut || options.drift) recipes.push(
    { name:'LeftMotor', kind:'propel()', parts:[['anchor',0.005],['motor',0.01],['filament',0.01]], ready:movementReady, activity:`${restMotors}clamp(move - steer * 0.5, 0, 1)`, at:Math.PI/2, direction:0 },
    { name:'RightMotor', kind:'propel()', parts:[['anchor',0.005],['motor',0.01],['filament',0.01]], ready:movementReady, activity:`${restMotors}clamp(move + steer * 0.5, 0, 1)`, at:-Math.PI/2, direction:0 },
    ...(usesVision ? [{ name:'Vision', kind:'sense(optical)', parts:[['anchor',0.004],['receptor',0.012],['pigment',0.004]] as CellularPart[], ready:movementReady }] : []),
    ...(usesTouch ? [{ name:'Touch', kind:'sense(contact)', parts:[['anchor',0.002],['receptor',0.006],['filament',0.002]] as CellularPart[], ready:movementReady }] : []),
    ...(usesChemistry ? options.local && options.memory ? [
      { name:'LeftChemistry', kind:'sense(chemical)', parts:[['anchor',0.003],['receptor',0.012]] as CellularPart[], ready:movementReady, at:Math.PI/2 },
      { name:'RightChemistry', kind:'sense(chemical)', parts:[['anchor',0.003],['receptor',0.012]] as CellularPart[], ready:movementReady, at:-Math.PI/2 },
    ] : [{ name:'Chemistry', kind:'sense(chemical)', parts:[['anchor',0.003],['receptor',0.012]] as CellularPart[], ready:options.local ? 'surplus' : movementReady }] : []),
  );
  if (options.predator) recipes.push(
    { name:'Piercer', kind:'pierce()', parts:[['anchor',0.01],['filament',0.03],['enzyme',0.01]], ready:options.local ? 'powered' : 'powered && body.area > 1.2', activity:`if ${options.local?'!heat_escape && firm_contact':'contact.valid'} && gut.fraction < 0.5 then 0.8 else 0` },
    { name:'Engulfment', kind:'engulf()', parts:[['anchor',0.005],['motor',0.01],['membrane',0.01]], ready:options.local ? 'surplus && body.area > 1.2' : 'powered && body.area > 1.2', activity:'if gut.fraction < 0.5 then 1 else 0' },
  );
  // Damage may itself disable developmental prerequisites. Maintenance uses
  // the shared funding guard, so it can complete while those machines recover.
  if (options.local) recipes.push({ name:'Repair', kind:'repair()', parts:[['enzyme',0.025]], ready:'integrity < 0.995' });
  const growthArea = options.giant ? 'max(3, body.birth_area * 2)' : 'body.birth_area * 2';
  recipes.push(
    { name:'Support', kind:'support()', parts:[['scaffold',0.06]], ready:options.local && hasGut ? `equipped && energy > ${options.conserve?70:65}${hasLight?'':' && gut.energy > 2 && carbon.organic > 0.3 * body.area'}` : `powered && energy > ${options.conserve?60:45}`, goal:options.giant?'max(0.24, body.birth_area * 0.12)':'max(0.12, body.birth_area * 0.12)' },
    { name:'Envelope', kind:'grow()', parts:[['membrane',0.015],['enzyme',0.005]], ready:sufficient('Support', 0.12), activity:nursery ? 'if !nursing && energy > (if developing then 45 else 90) && body.area < body.birth_area * 2 then 0.8 else 0' : `if energy > 90 && body.area < ${growthArea} then 0.8 else 0` },
    { name:'Replication', kind:'replicate()', parts:[['scaffold',0.005],['replicase',0.015]], ready:sufficient('Support', 0.12), activity:nursery ? 'if !nursing && energy > (if developing then 50 else 80) then 1 else 0' : 'if energy > 80 then 1 else 0' },
    { name:'Division', kind:'constrict()', parts:[['anchor',0.004],['motor',0.008],['constrictor',0.008]], ready:'expanding', activity:`if body.area + 0.000001 >= ${growthArea} && energy > 100 then 1 else 0` },
  );
  // Paired flank thermoreceptors add paid local information; keep their tested
  // placement exact and append them after existing recipes to retain work order.
  if (options.local && options.predator) recipes.push(
    { name:'LeftHeat', kind:'sense(heat)', parts:[['anchor',0.002],['receptor',0.008]], ready:'powered', at:String(Math.PI/2) },
    { name:'RightHeat', kind:'sense(heat)', parts:[['anchor',0.002],['receptor',0.008]], ready:'powered', at:String(-Math.PI/2) },
  );
  return options.local ? controlledProgram(recipes, `let powered = ${ready};\nlet equipped = ${equipped};\nlet surplus = equipped && energy > 55${hasGut&&!hasLight?' && gut.energy > 2 && carbon.organic > 0.3 * body.area':''};\n${behavior}`, options.memory)
    : cellularProgram(recipes, `let powered = ${ready};\n${behavior}`, options.memory, 0.001);
}

/** Defaults compose explicit operations from paid product stocks. */
export const MECHANISM_PRESETS_V6: Preset[] = [
  { name: 'Forager', description: 'Build separate food storage, substrate enzymes, particle import and placed motors.', source: cellularOrganism('organic', cellularForaging) },
  { name: 'Drifter', description: 'Use quiet paired motors and opportunistic contact feeding; most motion comes from the water.', source: cellularOrganism('organic', 'let steer = sin(age * 0.4) * 0.12;\nlet move = if gut.fraction < 0.3 then 0.08 else 0;', { drift: true }) },
  { name: 'Conserver', description: 'Express machinery in stages and preserve energy before copying its genome.', source: cellularOrganism('organic', 'let steer = if object.valid then clamp(angle(object.offset), -1, 1) else sin(age * 0.3) * 0.2;\nlet move = if energy.fraction < 0.6 && gut.fraction < 0.3 then 0.25 else 0.05;', { conserve: true }) },
  { name: 'Giant', description: 'Build extra structural support and separate translation, storage and growth machinery.', source: cellularOrganism('organic', cellularForaging, { giant: true }) },
  { name: 'Rememberer', description: 'A regulator with an eight-second half-life preserves a material memory of an organic cue.', source: cellularOrganism('organic', 'let steer = if chemical.valid then clamp(chemical.organic_bearing, -1, 1) else sin(age * 0.5) * 0.3;\nlet move = if gut.fraction > 0.5 then 0 else if pool.Persistence > 0.002 then 0.4 else 0.15;', { memory: true }) },
  { name: 'Phototroph', description: 'Assemble pigment, enzyme and anchors into a light harvester, then build growth and division machinery.', source: cellularOrganism('light', '# This stationary producer spends no material on locomotion.') },
  { name: 'Predator', description: 'Build tissue enzymes, a piercer and engulfment machinery; paired motors approach visible objects.', source: cellularOrganism('organic', 'let hungry = energy.fraction < 0.7 && gut.fraction < 0.45;\nlet steer = if hungry && object.valid then clamp(angle(object.offset) * 1.8, -1, 1) else sin(age * 0.5) * 0.4;\nlet move = if contact.valid || gut.fraction > 0.45 then 0 else if hungry && object.valid then 0.5 else 0.07;', { predator: true }) },
  { name: 'Mixotroph', description: 'Combine light harvesting with independently built food storage, capture and substrate enzymes.', source: cellularOrganism('mixed', cellularForaging) },
];

function localForaging(scavenger = false): string {
  const satiation = scavenger ? 'meal_fraction' : 'gut.fraction';
  return `# Read the receptors the genome actually builds.
${scavenger ? 'let injured = integrity < 0.995;\n' : ''}let seen = sensed.Vision.valid;
let blocked = sensed.Touch.valid && sensed.Touch.resistance >= 0.12;
let obstacle = if sensed.Touch.valid then angle(sensed.Touch.offset) else 0;
let escape = if obstacle >= 0 then obstacle - 1.8 else obstacle + 1.8;
${scavenger ? 'let meal = gut.exudate.energy + gut.tissue.energy;\nlet meal_fraction = if gut.capacity > 0 then meal / gut.capacity else 0;\n' : ''}let seek = ${satiation} < 0.55;
let grazing = seen && sensed.Vision.area >= 0.35
  && length(sensed.Vision.offset) < 7 * sqrt(sensed.Vision.area) + 5;
let steer = if blocked then clamp(escape * 2, -1, 1)
${scavenger ? '  else if injured then 0\n' : ''}  else if grazing then clamp((angle(sensed.Vision.offset) - 1.4) * 1.8, -1, 1)
  else if seek && seen then clamp(angle(sensed.Vision.offset) * 1.8, -1, 1)
  else sin(age * 0.65) * 0.35;
let move = if ${scavenger ? 'blocked || injured' : 'blocked'} then 0.65 else if ${satiation} > 0.45 then 0
  else if seek && seen then 0.5
  else if sensed.Chemistry.valid && sensed.Chemistry.organic > 0.001 then 0.18 else 0.28;`;
}

const localHunting = `# Advertise a contact marker from birth. Spare matching targets of either weapon.
publish rf001 = 17;
avoid_message rf001;
let peer = sensed.Touch.message_valid && sensed.Touch.message == rf001;
let leave_peer = if angle(sensed.Touch.offset) >= 0 then -0.8 else 0.8;
let firm_contact = sensed.Touch.valid && sensed.Touch.resistance > 0.1;
let hungry = (energy.fraction < 0.7 || carbon.organic * 20 < energy.capacity * 0.4) && gut.fraction < 0.45;
# Paired local temperatures select the cooler side before tissue is damaged.
let heat_escape = sensed.LeftHeat.valid && sensed.RightHeat.valid
  && max(sensed.LeftHeat.value, sensed.RightHeat.value) > 0.78;
let heat_delta = sensed.RightHeat.value - sensed.LeftHeat.value;
let cool_turn = if heat_delta < -0.002 then -0.75 else 0.75;
let steer = if heat_escape then cool_turn else if peer then leave_peer else if gut.fraction > 0.45 then 0 else if hungry && sensed.Vision.valid then clamp(angle(sensed.Vision.offset) * 1.8, -1, 1) else sin(age * 0.5) * (if equipped then 0.12 else 0.4);
let move = if heat_escape then 0.4 else if peer then 0.25 else if firm_contact || gut.fraction > 0.45 then 0 else if hungry && sensed.Vision.valid then 0.5 else if hungry && equipped then 0.28 else 0.07;`;

const rememberGradient = `let difference = sensed.LeftChemistry.organic - sensed.RightChemistry.organic;
let cue = sensed.LeftChemistry.valid && sensed.RightChemistry.valid && abs(difference) > 0.000001;
let cue_turn = clamp(difference * 100, -1, 1);
rf010 = cue_turn when cue;
rf011 = age when cue;
let recent = populated(rf011) && age - rf011 < 1.5;
clear rf010 when !cue && !recent;
clear rf011 when !cue && !recent;
let steer = if cue then cue_turn else if recent then rf010 else sin(age * 0.5) * 0.3;
let move = if gut.fraction > 0.5 then 0 else 0.15 + 0.25 * activate(concentration.Persistence, 0.002);`;

const reserveForaging = `# Enter food-seeking below 55% energy and keep seeking until reserves reach 70%.
rb001 = true when energy.fraction < 0.55;
clear rb001 when energy.fraction >= 0.7;
let seeking = energy.fraction < 0.55 || (rb001 && energy.fraction < 0.7);
let steer = if sensed.Vision.valid then clamp(angle(sensed.Vision.offset), -1, 1) else sin(age * 0.3) * 0.2;
let move = if seeking && gut.fraction < 0.3 then 0.25 else 0.05;`;

const colonyContact = (marker: number) => `# Adhesion requires mutually recognized, active adhesive surfaces.
publish rf020 = ${marker};
adhere_message rf020;
let left_peer = sensed.LeftJunction.message_valid && sensed.LeftJunction.message == rf020;
let right_peer = sensed.RightJunction.message_valid && sensed.RightJunction.message == rf020;
let paired_contact = left_peer || right_peer;`;

const colonyBehavior = `# Adhesion requires mutually recognized, active adhesive surfaces.
publish rf020 = 31;
adhere_message rf020;
let paired_contact = (sensed.LeftJunction.message_valid && sensed.LeftJunction.message == rf020)
  || (sensed.RightJunction.message_valid && sensed.RightJunction.message == rf020);
# A connected producer builds extra energy storage from its own material.`;

const nurseBehavior = `${colonyContact(42)}
# Young cells ask their partners for support. Age runs from each cell's own birth.
signal rf021;
rf021 = if age < 50 then 1 else 0;
let developing = age < 50 && paired_contact;
let left_request = left_peer && sensed.LeftJunction.signal_valid && sensed.LeftJunction.signal > 0.5;
let right_request = right_peer && sensed.RightJunction.signal_valid && sensed.RightJunction.signal > 0.5;
let nursing = age >= 50 && (left_request || right_request);
# The attached nurse builds extra harvesters and pauses envelope growth.
# Its juvenile invests in growth. Each solute moves down its own gradient.`;

const relayBehavior = `${colonyContact(73)}
# The identity marker stays stable while rf021 carries a changing pulse.
signal rf021;
let left_wave = left_peer && sensed.LeftJunction.signal_valid && sensed.LeftJunction.signal > 0.5;
let right_wave = right_peer && sensed.RightJunction.signal_valid && sensed.RightJunction.signal > 0.5;
let pacemaker = paired_contact && age > 40 && sin(age * 0.9) > 0.985;
let trigger = ri031 == 0 && (pacemaker || left_wave || right_wave);
let firing = trigger || ri031 > 162;
# Each pulse lasts roughly 0.6 seconds. The six-second refractory period
# prevents the same pulse from bouncing repeatedly between two cells.
ri031 = if trigger then 180 else if ri031 > 0 then ri031 - 1 else 0;
rf021 = if firing then 1 else 0;`;

/** Current defaults share role stocks and control each recipe's initiation explicitly. */
export const PRESETS: Preset[] = [
  { name:'Forager', description:'Share product stocks between food storage, digestion and motors; navigate using named physical receptors.', source:cellularOrganism('organic',localForaging(true),{local:true,restOnMeal:true,scavenger:true}) },
  { name:'Drifter', description:'Use quiet paired motors and opportunistic particle feeding, with explicit control over starting new machinery.', source:cellularOrganism('organic','let steer = sin(age * 0.4) * 0.12;\nlet move = if gut.fraction < 0.3 then 0.08 else 0;',{local:true,drift:true}) },
  { name:'Conserver', description:'Remember a low-energy episode and keep foraging until reserves recover.', source:cellularOrganism('organic',reserveForaging,{local:true,conserve:true}) },
  { name:'Giant', description:'Build extra support before expanding, using a shared supply of each product role.', source:cellularOrganism('organic',localForaging(),{local:true,giant:true,restOnMeal:true}) },
  { name:'Rememberer', description:'Store the last chemical turn and its age, then follow it briefly when the gradient disappears.', source:cellularOrganism('organic',rememberGradient,{local:true,memory:true}) },
  { name:'Phototroph', description:'Fix dissolved inorganic carbon with light, then respire stored organic carbon with oxygen, including at night.', source:cellularOrganism('light','# This stationary producer spends no material on locomotion.',{local:true}) },
  { name:'Predator', description:'Advertise a peer marker, turn away from matching neighbors and hunt other cells with a piercer and engulfment machinery.', source:cellularOrganism('organic',localHunting,{local:true,predator:true}) },
  { name:'Colony phototroph', description:'Build adhesive surfaces, attach to matching neighbors and grow extra energy storage when a contact receptor senses a partner.', source:cellularOrganism('light',colonyBehavior,{local:true,colony:true}) },
  { name:'Nurse rosette', description:'Adult cells with a requesting juvenile build extra harvesters, pause growth and exchange energy, organic carbon and minerals through junctions.', source:cellularOrganism('light',nurseBehavior,{local:true,colony:true,multicellular:'nursery'}) },
  { name:'Pulse swimmer', description:'Attached cells relay brief pulses to their neighbors and activate propulsion. A refractory interval separates each burst.', source:cellularOrganism('light',relayBehavior,{local:true,colony:true,multicellular:'relay'}) },
  { name:'Mixotroph', description:'Share materials between light harvesting and a separately regulated feeding pathway.', source:cellularOrganism('mixed',localForaging(),{local:true,restOnMeal:true}) },
];
