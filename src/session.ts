import type { Engine, WorldSnapshot } from './engine';
import type { SimulationMetadata, SimulationRecord, SimulationSummary, Storage } from './storage';

export function summarize(world: WorldSnapshot): SimulationSummary {
  return { time: world.time, population: world.cells.length, food: world.food.length, births: world.births,
    deaths: world.deaths, generation: world.cells.reduce((max, cell) => Math.max(max, cell.generation), 0) };
}

/** A checkpoint belongs to one world, even while another world is being opened. */
export class SimulationSession {
  world: WorldSnapshot;
  metadata: SimulationMetadata;
  revision = 0;
  savedRevision = 0;
  pending: Promise<void> | null = null;
  release: () => void = () => {};
  constructor(readonly engine: Engine, metadata: SimulationMetadata, private readonly storage: Storage,
    private readonly status: (state: 'saving' | 'saved' | 'error', error?: unknown) => void,
    private readonly committed: (metadata: SimulationMetadata) => void) {
    this.metadata = structuredClone(metadata);
    this.world = engine.snapshot();
  }
  touch(): void { this.revision++; }
  refresh(): void {
    this.world = this.engine.snapshot();
    const history = this.metadata.history;
    if (!history.length || this.world.time - history[history.length - 1].time >= 1) {
      history.push(summarize(this.world));
      if (history.length > 720) history.splice(0, history.length - 720);
    }
    this.metadata.summary = summarize(this.world);
  }
  advance(steps: number): void { this.engine.tick(steps); this.refresh(); this.touch(); }
  async checkpoint(): Promise<void> {
    if (this.pending) return this.pending;
    if (this.revision === this.savedRevision) return;
    const revision = this.revision;
    this.status('saving');
    // Snapshot synchronously before any await: metadata and bytes describe one tick.
    let record: SimulationRecord;
    let write: Promise<void>;
    try {
      record = { ...structuredClone(this.metadata), updatedAt: Math.max(Date.now(), this.metadata.updatedAt), summary: summarize(this.world), state: this.engine.saveState() };
      // Validation can reject before IndexedDB creates its promise.
      write = this.storage.putSimulation(record);
    } catch (error) { this.status('error', error); throw error; }
    const operation = write.then(() => {
      this.savedRevision = revision;
      this.metadata.updatedAt = record.updatedAt;
      const { state: _state, ...metadata } = record;
      this.committed(metadata);
      this.status('saved');
    }).catch(error => { this.status('error', error); throw error; });
    this.pending = operation;
    try { await operation; } finally { if (this.pending === operation) this.pending = null; }
  }
  /** Call while advancement is suspended, before releasing ownership of this run. */
  async flush(): Promise<void> {
    while (this.pending || this.revision !== this.savedRevision) {
      if (this.pending) await this.pending;
      else await this.checkpoint();
    }
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
