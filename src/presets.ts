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

export const PRESETS: Preset[] = [
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
