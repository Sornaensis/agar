import { Engine } from './engine';
import { ENGINE_METHODS } from './simulation-protocol';
import type { CapturedCheckpoint, PresentationOptions, WirePresentation, WorkerRequest, WorkerResponse } from './simulation-protocol';

type Send = (response: WorkerResponse, transfer: Transferable[]) => void;

const message = (error: unknown): string => error instanceof Error ? error.message : String(error);

/** The one owner processes every command, checkpoint and inspection in order. */
export function createSimulationWorkerHandler(send: Send, load: () => Promise<Engine> = () => Engine.load()): (request: unknown) => void {
  let engine: Engine | null = null;
  let queue = Promise.resolve();
  const methods = new Set<string>(ENGINE_METHODS);

  function presentation(options: PresentationOptions = {}): WirePresentation {
    const frame = options.frame === false ? undefined : engine!.renderSnapshotData();
    const result: WirePresentation = frame
      ? { tick: frame[0], summary: { time: frame[1], population: frame[2], food: frame[3], births: frame[4], deaths: frame[5], generation: frame[11] }, frame }
      : engine!.status();
    if (options.metrics) {
      result.metrics = engine!.metricsSnapshotData();
      result.mineralAbsorbedTotal = engine!.mineralAbsorbedTotal();
      const audit = engine!.countAudit();
      if (audit) result.countAudit = audit;
      const losses = engine!.energyLoss();
      if (losses) result.energyLoss = losses;
    }
    if (options.metrics || options.ecology) result.ecology = engine!.ecologySnapshotData();
    if (options.environment) result.environment = options.environment === 'metadata' ? engine!.environmentMetadata() : engine!.environment();
    return result;
  }

  async function dispatch(request: WorkerRequest): Promise<unknown> {
    if (request.method === 'init') {
      engine ??= await load();
      return undefined;
    }
    if (!engine) throw new Error('The simulation worker is not ready.');
    if (request.method === 'presentation') return presentation(request.args[0] as PresentationOptions | undefined);
    if (request.method === 'advance') {
      const steps = request.args[0] as number;
      if (!Number.isSafeInteger(steps) || steps < 0 || steps > 120) throw new Error('A simulation batch must contain 0 to 120 whole ticks.');
      let error: string | undefined;
      try { engine.tick(steps); } catch (failure) { error = message(failure); }
      const result = presentation(request.args[1] as PresentationOptions | undefined);
      if (error) result.error = error;
      return result;
    }
    if (request.method === 'captureCheckpoint') {
      // Neither call yields: bytes and metadata describe exactly the same tick.
      const capture: CapturedCheckpoint = { state: engine.saveState(), summary: engine.summary() };
      return capture;
    }
    if (!methods.has(request.method)) throw new Error('Unknown simulation worker command.');
    const call = engine[request.method as keyof Engine];
    if (typeof call !== 'function') throw new Error('Unavailable simulation worker command.');
    return Reflect.apply(call, engine, request.args);
  }

  return (value: unknown): void => {
    if (!value || typeof value !== 'object') return;
    const request = value as WorkerRequest;
    if (!Number.isSafeInteger(request.id) || request.id < 1 || typeof request.method !== 'string' || !Array.isArray(request.args)) return;
    // Catch within each queued item: a rejected command must not poison later work.
    queue = queue.then(async () => {
      try {
        const result = await dispatch(request);
        const transfer: Transferable[] = [];
        if (ArrayBuffer.isView(result)) transfer.push(result.buffer as ArrayBuffer);
        else if (result && typeof result === 'object') {
          const record = result as Partial<WirePresentation & CapturedCheckpoint> & { samples?: Float32Array };
          if (record.frame) transfer.push(record.frame.buffer as ArrayBuffer);
          if (record.metrics) transfer.push(record.metrics.buffer as ArrayBuffer);
          // metricsSnapshot() returns decoded lineage rows; presentation()
          // returns a transferable projection under the same field name.
          if (record.ecology instanceof Float64Array) transfer.push(record.ecology.buffer as ArrayBuffer);
          if (record.environment && 'samples' in record.environment) transfer.push(record.environment.samples.buffer as ArrayBuffer);
          if (record.state) transfer.push(record.state.buffer as ArrayBuffer);
          if (record.samples) transfer.push(record.samples.buffer as ArrayBuffer);
        }
        send({ id: request.id, ok: true, value: result }, transfer);
      } catch (error) {
        send({ id: request.id, ok: false, error: message(error) }, []);
      }
    });
  };
}
