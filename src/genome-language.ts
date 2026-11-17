import { ORGANELLE_CATALOG } from './organelles';

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
  ['body.mass', 'scalar', 'Actual mineral material: frame, organelles, free reserves and gut minerals, each counted once. Energy does not add material mass.'],
  ['body.birth_area', 'scalar', 'Envelope area at this organism’s most recent birth or division.'],
  ['body.maturity', 'scalar', 'Envelope growth toward division, clamped to 0–1. Area must double from body.birth_area. Other physical requirements still apply.'],
  ['body.division_ready', 'boolean', 'Whether the current body can divide: sufficient area, energy, cooldown, viable cores and a conservative organelle partition.'],
  ['body.space.free', 'scalar', 'Unused internal space. Organelles and storage compartments occupy room even while damaged or incomplete.'],
  ['body.space.used', 'scalar', 'Internal space occupied by organs and their compartments. Contents are not counted a second time.'],
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
  ['digestion.rate', 'scalar', 'Usable energy gained from digestion per second during the last completed tick, after assimilation losses.'],
  ['growth.rate', 'scalar', 'Envelope area added per second during the last completed tick.'],
  ['lifespan', 'scalar', 'Maximum organism age in seconds.'],
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
  output('feed', 'scalar', 'Organic intake and digestion effort, clamped to 0–1; defaults to 1. Capture and digestion require the corresponding built machinery; ingested energy is processed with assimilation losses.'),
  ...ORGANELLE_CATALOG.flatMap(({kind, name, minimumMass}) => [
    output(`target.${kind}.count`, 'integer', `Requested number of ${name.toLowerCase()} instances; nonnegative integer, default 0. This does not create or remove organs by itself. Use build.${kind} or recycle.${kind}.`),
    output(`target.${kind}.size`, 'scalar', `Requested material per ${name.toLowerCase()} instance; default and minimum ${minimumMass}. Actual instances can differ through partial construction or damage.`),
    output(`build.${kind}`, 'scalar', `Construction effort toward the ${name.toLowerCase()} count and size targets, clamped to 0–1; default 0. Shares finite assembly, energy, minerals, internal room and membrane space with all other requests. Capabilities update on the next tick.`),
    output(`recycle.${kind}`, 'scalar', `Explicit dismantling effort for ${name.toLowerCase()} material above its targets, clamped to 0–1; default 0. Costs time and energy, recovers only accounted material and retained energy, and spills excess contents safely. Essential core remains protected.`),
  ]),
  output('scan.bearing', 'scalar', 'Body-relative scan center in radians, wrapped to −π…π; defaults to 0. Takes effect on the following tick.'),
  output('scan.width', 'scalar', 'Requested scan width in radians, clamped to 0…2π; defaults to a full circle. Built detector coverage, visibility and occlusion still limit observations. Takes effect on the following tick.'),
  output('scan.target', 'string', 'Optional opaque token to reacquire within the next physical scan; defaults to an empty string. It grants no access outside the detector’s observable region.'),
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

export const GENOME_SYMBOLS: readonly GenomeSymbol[] = Object.freeze([
  ...BUILTIN_FUNCTIONS, ...BUILTIN_SIGNALS, ...BUILTIN_OUTPUTS, ...LANGUAGE_KEYWORDS,
]);
const symbolByName = new Map(GENOME_SYMBOLS.map(item => [item.name, item]));

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
  return symbolByName.get(name) ?? registerSymbol(name);
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
      const name = source.slice(start, pos), known = symbolByName.get(name), register = registerSymbol(name);
      if (name === 'true' || name === 'false') { kind = 'boolean'; type = 'boolean'; }
      else if (register) { kind = 'register'; type = register.type as PrimitiveType; }
      else if (known) {
        kind = known.category as 'keyword' | 'signal' | 'function' | 'output';
        if (known.category === 'signal' || known.category === 'output') type = known.type as PrimitiveType;
      } else kind = 'identifier';
    } else if (['==', '!=', '<=', '>=', '&&', '||'].includes(source.slice(pos, pos + 2))) {
      pos += 2; kind = 'operator';
    } else if ('+-*/=<>!'.includes(character)) {
      pos++; kind = 'operator';
    } else if ('(),;'.includes(character)) {
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
  const found = lookupGenomeSymbol(span.text, bindings);
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
  kind: 'statement' | 'expression' | 'register' | 'none';
  /** A useful editing hint, never a substitute for the Rust type checker. */
  expected?: PrimitiveType | 'number' | 'register' | 'vec2 | string';
}

const numericFunctions = new Set(['sin', 'cos', 'abs', 'min', 'max', 'clamp', 'int', 'float', 'vec', 'sqrt', 'sign', 'atan2', 'lerp', 'noise']);
const vectorFunctions = new Set(['dot', 'normalize', 'angle', 'x', 'y']);

/** Incomplete code is expected here. Infer only the constraints visible at the cursor. */
export function genomeCompletionContext(source: string, position: number, bindings: readonly GenomeBinding[] = []): GenomeCompletionContext {
  if (!Number.isInteger(position) || position < 0 || position > source.length) return { kind: 'none' };
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
  const needle = prefix.toLowerCase();
  const registers = registerCompletions(prefix || 'r', source, prefix ? 40 : 15);
  if (!prefix) for (const family of REGISTER_FAMILIES) {
    const name = `${family.prefix}001`;
    if (!registers.some(item => item.name === name)) registers.push(registerSymbol(name)!);
  }
  const available = new Map<string, GenomeSymbol>();
  const add = (item: GenomeSymbol): void => { if (!needle || item.name.toLowerCase().startsWith(needle)) available.set(item.name, item); };
  if (context.kind === 'register') {
    registers.forEach(add);
    return [...available.values()];
  }
  if (context.kind === 'statement') {
    BUILTIN_OUTPUTS.forEach(item => add({ ...item, insertText: `${item.name} = ` }));
    registers.forEach(item => add({ ...item, insertText: `${item.name} = ` }));
    for (const name of ['let', 'clear']) add({ ...lookupGenomeSymbol(name)!, insertText: `${name} ` });
    return [...available.values()];
  }
  [...BUILTIN_SIGNALS, ...BUILTIN_FUNCTIONS, ...registers].forEach(add);
  bindings.forEach(binding => add(lookupGenomeSymbol(binding.name, [binding])!));
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
  feed: 'feed = 1;', 'scan.bearing': 'scan.bearing = sin(age * 0.2) * 1.5;', 'scan.width': 'scan.width = 3.141593;',
  'scan.target': 'scan.target = if populated(rs001) then rs001 else "";',
});

export interface GenomeSymbolDocumentation { example: string; notes: readonly string[] }
export function genomeSymbolDocumentation(item: GenomeSymbol): GenomeSymbolDocumentation {
  if (item.category === 'register') {
    return { example: `${item.name} = ${REGISTER_FAMILIES.find(family => family.type === item.type)?.empty ?? '0'};\nclear ${item.name} when age > 10;`,
      notes: ['Addresses run from 001 to 999 for each of the five primitive types.', 'Reads use memory from the start of the evaluation. Writes commit together after a real tick; clear wins when both rules fire.'] };
  }
  if (item.category === 'output') {
    const family = ORGANELLE_CATALOG.find(entry => item.name.includes(`.${entry.kind}`));
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
      sensed ? `Check ${group}.available and ${group}.valid before using a measurement. Missing/invalid numbers and vectors are zero; tokens are empty.`
        : 'Core self-signals remain available without external sensing hardware. Local bindings can combine them with pure expressions.'] };
  }
  return { example: item.example ?? item.signature, notes: [] };
}

/** Reference menus include one address per family; typing a prefix explores all 999. */
export function genomeSignalCatalog(direction: 'read' | 'write'): readonly GenomeSymbol[] {
  return [...(direction === 'read' ? BUILTIN_SIGNALS : BUILTIN_OUTPUTS), ...REGISTER_FAMILIES.map(family => registerSymbol(`${family.prefix}001`)!)];
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
