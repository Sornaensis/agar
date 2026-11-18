import engineUrl from '../engine/pkg/agar_engine.wasm?url';
import engineContract from './engine-contract.json';

/** Versioned boundary to the Rust engine. All simulation runs in WASM. */
export const ORGANELLE_KINDS = ['core','photosynthetic','metabolic','digestive','reserve','transporter','phagocyte','flagellum','piercer','cortex','repair','photoreceptor','thermoreceptor','chemoreceptor','mechanoreceptor','optical'] as const;
export type OrganelleKind = typeof ORGANELLE_KINDS[number];
export interface OrganelleSummary { kind: OrganelleKind; count:number; mass:number; capacity:number; damaged:number; building:number; functionalMass:number; protein:number }
export type ProductRole = 'scaffold' | 'anchor' | 'motor' | 'filament' | 'pigment' | 'enzyme' | 'membrane' | 'transporter' | 'receptor' | 'regulator' | 'replicase' | 'constrictor';
export const MECHANISM_KINDS = [
  'support()', 'translate()', 'store(energy)', 'store(mineral)', 'store(food)', 'harvest(light)',
  'import(mineral)', 'import(particle)', 'engulf()', 'digest(exudate)', 'digest(tissue)', 'digest(waste)',
  'propel()', 'pierce()', 'shield()', 'repair()', 'sense(light)', 'sense(heat)', 'sense(chemical)',
  'sense(contact)', 'sense(optical)', 'grow()', 'replicate()', 'constrict()', 'recycle()',
] as const;
export type MechanismKind = typeof MECHANISM_KINDS[number];
export type CellularAssemblyKind = OrganelleKind | 'membrane' | 'replicator' | 'constrictor' | MechanismKind;
export interface CellularProduct {
  name:string;role:ProductRole;mass:number;concentration:number;halfLife:number;
  expressionRequested:number;expressionRate:number;current:boolean;
}
export interface CellularRecipe {
  name:string;kind:CellularAssemblyKind;requiredMass:number;at:number;direction:number;
  components:{name:string;role:ProductRole;requiredMass:number}[];
  startAllowed?:boolean;recycleRequested?:number;recycleRate?:number;
}
export interface CellularAssembly extends CellularRecipe {
  id:number;mass:number;functionalMass:number;complete:boolean;integrity:number;activity:number;
  bottleneck:string|null;components:{name:string;role:ProductRole;mass:number;requiredMass:number}[];
}
export interface CellularBodyInspection {
  products:CellularProduct[];assemblies:CellularAssembly[];recipes:CellularRecipe[];
  replication:{mass:number;requiredMass:number;progress:number;genomeMass:number};constriction:{progress:number};
}
export interface BodyInspection {
  cellId:number; area:number; mass:number; spaceUsed:number; spaceCapacity:number; surfaceUsed:number; surfaceCapacity:number;
  constructionRate:number; recyclingRate:number; repairRate:number; synthesisRate:number; assemblyCapacity:number;
  organs: { id:number; kind:OrganelleKind; mass:number; integrity:number; functional:boolean; minimumMass:number; embodiedEnergy:number }[];
  requested: { kind:OrganelleKind; count:number; size:number; build:number; recycle:number; proteinMass:number }[];
  cellular?:CellularBodyInspection|null;
}

/** Physical inventories are validated before use in the inspector or canvas. */
export function validateBodyInspection(value: unknown, expectedCellId: number): BodyInspection {
  const fail = (): never => { throw new Error('The engine returned an invalid body inspection.'); };
  if (!value || typeof value !== 'object') return fail();
  const body = value as BodyInspection;
  const finite=(n:unknown):boolean=>typeof n==='number' && Number.isFinite(n) && n>=0 && n<=1e12;
  const angle=(n:unknown):boolean=>typeof n==='number' && Number.isFinite(n) && Math.abs(n)<=1e12;
  const kind=(v:unknown):boolean=>ORGANELLE_KINDS.includes(v as OrganelleKind);
  if (body.cellId!==expectedCellId || ![body.area,body.mass,body.spaceUsed,body.spaceCapacity,body.surfaceUsed,body.surfaceCapacity,body.constructionRate,body.recyclingRate,body.repairRate,body.synthesisRate,body.assemblyCapacity].every(finite)
    || !Array.isArray(body.organs) || body.organs.length>24000 || !body.organs.every(o=>o && Number.isInteger(o.id) && o.id>0 && kind(o.kind) && [o.mass,o.integrity,o.minimumMass,o.embodiedEnergy].every(finite) && o.integrity<=1 && typeof o.functional==='boolean')
    || new Set(body.organs.map(o=>o.id)).size!==body.organs.length
    || !Array.isArray(body.requested) || body.requested.length!==16 || !body.requested.every((r,i)=>r.kind===ORGANELLE_KINDS[i] && Number.isInteger(r.count) && r.count>=0 && [r.size,r.build,r.recycle,r.proteinMass].every(finite) && r.build<=1 && r.recycle<=1)) return fail();
  if (body.cellular != null) {
    const cellular = body.cellular;
    const roles = new Set(['scaffold','anchor','motor','filament','pigment','enzyme','membrane','transporter','receptor','regulator','replicase','constrictor']);
    const name=(n:unknown):boolean=>typeof n==='string' && /^[A-Za-z_][A-Za-z0-9_]{0,127}$/.test(n);
    const recipe=(r:CellularRecipe):boolean=>Boolean(r) && name(r.name) && (kind(r.kind)||['membrane','replicator','constrictor'].includes(r.kind)||MECHANISM_KINDS.includes(r.kind as MechanismKind))
      && finite(r.requiredMass) && angle(r.at) && angle(r.direction) && Array.isArray(r.components) && r.components.length<=64
      && (r.startAllowed===undefined || typeof r.startAllowed==='boolean')
      && (r.recycleRequested===undefined || finite(r.recycleRequested) && r.recycleRequested<=1)
      && (r.recycleRate===undefined || finite(r.recycleRate))
      && r.components.every(part=>part && name(part.name) && roles.has(part.role) && finite(part.requiredMass));
    if (!Array.isArray(cellular.products) || cellular.products.length>1024
      || !cellular.products.every(p=>p && name(p.name) && roles.has(p.role) && typeof p.current==='boolean'
        && [p.mass,p.concentration,p.halfLife,p.expressionRequested,p.expressionRate].every(finite))
      || !Array.isArray(cellular.recipes) || cellular.recipes.length>64 || !cellular.recipes.every(recipe)
      || !Array.isArray(cellular.assemblies) || cellular.assemblies.length>32768
      || !cellular.assemblies.every(a=>recipe(a) && Number.isInteger(a.id) && a.id>0
        && [a.mass,a.functionalMass,a.integrity,a.activity].every(finite) && a.integrity<=1 && a.activity<=1 && typeof a.complete==='boolean'
        && (a.bottleneck===null || typeof a.bottleneck==='string' && a.bottleneck.length<=256) && a.components.every(part=>finite(part.mass)))
      || new Set(cellular.assemblies.map(a=>a.id)).size!==cellular.assemblies.length
      || !cellular.replication || ![cellular.replication.mass,cellular.replication.requiredMass,cellular.replication.progress,cellular.replication.genomeMass].every(finite)
      || cellular.replication.progress>1 || !cellular.constriction || !finite(cellular.constriction.progress) || cellular.constriction.progress>1) return fail();
  }
  return body;
}
export interface LanguageSchema {
  version:3; signals:{name:string;type:GenomeValueType}[]; outputs:{name:string;type:GenomeValueType;required:boolean;default:GenomeValue}[];
  functions?:{name:string;arity:number}[];
  maxNodes:number;maxSourceBytes:number;registersPerType:number;
  cellular?: { marker:'cellular;'; products:{name:string;arguments:string[]}[]; assemblies:{name:string;requiredRoles:string[]}[];
    mechanisms?:{name:string;constructor:string;target:string|null;requiredRoles:string[];description:string;receptorFields?:{name:string;type:GenomeValueType;requiresValid:boolean}[]}[];
    recipeControls?:{name:string;type:GenomeValueType;default:GenomeValue}[];
    concentration?:string;userFunctions?:{keyword:'fn';maxParameters:number;parameterType:'scalar';maxFunctions:number;recursive:false};
    reads:{name:string;type:GenomeValueType}[];maxProducts:number;maxAssemblies:number;maxGenes:number };
}
export interface Cell {
  id:number; genomeId:number; x:number; y:number; vx:number; vy:number; angle:number; energy:number; age:number; generation:number; parentId:number;
  radius:number; bodyArea:number; bodyMass:number; birthArea:number; energyCapacity:number; basalMetabolism:number; memoryUsed:number; memoryCost:number;
  integrity:number; photosynthesisRate:number; nutrientStored:number; nutrientCapacity:number; nutrientUptakeRate:number;
  gutEnergy:number; gutCapacity:number; gutNutrients:number; digestionRate:number; growthRate:number; maturity:number; exudationRate:number;
  gutExudateEnergy:number; gutTissueEnergy:number; gutWasteEnergy:number;
  gutExudateNutrients:number; gutTissueNutrients:number; gutWasteNutrients:number; expressionRate:number;
  bodyUpkeep:number; spaceUsed:number; spaceCapacity:number; surfaceUsed:number; surfaceCapacity:number; divisionReady:boolean; opticalRange:number;
  organelles:OrganelleSummary[];
}
export type OrganicKind = 0 | 1 | 2;
export interface Food { x: number; y: number; energy: number; nutrients: number; kind:OrganicKind }
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
  mode?: 'legacy' | 'cellular';
  time: number; tick: number; source: string;
  expressions: GenomeExpressionEvaluation[]; bindings: GenomeBindingEvaluation[];
  signals: GenomeSignalEvaluation[]; outputs: GenomeOutputEvaluation[]; writes: GenomeWriteEvaluation[];
}

/** Keep source ranges and runtime values trustworthy before rendering inspection UI. */
export function validateGenomeEvaluation(value: unknown, expectedCellId?: number): GenomeEvaluation {
  const fail = (): never => { throw new Error('The engine returned an invalid program evaluation. Rebuild the current WASM engine.'); };
  if (!value || typeof value !== 'object') return fail();
  const trace = value as GenomeEvaluation;
  const cellular = trace.mode === 'cellular';
  const types = new Set<GenomeValueType>(['integer', 'scalar', 'boolean', 'string', 'vec2']);
  const positiveId = (id: number): boolean => Number.isInteger(id) && id >= 1 && id <= 0xffff_ffff;
  if (trace.version !== 1 || trace.semantics !== 'current-snapshot'
    || (trace.mode !== undefined && trace.mode !== 'legacy' && !cellular)
    || !positiveId(trace.cellId) || !positiveId(trace.genomeId)
    || (expectedCellId !== undefined && trace.cellId !== expectedCellId)
    || !Number.isSafeInteger(trace.tick) || trace.tick < 0 || !Number.isFinite(trace.time) || trace.time < 0
    || typeof trace.source !== 'string' || trace.source.length > MAX_PROGRAM_BYTES
    || new TextEncoder().encode(trace.source).length > MAX_PROGRAM_BYTES
    || !Array.isArray(trace.expressions) || (!cellular && !trace.expressions.length) || trace.expressions.length > 8192
    || !Array.isArray(trace.bindings) || trace.bindings.length > 8192
    || !Array.isArray(trace.signals) || (cellular ? trace.signals.length > READ_SIGNAL_COUNT + 64 * 3 + 64 * 9 : trace.signals.length !== READ_SIGNAL_COUNT)
    || !Array.isArray(trace.outputs) || (cellular ? trace.outputs.length > 256 + 64 * 3 : trace.outputs.length !== WRITE_SIGNAL_COUNT)
    || !Array.isArray(trace.writes) || trace.writes.length > (cellular ? 0 : 9990)) return fail();
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
  let previousExpressionId = -1;
  for (const expression of trace.expressions) {
    // Inlined function internals have graph IDs but no unambiguous authored
    // source value. Their invocation result remains inspectable, leaving gaps.
    if (!expression || !Number.isInteger(expression.id) || expression.id <= previousExpressionId || expression.id >= 8192
      || !range(expression.from, expression.to) || !typed(expression.type, expression.value)) return fail();
    previousExpressionId = expression.id;
  }
  for (const binding of trace.bindings) {
    if (!named(binding) || !range(binding.from, binding.to) || trace.source.slice(binding.from, binding.to) !== binding.name) return fail();
  }
  if (!trace.signals.every(named) || new Set(trace.signals.map(signal => signal.name)).size !== trace.signals.length) return fail();
  const outputNames = new Set<string>();
  for (const output of trace.outputs) {
    if (!named(output) || outputNames.has(output.name)
      || (cellular && (!/^(gene|activity|start_when|recycle)\.[A-Za-z_][A-Za-z0-9_]*$/.test(output.name)
        || output.type !== (output.name.startsWith('start_when.') ? 'boolean' : 'scalar')))
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
export interface GenomeGraph { version: 1; mode?:'legacy'|'cellular'; nodes: GenomeGraphNode[]; roots: GenomeGraphRoot[] }

/** Validate the owned JSON boundary before any graph traversal or DOM creation. */
export function validateGenomeGraph(value: unknown): GenomeGraph {
  const fail = (): never => { throw new Error('The engine returned an invalid genome graph. Rebuild the current WASM engine.'); };
  if (!value || typeof value !== 'object') return fail();
  const graph = value as GenomeGraph;
  if (graph.version !== 1 || (graph.mode!==undefined && graph.mode!=='legacy' && graph.mode!=='cellular') || !Array.isArray(graph.nodes) || graph.nodes.length > 8192
    || !Array.isArray(graph.roots) || graph.roots.length > 9990 + WRITE_SIGNAL_COUNT) return fail();
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
const READ_SIGNAL_COUNT = 169;
const WRITE_SIGNAL_COUNT = 81;
const CELL_STRIDE = 158;
const FOOD_STRIDE = 5;
export type MutationKind = 0 | 1 | 2 | 3 | 4 | 5 | 6;
export interface GenomeRecord { source: string; parentId: number; mutationKind: MutationKind }
export interface MutationResult { id: number; changed: boolean; reason: string }
export interface WorldSnapshot {
  tick: number; time: number; cells: Cell[]; food: Food[]; births: number; deaths: number;
  eaten: number; width: number; height: number; foodRate: number;
  mutationRate: number; mutatedBirths: number; mutationSkipped: number; genomeCount: number; genomeLimit: number;
  genomeBytes: number; genomeByteLimit: number; predationCount: number; attackCount: number; photosynthesisTotal: number;
  energyTotal:number; energyInputTotal:number;
  nutrientTotal: number; nutrientInputTotal: number; nutrientDissolvedTotal: number; nutrientFoodTotal: number; nutrientCellTotal: number;
  environment: EnvironmentSnapshot;
}
export interface RenderCell {
  id:number; genomeId:number; x:number; y:number; vx:number; vy:number; angle:number; radius:number;
  energy:number; energyCapacity:number; gutEnergy:number; gutCapacity:number; integrity:number; photosynthesisRate:number;
  flagellumCount:number; flagellumCapacity:number; cortexMass:number; cortexCapacity:number;
  photosyntheticCapacity:number; piercerCapacity:number; generation:number;
}
export interface RenderSnapshot {
  tick:number; time:number; width:number; height:number; births:number; deaths:number; eaten:number;
  foodRate:number; mutationRate:number; generation:number; cells:RenderCell[]; food:Food[];
}
export type RenderWorldSnapshot = RenderSnapshot & { environment: EnvironmentSnapshot };
export interface SimulationSummary {
  time:number; population:number; food:number; births:number; deaths:number; generation:number;
}
export interface MetricsSnapshot extends SimulationSummary {
  tick:number; eaten:number; mutationRate:number; mutatedBirths:number; mutationSkipped:number;
  genomeCount:number; genomeLimit:number; genomeBytes:number; genomeByteLimit:number;
  predationCount:number; attackCount:number; photosynthesisTotal:number; energyTotal:number; energyInputTotal:number;
  nutrientTotal:number; nutrientInputTotal:number; nutrientDissolvedTotal:number; nutrientFoodTotal:number; nutrientCellTotal:number;
  trophic:{autotrophs:number;heterotrophs:number;mixotrophs:number;hunters:number};
  flow:{production:number;digestion:number;exudation:number};
  organicCounts:[number,number,number];
  genomes:{id:number;population:number;maxGeneration:number}[];
}

const RENDER_HEADER = 12;
const RENDER_CELL_STRIDE = 21;
const METRICS_HEADER = 36;

/** Decode a worker-transferred compact buffer without any WASM calls. */
export function decodeRenderSnapshot(data: Float64Array): RenderSnapshot {
  if (data.length < RENDER_HEADER || !Number.isSafeInteger(data[2]) || data[2] < 0
    || !Number.isSafeInteger(data[3]) || data[3] < 0
    || data.length !== RENDER_HEADER + data[2] * RENDER_CELL_STRIDE + data[3] * FOOD_STRIDE) {
    throw new Error('The render snapshot has an invalid layout.');
  }
  const cells: RenderCell[] = [];
  const food: Food[] = [];
  for (let i = 0; i < data[2]; i++) {
    const n = RENDER_HEADER + i * RENDER_CELL_STRIDE;
    cells.push({id:data[n],genomeId:data[n+1],x:data[n+2],y:data[n+3],vx:data[n+4],vy:data[n+5],
      angle:data[n+6],radius:data[n+7],energy:data[n+8],energyCapacity:data[n+9],gutEnergy:data[n+10],
      gutCapacity:data[n+11],integrity:data[n+12],photosynthesisRate:data[n+13],flagellumCount:data[n+14],
      flagellumCapacity:data[n+15],cortexMass:data[n+16],cortexCapacity:data[n+17],
      photosyntheticCapacity:data[n+18],piercerCapacity:data[n+19],generation:data[n+20]});
  }
  const start = RENDER_HEADER + cells.length * RENDER_CELL_STRIDE;
  for (let i = 0; i < data[3]; i++) {
    const n = start + i * FOOD_STRIDE;
    const kind = data[n+4];
    if (kind !== 0 && kind !== 1 && kind !== 2) throw new Error('The render snapshot has an invalid organic material kind.');
    food.push({x:data[n],y:data[n+1],energy:data[n+2],nutrients:data[n+3],kind});
  }
  return {tick:data[0],time:data[1],cells,food,births:data[4],deaths:data[5],eaten:data[6],
    width:data[7],height:data[8],foodRate:data[9],mutationRate:data[10],generation:data[11]};
}

export function decodeMetricsSnapshot(data: Float64Array): MetricsSnapshot {
  if (data.length < METRICS_HEADER || !Number.isSafeInteger(data[35]) || data[35] < 0
    || data.length !== METRICS_HEADER + data[35] * 3) throw new Error('The metrics snapshot has an invalid layout.');
  const genomes: MetricsSnapshot['genomes'] = [];
  for (let i = 0; i < data[35]; i++) {
    const n = METRICS_HEADER + i * 3;
    genomes.push({id:data[n],population:data[n+1],maxGeneration:data[n+2]});
  }
  return {tick:data[0],time:data[1],population:data[2],food:data[3],births:data[4],deaths:data[5],eaten:data[6],
    generation:data[7],mutationRate:data[8],mutatedBirths:data[9],mutationSkipped:data[10],genomeCount:data[11],
    genomeLimit:data[12],genomeBytes:data[13],genomeByteLimit:data[14],predationCount:data[15],attackCount:data[16],
    photosynthesisTotal:data[17],energyTotal:data[18],energyInputTotal:data[19],nutrientTotal:data[20],
    nutrientInputTotal:data[21],nutrientDissolvedTotal:data[22],nutrientFoodTotal:data[23],nutrientCellTotal:data[24],
    trophic:{autotrophs:data[25],heterotrophs:data[26],mixotrophs:data[27],hunters:data[28]},
    flow:{production:data[29],digestion:data[30],exudation:data[31]},organicCounts:[data[32],data[33],data[34]],genomes};
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
  reset_empty(seed: number, initialFood: number): number;
  alloc_state(len: number): number;
  save_state_ptr(): number; save_state_len(): number;
  restore_state(ptr: number, len: number): number;
  seed_cells(genomeId: number, count: number, x: number, y: number): number;
  language_schema_ptr():number; language_schema_len():number;
  cell_body_ptr(id:number):number; cell_body_len():number;
  energy_total():number; energy_input_total():number;
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
  render_snapshot_ptr(): number; metrics_snapshot_ptr(): number; cell_snapshot_ptr(id: number): number;
  summary_snapshot_ptr(): number; presentation_len(): number;
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
    if (!Number.isSafeInteger(initialFood) || initialFood < 0) throw new Error('Initial organic particle count must be a nonnegative safe whole number.');
    if (this.wasm.reset_empty(seed >>> 0, initialFood) !== 1) throw new Error(this.readError() || 'The simulation could not be reset with the requested organic particles.');
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
  addFood(x = -1, y = -1, count = 40): number {
    if (!Number.isSafeInteger(count) || count < 0) throw new Error('Organic particle count must be a nonnegative safe whole number.');
    const added = this.wasm.add_food(x, y, count);
    const error = this.readError();
    if (error || added !== count) throw new Error(error || 'The engine could not create the requested particles.');
    return added;
  }
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
  languageSchema():LanguageSchema {
    const ptr=this.wasm.language_schema_ptr(), len=this.wasm.language_schema_len();
    const schema=JSON.parse(new TextDecoder().decode(new Uint8Array(this.wasm.memory.buffer,ptr,len))) as LanguageSchema;
    if (schema.version!==3 || schema.signals?.length!==READ_SIGNAL_COUNT || schema.outputs?.length!==WRITE_SIGNAL_COUNT) throw new Error('The engine language schema is incompatible.');
    return schema;
  }
  cellBody(id:number):BodyInspection {
    const ptr=this.wasm.cell_body_ptr(id), len=this.wasm.cell_body_len();
    if (!ptr || !len) throw new Error(this.readError() || 'This organism is no longer available.');
    return validateBodyInspection(JSON.parse(new TextDecoder().decode(new Uint8Array(this.wasm.memory.buffer,ptr,len))),id);
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

  /** Owned compact bytes can be transferred to the UI without cloning cell objects. */
  renderSnapshotData(): Float64Array {
    const pointer = this.wasm.render_snapshot_ptr();
    return new Float64Array(this.wasm.memory.buffer, pointer, this.wasm.presentation_len()).slice();
  }
  renderSnapshot(): RenderSnapshot { return decodeRenderSnapshot(this.renderSnapshotData()); }
  metricsSnapshotData(): Float64Array {
    const pointer = this.wasm.metrics_snapshot_ptr();
    return new Float64Array(this.wasm.memory.buffer, pointer, this.wasm.presentation_len()).slice();
  }
  metricsSnapshot(): MetricsSnapshot { return decodeMetricsSnapshot(this.metricsSnapshotData()); }
  cellSnapshot(id: number): Cell | null {
    if (!Number.isInteger(id) || id < 1 || id > 0xffff_ffff) return null;
    const pointer = this.wasm.cell_snapshot_ptr(id);
    const length = this.wasm.presentation_len();
    if (!pointer && length === 0) return null;
    if (!pointer || length !== CELL_STRIDE) throw new Error('The selected cell snapshot has an invalid layout.');
    const cell = decodeCell(new Float64Array(this.wasm.memory.buffer, pointer, length), 0);
    if (cell.id !== id) throw new Error('The selected cell snapshot has the wrong organism.');
    return cell;
  }
  summary(): SimulationSummary {
    const pointer = this.wasm.summary_snapshot_ptr();
    const length = this.wasm.presentation_len();
    if (length !== 6) throw new Error('The simulation summary has an invalid layout.');
    const data = new Float64Array(this.wasm.memory.buffer, pointer, length);
    return {time:data[0],population:data[1],food:data[2],births:data[3],deaths:data[4],generation:data[5]};
  }

  snapshot(): WorldSnapshot {
    const ptr = this.wasm.snapshot_ptr();
    const data = new Float64Array(this.wasm.memory.buffer, ptr, this.wasm.snapshot_len());
    if (data.length < 12 || !Number.isInteger(data[2]) || data[2] < 0 || !Number.isInteger(data[3]) || data[3] < 0
      || data.length !== 12 + data[2] * CELL_STRIDE + data[3] * FOOD_STRIDE) throw new Error('The world snapshot has an invalid layout.');
    const cells: Cell[] = [];
    const food: Food[] = [];
    for (let i = 0; i < data[2]; i++) {
      cells.push(decodeCell(data, 12 + i * CELL_STRIDE));
    }
    for (let i = 0; i < data[3]; i++) {
      const n = 12 + cells.length * CELL_STRIDE + i * FOOD_STRIDE;
      if (![0,1,2].includes(data[n+4])) throw new Error('The world snapshot has an invalid organic material kind.');
      food.push({ x:data[n], y:data[n+1], energy:data[n+2], nutrients:data[n+3], kind:data[n+4] as OrganicKind });
    }
    // Copy every snapshot value before further calls: Rust may grow its memory.
    const state = { tick:data[0], time:data[1], cells, food, births:data[4], deaths:data[5], eaten:data[6],
      width:data[7], height:data[8], foodRate:data[11] };
    return { ...state, mutationRate: this.wasm.mutation_rate(), mutatedBirths: this.wasm.mutation_count(),
      mutationSkipped: this.wasm.mutation_skipped_count(), genomeCount: this.wasm.genome_count(), genomeLimit: this.wasm.genome_limit(),
      genomeBytes: this.wasm.genome_bytes(), genomeByteLimit: this.wasm.genome_byte_limit(),
      predationCount: this.wasm.predation_count(), attackCount: this.wasm.attack_count(), photosynthesisTotal: this.wasm.photosynthesis_total(),
      energyTotal:this.wasm.energy_total(),energyInputTotal:this.wasm.energy_input_total(),
      nutrientTotal: this.wasm.nutrient_total(), nutrientInputTotal: this.wasm.nutrient_input_total(),
      nutrientDissolvedTotal: this.wasm.nutrient_dissolved_total(), nutrientFoodTotal: this.wasm.nutrient_food_total(), nutrientCellTotal: this.wasm.nutrient_cell_total(),
      environment: this.environment() };
  }
}

function decodeCell(data: Float64Array, n: number): Cell {
  const organelles:OrganelleSummary[] = ORGANELLE_KINDS.map((kind,index)=>{
    const k=n+39+index*6;return {kind,count:data[k],mass:data[k+1],capacity:data[k+2],damaged:data[k+3],building:data[k+4],functionalMass:data[k+5],protein:data[n+141+index]};
  });
  return {id:data[n],genomeId:data[n+1],x:data[n+2],y:data[n+3],vx:data[n+4],vy:data[n+5],angle:data[n+6],energy:data[n+7],age:data[n+8],generation:data[n+9],parentId:data[n+10],radius:data[n+11],
    bodyArea:data[n+12],bodyMass:data[n+13],birthArea:data[n+14],energyCapacity:data[n+15],basalMetabolism:data[n+16],memoryUsed:data[n+17],memoryCost:data[n+18],integrity:data[n+19],photosynthesisRate:data[n+20],
    nutrientStored:data[n+21],nutrientCapacity:data[n+22],nutrientUptakeRate:data[n+23],gutEnergy:data[n+24],gutCapacity:data[n+25],gutNutrients:data[n+26],digestionRate:data[n+27],growthRate:data[n+28],maturity:data[n+29],exudationRate:data[n+31],bodyUpkeep:data[n+32],spaceUsed:data[n+33],spaceCapacity:data[n+34],surfaceUsed:data[n+35],surfaceCapacity:data[n+36],divisionReady:data[n+37]!==0,opticalRange:data[n+38],
    gutExudateEnergy:data[n+135],gutTissueEnergy:data[n+136],gutWasteEnergy:data[n+137],gutExudateNutrients:data[n+138],gutTissueNutrients:data[n+139],gutWasteNutrients:data[n+140],expressionRate:data[n+157],organelles};
}
