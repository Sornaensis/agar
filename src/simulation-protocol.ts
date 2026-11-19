import type { EnvironmentMetadata, EnvironmentSnapshot, MetricsSnapshot, RenderSnapshot, SimulationSummary } from './engine';

export interface PresentationOptions {
  /** Omit drawing data when only the simulation clock or aggregates are needed. */
  frame?: boolean;
  metrics?: boolean;
  environment?: boolean | 'metadata';
}
export interface Presentation {
  tick: number;
  summary: SimulationSummary;
  frame?: RenderSnapshot;
  metrics?: MetricsSnapshot;
  environment?: EnvironmentMetadata | EnvironmentSnapshot;
  /** A tick batch may have advanced before stopping; tick/summary describe that state. */
  error?: string;
}
export interface WirePresentation {
  tick: number;
  summary: SimulationSummary;
  frame?: Float64Array;
  metrics?: Float64Array;
  environment?: EnvironmentMetadata | EnvironmentSnapshot;
  error?: string;
}
export interface CapturedCheckpoint { state: Uint8Array; summary: SimulationSummary }

export const ENGINE_METHODS = [
  'analyze', 'compile', 'reset', 'resetEmpty', 'saveState', 'restoreState', 'seed', 'placeCell', 'moveCell', 'setCellStores',
  'setMutationRate', 'setEnvironment', 'setInitialNutrients', 'addNutrients',
  'addSource', 'replaceSources', 'removeSource', 'clearSources', 'getMemory', 'getCellEvaluation', 'languageSchema',
  'setWorldSize', 'updateSource', 'addBarrier', 'updateBarrier', 'removeBarrier', 'clearBarriers',
  'setInitialGases', 'setGasExchange',
  'cellBody', 'mutate', 'getGenome', 'getGenomeGraph', 'tick', 'environment', 'renderSnapshot',
  'metricsSnapshot', 'cellSnapshot', 'summary', 'environmentMetadata', 'snapshot',
] as const;
export type EngineMethod = typeof ENGINE_METHODS[number];
export type WorkerMethod = EngineMethod | 'init' | 'advance' | 'presentation' | 'captureCheckpoint';
export interface WorkerRequest { id: number; method: WorkerMethod; args: unknown[] }
export type WorkerResponse = { id: number; ok: true; value: unknown } | { id: number; ok: false; error: string };
