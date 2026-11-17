export interface Preset { name: string; description: string; source: string }
export const PRESETS: Preset[] = [
  {
    name: 'Forager',
    description: 'Heterotroph. Digest organic food, avoid attackers, and grow before dividing.',
    source: `# Organic food supplies energy; dissolved minerals supply extra building material.
let danger = threat.visible;
let away = if threat.angle > 0 then threat.angle - 3.141593
           else threat.angle + 3.141593;
let seeking = food.visible && gut.fraction < 0.8;
let wander = sin(age * 0.7 + id) * 0.5;
turn = if danger then clamp(away * 2, -1, 1)
       else if seeking then clamp(food.angle * 1.8, -1, 1) else wander;
thrust = if danger then 0.8 else if seeking then 0.45
         else if gut.fraction > 0.3 then 0.05 else 0.15;

# Keep feeding enabled so stored food can continue digesting.
feed = 1;
photosynthesize = 0;
absorb = if nutrient.fraction < 0.5 then 1 else 0;
defend = if danger then 0.4 else 0;
repair = if integrity < 0.9 && energy.fraction > 0.6 then 0.3 else 0;
grow = if maturity < 1 && energy.fraction > 0.65
          && nutrient.stored > 0.05 then 0.75 else 0;
divide = maturity >= 1 && energy.fraction > 0.72;

invest.propulsion = 0.22;
invest.photosynthesis = 0;
invest.digestion = 0.45;
invest.armor = 0.12;
invest.sensing = 0.21;
invest.attack = 0;`,
  },
  {
    name: 'Drifter',
    description: 'Heterotroph. A modest motor and 40% unassigned core keep a drifting feeder inexpensive.',
    source: `# Drift gently; use the motor to escape or reach nearby food.
let danger = threat.visible;
let away = if threat.angle > 0 then threat.angle - 3.141593
           else threat.angle + 3.141593;
let seeking = food.visible && gut.fraction < 0.6;
turn = if danger then clamp(away * 2, -1, 1)
       else if seeking then clamp(food.angle * 1.5, -1, 1)
       else sin(age * 0.45 + id) * 0.22;
thrust = if danger then 0.75 else if seeking then 0.3 else 0.06;
feed = 1;
photosynthesize = 0;
absorb = if nutrient.fraction < 0.5 then 1 else 0;
defend = if danger then 0.3 else 0;
grow = if maturity < 1 && energy.fraction > 0.7
          && nutrient.stored > 0.05 then 0.5 else 0;
divide = maturity >= 1 && energy.fraction > 0.8;

invest.propulsion = 0.08;
invest.photosynthesis = 0;
invest.digestion = 0.30;
invest.armor = 0.10;
invest.sensing = 0.12;
invest.attack = 0;`,
  },
  {
    name: 'Conserver',
    description: 'Heterotroph. Spend little on movement, retain an energy reserve, and mature slowly.',
    source: `let danger = threat.visible;
let hungry = energy.fraction < 0.55 && gut.fraction < 0.4;
let away = if threat.angle > 0 then threat.angle - 3.141593
           else threat.angle + 3.141593;
let seeking = food.visible && gut.fraction < 0.7;
turn = if danger then clamp(away * 2, -1, 1)
       else if seeking then clamp(food.angle * 2, -1, 1)
       else sin(age * 0.4 + id) * 0.4;
thrust = if danger then 0.75 else if seeking then 0.35
         else if hungry then 0.12 else 0.03;
feed = 1;
photosynthesize = 0;
absorb = if nutrient.fraction < 0.6 then 1 else 0;
defend = if danger then 0.3 else 0;
grow = if maturity < 1 && energy.fraction > 0.75
          && nutrient.stored > 0.05 then 0.35 else 0;
divide = maturity >= 1 && energy.fraction > 0.85;

# Leave 40% unassigned, while retaining digestion and a lookout.
invest.propulsion = 0.10;
invest.photosynthesis = 0;
invest.digestion = 0.28;
invest.armor = 0.08;
invest.sensing = 0.14;
invest.attack = 0;`,
  },
  {
    name: 'Giant',
    description: 'Heterotroph. Invest in digestion and armor; a large body takes time and food to build.',
    source: `let target = max(3.5, 2 * birth.size);
let danger = threat.visible;
let away = if threat.angle > 0 then threat.angle - 3.141593
           else threat.angle + 3.141593;
let seeking = food.visible && gut.fraction < 0.8;
turn = if danger then clamp(away * 2, -1, 1)
       else if seeking then clamp(food.angle * 1.8, -1, 1)
       else sin(age * 0.6 + id) * 0.4;
thrust = if danger then 0.8 else if seeking then 0.4 else 0.1;
feed = 1;
photosynthesize = 0;
absorb = if nutrient.fraction < 0.6 then 1 else 0;
defend = if danger then 0.5 else 0;
repair = if integrity < 0.9 && energy.fraction > 0.6 then 0.3 else 0;
grow = if size < target && energy.fraction > 0.65
          && nutrient.stored > 0.05 then 0.8 else 0;
divide = size >= target && maturity >= 1 && energy.fraction > 0.8;

invest.propulsion = 0.12;
invest.photosynthesis = 0;
invest.digestion = 0.43;
invest.armor = 0.25;
invest.sensing = 0.20;
invest.attack = 0;`,
  },
  {
    name: 'Rememberer',
    description: 'Heterotroph. Remember food bearings and a feeding mode while keeping watch for attackers.',
    source: `# Reads see old memory. Writes commit together after this tick.
ri001 = ri001 + 1 when !food.visible;
clear ri001 when food.visible;
rf001 = food.angle when food.visible;
clear rf001 when !food.visible && ri001 > 90;
rb001 = if energy.fraction < 0.35 then true
        else if energy.fraction > 0.75 then false else rb001;
rs001 = if food.visible then "forage" else "search";
rv001 = vec(flow.x, flow.y);

let danger = threat.visible;
let away = if threat.angle > 0 then threat.angle - 3.141593
           else threat.angle + 3.141593;
let remembered = populated(rf001) && ri001 < 90;
turn = if danger then clamp(away * 2, -1, 1)
       else if food.visible then clamp(food.angle * 1.8, -1, 1)
       else if remembered then clamp(rf001, -1, 1)
       else sin(age * 0.7 + id) * 0.5;
thrust = if danger then 0.8
         else if rs001 == "forage" && gut.fraction < 0.8 then 0.42
         else if rb001 || length(rv001) > 10 then 0.2 else 0.05;
feed = 1;
photosynthesize = 0;
absorb = if nutrient.fraction < 0.6 then 1 else 0;
defend = if danger then 0.4 else 0;
grow = if maturity < 1 && energy.fraction > 0.65
          && nutrient.stored > 0.05 then 0.6 else 0;
divide = maturity >= 1 && energy.fraction > 0.78;

invest.propulsion = 0.20;
invest.photosynthesis = 0;
invest.digestion = 0.40;
invest.armor = 0.12;
invest.sensing = 0.28;
invest.attack = 0;`,
  },
  {
    name: 'Phototroph',
    description: 'Autotroph. Turn light and dissolved minerals into growth; release organic food for consumers.',
    source: `# A pure producer: no organic feeding and no digestive tissue.
let danger = threat.visible;
let hot = temperature.value > 0.8;
let needs_minerals = nutrient.fraction < 0.15;
let flee = if threat.angle > 0 then threat.angle - 3.141593
           else threat.angle + 3.141593;
let cool = if temperature.angle > 0 then temperature.angle - 3.141593
           else temperature.angle + 3.141593;
turn = if danger then clamp(flee * 2, -1, 1)
       else if hot then clamp(cool * 1.5, -1, 1)
       else if needs_minerals then clamp(nutrient.angle * 1.8, -1, 1)
       else clamp(light.angle * 1.8, -1, 1);
thrust = if danger then 0.8 else if hot then 0.6
         else if needs_minerals then 0.15
         else if light.level < 0.35 then 0.12 else 0.025;
feed = 0;
photosynthesize = 1;
absorb = if nutrient.fraction < 0.7 then 1 else 0;
defend = if danger then 0.4 else 0;
repair = if integrity < 0.9 && energy.fraction > 0.6 then 0.3 else 0;
grow = if maturity < 1 && energy.fraction > 0.62
          && nutrient.stored > 0.1 then 0.65 else 0;
divide = maturity >= 1 && energy.fraction > 0.75;

# Mineral absorption is independent of digestion.
invest.propulsion = 0.06;
invest.photosynthesis = 0.70;
invest.digestion = 0;
invest.armor = 0.14;
invest.sensing = 0.10;
invest.attack = 0;`,
  },
  {
    name: 'Predator',
    description: 'Heterotroph. Split a finite body between attacking and digesting; hunt only when hungry.',
    source: `# Organic feeding and attacks share a finite gut.
let hungry = energy.fraction < 0.72 && gut.fraction < 0.35;
let vulnerable = prey.visible && !prey.same_genome
                 && prey.attack < body.attack;
let hunt = hungry && vulnerable;
# Contact distance follows the cells' radius = 7 * sqrt(size).
let contact = prey.distance <= 7 * (sqrt(size) + sqrt(prey.size)) + 0.75;
let seeking = food.visible && gut.fraction < 0.7;
let escape = threat.visible && threat.size > size * 1.25;
let away = if threat.angle > 0 then threat.angle - 3.141593
           else threat.angle + 3.141593;
turn = if escape then clamp(away * 2, -1, 1)
       else if hunt then clamp(prey.angle * 2, -1, 1)
       else if seeking then clamp(food.angle * 1.8, -1, 1)
       else sin(age * 0.6 + id) * 0.4;
thrust = if escape then 0.85 else if hunt then 0.6
         else if seeking then 0.4 else if gut.fraction > 0.3 then 0.04 else 0.12;
feed = 1;
photosynthesize = 0;
bite = if hunt && contact then 0.8 else 0;
engulf = hunt && contact && prey.integrity <= 0.35
         && size >= 1.6 * prey.size && gut.fraction < 0.35;
defend = if escape then 0.5 else 0;
repair = if integrity < 0.9 && energy.fraction > 0.6 then 0.3 else 0;
absorb = if nutrient.fraction < 0.6 then 1 else 0;
grow = if maturity < 1 && energy.fraction > 0.7
          && nutrient.stored > 0.05 then 0.6 else 0;
divide = maturity >= 1 && energy.fraction > 0.8;

# Attack tissue has to displace another capability.
invest.propulsion = 0.23;
invest.photosynthesis = 0;
invest.digestion = 0.28;
invest.armor = 0.12;
invest.sensing = 0.12;
invest.attack = 0.25;`,
  },
  {
    name: 'Mixotroph',
    description: 'Mixotroph. Combine light harvesting with organic feeding, trading specialist capacity for flexibility.',
    source: `let danger = threat.visible;
let away = if threat.angle > 0 then threat.angle - 3.141593
           else threat.angle + 3.141593;
let seeking = food.visible && gut.fraction < 0.55 && energy.fraction < 0.8;
turn = if danger then clamp(away * 2, -1, 1)
       else if seeking then clamp(food.angle * 1.8, -1, 1)
       else clamp(light.angle * 1.8, -1, 1);
thrust = if danger then 0.8 else if seeking then 0.35
         else if light.level < 0.3 then 0.12 else 0.035;
feed = 1;
photosynthesize = if light.level > 0.08 then 1 else 0;
absorb = if nutrient.fraction < 0.6 then 1 else 0;
defend = if danger then 0.4 else 0;
repair = if integrity < 0.9 && energy.fraction > 0.6 then 0.3 else 0;
grow = if maturity < 1 && energy.fraction > 0.7
          && nutrient.stored > 0.05 then 0.5 else 0;
divide = maturity >= 1 && energy.fraction > 0.8;

# Both energy paths share the same body budget.
invest.propulsion = 0.12;
invest.photosynthesis = 0.35;
invest.digestion = 0.30;
invest.armor = 0.10;
invest.sensing = 0.13;
invest.attack = 0;`,
  },
];
