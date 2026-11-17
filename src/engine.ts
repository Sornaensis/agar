import engineUrl from '../engine/pkg/agar_engine.wasm?url';
import engineContract from './engine-contract.json';

/** Versioned boundary to the Rust engine. All simulation runs in WASM. */
export interface Cell {
  id: number; genomeId: number; x: number; y: number; vx: number; vy: number;
  angle: number; energy: number; age: number; generation: number; parentId: number;
  radius: number; foodAngle: number; foodDistance: number; foodVisible: boolean;
  neighborAngle: number; neighborDistance: number; neighborVisible: boolean;
  size: number; energyCapacity: number; basalMetabolism: number;
  memoryUsed: number; memoryCost: number; lightLevel: number; lightAngle: number;
  temperature: number; temperatureAngle: number; flowX: number; flowY: number;
  preyId: number; preyAngle: number; preyDistance: number; preySize: number; preyVisible: boolean;
  threatAngle: number; threatDistance: number; threatSize: number; threatVisible: boolean;
  integrity: number; photosynthesisRate: number;
  nutrientStored: number; nutrientCapacity: number; nutrientLevel: number; nutrientAngle: number; nutrientUptakeRate: number;
  bodyPropulsion: number; bodyPhotosynthesis: number; bodyDigestion: number; bodyArmor: number; bodySensing: number;
  senseRange: number; remodelingRate: number; bodyUpkeep: number;
  bodyAttack: number; gutEnergy: number; gutCapacity: number; gutNutrients: number;
  digestionRate: number; growthRate: number; birthSize: number; maturity: number; lifespan: number; exudationRate: number;
}
export interface Food { x: number; y: number; energy: number; nutrients: number }
export type SourceKind = 0 | 1 | 2 | 3;
export interface EnvironmentSource { id: number; kind: SourceKind; x: number; y: number; strength: number; radius: number }
export interface EnvironmentSnapshot {
  columns: number; rows: number; ambientTemperature: number; ambientLight: number; currentStrength: number; initialNutrients: number;
  /** Interleaved light, temperature, flow x, flow y, dissolved nutrients; an owned copy. */
  samples: Float32Array; sources: EnvironmentSource[];
}
export interface RegisterEntry { name: string; type: string; value: unknown }
export type GenomeValueType = 'integer' | 'scalar' | 'boolean' | 'string' | 'vec2';
export interface GenomeDiagnostic {
  severity: 'error'; message: string;
  /** One-based line and UTF-16 column. */
  line: number; column: number;
  /** UTF-16 offsets; EOF diagnostics may have an empty range. */
  from: number; to: number;
}
export interface GenomeBinding {
  name: string; type: GenomeValueType;
  /** UTF-16 range of the declaration's identifier. */
  from: number; to: number;
}
export interface GenomeAnalysis {
  valid: boolean; diagnostics: GenomeDiagnostic[]; bindings: GenomeBinding[];
  nodeCount: number; sourceBytes: number;
}
export type GenomeValue = number | boolean | string | [number, number];
export interface GenomeExpressionEvaluation {
  id: number; from: number; to: number; type: GenomeValueType; value: GenomeValue;
}
export interface GenomeBindingEvaluation {
  name: string; from: number; to: number; type: GenomeValueType; value: GenomeValue;
}
export interface GenomeSignalEvaluation { name: string; type: GenomeValueType; value: GenomeValue }
export interface GenomeOutputEvaluation {
  name: string; type: GenomeValueType; value: GenomeValue;
  /** The expression range, or null for an omitted output's default. */
  from: number | null; to: number | null;
}
export interface GenomeWriteEvaluation {
  name: string; kind: 'write' | 'clear'; type: GenomeValueType; value: GenomeValue | null;
  guard: boolean; applies: boolean; from: number; to: number;
}
/** A side-effect-free evaluation against this cell's current signals and memory.
 * It is a snapshot preview, not a record of its last tick or a committed action. */
export interface GenomeEvaluation {
  version: 1; semantics: 'current-snapshot'; cellId: number; genomeId: number;
  time: number; tick: number; source: string;
  expressions: GenomeExpressionEvaluation[]; bindings: GenomeBindingEvaluation[];
  signals: GenomeSignalEvaluation[]; outputs: GenomeOutputEvaluation[]; writes: GenomeWriteEvaluation[];
}

/** Keep source ranges and runtime values trustworthy before rendering inspection UI. */
export function validateGenomeEvaluation(value: unknown, expectedCellId?: number): GenomeEvaluation {
  const fail = (): never => { throw new Error('The engine returned an invalid program evaluation. Rebuild the current WASM engine.'); };
  if (!value || typeof value !== 'object') return fail();
  const trace = value as GenomeEvaluation;
  const types = new Set<GenomeValueType>(['integer', 'scalar', 'boolean', 'string', 'vec2']);
  const positiveId = (id: number): boolean => Number.isInteger(id) && id >= 1 && id <= 0xffff_ffff;
  if (trace.version !== 1 || trace.semantics !== 'current-snapshot'
    || !positiveId(trace.cellId) || !positiveId(trace.genomeId)
    || (expectedCellId !== undefined && trace.cellId !== expectedCellId)
    || !Number.isSafeInteger(trace.tick) || trace.tick < 0 || !Number.isFinite(trace.time) || trace.time < 0
    || typeof trace.source !== 'string' || trace.source.length > MAX_PROGRAM_BYTES
    || new TextEncoder().encode(trace.source).length > MAX_PROGRAM_BYTES
    || !Array.isArray(trace.expressions) || !trace.expressions.length || trace.expressions.length > 8192
    || !Array.isArray(trace.bindings) || trace.bindings.length > 8192
    || !Array.isArray(trace.signals) || trace.signals.length !== 61
    || !Array.isArray(trace.outputs) || trace.outputs.length !== 17
    || !Array.isArray(trace.writes) || trace.writes.length > 9990) return fail();
  const range = (from: number, to: number): boolean => Number.isInteger(from) && Number.isInteger(to)
    && from >= 0 && to > from && to <= trace.source.length;
  const scalar = (item: unknown): item is number => typeof item === 'number' && Number.isFinite(item) && Math.abs(item) <= 1e12;
  const typed = (type: GenomeValueType, item: unknown): boolean => {
    if (!types.has(type)) return false;
    switch (type) {
      case 'integer': return typeof item === 'number' && Number.isInteger(item) && item >= -2147483648 && item <= 2147483647;
      case 'scalar': return scalar(item);
      case 'boolean': return typeof item === 'boolean';
      case 'string': return typeof item === 'string' && new TextEncoder().encode(item).length <= 128;
      case 'vec2': return Array.isArray(item) && item.length === 2 && item.every(scalar);
    }
  };
  const named = (item: GenomeSignalEvaluation): boolean => Boolean(item) && typeof item.name === 'string' && item.name.length > 0 && typed(item.type, item.value);
  for (let id = 0; id < trace.expressions.length; id++) {
    const expression = trace.expressions[id];
    if (!expression || expression.id !== id || !range(expression.from, expression.to) || !typed(expression.type, expression.value)) return fail();
  }
  for (const binding of trace.bindings) {
    if (!named(binding) || !range(binding.from, binding.to) || trace.source.slice(binding.from, binding.to) !== binding.name) return fail();
  }
  if (!trace.signals.every(named) || new Set(trace.signals.map(signal => signal.name)).size !== trace.signals.length) return fail();
  const outputNames = new Set<string>();
  for (const output of trace.outputs) {
    if (!named(output) || outputNames.has(output.name)
      || !((output.from === null && output.to === null)
        || (typeof output.from === 'number' && typeof output.to === 'number' && range(output.from, output.to)))) return fail();
    outputNames.add(output.name);
  }
  const ruleNames = new Set<string>();
  const registerTypes: Record<string, GenomeValueType> = { ri: 'integer', rf: 'scalar', rb: 'boolean', rs: 'string', rv: 'vec2' };
  for (const rule of trace.writes) {
    if (!rule || typeof rule.name !== 'string' || !/^r[ifbsv](?!000)\d{3}$/.test(rule.name)
      || registerTypes[rule.name.slice(0, 2)] !== rule.type || !['write', 'clear'].includes(rule.kind)
      || !range(rule.from, rule.to) || trace.source.slice(rule.from, rule.to) !== rule.name
      || typeof rule.guard !== 'boolean' || typeof rule.applies !== 'boolean' || (rule.applies && !rule.guard)
      || (rule.kind === 'clear' ? rule.value !== null || rule.applies !== rule.guard : !typed(rule.type, rule.value))
      || ruleNames.has(`${rule.kind}:${rule.name}`)) return fail();
    ruleNames.add(`${rule.kind}:${rule.name}`);
  }
  return trace;
}
export interface GenomeGraphNode {
  id: number;
  kind: 'constant' | 'sensor' | 'register' | 'populated' | 'reference' | 'unary' | 'binary' | 'conditional' | 'call';
  type: GenomeValueType;
  label: string;
  inputs: number[];
  bindings: string[];
  reachable: boolean;
}
export interface GenomeGraphRoot {
  id: string; kind: 'output' | 'write' | 'clear'; name: string; type: GenomeValueType;
  value: number | null; guard: number | null; default: string | null;
}
export interface GenomeGraph { version: 1; nodes: GenomeGraphNode[]; roots: GenomeGraphRoot[] }

/** Validate the owned JSON boundary before any graph traversal or DOM creation. */
export function validateGenomeGraph(value: unknown): GenomeGraph {
  const fail = (): never => { throw new Error('The engine returned an invalid genome graph. Rebuild the current WASM engine.'); };
  if (!value || typeof value !== 'object') return fail();
  const graph = value as GenomeGraph;
  if (graph.version !== 1 || !Array.isArray(graph.nodes) || graph.nodes.length > 8192
    || !Array.isArray(graph.roots) || graph.roots.length > 10007) return fail();
  const types = new Set(['integer', 'scalar', 'boolean', 'string', 'vec2']);
  const kinds = new Set(['constant', 'sensor', 'register', 'populated', 'reference', 'unary', 'binary', 'conditional', 'call']);
  for (let index = 0; index < graph.nodes.length; index++) {
    const node = graph.nodes[index];
    if (!node || node.id !== index || !kinds.has(node.kind) || !types.has(node.type)
      || typeof node.label !== 'string' || typeof node.reachable !== 'boolean'
      || !Array.isArray(node.bindings) || !node.bindings.every(name => typeof name === 'string')
      || !Array.isArray(node.inputs) || node.inputs.length > 3
      || !node.inputs.every(id => Number.isInteger(id) && id >= 0 && id < index)) return fail();
  }
  const ids = new Set<string>();
  const validNode = (id: number | null): boolean => id === null || (Number.isInteger(id) && id >= 0 && id < graph.nodes.length);
  for (const root of graph.roots) {
    if (!root || typeof root.id !== 'string' || ids.has(root.id) || typeof root.name !== 'string'
      || !['output', 'write', 'clear'].includes(root.kind) || !types.has(root.type)
      || !validNode(root.value) || !validNode(root.guard)
      || !(root.default === null || typeof root.default === 'string')) return fail();
    ids.add(root.id);
  }
  return graph;
}
export const MAX_PROGRAM_BYTES = 262_144;
export const MAX_STATE_BYTES = 128 * 1024 * 1024;
const CELL_STRIDE = 63;
export type MutationKind = 0 | 1 | 2 | 3;
export interface GenomeRecord { source: string; parentId: number; mutationKind: MutationKind }
export interface MutationResult { id: number; changed: boolean; reason: string }
export interface WorldSnapshot {
  tick: number; time: number; cells: Cell[]; food: Food[]; births: number; deaths: number;
  eaten: number; width: number; height: number; maxFood: number; foodRate: number;
  mutationRate: number; mutatedBirths: number; mutationSkipped: number; genomeCount: number; genomeLimit: number;
  genomeBytes: number; genomeByteLimit: number; predationCount: number; attackCount: number; photosynthesisTotal: number;
  nutrientTotal: number; nutrientInputTotal: number; nutrientDissolvedTotal: number; nutrientFoodTotal: number; nutrientCellTotal: number;
  environment: EnvironmentSnapshot;
}
interface WasmExports extends WebAssembly.Exports {
  memory: WebAssembly.Memory;
  abi_version(): number;
  alloc(len: number): number;
  dealloc(ptr: number, len: number): void;
  compile_genome(ptr: number, len: number): number;
  analyze_genome(ptr: number, len: number): number;
  genome_analysis_len(): number;
  error_ptr(): number; error_len(): number;
  reset(seed: number, genomeId: number): number;
  reset_empty(seed: number, initialFood: number): void;
  alloc_state(len: number): number;
  save_state_ptr(): number; save_state_len(): number;
  restore_state(ptr: number, len: number): number;
  seed_cells(genomeId: number, count: number, x: number, y: number): number;
  add_food(x: number, y: number, count: number): number;
  set_food_rate(rate: number): void;
  mutate_genome(genomeId: number, seed: number, kind: number): number;
  genome_source_ptr(genomeId: number): number; genome_source_len(): number;
  genome_graph_ptr(genomeId: number): number; genome_graph_len(): number;
  genome_parent(genomeId: number): number; genome_mutation_kind(genomeId: number): number;
  set_mutation_rate(rate: number): void; mutation_rate(): number;
  mutation_count(): number; mutation_skipped_count(): number;
  genome_count(): number; genome_limit(): number;
  tick(steps: number): number;
  snapshot_ptr(): number; snapshot_len(): number;
  set_environment(temperature: number, light: number, current: number): void;
  set_initial_nutrients(level: number): void; add_nutrients(x: number, y: number, amount: number): number;
  nutrient_total(): number; nutrient_input_total(): number; nutrient_dissolved_total(): number;
  nutrient_food_total(): number; nutrient_cell_total(): number;
  add_source(kind: number, x: number, y: number, strength: number, radius: number): number;
  remove_source(id: number): number; clear_sources(): void;
  environment_snapshot_ptr(): number; environment_snapshot_len(): number;
  cell_memory_ptr(id: number): number; cell_memory_len(): number;
  cell_evaluation_ptr(id: number): number; cell_evaluation_len(): number;
  predation_count(): number; attack_count(): number; photosynthesis_total(): number;
  genome_bytes(): number; genome_byte_limit(): number;
}

export class Engine {
  private static compiledModule: Promise<WebAssembly.Module> | undefined;
  private constructor(private readonly wasm: WasmExports) {}

  static async load(): Promise<Engine> {
    // Share only immutable compiled code. Every editor and simulation receives
    // a new instance, including independent linear memory and Rust globals.
    const pending = Engine.compiledModule ??= (async () => {
      // Vite fingerprints this imported asset, so a new frontend cannot reuse
      // a cached engine from a different deployment at the same public URL.
      const response = await fetch(engineUrl);
      if (!response.ok) throw new Error(`The simulation engine could not load (HTTP ${response.status}). Reload the page; if this persists, redeploy the complete site build.`);
      // Byte compilation also works on static hosts without a WASM MIME type.
      return WebAssembly.compile(await response.arrayBuffer());
    })();
    try {
      const instance = await WebAssembly.instantiate(await pending, {});
      return Engine.fromInstance(instance);
    } catch (error) {
      // A failed request or incompatible build must not poison all later loads.
      if (Engine.compiledModule === pending) Engine.compiledModule = undefined;
      throw error;
    }
  }

  private static fromInstance(instance: WebAssembly.Instance): Engine {
    const wasm = instance.exports as WasmExports;
    const mismatch = 'The agar interface and simulation engine are from different builds. Reload the page; if this persists, redeploy the complete site build.';
    const missing = engineContract.functions.filter(name => typeof wasm[name] !== 'function');
    if (!(wasm[engineContract.memory] instanceof WebAssembly.Memory)) missing.push(engineContract.memory);
    if (missing.length) throw new Error(`${mismatch} Missing engine exports: ${missing.join(', ')}.`);
    const abi = wasm.abi_version();
    if (abi !== engineContract.abi) throw new Error(`${mismatch} Expected engine ABI ${engineContract.abi}, received ${abi}.`);
    return new Engine(wasm);
  }

  /** Runs the authoritative compiler without registering a genome or changing the simulation. */
  analyze(source: string): GenomeAnalysis {
    const bytes = new TextEncoder().encode(source);
    // Oversized input is rejected by Rust without allocating or reading a buffer.
    const allocation = bytes.length > MAX_PROGRAM_BYTES ? 0 : Math.max(1, bytes.length);
    const ptr = allocation ? this.wasm.alloc(allocation) : 0;
    if (allocation && !ptr) throw new Error('The engine could not allocate the analysis buffer.');
    try {
      if (ptr) new Uint8Array(this.wasm.memory.buffer, ptr, bytes.length).set(bytes);
      const result = this.wasm.analyze_genome(ptr, bytes.length);
      const length = this.wasm.genome_analysis_len();
      if (!result || !length) throw new Error('The engine could not analyze the genome.');
      const value: unknown = JSON.parse(new TextDecoder().decode(new Uint8Array(this.wasm.memory.buffer, result, length)));
      if (!value || typeof value !== 'object') throw new Error('The engine returned invalid genome analysis.');
      const analysis = value as GenomeAnalysis;
      const range = (from: number, to: number): boolean => Number.isInteger(from) && Number.isInteger(to) && from >= 0 && to >= from && to <= source.length;
      const types = new Set<GenomeValueType>(['integer', 'scalar', 'boolean', 'string', 'vec2']);
      if (typeof analysis.valid !== 'boolean' || analysis.sourceBytes !== bytes.length
        || !Number.isInteger(analysis.nodeCount) || analysis.nodeCount < 0 || analysis.nodeCount > 8192
        || !Array.isArray(analysis.diagnostics) || analysis.diagnostics.length !== (analysis.valid ? 0 : 1)
        || !analysis.diagnostics.every(item => item && item.severity === 'error' && typeof item.message === 'string'
          && Number.isInteger(item.line) && item.line >= 1 && Number.isInteger(item.column) && item.column >= 1 && range(item.from, item.to))
        || !Array.isArray(analysis.bindings) || analysis.bindings.length > 8192
        || !analysis.bindings.every(item => item && typeof item.name === 'string' && types.has(item.type)
          && range(item.from, item.to) && source.slice(item.from, item.to) === item.name)) {
        throw new Error('The engine returned invalid genome analysis.');
      }
      return analysis;
    } finally { if (ptr) this.wasm.dealloc(ptr, allocation); }
  }

  compile(source: string): number {
    const bytes = new TextEncoder().encode(source);
    if (bytes.length > MAX_PROGRAM_BYTES) throw new Error('Genome source must fit in 256 KiB of UTF-8 text.');
    const ptr = this.wasm.alloc(bytes.length);
    if (!ptr) throw new Error('The engine could not allocate the genome buffer.');
    try {
      new Uint8Array(this.wasm.memory.buffer, ptr, bytes.length).set(bytes);
      const id = this.wasm.compile_genome(ptr, bytes.length);
      if (!id) {
        const error = new Uint8Array(this.wasm.memory.buffer, this.wasm.error_ptr(), this.wasm.error_len());
        throw new Error(new TextDecoder().decode(error) || 'The genome could not be compiled.');
      }
      return id;
    } finally { this.wasm.dealloc(ptr, bytes.length); }
  }
  reset(seed: number, genomeId: number): void {
    if (this.wasm.reset(seed >>> 0, genomeId) !== 1) throw new Error(this.readError() || 'The simulation could not be reset.');
  }
  /** Fresh dynamics and no cells; the configured environment and rates are retained. */
  resetEmpty(seed: number, initialFood = 360): void {
    if (!Number.isInteger(initialFood) || initialFood < 0 || initialFood > 1200) throw new Error('Initial food must be between 0 and 1,200 particles.');
    this.wasm.reset_empty(seed >>> 0, initialFood);
  }
  /** Owns all bytes; later engine calls cannot modify this checkpoint. */
  saveState(): Uint8Array {
    const ptr = this.wasm.save_state_ptr();
    const len = this.wasm.save_state_len();
    if (!ptr || !len || len > MAX_STATE_BYTES) throw new Error(this.readError() || 'The simulation could not be saved.');
    return new Uint8Array(this.wasm.memory.buffer, ptr, len).slice();
  }
  /** Validation is atomic: a rejected checkpoint leaves the current engine intact. */
  restoreState(bytes: Uint8Array): void {
    if (!(bytes instanceof Uint8Array) || bytes.byteLength < 20 || bytes.byteLength > MAX_STATE_BYTES) throw new Error('The simulation save must fit in 128 MiB.');
    const ptr = this.wasm.alloc_state(bytes.byteLength);
    if (!ptr) throw new Error('The engine could not allocate the simulation save buffer.');
    try {
      new Uint8Array(this.wasm.memory.buffer, ptr, bytes.byteLength).set(bytes);
      if (this.wasm.restore_state(ptr, bytes.byteLength) !== 1) throw new Error(this.readError() || 'The simulation save is invalid.');
    } finally { this.wasm.dealloc(ptr, bytes.byteLength); }
  }
  private readError(): string {
    return new TextDecoder().decode(new Uint8Array(this.wasm.memory.buffer, this.wasm.error_ptr(), this.wasm.error_len()));
  }
  seed(genomeId: number, count = 8, x = -1, y = -1): number {
    if (!Number.isSafeInteger(count) || count < 0) throw new Error('Organism count must be a nonnegative safe whole number.');
    const added = this.wasm.seed_cells(genomeId, count, x, y);
    const error = this.readError();
    if (error || added !== count) throw new Error(error || 'The engine could not create the requested organisms.');
    return added;
  }
  addFood(x = -1, y = -1, count = 40): number { return this.wasm.add_food(x, y, count); }
  setFoodRate(rate: number): void { this.wasm.set_food_rate(rate); }
  setMutationRate(rate: number): void { this.wasm.set_mutation_rate(rate); }
  setEnvironment(temperature: number, light: number, current: number): void { this.wasm.set_environment(temperature, light, current); }
  setInitialNutrients(level: number): void { this.wasm.set_initial_nutrients(level); }
  addNutrients(x = -1, y = -1, amount = 25): number { return this.wasm.add_nutrients(x, y, amount); }
  addSource(kind: SourceKind, x: number, y: number, strength: number, radius: number): number {
    return this.wasm.add_source(kind, x, y, strength, radius);
  }
  removeSource(id: number): boolean { return this.wasm.remove_source(id) !== 0; }
  clearSources(): void { this.wasm.clear_sources(); }
  getMemory(id: number): RegisterEntry[] {
    const pointer = this.wasm.cell_memory_ptr(id);
    const length = this.wasm.cell_memory_len();
    if (!pointer || !length) return [];
    const entries: unknown = JSON.parse(new TextDecoder().decode(new Uint8Array(this.wasm.memory.buffer, pointer, length)));
    if (!Array.isArray(entries)) throw new Error('The engine returned an invalid memory snapshot.');
    return entries as RegisterEntry[];
  }
  getCellEvaluation(id: number): GenomeEvaluation {
    if (!Number.isInteger(id) || id < 1 || id > 0xffff_ffff) throw new Error('Choose a living organism to inspect its program.');
    const pointer = this.wasm.cell_evaluation_ptr(id);
    const length = this.wasm.cell_evaluation_len();
    if (!pointer || !length) throw new Error(this.readError() || `Organism ${id} is no longer available.`);
    // Decode before another engine call can replace the buffer or grow memory.
    const json = new TextDecoder().decode(new Uint8Array(this.wasm.memory.buffer, pointer, length));
    return validateGenomeEvaluation(JSON.parse(json), id);
  }
  mutate(genomeId: number, seed: number, kind: MutationKind = 0): MutationResult {
    const id = this.wasm.mutate_genome(genomeId, seed >>> 0, kind);
    // A rejected/no-op mutation leaves readable feedback in the engine's error buffer.
    const reason = new TextDecoder().decode(new Uint8Array(this.wasm.memory.buffer, this.wasm.error_ptr(), this.wasm.error_len()));
    if (!id) throw new Error(reason || 'The genome could not be mutated.');
    return { id, changed: id !== genomeId, reason };
  }
  getGenome(id: number): GenomeRecord {
    if (!Number.isSafeInteger(id) || id < 1 || id > this.wasm.genome_count()) {
      throw new Error(`Genome ${id} is not available in this session.`);
    }
    const pointer = this.wasm.genome_source_ptr(id);
    const length = this.wasm.genome_source_len();
    if (!pointer || !length) throw new Error(`Genome ${id} is not available in this session.`);
    // Rust owns the source buffer; decode before another engine call can replace/grow it.
    const source = new TextDecoder().decode(new Uint8Array(this.wasm.memory.buffer, pointer, length));
    return { source, parentId: this.wasm.genome_parent(id), mutationKind: this.wasm.genome_mutation_kind(id) as MutationKind };
  }
  getGenomeGraph(id: number): GenomeGraph {
    if (!Number.isSafeInteger(id) || id < 1 || id > this.wasm.genome_count()) {
      throw new Error(`Genome ${id} is not available in this session.`);
    }
    const pointer = this.wasm.genome_graph_ptr(id);
    const length = this.wasm.genome_graph_len();
    if (!pointer || !length) throw new Error(`The graph for genome ${id} is not available.`);
    // Decode immediately: later calls may replace the engine-owned buffer or grow memory.
    const source = new TextDecoder().decode(new Uint8Array(this.wasm.memory.buffer, pointer, length));
    return validateGenomeGraph(JSON.parse(source));
  }
  tick(steps = 1): void {
    if (this.wasm.tick(steps) !== 1) throw new Error(this.readError() || 'The simulation could not advance.');
  }

  environment(): EnvironmentSnapshot {
    const pointer = this.wasm.environment_snapshot_ptr();
    const data = new Float32Array(this.wasm.memory.buffer, pointer, this.wasm.environment_snapshot_len());
    const columns = data[0], rows = data[1], count = data[5];
    const sampleEnd = 7 + columns * rows * 5;
    if (!Number.isInteger(columns) || !Number.isInteger(rows) || columns < 1 || rows < 1
      || !Number.isInteger(count) || count < 0 || count > 16
      || data.length !== sampleEnd + count * 6) throw new Error('The environment snapshot has an invalid layout.');
    const samples = data.slice(7, sampleEnd);
    const sources: EnvironmentSource[] = [];
    for (let i = 0; i < count; i++) {
      const n = sampleEnd + i * 6;
      sources.push({ id: data[n], kind: data[n + 1] as SourceKind, x: data[n + 2], y: data[n + 3], strength: data[n + 4], radius: data[n + 5] });
    }
    return { columns, rows, ambientTemperature: data[2], ambientLight: data[3], currentStrength: data[4], initialNutrients: data[6], samples, sources };
  }

  snapshot(): WorldSnapshot {
    const ptr = this.wasm.snapshot_ptr();
    const data = new Float32Array(this.wasm.memory.buffer, ptr, this.wasm.snapshot_len());
    if (data.length < 12 || !Number.isInteger(data[2]) || data[2] < 0 || !Number.isInteger(data[3]) || data[3] < 0
      || data.length !== 12 + data[2] * CELL_STRIDE + data[3] * 4) throw new Error('The world snapshot has an invalid layout.');
    const cells: Cell[] = [];
    const food: Food[] = [];
    for (let i = 0; i < data[2]; i++) {
      const n = 12 + i * CELL_STRIDE;
      cells.push({ id:data[n], genomeId:data[n+1], x:data[n+2], y:data[n+3], vx:data[n+4], vy:data[n+5],
        angle:data[n+6], energy:data[n+7], age:data[n+8], generation:data[n+9], parentId:data[n+10], radius:data[n+11],
        foodAngle:data[n+12], foodDistance:data[n+13], foodVisible:data[n+14]!==0,
        neighborAngle:data[n+15], neighborDistance:data[n+16], neighborVisible:data[n+17]!==0,
        size:data[n+18], energyCapacity:data[n+19], basalMetabolism:data[n+20],
        memoryUsed:data[n+21], memoryCost:data[n+22], lightLevel:data[n+23], lightAngle:data[n+24],
        temperature:data[n+25], temperatureAngle:data[n+26], flowX:data[n+27], flowY:data[n+28],
        preyId:data[n+29], preyAngle:data[n+30], preyDistance:data[n+31], preySize:data[n+32], preyVisible:data[n+33]!==0,
        threatAngle:data[n+34], threatDistance:data[n+35], threatSize:data[n+36], threatVisible:data[n+37]!==0,
        integrity:data[n+38], photosynthesisRate:data[n+39],
        nutrientStored:data[n+40], nutrientCapacity:data[n+41], nutrientLevel:data[n+42], nutrientAngle:data[n+43], nutrientUptakeRate:data[n+44],
        bodyPropulsion:data[n+45], bodyPhotosynthesis:data[n+46], bodyDigestion:data[n+47], bodyArmor:data[n+48], bodySensing:data[n+49],
        senseRange:data[n+50], remodelingRate:data[n+51], bodyUpkeep:data[n+52],
        bodyAttack:data[n+53], gutEnergy:data[n+54], gutCapacity:data[n+55], gutNutrients:data[n+56],
        digestionRate:data[n+57], growthRate:data[n+58], birthSize:data[n+59], maturity:data[n+60], lifespan:data[n+61], exudationRate:data[n+62] });
    }
    for (let i = 0; i < data[3]; i++) {
      const n = 12 + cells.length * CELL_STRIDE + i * 4;
      food.push({ x:data[n], y:data[n+1], energy:data[n+2], nutrients:data[n+3] });
    }
    // Copy every snapshot value before further calls: Rust may grow its memory.
    const state = { tick:data[0], time:data[1], cells, food, births:data[4], deaths:data[5], eaten:data[6],
      width:data[7], height:data[8], maxFood:data[10], foodRate:data[11] };
    return { ...state, mutationRate: this.wasm.mutation_rate(), mutatedBirths: this.wasm.mutation_count(),
      mutationSkipped: this.wasm.mutation_skipped_count(), genomeCount: this.wasm.genome_count(), genomeLimit: this.wasm.genome_limit(),
      genomeBytes: this.wasm.genome_bytes(), genomeByteLimit: this.wasm.genome_byte_limit(),
      predationCount: this.wasm.predation_count(), attackCount: this.wasm.attack_count(), photosynthesisTotal: this.wasm.photosynthesis_total(),
      nutrientTotal: this.wasm.nutrient_total(), nutrientInputTotal: this.wasm.nutrient_input_total(),
      nutrientDissolvedTotal: this.wasm.nutrient_dissolved_total(), nutrientFoodTotal: this.wasm.nutrient_food_total(), nutrientCellTotal: this.wasm.nutrient_cell_total(),
      environment: this.environment() };
  }
}
