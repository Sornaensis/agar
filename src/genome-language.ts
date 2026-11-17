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
  ['energy', 'scalar', 'Current energy reserve. Capacity is 200 × size.'],
  ['age', 'scalar', 'This organism’s age in seconds. Cells die at the age reported by lifespan.'],
  ['speed', 'scalar', 'Current velocity magnitude in world units per second.'],
  ['time', 'scalar', 'Elapsed simulation time in seconds.'],
  ['id', 'scalar', 'This organism’s stable numeric ID, exposed as a scalar.'],
  ['size', 'scalar', 'Current biomass relative to a founder. Founders start at 1; living cells range from 0.5 to 4.'],
  ['food.angle', 'scalar', 'Signed angle in radians to the selected food target, relative to heading. Accessible particles are ranked by raw energy / (distance + 20). Check food.visible first.'],
  ['food.distance', 'scalar', 'Distance to the selected food target’s center. Equals sense.range when no food is visible.'],
  ['food.visible', 'boolean', 'Whether a selectable food target is within sensing range. Particles buried inside another cell are excluded.'],
  ['neighbor.angle', 'scalar', 'Signed angle in radians to the nearest sensed other cell, relative to heading. Check neighbor.visible first.'],
  ['neighbor.distance', 'scalar', 'Distance to the nearest sensed other cell’s center. Equals sense.range when no neighbor is visible.'],
  ['neighbor.visible', 'boolean', 'Whether another living cell is within sensing range.'],
  ['energy.fraction', 'scalar', 'Energy divided by capacity, clamped to 0–1.'],
  ['memory.used', 'scalar', 'Number of populated registers at the start of this tick, exposed as a scalar.'],
  ['memory.cost', 'scalar', 'Register maintenance in energy per second: 0.0025 × memory.used.'],
  ['light.level', 'scalar', 'Local light level, from 0 to 1.'],
  ['light.angle', 'scalar', 'Direction of brighter light relative to heading, in radians. Zero when the local gradient is flat.'],
  ['temperature.value', 'scalar', 'Local temperature from 0 to 1. A value around 0.5 is mild.'],
  ['temperature.angle', 'scalar', 'Direction of warmer water relative to heading, in radians. Zero when the local gradient is flat.'],
  ['flow.x', 'scalar', 'Local water-current velocity along the world x axis.'],
  ['flow.y', 'scalar', 'Local water-current velocity along the world y axis.'],
  ['flow.speed', 'scalar', 'Magnitude of the local water-current velocity.'],
  ['flow.angle', 'scalar', 'Direction of the local current relative to heading, in radians. Zero for a stationary current.'],
  ['prey.id', 'scalar', 'Stable ID of a sensed potential prey cell of any size. Prefers a different genome, then the nearest target within that group. Zero when none is visible.'],
  ['prey.angle', 'scalar', 'Signed angle in radians to the sensed prey, relative to heading. Check prey.visible first.'],
  ['prey.distance', 'scalar', 'Distance to the sensed prey’s center. Equals sense.range when none is visible.'],
  ['prey.size', 'scalar', 'Biomass of the sensed prey. Zero when none is visible.'],
  ['prey.visible', 'boolean', 'Whether a potential prey cell of any size is within sensing range. Sensing a target does not guarantee it can be engulfed.'],
  ['prey.same_genome', 'boolean', 'Whether the sensed prey has the same genome as this cell. False when no prey is visible.'],
  ['threat.angle', 'scalar', 'Signed angle in radians to the nearest sensed cell with attack tissue, including smaller hunters. Check threat.visible first.'],
  ['threat.distance', 'scalar', 'Distance to the sensed threat’s center. Equals sense.range when none is visible.'],
  ['threat.size', 'scalar', 'Biomass of the sensed threat. Zero when none is visible.'],
  ['threat.visible', 'boolean', 'Whether a cell with positive attack allocation is within sensing range. Smaller hunters can also bite. This describes capability and proximity, not intent.'],
  ['integrity', 'scalar', 'Structural health from 0 to 1. The organism dies at zero integrity.'],
  ['nutrient.stored', 'scalar', 'Free mineral reserves. Minerals bound into biomass are counted separately.'],
  ['nutrient.capacity', 'scalar', 'Free mineral capacity: 2 × size.'],
  ['nutrient.fraction', 'scalar', 'Stored minerals divided by free reserve capacity, from 0 to 1.'],
  ['nutrient.level', 'scalar', 'Local dissolved mineral mass per 20 × 20 world-unit water patch.'],
  ['nutrient.angle', 'scalar', 'Direction of increasing dissolved minerals relative to heading, in radians. Zero for a flat gradient.'],
  ['body.propulsion', 'scalar', 'Current biomass fraction invested in propulsion, from 0 to 1. Controls movement and turning capacity.'],
  ['body.photosynthesis', 'scalar', 'Current biomass fraction invested in photosynthesis, from 0 to 1. Controls light-harvesting capacity.'],
  ['body.digestion', 'scalar', 'Current biomass fraction invested in digestion, from 0 to 1. Controls gut capacity, ingestion, and digestion. Dissolved mineral uptake and attacks use separate mechanisms.'],
  ['body.armor', 'scalar', 'Current biomass fraction invested in armor, from 0 to 1. Reduces attack and temperature damage.'],
  ['body.sensing', 'scalar', 'Current biomass fraction invested in sensing, from 0 to 1. Extends target sensing range.'],
  ['body.unassigned', 'scalar', 'Biomass fraction not currently assigned to a capability, from 0 to 1.'],
  ['sense.range', 'scalar', 'Current target-sensing radius in world units: 40 + 400 × body.sensing.'],
  ['body.attack', 'scalar', 'Current biomass fraction invested in attack, from 0 to 1. Controls biting and the ability to engulf weakened prey.'],
  ['gut.energy', 'scalar', 'Raw organic energy waiting in the gut. Digestion gradually converts it to usable energy at 65% efficiency.'],
  ['gut.capacity', 'scalar', 'Current raw-energy gut capacity: 100 × size × (body.digestion / 0.2).'],
  ['gut.fraction', 'scalar', 'Raw gut energy divided by gut capacity, from 0 to 1. Zero when there is no digestive capacity.'],
  ['digestion.rate', 'scalar', 'Usable energy gained from digestion per second during the last completed tick, after 65% assimilation efficiency.'],
  ['growth.rate', 'scalar', 'Biomass added per second during the last completed tick. Growth requires energy and stored minerals.'],
  ['birth.size', 'scalar', 'Biomass at this organism’s most recent birth or division. It must double this size before dividing again.'],
  ['maturity', 'scalar', 'Growth toward division, clamped to 0–1: size / birth.size − 1. Full maturity means birth biomass has doubled; energy and cooldown requirements still apply.'],
  ['lifespan', 'scalar', 'Maximum organism age in seconds. The current model uses a 300-second lifespan.'],
  ['exudation.rate', 'scalar', 'Raw organic energy actually released as food per second during the last completed tick. Photosynthesis releases at least 12% of gross production, plus energy-reserve overflow, outside the membrane when particle space is available.'],
  ['prey.integrity', 'scalar', 'Structural health of the sensed prey, from 0 to 1. Zero when no prey is visible.'],
  ['prey.attack', 'scalar', 'Actual attack allocation of the sensed prey, from 0 to 1. Zero when no prey is visible.'],
  ['prey.photosynthesis', 'scalar', 'Actual photosynthetic allocation of the sensed prey, from 0 to 1. Zero when no prey is visible.'],
  ['food.energy', 'scalar', 'Raw organic energy in the selected food particle, before digestion and assimilation loss. Zero when no food target is visible.'],
  ['food.nutrients', 'scalar', 'Mineral content of the selected food particle. Exudates can contain energy without minerals. Zero when no food target is visible.'],
];
export const BUILTIN_SIGNALS: readonly GenomeSymbol[] = Object.freeze(sensorRows.map(([name, type, description]) =>
  symbol(name, 'signal', type, `${name}: ${type}`, `${description} Sampled before any organism moves this tick.`)));

const output = (name: string, type: 'scalar' | 'boolean', description: string): GenomeSymbol =>
  symbol(name, 'output', type, `${name} = ${type};`, description);
const investment = 'Target body fraction, clamped to 0–1; defaults to 0.2. Targets above a total of 1 are normalized. Remodeling takes time and energy; read body.* for the actual allocation.';
export const BUILTIN_OUTPUTS: readonly GenomeSymbol[] = Object.freeze([
  output('thrust', 'scalar', 'Required. Forward movement effort, clamped to 0–1. Acceleration and its energy cost depend on propulsion investment.'),
  output('turn', 'scalar', 'Required. Signed turning effort, clamped to -1–1. Turning speed depends on propulsion investment.'),
  output('divide', 'boolean', 'Required. Requests division after doubling birth.size, meeting energy requirements, and a 12-second cooldown. Both cells receive half the biomass, gut contents, and remaining reserves; the daughter starts with empty registers.'),
  output('grow', 'scalar', 'Growth effort, clamped to 0–1; defaults to 0. Building biomass costs 80 energy and one stored mineral per biomass, preserves a 20% energy reserve, and stops at size 4.'),
  output('photosynthesize', 'scalar', 'Light-harvesting effort, clamped to 0–1; defaults to 0. Collects shared light over a footprint with radius 60 × sqrt(size); overlapping organisms compete for a finite areal supply. At least 12% of production, plus reserve overflow, is released as organic food when particle space is available.'),
  output('engulf', 'boolean', 'Requests engulfing weakened sensed prey on contact; defaults to false. Requires a size advantage, attack and gut capacity, energy, and an available engulfing cooldown. The meal enters the gut for later digestion.'),
  output('bite', 'scalar', 'Biting effort, clamped to 0–1; defaults to 0. Damages touching prey using attack tissue. With feeding and gut space, damage after defense can transfer some prey reserves into the gut for later digestion; it never immediately replenishes usable energy.'),
  output('defend', 'scalar', 'Active defense effort, clamped to 0–1; defaults to 0. Reduces bite damage at an energy cost, alongside passive armor.'),
  output('repair', 'scalar', 'Repair effort, clamped to 0–1; defaults to 0. Spends energy to restore integrity.'),
  output('absorb', 'scalar', 'Dissolved mineral uptake effort, clamped to 0–1; defaults to 1. Limited by local supply, energy, size, and free reserve space; independent of digestion allocation.'),
  ...['propulsion', 'photosynthesis', 'digestion', 'armor', 'sensing'].map(name => output(`invest.${name}`, 'scalar', investment)),
  output('feed', 'scalar', 'Feeding and digestion effort, clamped to 0–1; defaults to 1. Particles and extracted prey reserves enter the gut for later assimilation at 65% efficiency. Zero stops intake, bite extraction, engulfing, and digestion, but does not prevent bite damage.'),
  output('invest.attack', 'scalar', 'Target attack fraction, clamped to 0–1; defaults to 0. Shares the body budget with the five other capabilities and changes gradually through paid remodeling.'),
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
