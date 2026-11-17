import type { BodyRecipe } from './engine';

export interface Preset { name: string; description: string; starterBody: BodyRecipe; source: string }

type OrganTargets = ReadonlyArray<readonly [string, number]>;
/** Two physical units of each capability make conservative inheritance possible. */
function construction(organs: OrganTargets, energyFloor = 0.45): string {
  return `# Targets do not grant capabilities: each organ must be built and paid for.
let assemble = if energy.fraction > ${energyFloor} && nutrient.stored > 0.01 then 1 else 0;
target.core.count = 2;
target.core.size = max(0.04, body.area * 0.04);
build.core = assemble;
${organs.map(([family, mass]) => `target.${family}.count = 2;
target.${family}.size = ${mass};
build.${family} = assemble;`).join('\n')}`;
}
const producer: OrganTargets = [['photosynthetic',0.17],['metabolic',0.04],['reserve',0.06],['transporter',0.03],['flagellum',0.025],['photoreceptor',0.01],['thermoreceptor',0.01],['chemoreceptor',0.015],['cortex',0.02]];
const scavenger: OrganTargets = [['digestive',0.13],['metabolic',0.04],['reserve',0.06],['transporter',0.02],['phagocyte',0.05],['flagellum',0.05],['optical',0.025],['chemoreceptor',0.015],['mechanoreceptor',0.01],['cortex',0.02]];
const hunter: OrganTargets = [['digestive',0.13],['metabolic',0.04],['reserve',0.06],['transporter',0.02],['phagocyte',0.04],['flagellum',0.065],['piercer',0.07],['optical',0.035],['mechanoreceptor',0.015],['thermoreceptor',0.01],['cortex',0.035]];
const mixed: OrganTargets = [['photosynthetic',0.10],['digestive',0.09],['metabolic',0.04],['reserve',0.05],['transporter',0.025],['phagocyte',0.04],['flagellum',0.035],['photoreceptor',0.01],['chemoreceptor',0.015],['optical',0.02],['mechanoreceptor',0.01]];
const upkeep = `absorb = if nutrient.fraction < 0.85 then 1 else 0;
repair = if integrity < 0.92 && energy.fraction > 0.55 then 0.4 else 0;
grow = if body.maturity < 1 && energy.fraction > 0.6
          && nutrient.stored > 0.06 then 0.65 else 0;
divide = body.division_ready && energy.fraction > 0.62;`;
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
    name: 'Forager', starterBody: 'scavenger',
    description: 'Approach small visible objects and local organic cues; slide away from resistant surfaces.',
    source: `${forage}
${upkeep}

${construction(scavenger)}`,
  },
  {
    name: 'Drifter', starterBody: 'scavenger',
    description: 'Let currents bring encounters; use a small motor and contact feeding.',
    source: `${surroundings}
turn = if blocked || large_near then clamp(slide * 1.5, -1, 1)
       else if chemical.valid && chemical.organic > 0.001 && gut.fraction < 0.3 then clamp(chemical.organic_bearing, -1, 1)
       else sin(age * 0.4 + id) * 0.15;
thrust = if blocked || large_near then 0.45
         else if soft_contact || gut.fraction > 0.3 then 0 else 0.08;
feed = 1;
engulf = gut.fraction < 0.6;
photosynthesize = 0;
${upkeep}

${construction(scavenger.map(([kind,mass]) => [kind,kind === 'flagellum' ? 0.025 : mass]))}`,
  },
  {
    name: 'Conserver', starterBody: 'scavenger',
    description: 'Feed opportunistically and preserve reserves before building more machinery.',
    source: `${surroundings}
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
photosynthesize = 0;
${upkeep}

${construction(scavenger,0.65)}`,
  },
  {
    name: 'Giant', starterBody: 'scavenger',
    description: 'Grow a larger envelope, then fill it with digestive machinery and protection.',
    source: `${forage}
absorb = if nutrient.fraction < 0.85 then 1 else 0;
repair = if integrity < 0.92 && energy.fraction > 0.6 then 0.5 else 0;
let area_target = max(3, body.birth_area * 2);
grow = if body.area < area_target && energy.fraction > 0.6
          && nutrient.stored > 0.06 then 0.8 else 0;
divide = body.area >= area_target && body.division_ready && energy.fraction > 0.65;

${construction(scavenger.map(([kind,mass]) => [kind, kind === 'digestive' ? 0.20 : kind === 'cortex' ? 0.06 : mass]))}`,
  },
  {
    name: 'Rememberer', starterBody: 'scavenger',
    description: 'Remember an observed object token and bearing; reacquisition still needs a physical sensor.',
    source: `${surroundings}
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
photosynthesize = 0;
${upkeep}

${construction(scavenger)}`,
  },
  {
    name: 'Phototroph', starterBody: 'producer',
    description: 'Build photosynthetic units and transporters, using paid light and chemical sensors.',
    source: `# Harvesting light and measuring light are separate capabilities.
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
photosynthesize = 1;
${upkeep}

${construction(producer)}`,
  },
  {
    name: 'Predator', starterBody: 'hunter',
    description: 'Approach visible objects when hungry and test contacted surfaces with a built piercer.',
    source: `# Neither vision nor contact reveals another cell’s genome, diet or energy.
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
defend = if touching then 0.2 else 0;
${upkeep}

${construction(hunter,0.55)}`,
  },
  {
    name: 'Mixotroph', starterBody: 'mixed',
    description: 'Share finite internal space between light harvesting and contact feeding.',
    source: `${surroundings}
let seek = particle && gut.fraction < 0.4 && energy.fraction < 0.75;
turn = if blocked || large_near then clamp(slide * 1.5, -1, 1)
       else if seek then clamp(angle(object.offset) * 1.5, -1, 1)
       else if light.valid then clamp(light.bearing, -1, 1) else 0;
thrust = if blocked || large_near then 0.45
         else if soft_contact || gut.fraction > 0.4 then 0
         else if seek then 0.3 else if light.valid && light.level < 0.3 then 0.1 else 0;
feed = 1;
engulf = gut.fraction < 0.6;
photosynthesize = 1;
${upkeep}

${construction(mixed)}`,
  },
];
