import type { EnvironmentSnapshot, MetricsSnapshot, RenderSnapshot, SimulationSummary } from './engine';

export interface PresentationOptions { metrics?: boolean; environment?: boolean }
export interface Presentation {
  frame: RenderSnapshot;
  metrics?: MetricsSnapshot;
  environment?: EnvironmentSnapshot;
  /** A tick batch may have advanced before stopping; frame describes that state. */
  error?: string;
}
export interface WirePresentation {
  frame: Float64Array;
  metrics?: Float64Array;
  environment?: EnvironmentSnapshot;
  error?: string;
}
export interface CapturedCheckpoint { state: Uint8Array; summary: SimulationSummary }

export const ENGINE_METHODS = [
  'analyze', 'compile', 'reset', 'resetEmpty', 'saveState', 'restoreState', 'seed', 'addFood',
  'setFoodRate', 'setMutationRate', 'setEnvironment', 'setInitialNutrients', 'addNutrients',
  'addSource', 'removeSource', 'clearSources', 'getMemory', 'getCellEvaluation', 'languageSchema',
  'cellBody', 'mutate', 'getGenome', 'getGenomeGraph', 'tick', 'environment', 'renderSnapshot',
  'metricsSnapshot', 'cellSnapshot', 'summary', 'snapshot',
] as const;
export type EngineMethod = typeof ENGINE_METHODS[number];
export type WorkerMethod = EngineMethod | 'init' | 'advance' | 'presentation' | 'captureCheckpoint';
export interface WorkerRequest { id: number; method: WorkerMethod; args: unknown[] }
export type WorkerResponse = { id: number; ok: true; value: unknown } | { id: number; ok: false; error: string };
