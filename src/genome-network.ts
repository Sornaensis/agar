import type { GenomeGraph, GenomeGraphNode, GenomeValueType } from './engine';

export type NetworkNodeKind = 'input' | 'operation' | 'register' | 'output';
export type NetworkEdgeKind = 'compute' | 'read' | 'write' | 'clear';
export interface NetworkConstant {
  port: string;
  value: string;
  rule?: string;
  /** Names attached to a folded constant, never names of its consumer. */
  bindings?: readonly string[];
}
export interface NetworkNode {
  id: string;
  kind: NetworkNodeKind;
  label: string;
  type: GenomeValueType;
  bindings: string[];
  constants: NetworkConstant[];
  sourceIds: number[];
  rootIds: string[];
  level: number;
  implicit: boolean;
}
export interface NetworkEdge {
  id: string;
  from: string;
  to: string;
  kind: NetworkEdgeKind;
  port: string;
  guard: boolean;
  /** A register commit, rather than a same-tick expression dependency. */
  temporal: boolean;
  rootId?: string;
}
export interface GenomeNetwork {
  nodes: NetworkNode[];
  edges: NetworkEdge[];
  expressionCount: number;
  unusedCount: number;
  constantCount: number;
  aliasCount: number;
}
export interface NetworkSlice {
  nodes: NetworkNode[];
  edges: NetworkEdge[];
  total: number;
  omitted: number;
  /** Visible nodes with at least one connection outside this slice. */
  frontier: string[];
}

interface FoldedConstant {
  constant: string;
  bindings: string[];
}
type Target = NetworkNode | FoldedConstant;
const isConstant = (target: Target): target is FoldedConstant => 'constant' in target;
const registerType = (address: string): GenomeValueType => {
  const types: Record<string, GenomeValueType> = { ri: 'integer', rf: 'scalar', rb: 'boolean', rs: 'string', rv: 'vec2' };
  const type = types[address.slice(0, 2)];
  if (!type) throw new Error(`Unknown register address in genome graph: ${address}`);
  return type;
};
const operandName = (node: GenomeGraphNode, index: number): string => node.kind === 'conditional'
  ? ['condition', 'then', 'else'][index]
  : node.kind === 'binary' ? ['left', 'right'][index]
  : node.kind === 'unary' ? 'value' : `input ${index + 1}`;

/**
 * Project a validated compiler graph without executing it or changing its data.
 * References/literals fold away; operations remain distinct, and memory is one
 * address node with old-state read edges and explicitly delayed commit edges.
 */
export function buildGenomeNetwork(graph: GenomeGraph): GenomeNetwork {
  const nodes: NetworkNode[] = [];
  const edges: NetworkEdge[] = [];
  const named = new Map<string, NetworkNode>();
  const targets: (Target | undefined)[] = new Array(graph.nodes.length);
  const create = (id: string, kind: NetworkNodeKind, label: string, type: GenomeValueType): NetworkNode => {
    const existing = named.get(id);
    if (existing) return existing;
    const result: NetworkNode = { id, kind, label, type, bindings: [], constants: [], sourceIds: [], rootIds: [], level: 0, implicit: false };
    named.set(id, result); nodes.push(result);
    return result;
  };
  const register = (address: string): NetworkNode => create(`register:${address}`, 'register', address, registerType(address));
  const resolve = (id: number): Target => {
    const target = targets[id];
    if (!target) throw new Error('A connected genome expression references a missing dependency.');
    return target;
  };
  let constantCount = 0, aliasCount = 0, unusedCount = 0;

  // Compiler IDs are topological, so even a maximal alias chain takes one
  // forward pass rather than recursion or a repeated walk through the chain.
  for (const node of graph.nodes) {
    if (!node.reachable) { unusedCount++; continue; }
    let target: Target;
    switch (node.kind) {
      case 'constant':
        constantCount++;
        target = { constant: node.label, bindings: [] };
        break;
      case 'reference':
        aliasCount++;
        target = resolve(node.inputs[0]);
        break;
      case 'sensor':
        target = create(`input:${node.label}`, 'input', node.label, node.type);
        break;
      case 'register':
        target = register(node.label);
        break;
      case 'populated': {
        const address = /^populated\((r[ifbsv]\d{3})\)$/.exec(node.label)?.[1];
        if (!address) throw new Error('A genome occupancy check has an invalid register address.');
        register(address);
        target = create(`op:${node.id}`, 'operation', 'populated', node.type);
        break;
      }
      default:
        target = create(`op:${node.id}`, 'operation', node.label, node.type);
    }
    targets[node.id] = target;
    target.bindings.push(...node.bindings);
    if (!isConstant(target)) target.sourceIds.push(node.id);
  }

  // A long alias chain can feed thousands of roots. Share one immutable alias
  // list per folded literal instead of copying the chain into every port.
  const constantBindings = new Map<FoldedConstant, readonly string[]>();
  const connect = (target: Target, destination: NetworkNode, id: string, port: string,
    kind?: NetworkEdgeKind, guard = false, temporal = false, rootId?: string): void => {
    if (isConstant(target)) {
      const value: NetworkConstant = { port, value: target.constant };
      if (rootId !== undefined) value.rule = rootId;
      if (target.bindings.length) {
        let bindings = constantBindings.get(target);
        if (!bindings) {
          bindings = Object.freeze([...new Set(target.bindings)]);
          constantBindings.set(target, bindings);
        }
        value.bindings = bindings;
      }
      destination.constants.push(value);
      return;
    }
    edges.push({ id, from: target.id, to: destination.id,
      kind: kind ?? (target.kind === 'input' || target.kind === 'register' ? 'read' : 'compute'),
      port, guard, temporal, ...(rootId === undefined ? {} : { rootId }) });
  };

  // Only actual operation instances produce these edges. Alias occurrences do
  // not repeat an operation's edges, while duplicate operand ports stay distinct.
  for (const node of graph.nodes) {
    if (!node.reachable || ['constant', 'reference', 'sensor', 'register'].includes(node.kind)) continue;
    const target = resolve(node.id) as NetworkNode;
    if (node.kind === 'populated') {
      const address = /^populated\((r[ifbsv]\d{3})\)$/.exec(node.label)![1];
      connect(register(address), target, `operand:${node.id}:occupancy`, 'occupancy');
    } else {
      node.inputs.forEach((input, index) => connect(resolve(input), target, `operand:${node.id}:${index}`, operandName(node, index)));
    }
  }

  for (const root of graph.roots) {
    const target = root.kind === 'output'
      ? create(root.id, 'output', root.name, root.type) : register(root.name);
    target.rootIds.push(root.id);
    target.implicit = root.kind === 'output' && root.value === null;
    const kind: NetworkEdgeKind = root.kind === 'clear' ? 'clear' : 'write';
    const temporal = root.kind !== 'output';
    if (root.value !== null) connect(resolve(root.value), target, `${root.id}:value`, 'value', kind, false, temporal, root.id);
    else if (root.kind === 'output') target.constants.push({ port: 'default', value: root.default ?? '0', rule: root.id });
    if (root.guard !== null) connect(resolve(root.guard), target, `${root.id}:when`, 'when', kind, true, temporal, root.id);
    else if (root.kind === 'clear') target.constants.push({ port: 'clear', value: 'always', rule: root.id });
  }
  // A birth-initialized marker can serve its whole lifetime without appearing
  // in an expression or a write rule. It is still a visible register node.
  if (graph.contactMessages) for (const key of ['publish', 'avoid', 'adhere', 'signal'] as const) {
    const address = graph.contactMessages[key];
    if (address) register(address);
  }

  // Registers remain at level zero. Commit edges are deliberately absent from
  // this DAG calculation, including a legal direct register self-assignment.
  const inputs = new Map<string, NetworkEdge[]>();
  for (const edge of edges) if (!edge.temporal) {
    const list = inputs.get(edge.to) ?? []; list.push(edge); inputs.set(edge.to, list);
  }
  for (const node of nodes) {
    if (node.kind === 'operation' || node.kind === 'output') {
      node.level = (inputs.get(node.id) ?? []).reduce((level, edge) => Math.max(level, named.get(edge.from)!.level + 1), 1);
    }
    node.bindings = [...new Set(node.bindings)];
    node.sourceIds = [...new Set(node.sourceIds)].sort((a, b) => a - b);
  }
  return { nodes, edges, expressionCount: graph.nodes.length, unusedCount, constantCount, aliasCount };
}

const boundedInteger = (value: number, fallback: number, maximum: number): number =>
  Number.isFinite(value) ? Math.max(0, Math.min(maximum, Math.floor(value))) : fallback;

/**
 * Select a bounded, bidirectional neighborhood. Its edges are actual compiler
 * connections only; hidden chains are never replaced by invented shortcut edges.
 * Overview starts with authored actions and then memory rules. Focus includes
 * upstream inputs and downstream consumers, across explicit temporal links too.
 * Global levels are retained; renderers should rebase levels within the slice.
 */
export function networkSlice(network: GenomeNetwork, focus: string | null, limit = 112, depthLimit = 8): NetworkSlice {
  const maximum = boundedInteger(limit, 112, 256);
  const depth = boundedInteger(depthLimit, 8, 64);
  const byId = new Map(network.nodes.map(node => [node.id, node]));
  if (focus !== null && !byId.has(focus)) return { nodes: [], edges: [], total: 0, omitted: 0, frontier: [] };
  if (focus === null && network.nodes.length <= maximum) return { nodes: network.nodes.slice(), edges: network.edges.slice(), total: network.nodes.length, omitted: 0, frontier: [] };
  const adjacency = new Map<string, Set<string>>();
  for (const edge of network.edges) {
    const from = adjacency.get(edge.from) ?? new Set<string>(); from.add(edge.to); adjacency.set(edge.from, from);
    const to = adjacency.get(edge.to) ?? new Set<string>(); to.add(edge.from); adjacency.set(edge.to, to);
  }
  let total = network.nodes.length;
  if (focus !== null) {
    const reached = new Set<string>();
    const pending = [focus];
    while (pending.length) {
      const id = pending.pop()!;
      if (reached.has(id)) continue;
      reached.add(id);
      for (const neighbor of adjacency.get(id) ?? []) if (!reached.has(neighbor)) pending.push(neighbor);
    }
    total = reached.size;
  }
  const selected = new Set<string>();
  const queued = new Set<string>();
  const queue: { id: string; depth: number }[] = [];
  const enqueue = (id: string, level: number): void => {
    if (!queued.has(id)) { queued.add(id); queue.push({ id, depth: level }); }
  };
  const endpoints = focus !== null ? [focus] : [
    ...network.nodes.filter(node => node.kind === 'output' && !node.implicit),
    ...network.nodes.filter(node => node.kind === 'register'),
    ...network.nodes.filter(node => node.kind === 'output' && node.implicit),
  ].map(node => typeof node === 'string' ? node : node.id);
  let endpoint = 0, cursor = 0;
  // Starting all authored action roots together spreads the first layer over
  // program behavior instead of filling the view with the first deep formula.
  if (focus === null) {
    for (const node of network.nodes) if (node.kind === 'output' && !node.implicit) enqueue(node.id, 0);
  } else enqueue(focus, 0);
  while (selected.size < maximum) {
    if (cursor >= queue.length) {
      while (endpoint < endpoints.length && queued.has(endpoints[endpoint])) endpoint++;
      if (endpoint >= endpoints.length) break;
      enqueue(endpoints[endpoint++], 0);
    }
    const item = queue[cursor++];
    selected.add(item.id);
    if (item.depth < depth) for (const neighbor of adjacency.get(item.id) ?? []) enqueue(neighbor, item.depth + 1);
  }
  const nodes = network.nodes.filter(node => selected.has(node.id));
  const edges = network.edges.filter(edge => selected.has(edge.from) && selected.has(edge.to));
  const frontier = nodes.filter(node => [...adjacency.get(node.id) ?? []].some(neighbor => !selected.has(neighbor))).map(node => node.id);
  return { nodes, edges, total, omitted: total - nodes.length, frontier };
}
