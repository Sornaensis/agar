import type { EnvironmentSnapshot, GenomeEvaluation, MetricsSnapshot, RenderSnapshot } from './engine';
import type { WorkerEngine, Presentation, PresentationOptions } from './worker-engine';
import type { SimulationMetadata, SimulationRecord, SimulationSummary, Storage } from './storage';

export type RenderWorldSnapshot = RenderSnapshot & { environment: EnvironmentSnapshot };
export interface CellEvaluationSnapshot { evaluation: GenomeEvaluation; stateVersion: number }

export function summarize(world: RenderSnapshot): SimulationSummary {
  return { time: world.time, population: world.cells.length, food: world.food.length,
    births: world.births, deaths: world.deaths, generation: world.generation };
}

/** Owns a run's ordered commands, presentation state, and checkpoint revisions. */
export class SimulationSession {
  world: RenderWorldSnapshot;
  metrics: MetricsSnapshot;
  metadata: SimulationMetadata;
  revision = 0;
  savedRevision = 0;
  /** Latest engine state, independent of the last frame requested for drawing. */
  tick: number;
  stateVersion = 0;
  pending: Promise<void> | null = null;
  release: () => void = () => {};
  private tail: Promise<void> = Promise.resolve();
  private queued = 0;
  private flushing = 0;
  private disposed = false;
  private commandEpoch = 0;
  private readonly evaluations = new Map<number, { epoch: number; version: number; pending: boolean; value: Promise<CellEvaluationSnapshot> }>();

  constructor(readonly engine: WorkerEngine, metadata: SimulationMetadata, initial: Presentation,
    private readonly storage: Storage,
    private readonly status: (state: 'saving' | 'saved' | 'error', error?: unknown) => void,
    private readonly committed: (metadata: SimulationMetadata) => void) {
    if (!initial.frame || !initial.environment || !('samples' in initial.environment) || !initial.metrics) throw new Error('The initial simulation snapshot is incomplete.');
    this.metadata = structuredClone(metadata);
    this.world = { ...initial.frame, environment: initial.environment };
    this.metrics = initial.metrics;
    this.tick = initial.tick;
    this.metadata.summary = initial.summary;
  }

  get busy(): boolean { return this.queued > 0 || this.flushing > 0 || this.disposed; }
  touch(): void { this.revision++; }

  private enqueue<T>(operation: () => Promise<T>): Promise<T> {
    if (this.disposed) return Promise.reject(new Error('This simulation has been closed.'));
    this.queued++;
    const result = this.tail.then(operation).finally(() => { this.queued--; });
    this.tail = result.then(() => undefined, () => undefined);
    return result;
  }

  private accept(presentation: Presentation): void {
    this.tick = presentation.tick;
    const incoming = presentation.environment;
    let environment = this.world.environment;
    if (incoming) {
      environment = 'samples' in incoming ? incoming : { ...incoming,
        // A size edit normally returns a full snapshot. Do not attach old-grid
        // samples to new dimensions if a caller explicitly requests metadata.
        samples: incoming.columns === environment.columns && incoming.rows === environment.rows ? environment.samples : new Float32Array(0) };
    }
    if (presentation.frame || incoming) this.world = { ...(presentation.frame ?? this.world), environment };
    if (presentation.metrics) this.metrics = presentation.metrics;
    const summary = presentation.summary, history = this.metadata.history;
    if (!history.length || summary.time - history[history.length - 1].time >= 1) {
      history.push(summary);
      if (history.length > 720) history.splice(0, history.length - 720);
    }
    this.metadata.summary = summary;
  }

  refresh(options: PresentationOptions = { metrics: true, environment: true }): Promise<void> {
    return this.enqueue(async () => { this.accept(await this.engine.presentation(options)); });
  }

  /** Keep related detail reads on one tick without marking the world dirty. */
  inspect<T>(read: (engine: WorkerEngine) => Promise<T>): Promise<T> {
    return this.enqueue(() => read(this.engine));
  }

  /** Inline and expanded inspectors share one immutable trace for this state. */
  evaluation(cellId: number): Promise<CellEvaluationSnapshot> {
    if (this.disposed) return Promise.reject(new Error('This simulation has been closed.'));
    const cached = this.evaluations.get(cellId);
    if (cached && cached.epoch === this.commandEpoch && (cached.pending || cached.version === this.stateVersion)) return cached.value;
    const entry = { epoch: this.commandEpoch, version: this.stateVersion, pending: true, value: null as unknown as Promise<CellEvaluationSnapshot> };
    entry.value = this.inspect(async engine => {
      entry.version = this.stateVersion;
      return { evaluation: await engine.getCellEvaluation(cellId), stateVersion: entry.version };
    }).catch(error => { if (this.evaluations.get(cellId) === entry) this.evaluations.delete(cellId); throw error; })
      .finally(() => { entry.pending = false; });
    this.evaluations.set(cellId, entry);
    // Keep recent open inspectors cheap without retaining every selected cell.
    if (this.evaluations.size > 16) for (const [id, value] of this.evaluations) {
      if (!value.pending && id !== cellId) { this.evaluations.delete(id); break; }
    }
    return entry.value;
  }

  advance(steps: number, options: PresentationOptions = {}): Promise<void> {
    this.commandEpoch++;
    return this.enqueue(async () => {
      const presentation = await this.engine.advance(steps, options);
      // Even a failed batch can have completed earlier ticks. Accept that exact
      // state and mark it dirty before propagating the original engine error.
      this.accept(presentation);
      this.stateVersion++;
      this.touch();
      if (presentation.error) throw new Error(presentation.error);
    });
  }

  /** Serialize a UI edit with advancement and checkpoint capture. */
  mutate<T>(operation: (engine: WorkerEngine) => Promise<T>): Promise<T> {
    this.commandEpoch++;
    return this.enqueue(async () => {
      let value: T | undefined, failure: unknown, failed = false;
      try { value = await operation(this.engine); }
      catch (error) { failure = error; failed = true; }
      this.touch();
      this.stateVersion++;
      try { this.accept(await this.engine.presentation({ metrics: true, environment: true })); }
      catch (error) { if (!failed) { failure = error; failed = true; } }
      if (failed) throw failure;
      return value as T;
    });
  }

  checkpoint(): Promise<void> {
    if (this.pending) return this.pending;
    if (!this.queued && this.revision === this.savedRevision) return Promise.resolve();
    this.status('saving');
    // Capture metadata only after earlier queued mutations finish. Capture
    // bytes and their summary in one worker command before later mutations run.
    const capture = this.enqueue(async () => {
      const revision = this.revision, metadata = structuredClone(this.metadata);
      const saved = await this.engine.captureCheckpoint();
      const record: SimulationRecord = { ...metadata, state: saved.state, summary: saved.summary,
        updatedAt: Math.max(Date.now(), this.metadata.updatedAt) };
      return { revision, record };
    });
    const operation = capture.then(async ({ revision, record }) => {
      await this.storage.putSimulation(record);
      this.savedRevision = revision;
      this.metadata.updatedAt = record.updatedAt;
      const { state: _state, ...metadata } = record;
      this.committed(metadata);
      this.status('saved');
    }).catch(error => { this.status('error', error); throw error; });
    const pending = operation.finally(() => { if (this.pending === pending) this.pending = null; });
    this.pending = pending;
    return pending;
  }

  /** Drain in-flight work and dirty revisions before releasing run ownership. */
  async flush(): Promise<void> {
    this.flushing++;
    try {
      for (;;) {
        await this.tail;
        if (this.pending) await this.pending;
        else if (this.revision !== this.savedRevision) await this.checkpoint();
        else if (!this.queued) break;
      }
    } finally { this.flushing--; }
  }

  dispose(): void {
    this.disposed = true;
    this.evaluations.clear();
    this.engine.dispose();
    this.release();
    this.release = () => {};
  }
}

/** Prevent two tabs from advancing and saving different futures for one run. */
export async function claimSimulation(id: string): Promise<() => void> {
  if (!navigator.locks) throw new Error('This browser cannot safely open saved simulations. Use a browser with Web Locks support.');
  return new Promise((resolve, reject) => {
    void navigator.locks.request('cellarium:simulation:' + id, { mode: 'exclusive', ifAvailable: true }, lock => {
      if (!lock) { reject(new Error('This simulation is already open in another tab. Close it there first.')); return; }
      return new Promise<void>(release => resolve(release));
    }).catch(reject);
  });
}
