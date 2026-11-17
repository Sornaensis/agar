import { createSimulationWorkerHandler } from './simulation-worker-runtime';
import type { WorkerResponse } from './simulation-protocol';

// The project includes DOM types for the UI; this narrow surface also describes
// a module worker without loading conflicting DOM/WebWorker declaration sets.
const scope = globalThis as unknown as {
  postMessage(message: WorkerResponse, transfer: Transferable[]): void;
  onmessage: ((event: MessageEvent<unknown>) => void) | null;
};
const handle = createSimulationWorkerHandler((response, transfer) => scope.postMessage(response, transfer));
scope.onmessage = event => handle(event.data);
