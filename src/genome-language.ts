import { ORGANELLE_CATALOG, MECHANISM_CATALOG } from './organelles';

/** Editor assistance for the language in engine/src/genome.rs; the Rust compiler remains authoritative. */
export type PrimitiveType = 'count' | 'integer' | 'scalar' | 'boolean' | 'string' | 'vec2';
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
  readonly perContact?: true;
  readonly contactSafe?: false;
  /** UTF-16 source offsets from Engine.analyze(). */
  readonly from: number;
  readonly to: number;
}

const symbol = (name: string, category: GenomeSymbolCategory, type: string, signature: string, description: string): GenomeSymbol =>
  Object.freeze({ name, category, type, signature, description });
const fn = (name: string, parameters: string, type: string, description: string): GenomeSymbol =>
  symbol(name, 'function', type, `${name}(${parameters}) → ${type}`, description);

/** In signatures, number accepts a signed integer, unsigned count or scalar; T accepts any one primitive type. */
export const BUILTIN_FUNCTIONS: readonly GenomeSymbol[] = Object.freeze([
  fn('sin', 'angle: number', 'scalar', 'Sine of an angle in radians.'),
  fn('cos', 'angle: number', 'scalar', 'Cosine of an angle in radians.'),
  fn('abs', 'value: number', 'integer | count | scalar', 'Absolute value. Preserves the input numeric type.'),
  fn('min', 'a: number, b: number', 'integer | count | scalar', 'Smaller value. Whole operands stay integral. A scalar operand gives a scalar result.'),
  fn('max', 'a: number, b: number', 'integer | count | scalar', 'Larger value. Whole operands stay integral. A scalar operand gives a scalar result.'),
  fn('clamp', 'value: number, a: number, b: number', 'integer | count | scalar', 'Limits a value between a and b in either order. Whole operands stay integral. A scalar operand gives a scalar result.'),
  fn('int', 'value: number', 'integer', 'Converts a number to a signed 32-bit integer by truncating toward zero and clamping to its range.'),
  fn('float', 'value: number', 'scalar', 'Converts a numeric value to a scalar. Every unsigned 32-bit count is exactly representable.'),
  fn('vec', 'x: number, y: number', 'vec2', 'Constructs a two-component vector. Both components accept numeric values.'),
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
  ['object.token', 'string', 'Opaque token stable during a continuous observed track. You can store it in a string register and compare later observations. Empty when invalid.'],
  ['object.offset', 'vec2', 'Estimated body-relative displacement to the selected visible object. angle(object.offset) gives a steering bearing.'],
  ['object.area', 'scalar', 'Estimated apparent area of the visible object, not its exact material mass.'],
  ['object.contrast', 'scalar', 'Measured optical contrast. Selection depends on visibility, angle and distance, never hidden energy.'],
  ['object.pigment', 'scalar', 'Measured visible surface appearance. This is not a species, feeding-strategy or edible flag.'],
  ['object.motion', 'vec2', 'Estimated relative motion of the visible object in the cell’s body frame. Zero when invalid.'],
  ['gut.soluble.energy', 'scalar', 'Chemical energy in the soluble fraction of captured food.'],
  ['gut.soluble.nutrients', 'scalar', 'Minerals held with soluble carbon in the gut. Digestion can transfer it to free reserves.'],
  ['gut.structural.energy', 'scalar', 'Chemical energy in the structural fraction of captured food.'],
  ['gut.structural.nutrients', 'scalar', 'Minerals held with structural carbon in the gut. Some remains in the waste produced during digestion.'],
  ['gut.bound.energy', 'scalar', 'Chemical energy in the refractory, bound fraction of captured food.'],
  ['gut.bound.nutrients', 'scalar', 'Minerals held with bound carbon in the gut. Processing returns unassimilated minerals to the surrounding water.'],
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
  'receptor', 'regulator', 'replicase', 'constrictor', 'adhesin',
] as const);
export const CELLULAR_CONSTRUCTORS: readonly GenomeSymbol[] = Object.freeze(CELLULAR_PRODUCT_ROLES.map(role =>
  symbol(role, 'function', 'product', `${role}(${role === 'regulator' ? 'halfLife: number' : role === 'adhesin' ? 'precursor: scaffold product' : ''}) → product`,
    role === 'regulator'
      ? 'Declares a persistent regulatory product. Its positive half-life is measured in seconds; pool.Name reads its physical stock. Synthesis costs material and energy, and stock partitions at division.'
      : role === 'adhesin' ? 'Finishes a named scaffold precursor into a binding protein. Expression consumes existing precursor stock, usable energy and translation work. Adhesion surfaces install the finished protein together with membrane and anchor material.'
      : `Declares a ${role} product. Genes synthesize its stock; assembly recipes consume it as a physical component. A declaration by itself creates no material.`)));
export const CONTACT_RECIPE_KINDS = ['pierce()', 'import(particle)', 'engulf()', 'adhere()', 'piercer', 'phagocyte'] as const;
export const CONTACT_FIELDS: readonly GenomeSymbol[] = [
  ['message_valid', 'boolean', 'True when a working touch receptor on this surface reaches the contact partner’s active display and reads its occupied marker register.'],
  ['message', 'scalar', 'The marker measured where this cell’s working touch receptor reaches the partner’s active display. Check peer.message_valid to distinguish an absent marker from zero.'],
  ['signal_valid', 'boolean', 'True when a working touch receptor at this contact reaches the partner’s active display and reads its occupied payload register.'],
  ['signal', 'scalar', 'The payload measured where this cell’s working touch receptor reaches the partner’s active display. Check peer.signal_valid before reading it.'],
  ['division_interface', 'boolean', 'True while testing adhesion across a new cleavage, and at a division interface retained by adhesion. An unbonded parent and daughter subsequently meet through ordinary contact.'],
  ['resistance', 'scalar', 'Physical resistance measured by a working touch receptor at this interface.'],
  ['resistance_valid', 'boolean', 'A local contact receptor measured this candidate’s resistance.'],
].map(([name, type, description]) => symbol(`peer.${name}`, 'signal', type, `peer.${name}: ${type}`, description + ' Read this operand in an engage_when condition or a binding or function used by that condition.'));
export const CELLULAR_KEYWORDS: readonly GenomeSymbol[] = Object.freeze([
  { ...symbol('cellular', 'keyword', 'mode', 'cellular counts;', 'Starts a cellular program with whole resource counts. The earlier cellular; header uses continuous units. Declare products, express them with genes, and combine their stocks into physical assemblies.'), insertText: 'cellular counts;\n' },
  { ...symbol('product', 'keyword', 'declaration', 'product Name = motor();', 'Names a typed material product. Product roles determine which physical recipes it can satisfy.'), insertText: 'product ' },
  { ...symbol('gene', 'keyword', 'expression', 'gene drive: Motor = expression;', 'Allocates synthesis effort to a named product. Effort is bounded to 0–1; actual production shares finite translation work, minerals, energy and space.'), insertText: 'gene ' },
  { ...symbol('assembly', 'keyword', 'recipe', 'assembly Swim: propel() { components; }', 'Names a physical recipe. Its product masses determine composition; stocks from the start of the tick are consumed as components are installed.'), insertText: 'assembly ' },
  { ...symbol('fn', 'keyword', 'declaration', 'fn response(value, threshold) = expression;', 'Defines a pure function with numeric arguments. It can read earlier bindings and start-of-tick registers. Calls expand into the bounded expression graph.'), insertText: 'fn ' },
  { ...symbol('publish', 'keyword', 'contact message', 'publish rf001;', 'Selects the marker register for display() patches. Assign a value with an ordinary register write. Only an active patch facing a working touch receptor can present it.'), insertText: 'publish ' },
  { ...symbol('signal', 'keyword', 'contact signal', 'signal rf002;', 'Selects the payload register for display() or junction(signal). An empty register is silent. A stored zero is a valid payload.'), insertText: 'signal ' },
  { ...symbol('engage_when', 'keyword', 'boolean', 'engage_when: condition;', 'Evaluated for each candidate at a particle inlet, piercing, engulfing or adhesive surface. Both adhesive surfaces must allow a bond. Unguarded weapons engage by default; unguarded adhesive surfaces remain closed.'), insertText: 'engage_when: ' },
  { ...symbol('start_when', 'keyword', 'boolean', 'start_when: condition;', 'Allows another instance of this recipe to start when true; defaults to true. Existing incomplete instances continue paid installation when this becomes false.'), insertText: 'start_when: ' },
  { ...symbol('recycle', 'keyword', 'scalar', 'recycle: expression;', 'Requests paid disassembly of machinery with this recipe name, bounded to 0–1; defaults to zero. Requires a built, active recycle() mechanism. Recovered ingredients keep their original identities and become available on a later tick.'), insertText: 'recycle: ' },
  { ...symbol('activity', 'keyword', 'scalar', 'activity: expression;', 'Sets operating effort from 0 to 1 on working machinery. Storage capacity comes from installed compartments. Use start_when to build storage or recycle to remove it.'), insertText: 'activity: ' },
  { ...symbol('at', 'keyword', 'radians', 'at: 0;', 'Attachment angle on the membrane, measured in body-relative radians. Placement sets the position of surface machinery and can affect its inheritance at division. Internal machinery has no membrane attachment.'), insertText: 'at: 0;' },
  { ...symbol('direction', 'keyword', 'radians', 'direction: 0;', 'Body-relative angle in radians. It aims propulsion in propel(), the piercing tip in pierce(), and the viewing direction in sense(optical).'), insertText: 'direction: 0;' },
].map(item => Object.freeze(item)));
export const CELLULAR_READ_SIGNALS: readonly GenomeSymbol[] = Object.freeze(BUILTIN_SIGNALS.filter(item =>
  ['energy', 'energy.fraction', 'energy.capacity', 'age', 'speed', 'integrity', 'oxygen', 'respiration.rate', 'memory.used', 'memory.cost'].includes(item.name)
  || (item.name.startsWith('body.') && item.name !== 'body.division_ready')
  || /^(nutrient|gut|carbon)\./.test(item.name)));
const cellularReadNames = new Set(CELLULAR_READ_SIGNALS.map(item => item.name));
export const CELLULAR_ASSEMBLY_KINDS: readonly GenomeSymbol[] = Object.freeze([
  ...ORGANELLE_CATALOG.map(item => symbol(item.kind, 'keyword', 'assembly', `${item.kind} { components; activity: expression; }`, item.description)),
  symbol('membrane', 'keyword', 'assembly', 'membrane { components; activity: expression; }', 'Membrane and enzyme products support paid envelope growth, adding space with finite minerals, energy and core work.'),
  symbol('replicator', 'keyword', 'assembly', 'replicator { components; activity: expression; }', 'Scaffold and replicase products build machinery that performs paid genome replication over time.'),
  symbol('constrictor', 'keyword', 'assembly', 'constrictor { components; activity: expression; }', 'Anchor, motor and constrictor products build machinery that advances physical constriction after growth and replication.'),
]);
const internalMechanisms = new Set(['support','translate','store','digest','repair','replicate','recycle','respire']);
const directionalMechanisms = new Set(['propel()','pierce()','sense(optical)']);
function mechanismHasControl(kind: string | undefined, field: string): boolean {
  if (!kind) return true;
  const aliases: Record<string,string> = {'digest(exudate)':'digest(soluble)','digest(tissue)':'digest(structural)','digest(waste)':'digest(bound)'};
  const mechanism = MECHANISM_CATALOG.find(item => item.kind === (aliases[kind] ?? kind));
  if (!mechanism) return true;
  if (field === 'at') return !internalMechanisms.has(mechanism.constructor);
  if (field === 'direction') return directionalMechanisms.has(mechanism.kind);
  if (field === 'activity') return mechanism.constructor !== 'store';
  return true;
}
const defaultMechanismControl = (kind:string): string => mechanismHasControl(kind,'activity') ? 'activity: 1;' : 'start_when: true;';
export const CELLULAR_MECHANISMS: readonly GenomeSymbol[] = Object.freeze(MECHANISM_CATALOG.map(item =>
  symbol(item.kind, 'function', 'mechanism', `${item.kind} { components; ${defaultMechanismControl(item.kind)} }`, item.description)));
const mechanismConstructorByName = new Map(MECHANISM_CATALOG.map(item => [item.constructor,
  symbol(item.constructor, 'function', 'mechanism', `${item.constructor}(${MECHANISM_CATALOG.filter(other=>other.constructor===item.constructor).flatMap(other=>other.target?[other.target]:[]).join(' | ')})`,
    MECHANISM_CATALOG.filter(other=>other.constructor===item.constructor).map(other=>other.description).join(' '))]));
const cellularSymbolByName = new Map([...CELLULAR_CONSTRUCTORS, ...CELLULAR_KEYWORDS, ...CONTACT_FIELDS].map(item => [item.name, item]));

type InputScope = Pick<GenomeBinding,'perContact' | 'contactSafe'>;
export interface CellularDeclarations {
  locals: (InputScope & {name:string; from:number; to:number; end:number})[];
  productUses: ReadonlySet<number>;
  products: { name: string; role: string; precursor?: string; from: number; to: number }[];
  genes: { name: string; product: string; from: number; to: number }[];
  assemblies: { name: string; kind: string; from: number; to: number; kindFrom: number; kindTo: number }[];
  functions: (InputScope & { name: string; parameters: string[]; parameterRanges: {name: string; from: number; to: number}[];
    from: number; to: number; bodyFrom: number; end: number })[];
}
let declarationCache:{source:string;value:CellularDeclarations}|undefined;

/** Tolerant authoring assistance only. Rust validates names, recipes and types. */
export function cellularDeclarations(source: string): CellularDeclarations {
  if (declarationCache?.source===source) return declarationCache.value;
  const tokens = tokensFor(source).filter(token => token.kind !== 'comment');
  const productUses = new Set<number>();
  const result: CellularDeclarations = { locals: [], productUses, products: [], genes: [], assemblies: [], functions: [] };
  // While a draft is being checked, keep obvious contact-only and cell-only
  // dependencies out of incompatible contexts. Compiler metadata takes priority.
  const previous = new Map<string,InputScope>();
  function scopeOf(start:number,end:number,parameters:string[] = []): InputScope {
    let peer = false, local = false;
    for (let i=start; i<end; i++) {
      const name = tokens[i].text;
      if (parameters.includes(name) && tokens[i+1]?.text !== '(') continue;
      const inherited = previous.get(name);
      peer ||= name.startsWith('peer.') || inherited?.perContact === true;
      local ||= inherited?.contactSafe === false || cellularReadNames.has(name) || /^(sensed|pool|built|concentration|gut)\./.test(name);
    }
    return {...(peer ? {perContact:true} : {}), ...(local ? {contactSafe:false} : {})};
  }
  for (let index = 0; index + 3 < tokens.length; index++) {
    const declaration = tokens[index].text, name = tokens[index + 1];
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name.text)) continue;
    const detail = tokens[index + 3].text;
    if (declaration === 'let' && tokens[index + 2].text === '=') {
      let end = index + 3;
      while (end < tokens.length && tokens[end].text !== ';') end++;
      const scope = scopeOf(index+3,end);
      result.locals.push({name:name.text, from:name.from, to:name.to, end:tokens[end]?.to ?? source.length + 1,...scope});
      previous.set(name.text,scope);
    }
    if (declaration === 'product' && tokens[index + 2].text === '=') {
      const input = detail === 'adhesin' && tokens[index+4]?.text==='(' ? tokens[index+5]?.text : undefined;
      if (input && tokens[index+5]) productUses.add(tokens[index+5].from);
      result.products.push({ name:name.text, role:detail, from:name.from, to:name.to,
        ...(input && /^[A-Za-z_][A-Za-z0-9_]*$/.test(input) ? {precursor:input} : {}) });
    }
    if (declaration === 'gene' && tokens[index + 2].text === ':') {
      productUses.add(tokens[index+3].from);
      result.genes.push({name:name.text,product:detail,from:name.from,to:name.to});
    }
    if (declaration === 'assembly' && tokens[index + 2].text === ':') {
      let kind = detail, kindTo = tokens[index+3].to;
      if (tokens[index + 4]?.text === '(') {
        let close = index+5;
        while (close < tokens.length && ![')', '{', ';'].includes(tokens[close].text)) close++;
        if (tokens[close]?.text===')') { kind=tokens.slice(index+3,close+1).map(token=>token.text).join(''); kindTo=tokens[close].to; }
      }
      let open = index + 4;
      while (open < tokens.length && !['{','}',';'].includes(tokens[open].text)) open++;
      if (tokens[open]?.text === '{') {
        for (let i=open+1; i < tokens.length && tokens[i].text !== '}'; i++) {
          if (!['{',';'].includes(tokens[i-1].text) || tokens[i+1]?.text !== ':') continue;
          const field = tokens[i].text;
          if (!['activity','start_when','engage_when','recycle','at','direction'].includes(field)
            || field === 'engage_when' && tokens[i+2]?.kind === 'number' && tokens[i+3]?.text === ';') productUses.add(tokens[i].from);
        }
      }
      result.assemblies.push({ name: name.text, kind, from: name.from, to: name.to, kindFrom:tokens[index+3].from, kindTo });
    }
    if (declaration === 'fn' && tokens[index + 2].text === '(') {
      let close = index + 3;
      while (close < tokens.length && ![')', ';', '='].includes(tokens[close].text)) close++;
      if (tokens[close]?.text === ')') {
        const parameterRanges = tokens.slice(index + 3, close).filter(token => /^[A-Za-z_]\w*$/.test(token.text))
          .map(({text, from, to}) => ({name:text, from, to}));
        let stop = close + 1;
        while (stop < tokens.length && tokens[stop].text !== ';') stop++;
        const bodyFrom = tokens[close+1]?.text === '=' ? tokens[close+1].to : source.length + 1;
        const parameters = parameterRanges.map(item => item.name);
        const scope = scopeOf(close+2,stop,parameters);
        result.functions.push({name:name.text,parameters,parameterRanges,from:name.from,to:name.to,bodyFrom,end:tokens[stop]?.to ?? source.length + 1,...scope});
        previous.set(name.text,scope);
      }
    }
  }
  declarationCache={source,value:result};
  return result;
}

const cellularHeader = /^(?:\s|#[^\r\n]*(?:\r?\n|$))*cellular\b(?:\s|#[^\r\n]*(?:\r?\n|$))*(?:(counts)(?:\s|#[^\r\n]*(?:\r?\n|$))*)?;/;
export function isCellularSource(source: string): boolean { return cellularHeader.test(source); }
export function usesCountUnits(source: string): boolean { return cellularHeader.exec(source)?.[1] === 'counts'; }

export const COUNT_RESOURCE_SIGNALS = Object.freeze([
  'energy','energy.capacity','nutrient.stored','nutrient.capacity','gut.energy','gut.capacity',
  'gut.soluble.energy','gut.soluble.nutrients','gut.structural.energy','gut.structural.nutrients','gut.bound.energy','gut.bound.nutrients',
  'carbon.organic','carbon.inorganic','oxygen',
]);
const countSignals = CELLULAR_READ_SIGNALS.map(item => COUNT_RESOURCE_SIGNALS.includes(item.name)
  ? symbol(item.name,'signal','count',`${item.name}: count`,`${item.description} One unit equals 10,000 whole counts.`) : item);
const readSignals = (source:string) => usesCountUnits(source) ? countSignals : CELLULAR_READ_SIGNALS;

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
  'sense(contact)': [...receptorFlags, measured('offset', 'vec2', 'Measured body-relative displacement to a contacting surface.'), measured('pressure', 'scalar', 'Measured contact pressure.'), measured('resistance', 'scalar', 'Measured resistance of the contacting surface.'), measured('flow', 'vec2', 'Relative water flow at the receptor, in body coordinates.'), measured('speed', 'scalar', 'Magnitude of the measured relative flow.'), measured('bearing', 'scalar', 'Body-relative direction of the measured flow, in radians.'), measured('message_valid', 'boolean', 'True when a working display on the touched patch presents a marker.'), measured('message', 'scalar', 'The scalar that the touched cell published at the start of the tick. A published zero is valid.'), measured('signal_valid', 'boolean', 'True when a working display on the touched patch presents a payload.'), measured('signal', 'scalar', 'The signal payload from the same touched cell and start-of-tick snapshot as the recognition marker.'), measured('bonded','boolean','True when this receptor touches a cell directly bonded to its owner. A matching marker alone can come from a stranger.')],
  'junction(signal)': [...receptorFlags, measured('value', 'scalar', 'Start-of-tick payload at the paired signal port.'), measured('signal_valid', 'boolean', 'An occupied register supplies a payload across this port, including zero.'), measured('signal', 'scalar', 'Start-of-tick payload at the contacting signal port.')],
  'sense(optical)': [...receptorFlags, measured('offset', 'vec2', 'Body-relative displacement to an object visible through this receptor.'), measured('area', 'scalar', 'Measured apparent area of the visible object.'), measured('contrast', 'scalar', 'Measured optical contrast.'), measured('pigment', 'scalar', 'Measured visible surface appearance.'), measured('motion', 'vec2', 'Measured relative motion in body coordinates.')],
});

// Older cellular recipes name the receptor family directly. Rust exposes the
// same named fields for both spellings, including contact messages.
const receptorMechanisms: Readonly<Record<string, string>> = {
  photoreceptor: 'sense(light)', thermoreceptor: 'sense(heat)', chemoreceptor: 'sense(chemical)',
  mechanoreceptor: 'sense(contact)', optical: 'sense(optical)',
};

function declaredFunctionSymbol(item: CellularDeclarations['functions'][number], checked?: GenomeBinding): GenomeSymbol {
  const type = checked?.type ?? 'not checked';
  const signature = `${item.name}(${item.parameters.map(name => `${name}: number`).join(', ')})${checked ? ' → ' + type : ''}`;
  const scope = checked?.perContact ? ' Its result depends on the current contact partner.'
    : checked?.contactSafe === false ? ' It reads the cell’s own measurements and cannot run in an engage_when condition.' : '';
  return symbol(item.name, 'function', type, signature, 'Pure function declared in this program. Numeric arguments are available as scalar parameters.' + scope);
}
function parameterSymbol(name: string, fn: string): GenomeSymbol {
  return symbol(name, 'binding', 'scalar', `${name}: scalar`, `Scalar parameter of ${fn}. It is visible only inside this function.`);
}

let dynamicCache: {source:string; symbols:GenomeSymbol[]} | undefined;
export function cellularDynamicSymbols(source: string): GenomeSymbol[] {
  if (dynamicCache?.source === source) return dynamicCache.symbols;
  const declarations = cellularDeclarations(source);
  const stockType = usesCountUnits(source) ? 'count' : 'scalar';
  const symbols = [
    ...declarations.products.map(item => symbol(`pool.${item.name}`, 'signal', stockType, `pool.${item.name}: ${stockType}`,
      `Available ${item.role} product ${stockType === 'count' ? 'count' : 'mass'} before this tick. Stock persists, occupies space and is shared by all recipes that name ${item.name}. Regulator stocks decay according to their half-life.`)),
    ...declarations.products.map(item => symbol(`concentration.${item.name}`, 'signal', 'scalar', `concentration.${item.name}: scalar`,
      `Available ${item.role} product mass divided by current envelope area. Growth dilutes this value; synthesis, decay and inheritance change the underlying physical stock.`)),
    ...declarations.assemblies.map(item => symbol(`built.${item.name}`, 'signal', 'scalar', `built.${item.name}: scalar`,
      `Functional material in complete ${item.name} assemblies, reduced by damage. Incomplete assemblies contribute zero; their installed components are shown in the Body inspector.`)),
    ...declarations.assemblies.flatMap(item => (CELLULAR_RECEPTOR_FIELDS[receptorMechanisms[item.kind] ?? item.kind] ?? []).map(field =>
      symbol(`sensed.${item.name}.${field.name}`, 'signal', field.type, `sensed.${item.name}.${field.name}: ${field.type}`,
        `${field.description} Sampling uses existing machinery and the activity stored from the preceding tick. ${(field.name.startsWith('message') || field.name.startsWith('signal')) ? `Check sensed.${item.name}.${field.name.split('_')[0]}_valid before using the value. An occupied zero is valid.` : (item.kind==='sense(contact)' || item.kind==='mechanoreceptor') && ['flow','speed','bearing'].includes(field.name) ? `Check sensed.${item.name}.available; flow is measurable even without contact.` : `Invalid measurements are zero; check sensed.${item.name}.valid.`}`))),
  ];
  dynamicCache = {source, symbols};
  return symbols;
}

function productSymbol(product: CellularDeclarations['products'][number]): GenomeSymbol {
  const {name} = product;
  return symbol(name, 'binding', `${product.role} product`, `${name}: ${product.role}`, `${product.precursor ? `Finishing consumes an equal mass of existing ${product.precursor} stock.` : 'Physical product declared by this genome.'} Use pool.${name} to read its available stock, or name it in an assembly recipe with a positive material amount.`);
}

export function lookupCellularSymbol(name: string, source: string, functions: readonly GenomeBinding[] = []): GenomeSymbol | undefined {
  const historical = /^gut\.(exudate|tissue|waste)\.(energy|nutrients)$/.exec(name);
  if (historical) {
    const substrates: Record<string,string> = {exudate:'soluble', tissue:'structural', waste:'bound'};
    const current = readSignals(source).find(item => item.name === `gut.${substrates[historical[1]]}.${historical[2]}`);
    if (current) return symbol(name,'signal',current.type,`${name}: ${current.type}`,`Historical spelling of ${current.name}. ${current.description}`);
  }
  if (usesCountUnits(source) && COUNT_RESOURCE_SIGNALS.includes(name)) return countSignals.find(item => item.name === name);
  const dynamic = cellularDynamicSymbols(source).find(item => item.name === name);
  if (dynamic) return dynamic;
  const declarations = cellularDeclarations(source);
  const product = declarations.products.find(item => item.name === name);
  if (product) return productSymbol(product);
  const gene = declarations.genes.find(item => item.name === name);
  if (gene) return symbol(name, 'binding', 'gene', `gene ${name}: ${gene.product}`, `Expression effort for ${gene.product}. The Body inspector compares requested expression with actual material synthesized.`);
  const declaredFunction = declarations.functions.find(item => item.name === name);
  if (declaredFunction) return declaredFunctionSymbol(declaredFunction, functions.find(item => item.from === declaredFunction.from));
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
  'A number can be a signed integer, an unsigned resource count or a scalar. Counts and integers convert exactly to scalar registers. Assigning a count to ri clamps it to the signed 32-bit range. Use int() to convert a scalar.',
  'Addition, subtraction and multiplication of whole operands stay integral. A count operand gives an unsigned result bounded from 0 through 4,294,967,295. A scalar operand or division gives a scalar. Comparisons return booleans.',
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
    binding.perContact ? 'Immutable binding evaluated for the current physical contact partner. Use it in an engage_when condition.'
      : 'Immutable local signal. Its expression uses the current inputs and start-of-tick register values.');
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
export function tokenizeGenome(source: string, cellular = isCellularSource(source)): GenomeToken[] {
  const counted = usesCountUnits(source);
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
      const name = source.slice(start, pos), shared = symbolByName.get(name), register = registerSymbol(name);
      const known = cellular ? cellularSymbolByName.get(name) ?? mechanismConstructorByName.get(name)
        ?? (shared && (shared.category === 'keyword' || shared.category === 'function' || cellularReadNames.has(name)) ? shared : undefined)
        : shared ?? compatibilityOutputs.get(name);
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
        if (known.category === 'signal' || known.category === 'output') type = counted && COUNT_RESOURCE_SIGNALS.includes(name) ? 'count' : known.type as PrimitiveType;
      } else if (/^(pool|built|concentration)\.[A-Za-z_][A-Za-z0-9_]*$/.test(name)) { kind = 'signal'; type = counted && name.startsWith('pool.') ? 'count' : 'scalar'; }
      else if (/^sensed\.[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_]+$/.test(name)) {
        kind = 'signal';
        type = /\.(?:valid|available|message_valid|signal_valid|bonded)$/.test(name) ? 'boolean' : /\.(offset|flow|motion)$/.test(name) ? 'vec2' : 'scalar';
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

// Repeated hover, completion and diagnostic queries share one immutable draft
// tokenization. The public scanner still returns a fresh array to its caller.
let tokenCache: {source:string; tokens:readonly GenomeToken[]} | undefined;
function tokensFor(source: string): readonly GenomeToken[] {
  if (tokenCache?.source !== source) tokenCache = {source, tokens:tokenizeGenome(source)};
  return tokenCache.tokens;
}

const identifierKinds = new Set<GenomeTokenKind>(['keyword', 'boolean', 'identifier', 'signal', 'function', 'output', 'register']);
/** Finds an identifier under the cursor, or immediately before it at whitespace/end of source. */
export function identifierAt(source: string, position: number): { from: number; to: number; text: string } | null {
  if (!Number.isInteger(position) || position < 0 || position > source.length) return null;
  const tokens = tokensFor(source);
  const current = tokens.find(token => token.from <= position && position < token.to);
  const token = current ?? tokens.find(token => token.to === position);
  return token && identifierKinds.has(token.kind) ? { from: token.from, to: token.to, text: token.text } : null;
}

export function lookupProgramSymbol(name: string, source: string, bindings: readonly GenomeBinding[] = [], functions: readonly GenomeBinding[] = []): GenomeSymbol | undefined {
  const shared = lookupGenomeSymbol(name, bindings);
  if (!isCellularSource(source)) return shared;
  if (shared?.category === 'binding') return shared;
  return lookupCellularSymbol(name, source, functions)
    ?? (shared && ['function','keyword','register'].includes(shared.category) ? shared : undefined)
    ?? readSignals(source).find(item => item.name === name);
}

export function symbolAt(source: string, position: number, bindings: readonly GenomeBinding[] = [], functions: readonly GenomeBinding[] = [],
  span: {from:number; to:number; text:string} | null = identifierAt(source, position)): { from: number; to: number; symbol: GenomeSymbol } | null {
  if (!span) return null;
  const cellular = isCellularSource(source);
  if (cellular) {
    const declarations = cellularDeclarations(source);
    const product = declarations.products.find(item => item.name === span.text);
    if (product && (product.from === span.from || declarations.productUses.has(span.from)))
      return {...span,symbol:productSymbol(product)};
    const gene = declarations.genes.find(item => item.from === span.from);
    if (gene) return {...span,symbol:symbol(gene.name,'binding','gene',`gene ${gene.name}: ${gene.product}`,`Expression effort for ${gene.product}.`)};
    const declaredAssembly = declarations.assemblies.find(item => item.from === span.from);
    if (declaredAssembly) return {...span,symbol:symbol(declaredAssembly.name,'binding',declaredAssembly.kind+' assembly',`${declaredAssembly.name}: ${declaredAssembly.kind}`,`Physical recipe. Read built.${declaredAssembly.name} to measure its functional mass.`)};
    const fn = declarations.functions.find(item => item.from <= span.from && span.from < item.end);
    if (fn?.parameterRanges.some(p => p.name === span.text && (p.from === span.from || span.from >= fn.bodyFrom)))
      return { ...span, symbol:parameterSymbol(span.text, fn.name) };
    // A target like 'signal' in junction(signal) describes the assembly, while
    // a top-level 'signal' selects an outgoing register.
    const assembly = declarations.assemblies.find(item => item.kindFrom <= span.from && span.to <= item.kindTo);
    if (assembly) {
      const mechanism = CELLULAR_MECHANISMS.find(item => item.name === assembly.kind);
      if (mechanism && [mechanism.name.split('(')[0], mechanism.name.slice(mechanism.name.indexOf('(')+1,-1)].includes(span.text))
        return { ...span, symbol:mechanism };
    }
    const local = declarations.locals.find(item => item.name === span.text);
    if (local && local.from !== span.from && local.end > span.from) return null;
    const declared = declarations.functions.find(item => item.name === span.text);
    if (declared && (declared.from === span.from || /^\s*\(/.test(source.slice(span.to)))) {
      if (declared.from !== span.from && declared.end > span.from) return null;
      return {...span,symbol:declaredFunctionSymbol(declared,functions.find(item => item.from === declared.from))};
    }
  }
  const found = lookupProgramSymbol(span.text, source, bindings, functions);
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
  for (const token of tokensFor(source)) {
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
  kind: 'statement' | 'expression' | 'continuation' | 'register' | 'none' | 'constructor' | 'product' | 'assembly-kind' | 'component' | 'mechanism-target' | 'scaffold-precursor';
  keyword?: 'then' | 'else' | 'when';
  fieldsPresent?: ReadonlySet<string>;
  mechanismConstructor?: string;
  contactGuard?: boolean;
  definition?: boolean;
  contactRecipe?: boolean;
  recipeKind?: string;
  /** A useful editing hint, never a substitute for the Rust type checker. */
  expected?: PrimitiveType | 'number' | 'register' | 'vec2 | string' | 'number | vec2';
}

const numericFunctions = new Set(['sin', 'cos', 'abs', 'min', 'max', 'clamp', 'int', 'float', 'vec', 'sqrt', 'sign', 'atan2', 'lerp', 'noise', 'activate', 'repress']);
const vectorFunctions = new Set(['dot', 'normalize', 'angle', 'x', 'y']);

function cellularCompletionContext(source: string, position: number, bindings: readonly GenomeBinding[], functions: readonly GenomeBinding[]): GenomeCompletionContext {
  const all = tokensFor(source);
  const enclosing = all.find(token => token.from <= position && position <= token.to);
  if (enclosing?.kind === 'comment' && position > enclosing.from
    || enclosing?.kind === 'string' && position > enclosing.from) return { kind: 'none' };
  const preceding = all.filter(token => token.kind !== 'comment' && token.from < position);
  const last = preceding.at(-1);
  if (last && last.to >= position && identifierKinds.has(last.kind)
    && !['cellular', 'product', 'gene', 'assembly', 'fn', 'if', 'then', 'else', 'when', 'clear', 'publish', 'signal', 'engage_when', 'activity', 'start_when', 'recycle', 'at', 'direction'].includes(last.text)) preceding.pop();
  const braces: number[] = [];
  for (let index = 0; index < preceding.length; index++) {
    if (preceding[index].text === '{') braces.push(index);
    if (preceding[index].text === '}') braces.pop();
  }
  let start = preceding.length;
  while (start > 0 && ![';', '{', '}'].includes(preceding[start - 1].text)) start--;
  const tokens = preceding.slice(start);
  const infer = (rhs: GenomeToken[], expected?: PrimitiveType, canGuard = false): GenomeCompletionContext => {
    const declarations = cellularDeclarations(source);
    const current = declarations.functions.find(item => item.bodyFrom <= position && position < item.end);
    const lookup = (name: string) => current?.parameters.includes(name) ? 'scalar' : lookupProgramSymbol(name, source, bindings, functions)?.type;
    const numericCall = (name: string) => declarations.functions.some(item => item.name === name && item.end <= position);
    return expressionContext(rhs, expected, lookup, numericCall, canGuard);
  };
  const rhs = (separator: string) => tokens.slice(tokens.findIndex(token => token.text === separator) + 1);
  if (braces.length) {
    const brace = braces[braces.length - 1];
    // The opening brace follows either a named family or a mechanism call.
    const header = preceding.slice(Math.max(0, brace - 5), brace).map(token => token.text);
    const kind = header.slice(header.lastIndexOf(':') + 1).join('');
    const contactRecipe = CONTACT_RECIPE_KINDS.some(candidate => candidate === kind);
    if (!tokens.length || !tokens.some(token => token.text === ':')) {
      const fieldsPresent = new Set<string>();
      const editing = last && last.to >= position && identifierKinds.has(last.kind) ? last.from : -1;
      const open = all.findIndex(token => token.from === preceding[brace].from);
      let atField = true;
      for (let i = open + 1; i < all.length && all[i].text !== '}'; i++) {
        const t = all[i];
        if (t.kind === 'comment') continue;
        if (atField && t.from !== editing && t.text !== ';') fieldsPresent.add(t.text);
        atField = t.text === ';';
      }
      return {kind:'component',contactRecipe,fieldsPresent,recipeKind:kind};
    }
    if (tokens[0].text === 'engage_when') {
      if (!contactRecipe) return { kind: 'none' };
      return { ...infer(rhs(':'), 'boolean'), contactGuard: true };
    }
    if (!['activity', 'start_when', 'recycle'].includes(tokens[0].text)) return { kind: 'none' };
    return infer(rhs(':'), tokens[0].text === 'start_when' ? 'boolean' : 'scalar');
  }
  if (!tokens.length) return { kind: 'statement' };
  const assignment = tokens.some(token => token.text === '='), colon = tokens.some(token => token.text === ':');
  if (['publish', 'signal'].includes(tokens[0].text)) {
    return tokens.length === 1 ? { kind: 'register', expected: 'scalar' } : { kind: 'none' };
  }
  if (tokens[0].text === 'clear') return tokens.some(token => token.text === 'when')
    ? infer(rhs('when'), 'boolean') : tokens[1]?.kind === 'register' ? {kind:'continuation', keyword:'when'} : {kind:'register', expected:'register'};
  if (tokens[0].kind === 'register') {
    if (!assignment) return {kind:'statement'};
    return tokens.some(token => token.text === 'when') ? infer(rhs('when'),'boolean') : infer(rhs('='),tokens[0].type,true);
  }
  if (tokens[0].text === 'product') {
    if (tokens.length === 5 && tokens[3].text === 'adhesin' && tokens[4].text === '(') return {kind:'scaffold-precursor'};
    return { kind: assignment && !tokens.some(token=>token.text==='(') ? 'constructor' : 'none' };
  }
  if (tokens[0].text === 'gene') {
    if (!assignment) return { kind: colon ? 'product' : 'none' };
    return infer(rhs('='),'scalar');
  }
  if (tokens[0].text === 'assembly') {
    if (!colon) return { kind:'none' };
    const open = tokens.findIndex(token=>token.text==='(');
    if (open>=0) return tokens.some(token=>token.text===')') ? {kind:'none'} : {kind:'mechanism-target',mechanismConstructor:tokens[open-1]?.text};
    return {kind:'assembly-kind'};
  }
  if (tokens[0].text === 'let' || tokens[0].text === 'fn') {
    if (!assignment) return { kind: 'none' };
    return { ...infer(rhs('=')), definition:true };
  }
  return { kind: 'statement' };
}

/** Incomplete code is expected here. Infer only the constraints visible at the cursor. */
export function genomeCompletionContext(source: string, position: number, bindings: readonly GenomeBinding[] = [], functions: readonly GenomeBinding[] = []): GenomeCompletionContext {
  if (!Number.isInteger(position) || position < 0 || position > source.length) return { kind: 'none' };
  if (isCellularSource(source)) return cellularCompletionContext(source, position, bindings, functions);
  const all = tokensFor(source);
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
  if (tokens[0].text === 'clear' && !tokens.some(token => token.text === 'when'))
    return tokens[1]?.kind === 'register' ? {kind:'continuation',keyword:'when'} : { kind: 'register', expected: 'register' };
  const assignment = tokens.findIndex(token => token.text === '=');
  if (assignment < 0 && tokens[0].text === 'let') return { kind: 'none' };
  if (assignment < 0 && tokens.length === 1) return { kind: 'statement' };
  const expressionTokens = assignment >= 0 ? tokens.slice(assignment + 1) : tokens;

  const target = assignment > 0 && tokens[0].text !== 'let' ? lookupGenomeSymbol(tokens[0].text, bindings) : undefined;
  return expressionContext(expressionTokens, target?.type as PrimitiveType | undefined, name => lookupGenomeSymbol(name, bindings)?.type, undefined, tokens[0].kind === 'register' && !tokens.some(token => token.text === 'when'));
}

function expressionContext(expressionTokens: GenomeToken[], expected: PrimitiveType | undefined,
  lookupType: (name:string) => string | undefined, numericCall: (name:string) => boolean = () => false, canGuard = false): GenomeCompletionContext {
  // Find the innermost still-open call, skipping complete nested expressions.
  const calls: { name: string | undefined; index: number }[] = [];
  const branches: {depth:number; needs:'then' | 'else'}[] = [];
  for (let index = 0; index < expressionTokens.length; index++) {
    const token = expressionTokens[index];
    if (token.text === 'if') branches.push({depth:calls.length, needs:'then'});
    else if (token.text === 'then' && branches.at(-1)?.needs === 'then') branches[branches.length-1].needs = 'else';
    else if (token.text === 'else' && branches.at(-1)?.needs === 'else') branches.pop();
    if (token.text === '(') calls.push({ name: expressionTokens[index - 1]?.kind === 'function' ? expressionTokens[index - 1].text : undefined, index });
    else if (token.text === ')') calls.pop();
  }
  const currentCall = calls.at(-1);
  if (currentCall?.name === 'populated') return { kind: 'register', expected: 'register' };
  const tail = expressionTokens.at(-1);
  const last = tail?.text;
  if (tail && (last === ')' || ['number','string','boolean','identifier','signal','register'].includes(tail.kind))) {
    const branch = branches.at(-1);
    if (branch?.depth === calls.length) return {kind:'continuation',keyword:branch.needs};
    if (canGuard && !calls.length) return {kind:'continuation',keyword:'when'};
  }
  if (['if', 'when', '&&', '||', '!'].includes(last ?? '')) return { kind: 'expression', expected: 'boolean' };
  if (currentCall?.name && (!last || last === '(' || last === ',')) {
    if (numericFunctions.has(currentCall.name) || numericCall(currentCall.name)) return { kind: 'expression', expected: 'number' };
    if (vectorFunctions.has(currentCall.name)) return { kind: 'expression', expected: 'vec2' };
    if (currentCall.name === 'length') return { kind: 'expression', expected: 'vec2 | string' };
  }
  if (last === '*') {
    const previous = expressionTokens.at(-2);
    return {kind:'expression', expected: previous && lookupType(previous.text) === 'vec2' ? 'number' : 'number | vec2'};
  }
  if (last && ['<', '<=', '>', '>=', '/'].includes(last)) return { kind: 'expression', expected: 'number' };
  if (last && ['==', '!=', '+', '-'].includes(last)) {
    const previous = expressionTokens.at(-2);
    const known = previous && (lookupType(previous.text) ?? (previous.kind === 'string' ? 'string' : previous.kind === 'number' ? 'number' : undefined));
    if (known && ['integer', 'count', 'scalar', 'boolean', 'string', 'vec2', 'number'].includes(known)) {
      return { kind: 'expression', expected: known === 'integer' || known === 'count' || known === 'scalar' ? 'number' : known as PrimitiveType | 'number' };
    }
  }
  // A branch or operator can change the required operand type. Do not infer through it.
  if (expressionTokens.length) return { kind: 'expression' };
  return {kind:'expression', ...(expected ? {expected} : {})};
}

/** Blank completion is deliberately split into writable statements and readable expressions. */
export function genomeCompletions(source: string, position: number, prefix = '', bindings: readonly GenomeBinding[] = [], functions: readonly GenomeBinding[] = []): GenomeSymbol[] {
  const context = genomeCompletionContext(source, position, bindings, functions);
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
  if (context.kind === 'continuation' && context.keyword) {
    add({...lookupGenomeSymbol(context.keyword)!, insertText:context.keyword + ' '});
    return [...available.values()];
  }
  if (cellular && ['constructor', 'product', 'assembly-kind', 'component', 'mechanism-target', 'scaffold-precursor'].includes(context.kind)) {
    if (context.kind === 'constructor') CELLULAR_CONSTRUCTORS.forEach(add);
    if (context.kind === 'mechanism-target') MECHANISM_CATALOG.filter(item=>item.constructor===context.mechanismConstructor && item.target).forEach(item=>add(symbol(item.target!, 'keyword', 'target', item.kind, item.description)));
    if (context.kind === 'assembly-kind') CELLULAR_MECHANISMS.forEach(item => add({ ...item, insertText: `${item.name} {\n  ${defaultMechanismControl(item.name)}\n}` }));
    if (context.kind === 'product' || context.kind === 'component' || context.kind === 'scaffold-precursor') {
      cellularDeclarations(source).products.forEach(item => {
        if (context.fieldsPresent?.has(item.name)) return;
        if (context.kind === 'scaffold-precursor' && item.role !== 'scaffold') return;
        const product = productSymbol(item);
        add(context.kind === 'component' ? { ...product, insertText: `${item.name}: ` } : product);
      });
      if (context.kind === 'component') CELLULAR_KEYWORDS.filter(item => !context.fieldsPresent?.has(item.name) && mechanismHasControl(context.recipeKind,item.name) &&
        (['activity', 'start_when', 'recycle', 'at', 'direction'].includes(item.name) || context.contactRecipe && item.name === 'engage_when')).forEach(add);
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
      CELLULAR_KEYWORDS.filter(item => ['product', 'gene', 'assembly', 'fn', 'publish', 'signal'].includes(item.name)).forEach(add);
    } else BUILTIN_OUTPUTS.forEach(item => add({ ...item, insertText: `${item.name} = ` }));
    registers.forEach(item => add({ ...item, insertText: `${item.name} = ` }));
    for (const name of ['let', 'clear']) add({ ...lookupGenomeSymbol(name)!, insertText: `${name} ` });
    return [...available.values()];
  }
  (cellular
    ? [...(context.contactGuard ? CONTACT_FIELDS : [...readSignals(source), ...cellularDynamicSymbols(source), ...(context.definition ? CONTACT_FIELDS : [])]), ...BUILTIN_FUNCTIONS, ...registers]
    : [...BUILTIN_SIGNALS, ...BUILTIN_FUNCTIONS, ...registers]).forEach(add);
  const declarations = cellular ? cellularDeclarations(source) : undefined;
  const localByStart = declarations && new Map(declarations.locals.map(item => [item.from, item]));
  for (const binding of bindings) {
    const local = localByStart?.get(binding.from);
    if (local && local.end > position) continue;
    const scope = binding.type === 'not checked' && local ? local : binding;
    if (context.contactGuard ? scope.contactSafe === false : scope.perContact && !context.definition) continue;
    add(lookupGenomeSymbol(binding.name,[binding])!);
  }
  if (declarations) {
    const declaredFunctions = declarations.functions;
    for (const fn of declaredFunctions) {
      if (fn.end > position) continue;
      const checked = functions.find(item => item.from === fn.from);
      const scope = checked ?? fn;
      if (context.contactGuard ? scope.contactSafe === false : scope.perContact && !context.definition) continue;
      add(declaredFunctionSymbol(fn,checked));
    }
    const current = declaredFunctions.find(item => item.bodyFrom <= position && position < item.end);
    current?.parameters.forEach(name => add(parameterSymbol(name,current.name)));
  }
  for (const name of ['if', 'true', 'false']) add(lookupGenomeSymbol(name)!);
  const expected = context.expected;
  const accepts = (item: GenomeSymbol): boolean => {
    if (!expected || item.type === 'T' || item.type === 'not checked') return true;
    const types = item.type.split(' | ');
    if (expected === 'number' || expected === 'scalar') return types.includes('integer') || types.includes('count') || types.includes('scalar');
    if (expected === 'integer' && types.includes('count')) return true;
    // Any primitive may begin a comparison. Prefer booleans without hiding useful comparison operands.
    if (expected === 'boolean') return true;
    if (expected === 'number | vec2') return types.some(type => ['integer','count','scalar','vec2'].includes(type));
    if (expected === 'vec2 | string') return types.includes('vec2') || types.includes('string');
    return types.includes(expected);
  };
  return [...available.values()].filter(accepts).sort((a, b) => {
    if (expected === 'boolean') return Number(b.type === 'boolean') - Number(a.type === 'boolean');
    return 0;
  });
}

// Compare one identifier component at a time. Keep qualifiers and other
// components intact, and accept just one edit or adjacent transposition.
function plausibleTypo(typed: string, candidate: string): boolean {
  const oldParts = typed.split('.'), newParts = candidate.split('.');
  if (oldParts.length !== newParts.length) return false;
  const changed = oldParts.map((part,i) => part !== newParts[i] ? i : -1).filter(i => i >= 0);
  if (changed.length !== 1) return false;
  let a = oldParts[changed[0]].toLowerCase(), b = newParts[changed[0]].toLowerCase();
  if (a === b) return true;
  if (Math.max(a.length,b.length) < 4 || Math.abs(a.length-b.length) > 1) return false;
  if (a.length > b.length) [a,b] = [b,a];
  let i = 0;
  while (i < a.length && a[i] === b[i]) i++;
  if (a.length < b.length) return a.slice(i) === b.slice(i+1);
  return a.slice(i+1) === b.slice(i+1)
    || a[i] === b[i+1] && a[i+1] === b[i] && a.slice(i+2) === b.slice(i+2);
}

export function genomeTypoFix(source: string, diagnostic: {from:number; to:number; message:string; severity:string},
  bindings: readonly GenomeBinding[] = [], functions: readonly GenomeBinding[] = []): string | null {
  if (diagnostic.severity !== 'error') return null;
  const typed = source.slice(diagnostic.from,diagnostic.to);
  if (!/^[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*$/.test(typed)) return null;
  const unknown = /^unknown (function|signal|output|receptor assembly|receptor measurement|contact-interface field|installed assembly|product stock|product concentration|recipe product|product|adhesin precursor) '([^']+)'/.exec(diagnostic.message);
  const field = /^assembly '([^']+)' does not measure '([^']+)'/.exec(diagnostic.message);
  if (field) {
    if (typed !== `sensed.${field[1]}.${field[2]}`) return null;
  } else {
    if (!unknown) return null;
    const namedToken = unknown[1] === 'receptor assembly' ? typed.split('.')[1] : typed;
    if (namedToken !== unknown[2]) return null;
  }
  const kind = unknown?.[1];
  let names: string[];
  if (kind === 'product' || kind === 'recipe product' || kind === 'adhesin precursor') {
    names = cellularDeclarations(source).products.filter(p => kind !== 'adhesin precursor' || p.role === 'scaffold').map(p => p.name);
  } else if (kind === 'output') {
    if (isCellularSource(source)) return null;
    names = BUILTIN_OUTPUTS.map(item => item.name);
  } else {
    const candidates = genomeCompletions(source,diagnostic.from,'',bindings,functions);
    names = candidates.filter(item => kind === 'function' ? item.category === 'function'
      : item.category === 'signal' || item.category === 'binding').map(item => item.name);
  }
  const matches = [...new Set(names.filter(name => plausibleTypo(typed,name)))];
  return matches.length === 1 ? matches[0] : null;
}

const outputExamples: Readonly<Record<string, string>> = Object.freeze({
  thrust: 'thrust = if energy.fraction > 0.3 then 0.3 else 0;', turn: 'turn = if object.valid then clamp(angle(object.offset), -1, 1) else 0;',
  divide: 'divide = body.division_ready;', grow: 'grow = if energy.fraction > 0.6 && body.maturity < 1 then 0.7 else 0;',
  photosynthesize: 'photosynthesize = 1;', engulf: 'engulf = contact.valid && gut.fraction < 0.5;',
  bite: 'bite = if contact.valid && gut.fraction < 0.5 then 0.8 else 0;', defend: 'defend = if contact.valid then 0.5 else 0;',
  repair: 'repair = if integrity < 0.9 && energy.fraction > 0.5 then 0.5 else 0;', absorb: 'absorb = if nutrient.fraction < 0.8 then 1 else 0;',
  feed: 'feed = 1;', 'digest.exudate': 'digest.exudate = if gut.soluble.energy > 0 then 1 else 0;',
  'digest.tissue': 'digest.tissue = if gut.structural.energy > 0 then 0.8 else 0;',
  'digest.waste': 'digest.waste = if gut.soluble.energy + gut.structural.energy < 1 then 1 else 0.1;',
  'scan.bearing': 'scan.bearing = sin(age * 0.2) * 1.5;', 'scan.width': 'scan.width = 3.141593;',
  'scan.target': 'scan.target = if populated(rs001) then rs001 else "";',
});

export interface GenomeSymbolDocumentation { example: string; notes: readonly string[] }
export function genomeSymbolDocumentation(item: GenomeSymbol, counted = false): GenomeSymbolDocumentation {
  const mechanism = MECHANISM_CATALOG.find(entry=>entry.kind===item.name);
  if (mechanism) {
    const parts=mechanism.roles.map(role=>({name:role[0].toUpperCase()+role.slice(1)+'Part',role}));
    return {example:`${parts.map(part=>part.role === 'adhesin' ? `product Precursor = scaffold();\nproduct ${part.name} = adhesin(Precursor);` : `product ${part.name} = ${part.role}();`).join('\n')}\nassembly Machine: ${mechanism.kind} {\n${parts.map(part=>`  ${part.name}: ${counted ? '400' : '0.04'};`).join('\n')}\n  ${defaultMechanismControl(mechanism.kind)}\n}`,
      notes:[mechanism.description,'Build separate mechanisms for the other operations the organism needs. Each recipe retains its physical ingredients and identity.']};
  }
  if (item.category === 'keyword' && ['publish','signal','engage_when'].includes(item.name)
    || CONTACT_FIELDS.some(field => field.name === item.name)) {
    const mass = counted ? '200' : '0.02';
    const stock = (...roles: string[]): string => roles.map(role => {
      const name = role[0].toUpperCase() + role.slice(1);
      const precursor = role === 'adhesin' ? 'Scaffold' : '';
      return `product ${name} = ${role}(${precursor}); gene make_${role}: ${name} = 0.4;`;
    }).join('\n');
    const display = `assembly Badge: display() { Anchor: ${mass}; Membrane: ${mass}; }`;
    if (item.name === 'publish') {
      return {example: `${stock('anchor','membrane')}\n${display}\npublish rf001;\nrf001 = 17;`,
        notes:['A working touch receptor on the other cell reads the marker when it faces the active Badge. Check sensed.Touch.message_valid before using sensed.Touch.message. A displayed zero is valid. Clearing rf001 withdraws the marker.']};
    }
    if (item.name === 'signal') {
      return {example: `${stock('anchor','membrane','receptor')}\n${display}\nassembly Port: junction(signal) { Membrane: ${mass}; Receptor: ${mass}; }\nsignal rf002;\nrf002 = if energy.fraction < 0.4 then 1 else 0;\nrf003 = sensed.Port.value when sensed.Port.signal_valid;`,
        notes:['A facing touch receptor can read the payload from Badge before bonding. Across an existing bond, overlapping active signal ports exchange the payload directly. The read stores the last valid port payload in rf003, including zero. If you also display a recognition marker, select a different register for it.']};
    }
    if (item.name === 'peer.division_interface') {
      return {example: `${stock('anchor','membrane','scaffold','adhesin')}\nassembly Seam: adhere() {\n  Anchor: ${mass}; Membrane: ${mass}; Adhesin: ${mass};\n  engage_when: peer.division_interface;\n}`,
        notes:['The engine supplies the division-interface flag at the physical contact. A daughter bond also needs free sites on overlapping active adhesive surfaces, with both guards open.']};
    }
    const touch = `assembly Touch: sense(contact) { Anchor: ${mass}; Filament: ${mass}; Receptor: ${mass}; }`;
    if (item.name.startsWith('peer.resistance')) {
      return {example: `${stock('anchor','filament','receptor','motor','membrane')}\n${touch}\nassembly Gut: store(food) { Membrane: ${counted ? '400' : '0.04'}; }\nassembly Mouth: import(particle) {\n  Anchor: ${mass}; Motor: ${mass};\n  engage_when: peer.resistance_valid && peer.resistance < 0.08;\n}`,
        notes:['Touch and Mouth share the default mount. The inlet accepts a particle only when the active receptor measures that particle within reach of the opening and its resistance is below the threshold.']};
    }
    const signal = item.name.startsWith('peer.signal');
    const condition = signal ? 'peer.signal_valid && peer.signal > 0.5'
      : 'peer.resistance_valid && (!peer.message_valid || peer.message != rf001)';
    return {example: `${stock('anchor','filament','receptor','enzyme')}\n${touch}\n${signal ? '' : 'rf001 = 17;\n'}assembly Needle: pierce() {\n  Anchor: ${mass}; Filament: ${mass}; Enzyme: ${mass};\n  engage_when: ${condition};\n}`,
      notes:[signal
        ? 'Touch reads a payload from a facing active display at the same contact as Needle. This guard permits piercing only when the measured payload exceeds 0.5.'
        : 'The cell remembers its comparison marker in rf001. Touch reads the candidate’s facing display at Needle’s contact. A matching displayed marker protects the candidate. A physically touched cell without a readable marker remains eligible prey.']};
  }
  if (CELLULAR_KEYWORDS.some(keyword=>keyword.name===item.name) || item.category==='function' && item.type==='product') {
    const marker = counted ? 'cellular counts;' : 'cellular;';
    const stock = counted ? '100' : '0.01';
    const snippets:Record<string,string> = {
      cellular:`${marker}\nproduct Memory = regulator(8);\ngene remember: Memory = if energy > ${counted ? '500000' : '50'} then 0.1 else 0;`,
      product:'product Rotor = motor();\nproduct Memory = regulator(8);',
      adhesin:`product Precursor = scaffold();\nproduct Glue = adhesin(Precursor);\ngene make_precursor: Precursor = if pool.Precursor < ${stock} then 1 else 0;\ngene finish: Glue = if pool.Glue < ${stock} then 1 else 0;`,
      gene:`product Rotor = motor();\ngene drive: Rotor = if pool.Rotor < ${stock} then 0.5 else 0;`,
      assembly:`product Anchor = anchor();\nproduct Rotor = motor();\nproduct Tail = filament();\nassembly Swim: propel() {\n  Anchor: ${counted ? '50' : '0.005'};\n  Rotor: ${stock};\n  Tail: ${counted ? '250' : '0.025'};\n  activity: 0.6;\n  at: 1.570796;\n  direction: 0;\n}`,
      fn:'fn response(value, threshold) = activate(value, threshold);\nlet response_now = response(energy.fraction, 0.4);',
      start_when:'start_when: concentration.GrowthSignal > 0.01;',
      recycle:'recycle: if concentration.Brake > 0.02 then 0.5 else 0;',
      activity:'activity: if energy.fraction > 0.3 then 0.6 else 0;',
      at:'at: 1.570796;\ndirection: 0;',
      direction:'at: -1.570796;\ndirection: 0;',
    };
    return { example: snippets[item.name] ?? `product ${item.name[0].toUpperCase()+item.name.slice(1)} = ${item.name}(${item.name==='regulator'?'8':''});`,
      notes: [`Use these declarations in a program beginning with ${marker} The compiler checks roles, recipe ingredients and expression types.`,
        ['activity','at','direction'].includes(item.name) ? item.description
          : 'Products are paid material. Synthesis and assembly share finite core work; only stock present at the start of a tick can be installed.'] };
  }
  if (item.category === 'register') {
    return { example: `${item.name} = ${REGISTER_FAMILIES.find(family => family.type === item.type)?.empty ?? '0'};\nclear ${item.name} when age > 10;`,
      notes: ['Addresses run from 001 to 999 in each of the five register banks.', 'Reads use memory from the start of the evaluation. Writes commit together after a real tick; clear wins when both rules fire.'] };
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
    ? [...cellularDynamicSymbols(source), ...readSignals(source), ...CONTACT_FIELDS, ...registers]
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
