import { decodeEcologySnapshot, decodeMetricsSnapshot, decodeRenderSnapshot } from './engine';
import type { BodyInspection, Cell, Food, EnvironmentMetadata, EnvironmentSnapshot, GasExchangeConfig, GenomeAnalysis, GenomeEvaluation, GenomeGraph, GenomeLineageEntry, GenomeRecord, LanguageSchema, MetricsSnapshot, MutationKind, MutationResult, RegisterEntry, RenderSnapshot, SimulationSummary, SourceKind, WorldSnapshot } from './engine';
import type { CapturedCheckpoint, Presentation, PresentationOptions, WirePresentation, WorkerMethod, WorkerRequest, WorkerResponse } from './simulation-protocol';
export type { CapturedCheckpoint, Presentation, PresentationOptions } from './simulation-protocol';

/** Injectable transport also supports real worker-thread integration tests. */
export type SimulationWorker = Pick<Worker, 'postMessage' | 'terminate' | 'onmessage' | 'onerror' | 'onmessageerror'>;

export class WorkerEngine {
  private sequence = 0;
  private stopped: Error | null = null;
  private readonly pending = new Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>();

  private constructor(private readonly worker: SimulationWorker) {
    worker.onmessage = event => {
      const reply = event.data as WorkerResponse;
      if (!reply || !Number.isSafeInteger(reply.id) || typeof reply.ok !== 'boolean') {
        this.stop(new Error('The simulation worker returned an invalid response.'));
        return;
      }
      const waiting = this.pending.get(reply.id);
      if (!waiting) return;
      this.pending.delete(reply.id);
      if (reply.ok) waiting.resolve(reply.value);
      else waiting.reject(new Error(reply.error));
    };
    worker.onerror = event => {
      event.preventDefault?.();
      this.stop(new Error(event.message || 'The simulation worker stopped unexpectedly.'));
    };
    worker.onmessageerror = () => this.stop(new Error('A simulation worker response could not be read.'));
  }

  static async load(): Promise<WorkerEngine> {
    if (typeof Worker === 'undefined') throw new Error('This browser does not support background simulation workers.');
    return WorkerEngine.fromWorker(new Worker(new URL('./simulation-worker.ts', import.meta.url), { type: 'module', name: 'agar-simulation' }));
  }

  static async fromWorker(worker: SimulationWorker): Promise<WorkerEngine> {
    const engine = new WorkerEngine(worker);
    try { await engine.call('init'); return engine; }
    catch (error) { engine.dispose(); throw error; }
  }

  private stop(error: Error): void {
    if (this.stopped) return;
    this.stopped = error;
    this.worker.terminate();
    this.worker.onmessage = null;
    this.worker.onerror = null;
    this.worker.onmessageerror = null;
    for (const waiting of this.pending.values()) waiting.reject(error);
    this.pending.clear();
  }

  dispose(): void { this.stop(new Error('This simulation has been closed.')); }

  private call<T>(method: WorkerMethod, args: unknown[] = [], transfer: Transferable[] = []): Promise<T> {
    if (this.stopped) return Promise.reject(this.stopped);
    const id = ++this.sequence;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: value => resolve(value as T), reject });
      try { this.worker.postMessage({ id, method, args } satisfies WorkerRequest, transfer); }
      catch (error) {
        this.pending.delete(id);
        reject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }

  private decodePresentation(value: WirePresentation): Presentation<Food[]>;
  private decodePresentation(value: WirePresentation, packedParticles: boolean): Presentation;
  private decodePresentation(value: WirePresentation, packedParticles = false): Presentation {
    const result: Presentation = { tick: value.tick, summary: value.summary };
    if (value.frame) result.frame = decodeRenderSnapshot(value.frame, packedParticles);
    if (value.ecology) result.ecology = decodeEcologySnapshot(value.ecology);
    if (value.metrics) {
      result.metrics = decodeMetricsSnapshot(value.metrics);
      result.metrics.mineralAbsorbedTotal = value.mineralAbsorbedTotal;
      if (result.ecology) result.metrics.ecology = result.ecology;
      if (value.countAudit) result.metrics.countAudit = value.countAudit;
      if (value.energyLoss) result.metrics.energyLoss = value.energyLoss;
    }
    if (value.environment) result.environment = value.environment;
    if (value.error) result.error = value.error;
    return result;
  }

  advance(steps: number, options?: PresentationOptions): Promise<Presentation<Food[]>>;
  advance(steps: number, options: PresentationOptions, packedParticles: true, workerSliceMs?: number): Promise<Presentation>;
  async advance(steps: number, options: PresentationOptions = {}, packedParticles = false, workerSliceMs?: number): Promise<Presentation> {
    return this.decodePresentation(await this.call<WirePresentation>('advance', [steps, options, workerSliceMs]), packedParticles);
  }
  async presentation(options: PresentationOptions = {}): Promise<Presentation<Food[]>> {
    return this.decodePresentation(await this.call<WirePresentation>('presentation', [options]));
  }
  captureCheckpoint(): Promise<CapturedCheckpoint> { return this.call('captureCheckpoint'); }

  analyze(source: string): Promise<GenomeAnalysis> { return this.call('analyze', [source]); }
  compile(source: string): Promise<number> { return this.call('compile', [source]); }
  reset(seed: number, genomeId: number): Promise<void> { return this.call('reset', [seed, genomeId]); }
  setCountChemistry(enabled:boolean):Promise<void> { return this.call('setCountChemistry',[enabled]); }
  usesCountChemistry():Promise<boolean> { return this.call('usesCountChemistry',[]); }
  resetEmpty(seed: number): Promise<void> { return this.call('resetEmpty', [seed]); }
  saveState(): Promise<Uint8Array> { return this.call('saveState'); }
  restoreState(bytes: Uint8Array): Promise<void> {
    // Preserve the caller's saved bytes even if a restore fails or worker closes.
    const copy = bytes.slice();
    return this.call('restoreState', [copy], [copy.buffer as ArrayBuffer]);
  }
  placeCell(genomeId:number, x:number, y:number): Promise<number> { return this.call('placeCell', [genomeId, x, y]); }
  moveCell(cellId:number, x:number, y:number, colony = false): Promise<number> { return this.call('moveCell', [cellId, x, y, colony]); }
  setCellStores(cellId:number, energy:number|null, minerals:number|null): Promise<void> { return this.call('setCellStores', [cellId, energy, minerals]); }
  seed(genomeId: number, count = 8, x = -1, y = -1): Promise<number> { return this.call('seed', [genomeId, count, x, y]); }
  setMutationRate(rate: number): Promise<void> { return this.call('setMutationRate', [rate]); }
  setEnvironment(temperature: number, light: number, current: number): Promise<void> { return this.call('setEnvironment', [temperature, light, current]); }
  setWorldSize(width: number, height: number): Promise<boolean> { return this.call('setWorldSize', [width, height]); }
  setInitialNutrients(level: number): Promise<void> { return this.call('setInitialNutrients', [level]); }
  setInitialGases(oxygen: number, carbon: number): Promise<boolean> { return this.call('setInitialGases', [oxygen, carbon]); }
  setGasExchange(config: GasExchangeConfig): Promise<boolean> { return this.call('setGasExchange', [config]); }
  addNutrients(x = -1, y = -1, amount = 25): Promise<number> { return this.call('addNutrients', [x, y, amount]); }
  addSource(kind: SourceKind, x: number, y: number, strength: number, radius: number, angle = 0, length = 600): Promise<number> { return this.call('addSource', [kind, x, y, strength, radius, angle, length]); }
  replaceSources(sources: Parameters<import('./engine').Engine['replaceSources']>[0]): Promise<boolean> { return this.call('replaceSources', [sources]); }
  updateSource(id: number, kind: SourceKind, x: number, y: number, strength: number, radius: number, angle: number, length: number): Promise<boolean> { return this.call('updateSource', [id, kind, x, y, strength, radius, angle, length]); }
  removeSource(id: number): Promise<boolean> { return this.call('removeSource', [id]); }
  clearSources(): Promise<void> { return this.call('clearSources'); }
  addBarrier(x: number, y: number, length: number, width: number, angle: number, floating: boolean): Promise<number> { return this.call('addBarrier', [x, y, length, width, angle, floating]); }
  updateBarrier(id: number, x: number, y: number, length: number, width: number, angle: number, floating: boolean): Promise<boolean> { return this.call('updateBarrier', [id, x, y, length, width, angle, floating]); }
  removeBarrier(id: number): Promise<boolean> { return this.call('removeBarrier', [id]); }
  clearBarriers(): Promise<void> { return this.call('clearBarriers'); }
  getMemory(id: number): Promise<RegisterEntry[]> { return this.call('getMemory', [id]); }
  getCellEvaluation(id: number): Promise<GenomeEvaluation> { return this.call('getCellEvaluation', [id]); }
  languageSchema(): Promise<LanguageSchema> { return this.call('languageSchema'); }
  cellBody(id: number): Promise<BodyInspection> { return this.call('cellBody', [id]); }
  mutate(genomeId: number, seed: number, kind: MutationKind = 0): Promise<MutationResult> { return this.call('mutate', [genomeId, seed, kind]); }
  getGenome(id: number): Promise<GenomeRecord> { return this.call('getGenome', [id]); }
  getGenomeLineage(afterId = 0): Promise<GenomeLineageEntry[]> { return this.call('getGenomeLineage', [afterId]); }
  getGenomeGraph(id: number): Promise<GenomeGraph> { return this.call('getGenomeGraph', [id]); }
  tick(steps = 1): Promise<void> { return this.call('tick', [steps]); }
  environment(): Promise<EnvironmentSnapshot> { return this.call('environment'); }
  environmentMetadata(): Promise<EnvironmentMetadata> { return this.call('environmentMetadata'); }
  renderSnapshot(): Promise<RenderSnapshot> { return this.presentation().then(value => value.frame!); }
  metricsSnapshot(): Promise<MetricsSnapshot> { return this.call('metricsSnapshot'); }
  cellSnapshot(id: number): Promise<Cell | null> { return this.call('cellSnapshot', [id]); }
  summary(): Promise<SimulationSummary> { return this.call('summary'); }
  snapshot(): Promise<WorldSnapshot> { return this.call('snapshot'); }
}
