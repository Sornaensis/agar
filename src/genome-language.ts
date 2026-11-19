import { ORGANELLE_CATALOG, MECHANISM_CATALOG } from './organelles';

/** Editor assistance for the language in engine/src/genome.rs; the Rust compiler remains authoritative. */
export type PrimitiveType = 'integer' | 'scalar' | 'boolean' | 'string' | 'vec2';
export type GenomeSymbolCategory = 'function' | 'signal' | 'output' | 'keyword' | 'register' | 'binding';
export interface GenomeSymbol {
  readonly name: string;
  readonly category: GenomeSymbolCategory;
  /** A primitive label, or an explicitly polymorphic result such as T or integer | scalar. */
  readonly type: string;
  readonly signature: string;
  readonly description: string;
  readonly insertText?: string;
  /** A complete, valid usage example shown by the reference browser. */
  readonly example?: string;
}
export interface GenomeBinding {
  readonly name: string;
  readonly type: string;
  /** UTF-16 source offsets from Engine.analyze(). */
  readonly from: number;
  readonly to: number;
}

const symbol = (name: string, category: GenomeSymbolCategory, type: string, signature: string, description: string): GenomeSymbol =>
  Object.freeze({ name, category, type, signature, description });
const fn = (name: string, parameters: string, type: string, description: string): GenomeSymbol =>
  symbol(name, 'function', type, `${name}(${parameters}) → ${type}`, description);

/** In signatures, number accepts integer or scalar; T accepts any one primitive type. */
export const BUILTIN_FUNCTIONS: readonly GenomeSymbol[] = Object.freeze([
  fn('sin', 'angle: number', 'scalar', 'Sine of an angle in radians.'),
  fn('cos', 'angle: number', 'scalar', 'Cosine of an angle in radians.'),
  fn('abs', 'value: number', 'integer | scalar', 'Absolute value. Preserves the input numeric type.'),
  fn('min', 'a: number, b: number', 'integer | scalar', 'Smaller value. Returns integer when both inputs are integers; otherwise scalar.'),
  fn('max', 'a: number, b: number', 'integer | scalar', 'Larger value. Returns integer when both inputs are integers; otherwise scalar.'),
  fn('clamp', 'value: number, a: number, b: number', 'integer | scalar', 'Limits a value between a and b; either bound order is accepted. Returns integer only when all three inputs are integers.'),
  fn('int', 'value: number', 'integer', 'Converts a number to a signed 32-bit integer by truncating toward zero and clamping to its range.'),
  fn('float', 'value: number', 'scalar', 'Converts an integer or scalar to a scalar number.'),
  fn('vec', 'x: number, y: number', 'vec2', 'Constructs a two-component vector. Both components accept integers or scalars.'),
  symbol('length', 'function', 'scalar | integer', 'length(value: vec2) → scalar; length(value: string) → integer', 'Vector magnitude, or the number of Unicode characters in a string. String length counts characters, not UTF-8 bytes.'),
  fn('dot', 'a: vec2, b: vec2', 'scalar', 'Dot product of two vectors.'),
  fn('normalize', 'value: vec2', 'vec2', 'Returns a unit vector with the same direction. The zero vector remains zero.'),
  fn('angle', 'value: vec2', 'scalar', 'Vector direction in radians, computed as atan2(y, x). Returns zero for the zero vector.'),
  fn('x', 'value: vec2', 'scalar', 'The first component of a vector.'),
  fn('y', 'value: vec2', 'scalar', 'The second component of a vector.'),
  fn('sqrt', 'value: number', 'scalar', 'Square root. Negative inputs return zero.'),
  fn('sign', 'value: number', 'scalar', 'Returns -1, 0, or 1 as a scalar according to the input sign.'),
  fn('atan2', 'y: number, x: number', 'scalar', 'Angle in radians from the x axis to (x, y). Returns zero when both inputs are zero.'),
  fn('lerp', 'a: number, b: number, amount: number', 'scalar', 'Computes a + (b - a) × amount. Amount is not clamped, so values outside 0–1 extrapolate.'),
  fn('noise', 'seed: number, phase: number', 'scalar', 'Deterministic smooth noise in -1–1. The same seed and phase always give the same result; use time or age as a changing phase.'),
  fn('activate', 'concentration: number, threshold: number', 'scalar', 'Saturating activation: c / (c + k), with c and k clamped to nonnegative values. Returns zero when both are zero.'),
  fn('repress', 'concentration: number, threshold: number', 'scalar', 'Saturating repression: 1 - activate(concentration, threshold). Returns one when both arguments are zero. Creates no persistent state.'),
  fn('identity', 'value: T', 'T', 'Returns the same value and primitive type. Accepts an integer, scalar, boolean, string, or vector.'),
  fn('populated', 'register: address', 'boolean', 'Tests whether a register is occupied at the start of the tick. Accepts a literal register address, not a value expression. Stored zero, false, and empty strings are still populated.'),
]);

const sensorRows: ReadonlyArray<readonly [string, PrimitiveType, string]> = [
  ['energy', 'scalar', 'Current usable energy reserve. It is separate from undigested and embodied structural energy.'],
  ['energy.fraction', 'scalar', 'Usable energy divided by current built storage capacity, clamped to 0–1.'],
  ['energy.capacity', 'scalar', 'Usable-energy storage provided by the core and built reserve vacuoles.'],
  ['age', 'scalar', 'This organism’s age in seconds.'],
  ['time', 'scalar', 'Elapsed simulation time in seconds.'],
  ['speed', 'scalar', 'Current velocity magnitude in world units per second.'],
  ['id', 'scalar', 'This organism’s stable numeric identifier. This is self-knowledge, not an object tracking token.'],
  ['integrity', 'scalar', 'Overall structural health from 0 to 1.'],
  ['memory.used', 'scalar', 'Number of populated registers at the start of this tick.'],
  ['memory.cost', 'scalar', 'Register maintenance in energy per second: 0.0025 × memory.used.'],
  ['body.area', 'scalar', 'Supported envelope area. Its frame and membrane must be grown using energy and minerals; collision radius is 7 × sqrt(body.area).'],
  ['body.mass', 'scalar', 'Actual mineral material: frame, organelles, assembly proteins, free reserves and gut minerals, each counted once. Energy does not add material mass.'],
  ['body.birth_area', 'scalar', 'Envelope area at this organism’s most recent birth or division.'],
  ['body.maturity', 'scalar', 'Envelope growth toward division, clamped to 0–1. Area must double from body.birth_area. Other physical requirements still apply.'],
  ['body.division_ready', 'boolean', 'Whether the current body can divide: sufficient area, energy, cooldown, viable cores and a conservative organelle partition.'],
  ['body.space.free', 'scalar', 'Unused internal space. Assembly proteins, organelles and storage compartments occupy room even while organs are damaged or incomplete.'],
  ['body.space.used', 'scalar', 'Internal space occupied by assembly proteins, organs and their compartments. Contents are not counted a second time.'],
  ['body.space.capacity', 'scalar', 'Internal capacity supported by the paid envelope.'],
  ['body.surface.free', 'scalar', 'Unused membrane attachment capacity. External organs need both anchors and internal support.'],
  ['body.surface.used', 'scalar', 'Membrane attachment capacity occupied by built and incomplete organs.'],
  ['body.surface.capacity', 'scalar', 'Attachment capacity, which grows with the envelope perimeter rather than its area.'],
  ['nutrient.stored', 'scalar', 'Free mineral reserves available for frame growth, construction and repair.'],
  ['nutrient.capacity', 'scalar', 'Mineral storage capacity of the core and built reserve vacuoles.'],
  ['nutrient.fraction', 'scalar', 'Stored minerals divided by current free-reserve capacity, clamped to 0–1.'],
  ['gut.energy', 'scalar', 'Raw organic energy waiting in digestive compartments. It is not yet usable.'],
  ['gut.capacity', 'scalar', 'Raw-energy storage provided by functional digestive vacuoles.'],
  ['gut.fraction', 'scalar', 'Gut energy divided by digestive capacity, clamped to 0–1. Zero when capacity is absent.'],
  ['digestion.rate', 'scalar', 'Chemical substrate potential processed per second during the last completed tick. Digestion releases organic carbon; oxygen-dependent respiration supplies usable energy.'],
  ['growth.rate', 'scalar', 'Envelope area added per second during the last completed tick.'],
  ['lifespan', 'scalar', 'Compatibility signal, always zero: cells have no age limit. Use age for elapsed lifetime; survival depends on resources and structural integrity.'],
  ['exudation.rate', 'scalar', 'Raw organic energy released as particles per second during the last completed tick.'],
  ...ORGANELLE_CATALOG.flatMap(({kind, name}) => [
    [`organelle.${kind}.count`, 'integer', `Actual ${name.toLowerCase()} instance count, including unfinished or damaged instances.`],
    [`organelle.${kind}.mass`, 'scalar', `Material held in this cell’s ${name.toLowerCase()} instances. Damage does not make this material disappear.`],
    [`organelle.${kind}.capacity`, 'scalar', `Equivalent minimum-sized functional units of ${name.toLowerCase()} machinery, after damage. Unbuilt or subthreshold modules provide none.`],
    [`organelle.${kind}.damaged`, 'scalar', `Damaged material in ${name.toLowerCase()} instances. Repair or explicit recycling is needed to recover function or space.`],
    [`organelle.${kind}.building`, 'scalar', `Material still missing before unfinished ${name.toLowerCase()} instances reach their minimum functional size. Their already-built material still occupies space.`],
  ] as Array<[string, PrimitiveType, string]>),
  ['light.available', 'boolean', 'A functional photoreceptor is present. Photosynthetic machinery alone does not reveal light measurements.'],
  ['light.valid', 'boolean', 'This tick has a usable photoreceptor measurement. Guard light values with this flag.'],
  ['light.level', 'scalar', 'Measured local brightness, from 0 to 1; resolution depends on built photoreceptors. Zero when invalid.'],
  ['light.bearing', 'scalar', 'Measured direction of increasing brightness relative to heading, in radians. A flat gradient gives zero.'],
  ['heat.available', 'boolean', 'A functional thermoreceptor is present. Heat still affects cells without one.'],
  ['heat.valid', 'boolean', 'This tick has a usable thermoreceptor measurement.'],
  ['heat.value', 'scalar', 'Measured temperature from 0 to 1. Around 0.5 is mild; zero without a valid reading does not mean cold water.'],
  ['heat.bearing', 'scalar', 'Measured direction of increasing temperature relative to heading, in radians.'],
  ['chemical.available', 'boolean', 'A functional chemoreceptor is present.'],
  ['chemical.valid', 'boolean', 'This tick has usable local chemical measurements.'],
  ['chemical.mineral', 'scalar', 'Measured local dissolved mineral concentration. This is a local field reading, not the contents of a nearby object.'],
  ['chemical.organic', 'scalar', 'Measured local organic cue concentration from release, leakage and decay. A cue does not guarantee edible material.'],
  ['chemical.mineral_bearing', 'scalar', 'Measured direction of increasing dissolved mineral concentration relative to heading, in radians.'],
  ['chemical.organic_bearing', 'scalar', 'Measured direction of increasing organic cue concentration relative to heading, in radians.'],
  ['flow.available', 'boolean', 'A functional mechanoreceptor is present. Currents still carry unsensing cells.'],
  ['flow.valid', 'boolean', 'This tick has a usable local flow measurement.'],
  ['flow.vector', 'vec2', 'Measured local relative water flow in the cell’s body frame. Zero vector when invalid.'],
  ['flow.speed', 'scalar', 'Magnitude of the measured relative water flow.'],
  ['flow.bearing', 'scalar', 'Measured relative-flow direction in radians from the cell’s heading.'],
  ['contact.available', 'boolean', 'A functional mechanoreceptor is present. Physical capture and collisions do not require awareness.'],
  ['contact.valid', 'boolean', 'One physical contact was detected this tick.'],
  ['contact.token', 'string', 'Opaque token for the observed contacting object. Tokens identify observations, not hidden properties or remote targets. Empty when invalid.'],
  ['contact.offset', 'vec2', 'Measured body-relative displacement toward the contacting surface. Zero vector when invalid.'],
  ['contact.pressure', 'scalar', 'Measured pressure of the selected physical contact.'],
  ['contact.resistance', 'scalar', 'Measured mechanical resistance of the contacted surface. It does not expose genotype, diet or energy.'],
  ['object.available', 'boolean', 'A functional optical detector is present.'],
  ['object.valid', 'boolean', 'A contrasting object was detected within the current scan and visibility limits. Cells and particles use the same return format.'],
  ['object.token', 'string', 'Opaque token stable during a continuous observed track. Store it in rs001 to request reacquisition; it cannot reveal an unseen location. Empty when invalid.'],
  ['object.offset', 'vec2', 'Estimated body-relative displacement to the selected visible object. angle(object.offset) gives a steering bearing.'],
  ['object.area', 'scalar', 'Estimated apparent area of the visible object, not its exact material mass.'],
  ['object.contrast', 'scalar', 'Measured optical contrast. Selection depends on visibility, angle and distance, never hidden energy.'],
  ['object.pigment', 'scalar', 'Measured visible surface appearance. This is not a species, feeding-strategy or edible flag.'],
  ['object.motion', 'vec2', 'Estimated relative motion of the visible object in the cell’s body frame. Zero when invalid.'],
  ['gut.exudate.energy', 'scalar', 'Raw energy in captured soluble organic releases. Exudate is readily processed; it is separate from usable energy.'],
  ['gut.exudate.nutrients', 'scalar', 'Mineral material carried with exudate in the gut. Digestion can transfer it to free reserves.'],
  ['gut.tissue.energy', 'scalar', 'Raw energy in captured cell tissue. Tissue takes more digestive work than exudate and produces refractory waste.'],
  ['gut.tissue.nutrients', 'scalar', 'Mineral material in captured cell tissue. Some remains in the waste produced during digestion.'],
  ['gut.waste.energy', 'scalar', 'Residual raw energy in captured waste. Waste is slow to process and yields less usable energy.'],
  ['gut.waste.nutrients', 'scalar', 'Mineral material in captured waste. Processing returns unassimilated minerals to the surrounding water.'],
  ['expression.rate', 'scalar', 'Assembly-protein material synthesized per second during the last completed tick. Synthesis spends energy and minerals and shares finite core throughput with assembly and growth.'],
  ['assembly.rate', 'scalar', 'Organelle material assembled per second during the last completed tick. Assembly consumes previously synthesized proteins of the matching family.'],
  ['assembly.capacity', 'scalar', 'Current shared core work capacity in material-equivalent units per second. Protein synthesis, organelle assembly, frame growth, recycling and repair compete for it.'],
  ...ORGANELLE_CATALOG.map(({kind, name}) => [
    `protein.${kind}`, 'scalar', `Unassembled ${name.toLowerCase()} protein material. Only this family can use it. Stocks consume internal space and upkeep, turn over with a 45-second half-life, and partition at division. Express only what the body can use.`,
  ] as [string, PrimitiveType, string]),
  ['carbon.organic', 'scalar', 'Organic carbon reserves available for respiration and construction. Structural carbon and gut contents are separate.'],
  ['carbon.inorganic', 'scalar', 'Internal dissolved inorganic carbon available for fixation or passive release through the membrane.'],
  ['oxygen', 'scalar', 'Internal oxygen available for aerobic respiration. Membrane exchange follows the concentration gradient.'],
  ['respiration.rate', 'scalar', 'Organic carbon oxidized per second during the last completed tick. The same amount of oxygen is consumed and inorganic carbon produced.'],
  ['carbon.fixation.rate', 'scalar', 'Inorganic carbon fixed into organic carbon per second during the last completed tick. The same amount of oxygen is produced.'],
];
export const BUILTIN_SIGNALS: readonly GenomeSymbol[] = Object.freeze(sensorRows.map(([name, type, description]) =>
  symbol(name, 'signal', type, `${name}: ${type}`, `${description} Sampled before actions this tick.`)));

const output = (name: string, type: PrimitiveType, description: string): GenomeSymbol =>
  symbol(name, 'output', type, `${name} = ${type};`, description);
export const BUILTIN_OUTPUTS: readonly GenomeSymbol[] = Object.freeze([
  output('thrust', 'scalar', 'Required. Forward effort, clamped to 0–1. Built flagella provide movement; force, drag and energy cost depend on the physical body.'),
  output('turn', 'scalar', 'Required. Signed steering effort, clamped to -1–1. Without functional flagella, this request cannot turn the cell.'),
  output('divide', 'boolean', 'Required. Requests conservative division when body.division_ready is true. Actual organs are partitioned, never cloned; the daughter starts with empty registers and tracking memory.'),
  output('grow', 'scalar', 'Paid frame and envelope growth effort, clamped to 0–1; defaults to 0. Adds space without granting capabilities. Construction shares finite energy, minerals and assembly throughput.'),
  output('photosynthesize', 'scalar', 'Light-harvesting effort, clamped to 0–1; defaults to 0. Requires built photosynthetic machinery, processing capacity and shared local light. Organic release supports consumers.'),
  output('engulf', 'boolean', 'Requests contact capture; defaults to false. Aperture, force, object geometry, energy and available digestive space determine whether capture succeeds.'),
  output('bite', 'scalar', 'Contact attack effort, clamped to 0–1; defaults to 0. Requires a built piercing apparatus. It can damage a nearby surface and transfer accessible material into available digestive space.'),
  output('defend', 'scalar', 'Active defense effort, clamped to 0–1; defaults to 0. Built cortex adds passive resistance; active defense also costs energy.'),
  output('repair', 'scalar', 'Paid repair effort, clamped to 0–1; defaults to 0. The core provides slow repair; repair complexes raise throughput. Damaged organs retain their material and space until repaired or recycled.'),
  output('absorb', 'scalar', 'Mineral uptake effort, clamped to 0–1; defaults to 1. Requires built mineral transporters, energy, local supply and reserve room.'),
  output('feed', 'scalar', 'Overall organic intake and digestion effort, clamped to 0–1; defaults to 1. Capture and digestion require built machinery. The digest.* controls divide finite processing work between gut contents.'),
  ...ORGANELLE_CATALOG.flatMap(({kind, name, minimumMass}) => [
    output(`target.${kind}.count`, 'integer', `Assembly target for ${name.toLowerCase()} instances; nonnegative integer, default 0. Matching proteins assemble toward this target. express.${kind} supplies protein; recycle.${kind} dismantles excess units.`),
    output(`target.${kind}.size`, 'scalar', `Requested material per ${name.toLowerCase()} instance; default and minimum ${minimumMass}. Actual instances can differ through partial construction or damage.`),
    output(`express.${kind}`, 'scalar', `Expression effort for ${name.toLowerCase()} assembly proteins, clamped to 0–1; default 0. Synthesis spends energy and minerals, shares core throughput, and occupies internal space. Existing proteins assemble toward targets on later ticks even after expression stops. Excess stock has upkeep and decays; use protein.${kind} and actual organ mass for feedback.`),
    output(`recycle.${kind}`, 'scalar', `Explicit dismantling effort for ${name.toLowerCase()} material above its targets, clamped to 0–1; default 0. Costs time and energy, recovers only accounted material and retained energy, and spills excess contents safely. Essential core remains protected.`),
  ]),
  output('scan.bearing', 'scalar', 'Body-relative scan center in radians, wrapped to −π…π; defaults to 0. Takes effect on the following tick.'),
  output('scan.width', 'scalar', 'Requested scan width in radians, clamped to 0…2π; defaults to a full circle. Built detector coverage, visibility and occlusion still limit observations. Takes effect on the following tick.'),
  output('scan.target', 'string', 'Optional opaque token to reacquire within the next physical scan; defaults to an empty string. It grants no access outside the detector’s observable region.'),
  output('digest.exudate', 'scalar', 'Processing effort for gut exudate, clamped to 0–1; defaults to 1. Transfers accessible organic carbon into reserves. Shares processing capacity with tissue and waste; usable energy requires respiration.'),
  output('digest.tissue', 'scalar', 'Processing effort for gut tissue, clamped to 0–1; defaults to 1. Transfers accessible carbon into reserves and retains refractory carbon and minerals in waste.'),
  output('digest.waste', 'scalar', 'Processing effort for gut waste, clamped to 0–1; defaults to 0.35. Slowly extracts accessible carbon. Remaining carbon stays in waste and unassimilated minerals return to water.'),
]);

export const LANGUAGE_KEYWORDS: readonly GenomeSymbol[] = Object.freeze([
  symbol('let', 'keyword', 'binding', 'let name = expression;', 'Defines an immutable signal from an expression. It is recomputed each tick and must be declared before use. Builtin names cannot be shadowed.'),
  symbol('if', 'keyword', 'T', 'if condition then value else other', 'Chooses between two expressions. The condition must be boolean; branches must have the same type or both be numeric. Mixed integer/scalar branches produce a scalar.'),
  symbol('then', 'keyword', 'syntax', 'if condition then value else other', 'Introduces the value selected when an if condition is true.'),
  symbol('else', 'keyword', 'syntax', 'if condition then value else other', 'Introduces the value selected when an if condition is false. Every if expression needs an else branch.'),
  symbol('when', 'keyword', 'syntax', 'rf001 = value when condition;', 'Optional boolean guard on a register write or clear. Guards read the previous tick’s memory; outputs and let bindings do not accept when.'),
  symbol('clear', 'keyword', 'syntax', 'clear rf001 when condition;', 'Empties a register, optionally guarded by when. Clear wins if a write and clear both fire. A register may have one write rule and one clear rule.'),
  symbol('true', 'keyword', 'boolean', 'true: boolean', 'Boolean true. Booleans are not interchangeable with numbers.'),
  symbol('false', 'keyword', 'boolean', 'false: boolean', 'Boolean false. Booleans are not interchangeable with numbers.'),
]);

/** Cellular declarations are kept separate from the v3 compatibility catalog. */
export const CELLULAR_PRODUCT_ROLES = Object.freeze([
  'scaffold', 'anchor', 'motor', 'filament', 'pigment', 'enzyme', 'membrane', 'transporter',
  'receptor', 'regulator', 'replicase', 'constrictor',
] as const);
export const CELLULAR_CONSTRUCTORS: readonly GenomeSymbol[] = Object.freeze(CELLULAR_PRODUCT_ROLES.map(role =>
  symbol(role, 'function', 'product', `${role}(${role === 'regulator' ? 'halfLife: number' : ''}) → product`,
    role === 'regulator'
      ? 'Declares a persistent regulatory product. Its positive half-life is measured in seconds; pool.Name reads its physical stock. Synthesis costs material and energy, and stock partitions at division.'
      : `Declares a ${role} product. Genes synthesize its stock; assembly recipes consume it as a physical component. A declaration by itself creates no material.`)));
export const CELLULAR_KEYWORDS: readonly GenomeSymbol[] = Object.freeze([
  { ...symbol('cellular', 'keyword', 'mode', 'cellular;', 'Starts a cellular program. Declare products, express them with genes, and combine their stocks into physical assemblies.'), insertText: 'cellular;\n' },
  { ...symbol('product', 'keyword', 'declaration', 'product Name = motor();', 'Names a typed material product. Product roles determine which physical recipes it can satisfy.'), insertText: 'product ' },
  { ...symbol('gene', 'keyword', 'expression', 'gene drive: Motor = expression;', 'Allocates synthesis effort to a named product. Effort is bounded to 0–1; actual production shares finite translation work, minerals, energy and space.'), insertText: 'gene ' },
  { ...symbol('assembly', 'keyword', 'recipe', 'assembly Swim: propel() { components; }', 'Names a physical recipe. Its product masses determine composition; stocks from the start of the tick are consumed as components are installed.'), insertText: 'assembly ' },
  { ...symbol('fn', 'keyword', 'declaration', 'fn response(value, threshold) = expression;', 'Defines a pure function with numeric arguments. It can read earlier bindings and start-of-tick registers. Calls expand into the bounded expression graph.'), insertText: 'fn ' },
  { ...symbol('publish', 'keyword', 'contact message', 'publish rf001 = 17;', 'Exposes one occupied scalar register to a touching cell. The optional initializer sets its value at birth. Later register writes change the message, and clearing it withdraws the message.'), insertText: 'publish ' },
  { ...symbol('avoid_message', 'keyword', 'contact message', 'avoid_message rf001;', 'Skips an attack on a candidate whose published scalar exactly matches this occupied register. Each candidate is checked separately.'), insertText: 'avoid_message ' },
  { ...symbol('signal', 'keyword', 'contact signal', 'signal rf002 = 0;', 'Exposes a second scalar to a touching receptor. Assign or clear this register to communicate while keeping the recognition marker stable.'), insertText: 'signal ' },
  { ...symbol('adhere_message', 'keyword', 'contact message', 'adhere_message rf001;', 'Requests adhesion on contact when the neighbor publishes this occupied register’s value. Both cells must match and opt in.'), insertText: 'adhere_message ' },
  { ...symbol('start_when', 'keyword', 'boolean', 'start_when: condition;', 'Allows another instance of this recipe to start when true; defaults to true. Existing incomplete instances continue paid installation when this becomes false.'), insertText: 'start_when: ' },
  { ...symbol('recycle', 'keyword', 'scalar', 'recycle: expression;', 'Requests paid disassembly of machinery with this recipe name, bounded to 0–1; defaults to zero. Requires a built, active recycle() mechanism. Recovered ingredients keep their original identities and become available on a later tick.'), insertText: 'recycle: ' },
  { ...symbol('activity', 'keyword', 'scalar', 'activity: expression;', 'Controls the activity of this assembly, bounded to 0–1. Activity does not create missing components or substitute for a finished machine.'), insertText: 'activity: ' },
  { ...symbol('at', 'keyword', 'radians', 'at: 0;', 'Body-relative attachment angle in radians. Placement and force direction determine a motor’s force and torque.'), insertText: 'at: 0;' },
  { ...symbol('direction', 'keyword', 'radians', 'direction: 0;', 'Body-relative force direction in radians. An off-center force can rotate the body as well as translate it.'), insertText: 'direction: 0;' },
].map(item => Object.freeze(item)));
export const CELLULAR_READ_SIGNALS: readonly GenomeSymbol[] = Object.freeze(BUILTIN_SIGNALS.filter(item =>
  ['energy', 'energy.fraction', 'energy.capacity', 'age', 'speed', 'integrity', 'oxygen', 'respiration.rate', 'memory.used', 'memory.cost'].includes(item.name)
  || (item.name.startsWith('body.') && item.name !== 'body.division_ready')
  || /^(nutrient|gut|light|heat|chemical|flow|contact|object|carbon)\./.test(item.name)));
export const CELLULAR_ASSEMBLY_KINDS: readonly GenomeSymbol[] = Object.freeze([
  ...ORGANELLE_CATALOG.map(item => symbol(item.kind, 'keyword', 'assembly', `${item.kind} { components; activity: expression; }`, item.description)),
  symbol('membrane', 'keyword', 'assembly', 'membrane { components; activity: expression; }', 'Membrane and enzyme products support paid envelope growth, adding space with finite minerals, energy and core work.'),
  symbol('replicator', 'keyword', 'assembly', 'replicator { components; activity: expression; }', 'Scaffold and replicase products build machinery that performs paid genome replication over time.'),
  symbol('constrictor', 'keyword', 'assembly', 'constrictor { components; activity: expression; }', 'Anchor, motor and constrictor products build machinery that advances physical constriction after growth and replication.'),
]);
export const CELLULAR_MECHANISMS: readonly GenomeSymbol[] = Object.freeze(MECHANISM_CATALOG.map(item =>
  symbol(item.kind, 'function', 'mechanism', `${item.kind} { components; activity: expression; }`, item.description)));
const mechanismConstructorByName = new Map(MECHANISM_CATALOG.map(item => [item.constructor,
  symbol(item.constructor, 'function', 'mechanism', `${item.constructor}(${MECHANISM_CATALOG.filter(other=>other.constructor===item.constructor).flatMap(other=>other.target?[other.target]:[]).join(' | ')})`,
    MECHANISM_CATALOG.filter(other=>other.constructor===item.constructor).map(other=>other.description).join(' '))]));
const cellularSymbolByName = new Map([...CELLULAR_CONSTRUCTORS, ...CELLULAR_KEYWORDS].map(item => [item.name, item]));

export interface CellularDeclarations {
  products: { name: string; role: string; from: number; to: number }[];
  genes: { name: string; product: string; from: number; to: number }[];
  assemblies: { name: string; kind: string; from: number; to: number }[];
  functions: { name: string; parameters: string[]; from: number; to: number; end: number }[];
}
let declarationCache:{source:string;value:CellularDeclarations}|undefined;

/** Tolerant authoring assistance only. Rust validates names, recipes and types. */
export function cellularDeclarations(source: string): CellularDeclarations {
  if (declarationCache?.source===source) return declarationCache.value;
  const tokens = tokenizeGenome(source).filter(token => token.kind !== 'comment');
  const result: CellularDeclarations = { products: [], genes: [], assemblies: [], functions: [] };
  for (let index = 0; index + 3 < tokens.length; index++) {
    const declaration = tokens[index].text, name = tokens[index + 1];
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name.text)) continue;
    const detail = tokens[index + 3].text;
    if (declaration === 'product' && tokens[index + 2].text === '=') result.products.push({ name: name.text, role: detail, from: name.from, to: name.to });
    if (declaration === 'gene' && tokens[index + 2].text === ':') result.genes.push({ name: name.text, product: detail, from: name.from, to: name.to });
    if (declaration === 'assembly' && tokens[index + 2].text === ':') {
      let kind = detail;
      if (tokens[index + 4]?.text === '(') {
        const close = tokens.findIndex((token,i)=>i>index+4 && (token.text===')' || token.text==='{'));
        if (close>index+4 && tokens[close].text===')') kind=tokens.slice(index+3,close+1).map(token=>token.text).join('');
      }
      result.assemblies.push({ name: name.text, kind, from: name.from, to: name.to });
    }
    if (declaration === 'fn' && tokens[index + 2].text === '(') {
      const close = tokens.findIndex((token, i) => i > index + 2 && [')', ';', '='].includes(token.text));
      if (close > index + 2 && tokens[close].text === ')') {
        const parameters = tokens.slice(index + 3, close).filter(token => token.text !== ',').map(token => token.text);
        const end = tokens.find((token, i) => i > close && token.text === ';')?.to ?? source.length + 1;
        result.functions.push({ name: name.text, parameters, from: name.from, to: name.to, end });
      }
    }
  }
  declarationCache={source,value:result};
  return result;
}

export function isCellularSource(source: string): boolean {
  return /^(?:\s|#[^\r\n]*(?:\r?\n|$))*cellular\s*;/.test(source);
}

export interface ReceptorFieldSymbol { name: string; type: PrimitiveType; description: string }
const measured = (name: string, type: PrimitiveType, description: string): ReceptorFieldSymbol => ({ name, type, description });
const receptorFlags = [
  measured('available', 'boolean', 'A matching complete, functioning receptor is active.'),
  measured('valid', 'boolean', 'This receptor has a usable measurement in the current snapshot.'),
] as const;
/** Named readings are restricted to fields supported by the declared receptor. */
export const CELLULAR_RECEPTOR_FIELDS: Readonly<Record<string, readonly ReceptorFieldSymbol[]>> = Object.freeze({
  'sense(light)': [...receptorFlags, measured('level', 'scalar', 'Brightness at the receptor attachment point, from 0 to 1.')],
  'sense(heat)': [...receptorFlags, measured('value', 'scalar', 'Temperature at the receptor attachment point, from 0 to 1.')],
  'sense(chemical)': [...receptorFlags, measured('mineral', 'scalar', 'Dissolved mineral concentration at this receptor.'), measured('organic', 'scalar', 'Organic cue concentration at this receptor, without a supplied gradient or food identity.')],
  'sense(contact)': [...receptorFlags, measured('offset', 'vec2', 'Measured body-relative displacement to a contacting surface.'), measured('pressure', 'scalar', 'Measured contact pressure.'), measured('resistance', 'scalar', 'Measured resistance of the contacting surface.'), measured('flow', 'vec2', 'Relative water flow at the receptor, in body coordinates.'), measured('speed', 'scalar', 'Magnitude of the measured relative flow.'), measured('bearing', 'scalar', 'Body-relative direction of the measured flow, in radians.'), measured('message_valid', 'boolean', 'True when the cell touched by this receptor has populated its published scalar register.'), measured('message', 'scalar', 'The scalar that the touched cell published at the start of the tick. A published zero is valid.'), measured('signal_valid', 'boolean', 'True when the touched cell has populated its signal register.'), measured('signal', 'scalar', 'The signal payload from the same touched cell and start-of-tick snapshot as the recognition marker.')],
  'sense(optical)': [...receptorFlags, measured('offset', 'vec2', 'Body-relative displacement to an object visible through this receptor.'), measured('area', 'scalar', 'Measured apparent area of the visible object.'), measured('contrast', 'scalar', 'Measured optical contrast.'), measured('pigment', 'scalar', 'Measured visible surface appearance.'), measured('motion', 'vec2', 'Measured relative motion in body coordinates.')],
});

// Older cellular recipes name the receptor family directly. Rust exposes the
// same named fields for both spellings, including contact messages.
const receptorMechanisms: Readonly<Record<string, string>> = {
  photoreceptor: 'sense(light)', thermoreceptor: 'sense(heat)', chemoreceptor: 'sense(chemical)',
  mechanoreceptor: 'sense(contact)', optical: 'sense(optical)',
};

function declaredFunctionSymbol(item: CellularDeclarations['functions'][number]): GenomeSymbol {
  return symbol(item.name, 'function', 'T', `${item.name}(${item.parameters.map(name => `${name}: number`).join(', ')})`,
    'Pure expression function declared in this program. Numeric arguments are evaluated from the same snapshot; calls create no persistent state.');
}

export function cellularDynamicSymbols(source: string): GenomeSymbol[] {
  const declarations = cellularDeclarations(source);
  return [
    ...declarations.products.map(item => symbol(`pool.${item.name}`, 'signal', 'scalar', `pool.${item.name}: scalar`,
      `Available ${item.role} product mass before this tick. Stock persists, occupies space and is shared by all recipes that name ${item.name}. Regulator stocks decay according to their half-life.`)),
    ...declarations.products.map(item => symbol(`concentration.${item.name}`, 'signal', 'scalar', `concentration.${item.name}: scalar`,
      `Available ${item.role} product mass divided by current envelope area. Growth dilutes this value; synthesis, decay and inheritance change the underlying physical stock.`)),
    ...declarations.assemblies.map(item => symbol(`built.${item.name}`, 'signal', 'scalar', `built.${item.name}: scalar`,
      `Functional material in complete ${item.name} assemblies, reduced by damage. Incomplete assemblies contribute zero; their installed components are shown in the Body inspector.`)),
    ...declarations.assemblies.flatMap(item => (CELLULAR_RECEPTOR_FIELDS[receptorMechanisms[item.kind] ?? item.kind] ?? []).map(field =>
      symbol(`sensed.${item.name}.${field.name}`, 'signal', field.type, `sensed.${item.name}.${field.name}: ${field.type}`,
        `${field.description} Sampling uses existing machinery and the activity stored from the preceding tick. ${(field.name.startsWith('message') || field.name.startsWith('signal')) ? `Check sensed.${item.name}.${field.name.split('_')[0]}_valid before using the value. An occupied zero is valid.` : (item.kind==='sense(contact)' || item.kind==='mechanoreceptor') && ['flow','speed','bearing'].includes(field.name) ? `Check sensed.${item.name}.available; flow is measurable even without contact.` : `Invalid measurements are zero; check sensed.${item.name}.valid.`}`))),
  ];
}

export function lookupCellularSymbol(name: string, source: string): GenomeSymbol | undefined {
  const dynamic = cellularDynamicSymbols(source).find(item => item.name === name);
  if (dynamic) return dynamic;
  const declarations = cellularDeclarations(source);
  const product = declarations.products.find(item => item.name === name);
  if (product) return symbol(name, 'binding', `${product.role} product`, `${name}: ${product.role}`, `Physical product declared by this genome. Use pool.${name} to read its available mass, or name it in an assembly recipe with a positive material amount.`);
  const gene = declarations.genes.find(item => item.name === name);
  if (gene) return symbol(name, 'binding', 'gene', `gene ${name}: ${gene.product}`, `Expression effort for ${gene.product}. The Body inspector compares requested expression with actual material synthesized.`);
  const declaredFunction = declarations.functions.find(item => item.name === name);
  if (declaredFunction) return declaredFunctionSymbol(declaredFunction);
  const assembly = declarations.assemblies.find(item => item.name === name);
  if (assembly) return symbol(name, 'binding', `${assembly.kind} assembly`, `${name}: ${assembly.kind}`, `Physical recipe with its own composition, activity and placement. Use built.${name} to read the functional mass in completed machinery, after damage.`);
  return cellularSymbolByName.get(name) ?? CELLULAR_MECHANISMS.find(item=>item.name===name) ?? mechanismConstructorByName.get(name) ?? CELLULAR_ASSEMBLY_KINDS.find(item => item.name === name);
}

export const GENOME_SYMBOLS: readonly GenomeSymbol[] = Object.freeze([
  ...BUILTIN_FUNCTIONS, ...BUILTIN_SIGNALS, ...BUILTIN_OUTPUTS, ...LANGUAGE_KEYWORDS,
]);
const symbolByName = new Map(GENOME_SYMBOLS.map(item => [item.name, item]));
const compatibilityOutputs = new Map(ORGANELLE_CATALOG.map(({kind}) => {
  const canonical = symbolByName.get(`express.${kind}`)!;
  const name = `build.${kind}`;
  return [name, symbol(name, 'output', 'scalar', `${name} = scalar;`, `Compatibility spelling for express.${kind}. It now requests paid protein synthesis, followed by assembly on later ticks. Use express.${kind} in new programs. ${canonical.description}`)] as const;
}));

export const LANGUAGE_RULES = Object.freeze([
  'number in a signature means integer or scalar. Integers promote to scalar when mixed with scalars, and integer values may be assigned to scalar outputs or registers. Scalar-to-integer assignment needs int().',
  'Numeric +, -, and * preserve integers only when both inputs are integers; numeric / always returns scalar. Comparisons return boolean. Boolean &&, ||, and ! require booleans.',
  'Strings support concatenation with + and equality comparisons. Vectors support +, -, unary -, multiplication by a number on either side, and division by a number.',
  'Statements end with semicolons. # starts a line comment. Strings use double quotes and cannot contain physical newlines; use escaped characters instead.',
  'Every register read sees old memory. Writes commit together after evaluation; clear wins. Each populated register costs 0.0025 energy per second, including a stored zero, false, or empty string.',
]);

export const REGISTER_FAMILIES: ReadonlyArray<Readonly<{ prefix: string; type: PrimitiveType; empty: string }>> = Object.freeze([
  { prefix: 'ri', type: 'integer', empty: '0' },
  { prefix: 'rf', type: 'scalar', empty: '0.0' },
  { prefix: 'rb', type: 'boolean', empty: 'false' },
  { prefix: 'rs', type: 'string', empty: '""' },
  { prefix: 'rv', type: 'vec2', empty: 'vec(0, 0)' },
].map(item => Object.freeze(item)) as Array<Readonly<{ prefix: string; type: PrimitiveType; empty: string }>>);

/** Registers are resolved on demand rather than putting 4,995 entries in the completion catalog. */
export function registerSymbol(name: string): GenomeSymbol | undefined {
  if (!/^r[ifbsv][0-9]{3}$/.test(name) || name.endsWith('000')) return undefined;
  const family = REGISTER_FAMILIES.find(item => name.startsWith(item.prefix))!;
  const detail = family.type === 'string' ? ' Strings are limited to 128 UTF-8 bytes.'
    : family.type === 'scalar' ? ' Integer writes also promote to scalar.' : '';
  return symbol(name, 'register', family.type, `${name}: ${family.type}`,
    `Per-cell ${family.type} register. Reads the previous tick’s value, or ${family.empty} when empty. Write with ${name} = expression; clear with clear ${name};. Each populated register costs 0.0025 energy per second.${detail}`);
}

export function lookupGenomeSymbol(name: string, bindings: readonly GenomeBinding[] = []): GenomeSymbol | undefined {
  const binding = bindings.find(item => item.name === name);
  if (binding) return symbol(name, 'binding', binding.type, `${name}: ${binding.type}`,
    'Immutable local signal. Its expression is recomputed from the current inputs and old register values each tick.');
  return symbolByName.get(name) ?? compatibilityOutputs.get(name) ?? registerSymbol(name);
}

export type GenomeTokenKind = 'keyword' | 'boolean' | 'number' | 'string' | 'comment' | 'operator'
  | 'punctuation' | 'identifier' | 'signal' | 'function' | 'output' | 'register' | 'invalid';
export interface GenomeToken {
  readonly from: number;
  readonly to: number;
  readonly text: string;
  readonly kind: GenomeTokenKind;
  readonly type?: PrimitiveType;
}

/** Tolerant highlighting scanner, not a second validator. Ranges use JavaScript/CodeMirror UTF-16 offsets. */
export function tokenizeGenome(source: string): GenomeToken[] {
  const tokens: GenomeToken[] = [];
  let pos = 0;
  while (pos < source.length) {
    const start = pos, character = source[pos];
    if (/[\t\n\v\f\r ]/.test(character)) { pos++; continue; }
    let kind: GenomeTokenKind;
    let type: PrimitiveType | undefined;
    if (character === '#') {
      while (pos < source.length && source[pos] !== '\n') pos++;
      kind = 'comment';
    } else if (character === '"') {
      pos++;
      while (pos < source.length && source[pos] !== '\n' && source[pos] !== '\r') {
        if (source[pos] === '"') { pos++; break; }
        if (source[pos] === '\\') {
          pos++;
          if (pos < source.length && source[pos] !== '\n' && source[pos] !== '\r') pos++;
        } else pos++;
      }
      kind = 'string';
    } else if (/[0-9]/.test(character) || (character === '.' && /[0-9]/.test(source[pos + 1] ?? ''))) {
      while (/[0-9]/.test(source[pos] ?? '')) pos++;
      if (source[pos] === '.') { pos++; while (/[0-9]/.test(source[pos] ?? '')) pos++; }
      if (source[pos] === 'e' || source[pos] === 'E') {
        pos++;
        if (source[pos] === '+' || source[pos] === '-') pos++;
        while (/[0-9]/.test(source[pos] ?? '')) pos++;
      }
      kind = 'number';
    } else if (/[A-Za-z_]/.test(character)) {
      pos++;
      while (/[A-Za-z0-9_.]/.test(source[pos] ?? '')) pos++;
      const name = source.slice(start, pos), known = symbolByName.get(name) ?? compatibilityOutputs.get(name) ?? cellularSymbolByName.get(name) ?? mechanismConstructorByName.get(name), register = registerSymbol(name);
      let mechanismCall = false;
      if (mechanismConstructorByName.has(name)) {
        const previous: string[] = [];
        for (let i=tokens.length-1; i>=0 && previous.length<3; i--) if (tokens[i].kind!=='comment') previous.push(tokens[i].text);
        mechanismCall=previous[0]===':' && previous[2]==='assembly';
      }
      if (mechanismCall) kind='function';
      else if (name === 'true' || name === 'false') { kind = 'boolean'; type = 'boolean'; }
      else if (register) { kind = 'register'; type = register.type as PrimitiveType; }
      else if (known) {
        kind = known.category as 'keyword' | 'signal' | 'function' | 'output';
        if (known.category === 'signal' || known.category === 'output') type = known.type as PrimitiveType;
      } else if (/^(pool|built|concentration)\.[A-Za-z_][A-Za-z0-9_]*$/.test(name)) { kind = 'signal'; type = 'scalar'; }
      else if (/^sensed\.[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_]+$/.test(name)) {
        kind = 'signal';
        type = name.endsWith('.valid') || name.endsWith('.available') ? 'boolean' : /\.(offset|flow|motion)$/.test(name) ? 'vec2' : 'scalar';
      }
      else if (/^\s*\(/.test(source.slice(pos))) kind = 'function';
      else kind = 'identifier';
    } else if (['==', '!=', '<=', '>=', '&&', '||'].includes(source.slice(pos, pos + 2))) {
      pos += 2; kind = 'operator';
    } else if ('+-*/=<>!'.includes(character)) {
      pos++; kind = 'operator';
    } else if ('(),;{}:'.includes(character)) {
      pos++; kind = 'punctuation';
    } else {
      pos += (source.codePointAt(pos) ?? 0) > 0xffff ? 2 : 1; kind = 'invalid';
    }
    tokens.push({ from: start, to: pos, text: source.slice(start, pos), kind, ...(type ? { type } : {}) });
  }
  return tokens;
}

const identifierKinds = new Set<GenomeTokenKind>(['keyword', 'boolean', 'identifier', 'signal', 'function', 'output', 'register']);
/** Finds an identifier under the cursor, or immediately before it at whitespace/end of source. */
export function identifierAt(source: string, position: number): { from: number; to: number; text: string } | null {
  if (!Number.isInteger(position) || position < 0 || position > source.length) return null;
  const tokens = tokenizeGenome(source);
  const current = tokens.find(token => token.from <= position && position < token.to);
  const token = current ?? tokens.find(token => token.to === position);
  return token && identifierKinds.has(token.kind) ? { from: token.from, to: token.to, text: token.text } : null;
}

export function symbolAt(source: string, position: number, bindings: readonly GenomeBinding[] = []): { from: number; to: number; symbol: GenomeSymbol } | null {
  const span = identifierAt(source, position);
  if (!span) return null;
  const found = (isCellularSource(source) ? lookupCellularSymbol(span.text, source) : undefined) ?? lookupGenomeSymbol(span.text, bindings);
  return found ? { from: span.from, to: span.to, symbol: found } : null;
}

function boundedLimit(limit: number, maximum: number): number {
  return Number.isFinite(limit) ? Math.max(0, Math.min(maximum, Math.floor(limit))) : 0;
}

/** Suggests existing references first, then a small number of valid matching addresses. */
export function registerCompletions(prefix: string, source = '', limit = 12): GenomeSymbol[] {
  const count = boundedLimit(limit, 100);
  if (!count || !/^r(?:[ifbsv][0-9]{0,3})?$/.test(prefix)) return [];
  const selected = new Map<string, GenomeSymbol>();
  for (const token of tokenizeGenome(source)) {
    if (token.kind === 'register' && token.text.startsWith(prefix)) selected.set(token.text, registerSymbol(token.text)!);
    if (selected.size >= count) return [...selected.values()];
  }
  const families = REGISTER_FAMILIES.filter(item => item.prefix.startsWith(prefix) || prefix.startsWith(item.prefix));
  const digits = prefix.length > 2 ? prefix.slice(2) : '';
  const first = Math.max(1, digits ? Number(digits.padEnd(3, '0')) : 1);
  const last = digits ? Number(digits.padEnd(3, '9')) : 999;
  for (let index = first; index <= last && selected.size < count; index++) {
    for (const family of families) {
      const name = family.prefix + String(index).padStart(3, '0');
      if (name.startsWith(prefix)) selected.set(name, registerSymbol(name)!);
      if (selected.size >= count) break;
    }
  }
  return [...selected.values()];
}

/** Search names and concise help text. Callers pass only local bindings visible at their cursor. */
export function searchGenomeSymbols(query: string, bindings: readonly GenomeBinding[] = [], limit = 80): GenomeSymbol[] {
  const count = boundedLimit(limit, 200), needle = query.trim().toLowerCase();
  if (!count) return [];
  const candidates = new Map(GENOME_SYMBOLS.map(item => [item.name, item]));
  for (const binding of bindings) candidates.set(binding.name, lookupGenomeSymbol(binding.name, [binding])!);
  for (const register of registerCompletions(needle, '', Math.min(count, 12))) candidates.set(register.name, register);
  const rank = (item: GenomeSymbol): number => {
    const name = item.name.toLowerCase();
    if (name === needle) return 0;
    if (name.startsWith(needle)) return 1;
    if (name.includes(needle)) return 2;
    return 3;
  };
  return [...candidates.values()]
    .filter(item => !needle || `${item.name} ${item.signature} ${item.description}`.toLowerCase().includes(needle))
    .sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name))
    .slice(0, count);
}

export interface GenomeCompletionContext {
  kind: 'statement' | 'expression' | 'register' | 'none' | 'constructor' | 'product' | 'assembly-kind' | 'component' | 'mechanism-target';
  mechanismConstructor?: string;
  /** A useful editing hint, never a substitute for the Rust type checker. */
  expected?: PrimitiveType | 'number' | 'register' | 'vec2 | string';
}

const numericFunctions = new Set(['sin', 'cos', 'abs', 'min', 'max', 'clamp', 'int', 'float', 'vec', 'sqrt', 'sign', 'atan2', 'lerp', 'noise', 'activate', 'repress']);
const vectorFunctions = new Set(['dot', 'normalize', 'angle', 'x', 'y']);

function cellularCompletionContext(source: string, position: number): GenomeCompletionContext {
  const all = tokenizeGenome(source);
  const enclosing = all.find(token => token.from <= position && position <= token.to);
  if (enclosing?.kind === 'comment' && position > enclosing.from
    || enclosing?.kind === 'string' && position > enclosing.from) return { kind: 'none' };
  const preceding = all.filter(token => token.kind !== 'comment' && token.from < position);
  const last = preceding.at(-1);
  if (last && last.to >= position && identifierKinds.has(last.kind)
    && !['cellular', 'product', 'gene', 'assembly', 'fn', 'if', 'then', 'else', 'when', 'clear', 'publish', 'avoid_message', 'adhere_message', 'signal', 'activity', 'start_when', 'recycle', 'at', 'direction'].includes(last.text)) preceding.pop();
  const braces: number[] = [];
  for (let index = 0; index < preceding.length; index++) {
    if (preceding[index].text === '{') braces.push(index);
    if (preceding[index].text === '}') braces.pop();
  }
  let start = preceding.length;
  while (start > 0 && ![';', '{', '}'].includes(preceding[start - 1].text)) start--;
  const tokens = preceding.slice(start);
  if (braces.length) {
    if (!tokens.length || !tokens.some(token => token.text === ':')) return { kind: 'component' };
    if (!['activity', 'start_when', 'recycle'].includes(tokens[0].text)) return { kind: 'none' };
    const expression = `${tokens[0].text === 'start_when' ? 'divide' : 'thrust'} = ${source.slice(tokens.find(token=>token.text===':')!.to,position)}`;
    return genomeCompletionContext(expression,expression.length);
  }
  if (!tokens.length) return { kind: 'statement' };
  const assignment = tokens.some(token => token.text === '='), colon = tokens.some(token => token.text === ':');
  if (['publish', 'avoid_message', 'adhere_message', 'signal'].includes(tokens[0].text)) {
    return tokens.length === 1 ? { kind: 'register', expected: 'scalar' } : { kind: 'none' };
  }
  if (tokens[0].text === 'clear' || tokens[0].kind === 'register') {
    const statement = source.slice(tokens[0].from, position);
    return genomeCompletionContext(statement, statement.length);
  }
  if (tokens[0].text === 'product') return { kind: assignment && !tokens.some(token=>token.text==='(') ? 'constructor' : 'none' };
  if (tokens[0].text === 'gene') {
    if (!assignment) return { kind: colon ? 'product' : 'none' };
    const expression = `thrust = ${source.slice(tokens.find(token=>token.text==='=')!.to,position)}`;
    return genomeCompletionContext(expression,expression.length);
  }
  if (tokens[0].text === 'assembly') {
    if (!colon) return { kind:'none' };
    const open = tokens.findIndex(token=>token.text==='(');
    if (open>=0) return tokens.some(token=>token.text===')') ? {kind:'none'} : {kind:'mechanism-target',mechanismConstructor:tokens[open-1]?.text};
    return {kind:'assembly-kind'};
  }
  if (tokens[0].text === 'let' || tokens[0].text === 'fn') {
    if (!assignment) return { kind: 'none' };
    const expression = 'let value = ' + source.slice(tokens.find(token => token.text === '=')!.to, position);
    return genomeCompletionContext(expression, expression.length);
  }
  return { kind: 'statement' };
}

/** Incomplete code is expected here. Infer only the constraints visible at the cursor. */
export function genomeCompletionContext(source: string, position: number, bindings: readonly GenomeBinding[] = []): GenomeCompletionContext {
  if (!Number.isInteger(position) || position < 0 || position > source.length) return { kind: 'none' };
  if (isCellularSource(source)) return cellularCompletionContext(source, position);
  const all = tokenizeGenome(source);
  const enclosing = all.find(token => token.from <= position && position <= token.to);
  if (enclosing?.kind === 'comment' && position > enclosing.from
    || enclosing?.kind === 'string' && position > enclosing.from
      && (position < enclosing.to || enclosing.text.length === 1 || !enclosing.text.endsWith('"'))) return { kind: 'none' };
  const preceding = all.filter(token => token.kind !== 'comment' && token.from < position);
  // A partial identifier is the completion prefix, not a completed grammar term.
  if (preceding.at(-1)?.to === position || (preceding.at(-1)?.to ?? 0) > position) {
    const last = preceding.at(-1)!;
    if (identifierKinds.has(last.kind) && !['if', 'then', 'else', 'when', 'clear'].includes(last.text)) preceding.pop();
  }
  let start = preceding.length;
  while (start > 0 && preceding[start - 1].text !== ';') start--;
  const tokens = preceding.slice(start);
  if (!tokens.length) return { kind: 'statement' };
  if (tokens[0].text === 'clear' && !tokens.some(token => token.text === 'when')) return { kind: 'register', expected: 'register' };
  const assignment = tokens.findIndex(token => token.text === '=');
  if (assignment < 0 && tokens[0].text === 'let') return { kind: 'none' };
  if (assignment < 0 && tokens.length === 1) return { kind: 'statement' };
  const expressionTokens = assignment >= 0 ? tokens.slice(assignment + 1) : tokens;

  // Find the innermost still-open call, skipping complete nested expressions.
  const calls: { name: string | undefined; index: number }[] = [];
  for (let index = 0; index < expressionTokens.length; index++) {
    const token = expressionTokens[index];
    if (token.text === '(') calls.push({ name: expressionTokens[index - 1]?.kind === 'function' ? expressionTokens[index - 1].text : undefined, index });
    else if (token.text === ')') calls.pop();
  }
  const currentCall = calls.at(-1);
  if (currentCall?.name === 'populated') return { kind: 'register', expected: 'register' };
  const last = expressionTokens.at(-1)?.text;
  if (['if', 'when', '&&', '||', '!'].includes(last ?? '')) return { kind: 'expression', expected: 'boolean' };
  if (currentCall?.name && (!last || last === '(' || last === ',')) {
    if (numericFunctions.has(currentCall.name)) return { kind: 'expression', expected: 'number' };
    if (vectorFunctions.has(currentCall.name)) return { kind: 'expression', expected: 'vec2' };
    if (currentCall.name === 'length') return { kind: 'expression', expected: 'vec2 | string' };
  }
  if (last && ['<', '<=', '>', '>=', '/', '*'].includes(last)) return { kind: 'expression', expected: 'number' };
  if (last && ['==', '!=', '+', '-'].includes(last)) {
    const previous = expressionTokens.at(-2);
    const known = previous && (lookupGenomeSymbol(previous.text, bindings)?.type ?? (previous.kind === 'string' ? 'string' : previous.kind === 'number' ? 'number' : undefined));
    if (known && ['integer', 'scalar', 'boolean', 'string', 'vec2', 'number'].includes(known)) {
      return { kind: 'expression', expected: known === 'integer' || known === 'scalar' ? 'number' : known as PrimitiveType | 'number' };
    }
  }
  // A branch or operator can change the required operand type. Do not infer through it.
  if (expressionTokens.length) return { kind: 'expression' };
  const target = assignment > 0 && tokens[0].text !== 'let' ? lookupGenomeSymbol(tokens[0].text, bindings) : undefined;
  return { kind: 'expression', ...(target && ['integer', 'scalar', 'boolean', 'string', 'vec2'].includes(target.type) ? { expected: target.type as PrimitiveType } : {}) };
}

/** Blank completion is deliberately split into writable statements and readable expressions. */
export function genomeCompletions(source: string, position: number, prefix = '', bindings: readonly GenomeBinding[] = []): GenomeSymbol[] {
  const context = genomeCompletionContext(source, position, bindings);
  if (context.kind === 'none') return [];
  const cellular = isCellularSource(source);
  const needle = prefix.toLowerCase();
  const registers = registerCompletions(prefix || 'r', source, prefix ? 40 : 15);
  if (!prefix) for (const family of REGISTER_FAMILIES) {
    const name = `${family.prefix}001`;
    if (!registers.some(item => item.name === name)) registers.push(registerSymbol(name)!);
  }
  const available = new Map<string, GenomeSymbol>();
  const add = (item: GenomeSymbol): void => { if (!needle || item.name.toLowerCase().startsWith(needle)) available.set(item.name, item); };
  if (cellular && ['constructor', 'product', 'assembly-kind', 'component', 'mechanism-target'].includes(context.kind)) {
    if (context.kind === 'constructor') CELLULAR_CONSTRUCTORS.forEach(add);
    if (context.kind === 'mechanism-target') MECHANISM_CATALOG.filter(item=>item.constructor===context.mechanismConstructor && item.target).forEach(item=>add(symbol(item.target!, 'keyword', 'target', item.kind, item.description)));
    if (context.kind === 'assembly-kind') [...CELLULAR_MECHANISMS,...CELLULAR_ASSEMBLY_KINDS].forEach(item => add({ ...item, insertText: `${item.name} {\n  activity: 1;\n}` }));
    if (context.kind === 'product' || context.kind === 'component') {
      cellularDeclarations(source).products.forEach(item => {
        const product = lookupCellularSymbol(item.name, source)!;
        add(context.kind === 'component' ? { ...product, insertText: `${item.name}: ` } : product);
      });
      if (context.kind === 'component') CELLULAR_KEYWORDS.filter(item => ['activity', 'start_when', 'recycle', 'at', 'direction'].includes(item.name)).forEach(add);
    }
    return [...available.values()];
  }
  if (context.kind === 'register') {
    if (context.expected === 'scalar') {
      const addressPrefix = !prefix || prefix === 'r' ? 'rf' : prefix;
      if (addressPrefix.startsWith('rf')) registerCompletions(addressPrefix, source, 40).forEach(add);
    } else registers.forEach(add);
    return [...available.values()];
  }
  if (context.kind === 'statement') {
    if (cellular) {
      CELLULAR_KEYWORDS.filter(item => ['product', 'gene', 'assembly', 'fn', 'publish', 'avoid_message', 'adhere_message', 'signal'].includes(item.name)).forEach(add);
    } else BUILTIN_OUTPUTS.forEach(item => add({ ...item, insertText: `${item.name} = ` }));
    registers.forEach(item => add({ ...item, insertText: `${item.name} = ` }));
    for (const name of ['let', 'clear']) add({ ...lookupGenomeSymbol(name)!, insertText: `${name} ` });
    return [...available.values()];
  }
  (cellular
    ? [...CELLULAR_READ_SIGNALS, ...cellularDynamicSymbols(source), ...BUILTIN_FUNCTIONS, ...registers]
    : [...BUILTIN_SIGNALS, ...BUILTIN_FUNCTIONS, ...registers]).forEach(add);
  bindings.forEach(binding => add(lookupGenomeSymbol(binding.name, [binding])!));
  if (cellular) {
    const functions = cellularDeclarations(source).functions;
    functions.filter(item => item.end <= position).forEach(item => add(declaredFunctionSymbol(item)));
    const current = functions.find(item => item.from < position && position < item.end);
    current?.parameters.forEach(name => add(symbol(name, 'binding', 'scalar', `${name}: scalar`, 'Numeric argument of this pure function.')));
  }
  for (const name of ['if', 'true', 'false']) add(lookupGenomeSymbol(name)!);
  const expected = context.expected;
  const accepts = (item: GenomeSymbol): boolean => {
    if (!expected || item.type === 'T' || item.type === 'not checked') return true;
    const types = item.type.split(' | ');
    if (expected === 'number' || expected === 'scalar') return types.includes('integer') || types.includes('scalar');
    // Any primitive may begin a comparison. Prefer booleans without hiding useful comparison operands.
    if (expected === 'boolean') return true;
    if (expected === 'vec2 | string') return types.includes('vec2') || types.includes('string');
    return types.includes(expected);
  };
  return [...available.values()].filter(accepts).sort((a, b) => {
    if (expected === 'boolean') return Number(b.type === 'boolean') - Number(a.type === 'boolean');
    return 0;
  });
}

const outputExamples: Readonly<Record<string, string>> = Object.freeze({
  thrust: 'thrust = if energy.fraction > 0.3 then 0.3 else 0;', turn: 'turn = if object.valid then clamp(angle(object.offset), -1, 1) else 0;',
  divide: 'divide = body.division_ready;', grow: 'grow = if energy.fraction > 0.6 && body.maturity < 1 then 0.7 else 0;',
  photosynthesize: 'photosynthesize = 1;', engulf: 'engulf = contact.valid && gut.fraction < 0.5;',
  bite: 'bite = if contact.valid && gut.fraction < 0.5 then 0.8 else 0;', defend: 'defend = if contact.valid then 0.5 else 0;',
  repair: 'repair = if integrity < 0.9 && energy.fraction > 0.5 then 0.5 else 0;', absorb: 'absorb = if nutrient.fraction < 0.8 then 1 else 0;',
  feed: 'feed = 1;', 'digest.exudate': 'digest.exudate = if gut.exudate.energy > 0 then 1 else 0;',
  'digest.tissue': 'digest.tissue = if gut.tissue.energy > 0 then 0.8 else 0;',
  'digest.waste': 'digest.waste = if gut.exudate.energy + gut.tissue.energy < 1 then 1 else 0.1;',
  'scan.bearing': 'scan.bearing = sin(age * 0.2) * 1.5;', 'scan.width': 'scan.width = 3.141593;',
  'scan.target': 'scan.target = if populated(rs001) then rs001 else "";',
});

export interface GenomeSymbolDocumentation { example: string; notes: readonly string[] }
export function genomeSymbolDocumentation(item: GenomeSymbol): GenomeSymbolDocumentation {
  const mechanism = MECHANISM_CATALOG.find(entry=>entry.kind===item.name);
  if (mechanism) {
    const parts=mechanism.roles.map(role=>({name:role[0].toUpperCase()+role.slice(1)+'Part',role}));
    return {example:`${parts.map(part=>`product ${part.name} = ${part.role}();`).join('\n')}\nassembly Machine: ${mechanism.kind} {\n${parts.map(part=>`  ${part.name}: 0.04;`).join('\n')}\n  activity: 1;\n}`,
      notes:[mechanism.description,'Build separate mechanisms for the other operations the organism needs. Each recipe retains its physical ingredients and identity.']};
  }
  if (item.name === 'signal' && item.category === 'keyword') {
    return { example: 'publish rf001 = 17;\nsignal rf002 = 0;\nrf002 = if energy.fraction < 0.4 then 1 else 0;',
      notes: ['Contact receptors read signal_valid and signal at the start of the tick. The two public channels use distinct scalar addresses.'] };
  }
  if (item.type === 'contact message') {
    return { example: ['publish rf001 = 17;', ...(item.name === 'publish' ? [] : [`${item.name} rf001;`])].join('\n'),
      notes: ['Each declaration accepts one scalar address from rf001 through rf999. Registers use their start-of-tick values. A stored zero counts as a message.'] };
  }
  if (CELLULAR_KEYWORDS.some(keyword=>keyword.name===item.name) || item.category==='function' && item.type==='product') {
    const snippets:Record<string,string> = {
      cellular:'cellular;\nproduct Memory = regulator(8);\ngene remember: Memory = if energy > 50 then 0.1 else 0;',
      product:'product Rotor = motor();\nproduct Memory = regulator(8);',
      gene:'product Rotor = motor();\ngene drive: Rotor = if pool.Rotor < 0.01 then 0.5 else 0;',
      assembly:'product Anchor = anchor();\nproduct Rotor = motor();\nproduct Tail = filament();\nassembly Swim: propel() {\n  Anchor: 0.005;\n  Rotor: 0.01;\n  Tail: 0.025;\n  activity: 0.6;\n  at: 1.570796;\n  direction: 0;\n}',
      fn:'fn response(value, threshold) = activate(value, threshold);\nlet response_now = response(energy.fraction, 0.4);',
      start_when:'start_when: concentration.GrowthSignal > 0.01;',
      recycle:'recycle: if concentration.Brake > 0.02 then 0.5 else 0;',
      activity:'activity: if energy.fraction > 0.3 then 0.6 else 0;',
      at:'at: 1.570796;\ndirection: 0;',
      direction:'at: -1.570796;\ndirection: 0;',
    };
    return { example: snippets[item.name] ?? `product ${item.name[0].toUpperCase()+item.name.slice(1)} = ${item.name}(${item.name==='regulator'?'8':''});`,
      notes: ['Use these declarations in a program beginning with cellular;. The compiler checks roles, recipe masses and expression types.',
        item.name==='at'||item.name==='direction' ? 'Angles are body-relative radians. Off-center motor force produces torque; placement is part of the physical recipe.'
          : 'Products are paid material. Synthesis and assembly share finite core work; only stock present at the start of a tick can be installed.'] };
  }
  if (item.category === 'register') {
    return { example: `${item.name} = ${REGISTER_FAMILIES.find(family => family.type === item.type)?.empty ?? '0'};\nclear ${item.name} when age > 10;`,
      notes: ['Addresses run from 001 to 999 for each of the five primitive types.', 'Reads use memory from the start of the evaluation. Writes commit together after a real tick; clear wins when both rules fire.'] };
  }
  if (item.category === 'output') {
    const family = ORGANELLE_CATALOG.find(entry => item.name.includes(`.${entry.kind}`));
    if (family && (item.name.startsWith('express.') || item.name.startsWith('build.'))) {
      return { example: `target.${family.kind}.count = 2;\ntarget.${family.kind}.size = ${family.minimumMass};\nlet missing = max(0, ${family.minimumMass * 2} - organelle.${family.kind}.mass);\nexpress.${family.kind} = if energy > 30 && protein.${family.kind} < min(0.01, missing) then 1 else 0;`,
        notes: ['Expression makes family-specific assembly proteins, not an instant organ. Synthesis and assembly share finite core throughput.', 'Protein already present at the beginning of a tick can assemble toward targets. It occupies internal space, has upkeep, and decays if unused.', 'Switch expression off when protein covers the remaining target. Lowering a target requires explicit recycling to remove an existing organ.'] };
    }
    const fallback = item.name.endsWith('.count') ? '2' : item.name.endsWith('.size') ? String(family?.minimumMass ?? 0.04) : '0.3';
    const typeNote = item.type === 'integer' ? 'Requires an integer expression such as 2 or int(value); 2.0 is a scalar.'
      : item.type === 'string' ? 'Requires a bounded string expression. Object tokens can be kept in string registers.'
      : item.type === 'boolean' ? 'Requires a boolean expression; numbers are not booleans.'
      : 'Accepts integers or scalars. The engine bounds the command to its supported range.';
    return { example: outputExamples[item.name] ?? `${item.name} = ${fallback};`,
      notes: [typeNote, 'This is a write-only request. Inspect actual body and organelle signals to see what has been built or achieved.'] };
  }
  if (item.category === 'signal') {
    const group = item.name.split('.')[0];
    const sensed = ['light', 'heat', 'chemical', 'flow', 'contact', 'object'].includes(group);
    return { example: `let observed = ${item.name};`, notes: ['Read-only input, sampled consistently for this evaluation.',
      group === 'sensed' && /\.(message|signal)(_valid)?$/.test(item.name) ? `Check ${item.name.split('.').slice(0,2).join('.')}.${item.name.split('.').at(-1)!.split('_')[0]}_valid before using the touched cell’s published value. A stored zero is valid.`
        : group === 'sensed' ? `Check ${item.name.split('.').slice(0,2).join('.')}.available and .valid. The receptor’s physical placement, damage and stored activity determine its observation.`
        : group === 'concentration' ? 'Concentration is free product mass divided by envelope area; it has no separate storage or state.'
        : sensed ? `Check ${group}.available and ${group}.valid before using a measurement. Missing/invalid numbers and vectors are zero; tokens are empty.`
        : 'Core self-signals remain available without external sensing hardware. Local bindings can combine them with pure expressions.'] };
  }
  return { example: item.example ?? item.signature, notes: [] };
}

/** Reference menus include one address per family; typing a prefix explores all 999. */
export function genomeSignalCatalog(direction: 'read' | 'write', source = ''): readonly GenomeSymbol[] {
  const registers = REGISTER_FAMILIES.map(family => registerSymbol(`${family.prefix}001`)!);
  if (isCellularSource(source)) return direction === 'read'
    ? [...cellularDynamicSymbols(source), ...CELLULAR_READ_SIGNALS, ...registers]
    : [...CELLULAR_KEYWORDS.filter(item => item.name !== 'cellular'), ...CELLULAR_CONSTRUCTORS, ...CELLULAR_MECHANISMS, ...registers, lookupGenomeSymbol('clear')!];
  return [...(direction === 'read' ? BUILTIN_SIGNALS : BUILTIN_OUTPUTS), ...registers];
}

/** Parentheses are transparent to the compiler; unwrap only a balanced group covering the whole selection. */
export function unwrapGenomeSelection(source: string, from: number, to: number, tokens: readonly GenomeToken[] = tokenizeGenome(source)): { from: number; to: number } {
  while (from < to && /\s/.test(source[from])) from++;
  while (to > from && /\s/.test(source[to - 1])) to--;
  const selected = tokens.filter(token => token.kind !== 'comment' && token.from >= from && token.to <= to);
  let left = 0, right = selected.length - 1;
  while (left < right && selected[left].text === '(' && selected[right].text === ')'
    && selected[left].from === from && selected[right].to === to) {
    let depth = 0, enclosing = true;
    for (let index = left; index <= right; index++) {
      if (selected[index].text === '(') depth++;
      if (selected[index].text === ')') depth--;
      if (depth <= 0 && index < right) { enclosing = false; break; }
    }
    if (!enclosing || depth !== 0) break;
    left++; right--;
    from = selected[left]?.from ?? selected[left - 1].to;
    to = right >= left ? selected[right].to : from;
  }
  return { from, to };
}
