export interface Preset { name: string; description: string; source: string }
export const PRESETS: Preset[] = [
  {
    name: 'Forager',
    description: 'Invest in digestion and sensing. Gather food and minerals, then grow and divide.',
    source: `# Follow food. Wander when nothing is near.
let wander = sin(age * 0.7 + id) * 0.6;
let seek = clamp(food.angle * 1.8, -1, 1);

turn = if food.visible then seek else wander;
thrust = if food.visible then 0.8 else 0.45;

# Growth needs stored nutrients as well as energy.
absorb = 1;
grow = if size < 2 && energy > 70 * size && nutrient.stored > 0.02 then 1 else 0;
divide = size >= 2 && energy > 125 * size && age > 5;

# All five targets are explicit; the body changes gradually.
invest.propulsion = 0.25;
invest.photosynthesis = 0;
invest.digestion = 0.35;
invest.armor = 0.10;
invest.sensing = 0.30;`,
  },
  {
    name: 'Drifter',
    description: 'Drift with a modest motor and an inexpensive body. Grow gradually to 1.5× biomass.',
    source: `# Gentle spirals through the food field.
let wave = sin(age * 0.45 + id);

turn = wave * 0.22;
thrust = 0.38;

# Grow slowly, keeping an energy reserve.
absorb = 1;
grow = if size < 1.5 && energy > 90 * size && nutrient.stored > 0.02 then 0.5 else 0;
divide = size >= 1.5 && energy > 145 * size && age > 8;

# The remaining 35% is unassigned core.
invest.propulsion = 0.12;
invest.photosynthesis = 0;
invest.digestion = 0.30;
invest.armor = 0.08;
invest.sensing = 0.15;`,
  },
  {
    name: 'Conserver',
    description: 'Stay small and leave 40% of the body unassigned to reduce maintenance.',
    source: `# Sense, then respond with pure expressions.
let hungry = energy < 100 * size;
let seek = clamp(food.angle * 2, -1, 1);
let wander = sin(age * 0.4 + id) * 0.5;

turn = if food.visible then seek else wander;
thrust = if food.visible then 0.7
         else if hungry then 0.3 else 0.15;

absorb = if nutrient.fraction < 0.6 then 1 else 0;
grow = if size < 1 && energy > 70 * size && nutrient.stored > 0.02 then 1 else 0;
divide = size >= 1 && energy > 140 * size && age > 6;

invest.propulsion = 0.15;
invest.photosynthesis = 0;
invest.digestion = 0.25;
invest.armor = 0.05;
invest.sensing = 0.15;`,
  },
  {
    name: 'Giant',
    description: 'A large digestive investment supports growth to 3.5× biomass, given enough minerals.',
    source: `# A larger body has a larger appetite.
let target = 3.5;
let seek = clamp(food.angle * 1.8, -1, 1);
let wander = sin(age * 0.6 + id) * 0.5;

turn = if food.visible then seek else wander;
thrust = if food.visible then 0.85 else 0.4;

absorb = 1;
grow = if size < target && energy > 60 * size && nutrient.stored > 0.02 then 1 else 0;
divide = size >= target && energy > 125 * size && age > 10;

invest.propulsion = 0.20;
invest.photosynthesis = 0;
invest.digestion = 0.40;
invest.armor = 0.15;
invest.sensing = 0.25;`,
  },
  {
    name: 'Rememberer',
    description: 'Keeps a food bearing, a search counter, and a feeding mode in typed memory.',
    source: `# Reads see the previous tick. Writes commit together.
ri001 = ri001 + 1 when !food.visible;
clear ri001 when food.visible;
rf001 = food.angle when food.visible;
clear rf001 when !food.visible && ri001 > 90;
rb001 = if energy.fraction < 0.35 then true
        else if energy.fraction > 0.75 then false else rb001;
rs001 = if food.visible then "forage" else "search";
rv001 = vec(flow.x, flow.y);

let remembered = populated(rf001) && ri001 < 90;
let wander = sin(age * 0.7 + id) * 0.6;
turn = if food.visible then clamp(food.angle * 1.8, -1, 1)
       else if remembered then clamp(rf001, -1, 1) else wander;
thrust = if rs001 == "forage" then 0.8
         else if rb001 || length(rv001) > 10 then 0.5 else 0.3;
absorb = 1;
grow = if size < 2 && energy.fraction > 0.45 && nutrient.stored > 0.02 then 1 else 0;
divide = size >= 2 && energy.fraction > 0.72;

invest.propulsion = 0.25;
invest.photosynthesis = 0;
invest.digestion = 0.30;
invest.armor = 0.10;
invest.sensing = 0.35;`,
  },
  {
    name: 'Phototroph',
    description: 'Build a photosynthetic body. Seek light and mineral-rich water, and avoid excess heat.',
    source: `# Try the Sunlit pond environment, or place a light.
let hot = temperature.value > 0.8;
let needs_minerals = nutrient.fraction < 0.25;
let away = if temperature.angle > 0
           then temperature.angle - 3.141593
           else temperature.angle + 3.141593;
turn = if hot then clamp(away * 1.5, -1, 1)
       else if needs_minerals then clamp(nutrient.angle * 1.8, -1, 1)
       else clamp(light.angle * 1.8, -1, 1);
thrust = if hot then 0.85 else if needs_minerals then 0.5
         else if light.level < 0.6 then 0.45 else 0.08;
photosynthesize = if light.level > 0.08 then 1 else 0;
absorb = 1;
repair = if integrity < 0.9 && energy.fraction > 0.4 then 0.5 else 0;
grow = if size < 2 && energy.fraction > 0.45 && nutrient.stored > 0.02 then 1 else 0;
divide = size >= 2 && energy.fraction > 0.72;

# Digestion also enables uptake from the water.
invest.propulsion = 0.12;
invest.photosynthesis = 0.50;
invest.digestion = 0.20;
invest.armor = 0.08;
invest.sensing = 0.10;`,
  },
  {
    name: 'Predator',
    description: 'Invest in movement, digestion, and armor. Recycle prey into energy and minerals.',
    source: `# Targets are chosen by the engine from the sensed world.
let hunt = prey.visible && !prey.same_genome;
let seek = if hunt then prey.angle else food.angle;
let wander = sin(age * 0.6 + id) * 0.5;
turn = if hunt || food.visible then clamp(seek * 2, -1, 1) else wander;
thrust = if hunt then 0.95 else if food.visible then 0.8 else 0.4;
engulf = hunt && size >= 1.6 * prey.size;
bite = if hunt then 1 else 0;
defend = if threat.visible then 0.6 else 0;
repair = if integrity < 0.9 && energy.fraction > 0.4 then 0.5 else 0;
absorb = 1;
grow = if size < 3 && energy.fraction > 0.4 && nutrient.stored > 0.02 then 1 else 0;
divide = size >= 3 && energy.fraction > 0.78 && age > 10;

invest.propulsion = 0.28;
invest.photosynthesis = 0;
invest.digestion = 0.35;
invest.armor = 0.17;
invest.sensing = 0.20;`,
  },
];
