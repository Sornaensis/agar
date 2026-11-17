import { MAX_PROGRAM_BYTES, MAX_STATE_BYTES } from './engine';

export interface DocumentBase { id: string; name: string; createdAt: number; updatedAt: number }
export const GENOME_LANGUAGE_VERSION = 3;
export interface GenomeDocument extends DocumentBase { source: string; description?: string; languageVersion: number }
export interface EnvironmentConfig {
  foodRate: number; ambientTemperature: number; ambientLight: number; currentStrength: number; initialNutrients: number;
  sources: Array<{ kind: 0 | 1 | 2 | 3; x: number; y: number; strength: number; radius: number }>;
}
export interface EnvironmentDocument extends DocumentBase { config: EnvironmentConfig; description?: string }
export interface HistoryPoint { time: number; population: number; food: number; births: number; deaths: number }
export interface SimulationSummary extends HistoryPoint { generation: number }
export interface SimulationRecord extends DocumentBase {
  state: Uint8Array; environmentName: string; seed: number; paused: boolean; speed: 1 | 2 | 5 | 10;
  genomeNames: Record<string, string>; history: HistoryPoint[]; summary: SimulationSummary;
}
export type SimulationMetadata = Omit<SimulationRecord, 'state'>;
export interface UiPreferences {
  tab: 'simulation' | 'environments' | 'genomes';
  simulationView: 'player' | 'metrics' | 'settings' | 'genomes';
  activeSimulationId: string | null;
  overlay: 'none' | 'light' | 'heat' | 'flow' | 'nutrients';
  sensors: boolean; trails: boolean;
}
export interface GenomeDraft { name: string; source: string }
export interface EnvironmentDraft { name: string; config: EnvironmentConfig }

export const MAX_HISTORY_POINTS = 720;
export const MAX_LIBRARY_GENOMES = 1000;
export const MAX_LIBRARY_ENVIRONMENTS = 1000;
export const MAX_LIBRARY_SIMULATIONS = 100;
export const MAX_GENOME_IMPORT_BYTES = 2 * 1024 * 1024;
export const MAX_ENVIRONMENT_IMPORT_BYTES = 64 * 1024;
// Keep the storage namespace stable when the application is renamed.
const DATABASE_NAME = 'cellarium';
const DATABASE_VERSION = 1;
const PREFERENCES_KEY = 'cellarium.preferences.v1';
const GENOME_DRAFT_KEY = 'cellarium.genome-draft.v1';
const ENVIRONMENT_DRAFT_KEY = 'cellarium.environment-draft.v1';
const encoder = new TextEncoder();
const DEFAULT_PREFERENCES: UiPreferences = {
  tab: 'simulation', simulationView: 'player', activeSimulationId: null, overlay: 'none', sensors: true, trails: false,
};

function object(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${label} must be an object.`);
  return value as Record<string, unknown>;
}
function number(value: unknown, label: string, min: number, max: number, integer = false): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max || (integer && !Number.isInteger(value))) {
    throw new Error(`${label} must be ${integer ? 'an integer' : 'a number'} between ${min} and ${max}.`);
  }
  return value;
}
export function validateName(value: unknown): string {
  if (typeof value !== 'string' || !value.trim() || value.trim().length > 80 || /[\u0000-\u001f\u007f]/.test(value)) {
    throw new Error('Choose a name between 1 and 80 characters.');
  }
  return value.trim();
}
function description(value: unknown): string | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== 'string' || value.length > 500) throw new Error('Description must fit in 500 characters.');
  return value;
}
function source(value: unknown): string {
  if (typeof value !== 'string' || !value.trim() || encoder.encode(value).byteLength > MAX_PROGRAM_BYTES) {
    throw new Error('Genome source must be nonempty and fit in 256 KiB of UTF-8 text.');
  }
  return value;
}
function documentId(value: unknown, allowBuiltin = false): string {
  if (typeof value !== 'string' || !/^[A-Za-z0-9:_-]{1,160}$/.test(value) || (!allowBuiltin && value.startsWith('builtin:'))) {
    throw new Error('This saved item has an invalid identifier. Built-in items must be copied before saving.');
  }
  return value;
}
function base(value: Record<string, unknown>, allowBuiltin = false): DocumentBase {
  const createdAt = number(value.createdAt, 'Creation time', 0, Number.MAX_SAFE_INTEGER, true);
  return {
    id: documentId(value.id, allowBuiltin), name: validateName(value.name), createdAt,
    updatedAt: number(value.updatedAt, 'Update time', createdAt, Number.MAX_SAFE_INTEGER, true),
  };
}
export function validateEnvironmentConfig(value: unknown): EnvironmentConfig {
  const config = object(value, 'Environment');
  if (!Array.isArray(config.sources) || config.sources.length > 16) throw new Error('An environment can contain at most 16 sources.');
  return {
    foodRate: number(config.foodRate, 'External organic input', 0, 60),
    ambientTemperature: number(config.ambientTemperature, 'Temperature', 0, 1),
    ambientLight: number(config.ambientLight, 'Light', 0, 1),
    currentStrength: number(config.currentStrength, 'Current', 0, 30),
    initialNutrients: number(config.initialNutrients, 'Starting nutrients', 0, 5),
    sources: config.sources.map((value, index) => {
      const item = object(value, `Source ${index + 1}`);
      const kind = number(item.kind, 'Source kind', 0, 3, true) as 0 | 1 | 2 | 3;
      return {
        kind, x: number(item.x, 'Source x', 0, 1200 - Number.EPSILON * 1200),
        y: number(item.y, 'Source y', 0, 760 - Number.EPSILON * 760),
        strength: number(item.strength, 'Source strength', kind === 0 || kind === 3 ? 0 : -1, 1),
        radius: number(item.radius, 'Source radius', 40, 400),
      };
    }),
  };
}
export function validateGenomeDocument(value: unknown, allowBuiltin = false): GenomeDocument {
  const item = object(value, 'Genome');
  return { ...base(item, allowBuiltin), source: source(item.source), description: description(item.description),
    languageVersion: item.languageVersion === undefined ? 1 : number(item.languageVersion, 'Genome language version', 1, GENOME_LANGUAGE_VERSION, true) };
}
export function validateEnvironmentDocument(value: unknown, allowBuiltin = false): EnvironmentDocument {
  const item = object(value, 'Environment');
  return { ...base(item, allowBuiltin), config: validateEnvironmentConfig(item.config), description: description(item.description) };
}
function historyPoint(value: unknown): HistoryPoint {
  const item = object(value, 'History point');
  return {
    time: number(item.time, 'Simulation time', 0, Number.MAX_SAFE_INTEGER),
    population: number(item.population, 'Population', 0, Number.MAX_SAFE_INTEGER, true),
    food: number(item.food, 'Organic particle count', 0, Number.MAX_SAFE_INTEGER, true),
    births: number(item.births, 'Births', 0, Number.MAX_SAFE_INTEGER, true),
    deaths: number(item.deaths, 'Deaths', 0, Number.MAX_SAFE_INTEGER, true),
  };
}
function validateSimulationMetadata(value: unknown): SimulationMetadata {
  const item = object(value, 'Simulation');
  if (typeof item.paused !== 'boolean') throw new Error('Simulation playback state is invalid.');
  if (![1, 2, 5, 10].includes(item.speed as number)) throw new Error('Simulation speed must be 1, 2, 5, or 10.');
  if (!Array.isArray(item.history) || item.history.length > MAX_HISTORY_POINTS) throw new Error(`History is limited to ${MAX_HISTORY_POINTS} points.`);
  const history = item.history.map(historyPoint);
  if (history.some((point, index) => index > 0 && point.time < history[index - 1].time)) throw new Error('Simulation history must be in time order.');
  const names = object(item.genomeNames, 'Genome names');
  if (Object.keys(names).length > 2048) throw new Error('The simulation contains too many genome names.');
  const genomeNames: Record<string, string> = {};
  for (const [id, name] of Object.entries(names)) {
    if (!/^[1-9][0-9]{0,3}$/.test(id) || Number(id) > 2048) throw new Error('A simulation genome identifier is invalid.');
    genomeNames[id] = validateName(name);
  }
  const summary = object(item.summary, 'Simulation summary');
  const checkedSummary = { ...historyPoint(summary), generation: number(summary.generation, 'Generation', 0, Number.MAX_SAFE_INTEGER, true) };
  if (history.length && history[history.length - 1].time > checkedSummary.time) throw new Error('Simulation history extends past the saved simulation.');
  return {
    ...base(item), environmentName: validateName(item.environmentName), seed: number(item.seed, 'Seed', 0, 0xffffffff, true),
    paused: item.paused, speed: item.speed as SimulationRecord['speed'], genomeNames, history, summary: checkedSummary,
  };
}
export function validateSimulationRecord(value: unknown): SimulationRecord {
  const item = object(value, 'Simulation');
  if (!(item.state instanceof Uint8Array) || item.state.byteLength === 0 || item.state.byteLength > MAX_STATE_BYTES) {
    throw new Error('The simulation checkpoint is missing or exceeds the 128 MiB limit.');
  }
  return { ...validateSimulationMetadata(item), state: item.state };
}
export function newGenome(name: string, code: string, detail?: string): GenomeDocument {
  const now = Date.now();
  return validateGenomeDocument({ id: `genome:${crypto.randomUUID()}`, name, source: code, description: detail, languageVersion: GENOME_LANGUAGE_VERSION, createdAt: now, updatedAt: now });
}
export function newEnvironment(name: string, config: EnvironmentConfig, detail?: string): EnvironmentDocument {
  const now = Date.now();
  return validateEnvironmentDocument({ id: `environment:${crypto.randomUUID()}`, name, config, description: detail, createdAt: now, updatedAt: now });
}
function parseImport(text: string, format: string, limit: number): Record<string, unknown> {
  if (encoder.encode(text).byteLength > limit) throw new Error('This import file is too large.');
  let parsed: unknown;
  try { parsed = JSON.parse(text); } catch { throw new Error('Choose a valid agar JSON export.'); }
  const item = object(parsed, 'Import');
  const genome = format === 'agar.genome';
  if ((item.format !== format && item.format !== format.replace('agar.', 'cellarium.')) || item.version !== (genome ? 2 : 1)) {
    if (genome && item.version === 1) throw new Error('This genome uses the older investment language. Its file is preserved; create a new organelle genome or manually update a copy.');
    throw new Error(`Expected a version ${genome ? 2 : 1} ${format} file.`);
  }
  if (genome && item.languageVersion !== GENOME_LANGUAGE_VERSION) throw new Error('Unsupported genome language version. This build uses organelle language 3.');
  const allowed = format === 'agar.genome' ? ['format', 'version', 'languageVersion', 'name', 'source', 'description'] : ['format', 'version', 'name', 'config', 'description'];
  if (Object.keys(item).some(key => !allowed.includes(key))) throw new Error('This import contains unsupported fields.');
  return item;
}
export function parseGenomeImport(text: string): GenomeDocument {
  const item = parseImport(text, 'agar.genome', MAX_GENOME_IMPORT_BYTES);
  return newGenome(validateName(item.name), source(item.source), description(item.description));
}
export function parseEnvironmentImport(text: string): EnvironmentDocument {
  const item = parseImport(text, 'agar.environment', MAX_ENVIRONMENT_IMPORT_BYTES);
  return newEnvironment(validateName(item.name), validateEnvironmentConfig(item.config), description(item.description));
}
export function exportGenome(value: GenomeDocument): string {
  const item = validateGenomeDocument(value, true);
  return JSON.stringify({ format: 'agar.genome', version: item.languageVersion === GENOME_LANGUAGE_VERSION ? 2 : 1,
    ...(item.languageVersion === GENOME_LANGUAGE_VERSION ? { languageVersion: item.languageVersion } : {}),
    name: item.name, source: item.source, description: item.description }, null, 2);
}
export function exportEnvironment(value: EnvironmentDocument): string {
  const item = validateEnvironmentDocument(value, true);
  return JSON.stringify({ format: 'agar.environment', version: 1, name: item.name, config: item.config, description: item.description }, null, 2);
}

function storageError(error: unknown): Error {
  if (error instanceof DOMException && error.name === 'QuotaExceededError') return new Error('Browser storage is full. Export or remove saved items, then try again.');
  if (error instanceof Error) return new Error(`Could not save or load data: ${error.message}`);
  return new Error('Could not save or load data. Check that browser storage is available.');
}

/** IndexedDB owns documents and full binary checkpoints. Success means the transaction committed. */
export class Storage {
  private closed = false;
  private constructor(private readonly database: IDBDatabase) {
    database.onversionchange = () => { this.closed = true; database.close(); };
    database.onclose = () => { this.closed = true; };
  }
  static open(): Promise<Storage> {
    return new Promise((resolve, reject) => {
      if (typeof indexedDB === 'undefined') { reject(new Error('This browser does not support saved simulations.')); return; }
      let settled = false;
      let request: IDBOpenDBRequest;
      try { request = indexedDB.open(DATABASE_NAME, DATABASE_VERSION); } catch (error) { reject(storageError(error)); return; }
      request.onupgradeneeded = () => {
        for (const name of ['genomes', 'environments', 'simulations', 'simulationMetadata']) {
          if (!request.result.objectStoreNames.contains(name)) request.result.createObjectStore(name, { keyPath: 'id' });
        }
      };
      request.onerror = () => { settled = true; reject(storageError(request.error)); };
      request.onblocked = () => { settled = true; reject(new Error('Close other agar tabs, then reload to open saved data.')); };
      request.onsuccess = () => {
        if (settled) { request.result.close(); return; }
        settled = true; resolve(new Storage(request.result));
      };
    });
  }
  close(): void { this.closed = true; this.database.close(); }
  private transaction<T>(names: string[], mode: IDBTransactionMode, operation: (tx: IDBTransaction, result: (value: T) => void, fail: (error: unknown) => void) => void): Promise<T> {
    return new Promise((resolve, reject) => {
      if (this.closed) { reject(new Error('Saved data was closed by another tab. Reload agar.')); return; }
      let transaction: IDBTransaction;
      try { transaction = this.database.transaction(names, mode); } catch (error) { reject(storageError(error)); return; }
      let result: T;
      let failure: unknown;
      transaction.oncomplete = () => resolve(result);
      transaction.onerror = () => { failure ||= transaction.error; };
      transaction.onabort = () => reject(storageError(failure || transaction.error));
      const fail = (error: unknown) => { failure = error; transaction.abort(); };
      try { operation(transaction, value => { result = value; }, fail); }
      catch (error) { fail(error); }
    });
  }
  private list<T>(name: string, validate: (value: unknown) => T): Promise<T[]> {
    return this.transaction([name], 'readonly', (tx, result, fail) => {
      const request = tx.objectStore(name).getAll();
      request.onsuccess = () => {
        try { result(request.result.map(value => validate(value))); }
        catch (error) { fail(error); /* Reject corrupted rows instead of silently dropping saved data. */ }
      };
    });
  }
  private put<T extends DocumentBase>(name: string, value: T, limit: number, metadata?: SimulationMetadata): Promise<void> {
    return this.transaction(metadata ? [name, 'simulationMetadata'] : [name], 'readwrite', (tx, result, fail) => {
      const store = tx.objectStore(name);
      // Clone at this call boundary. The count follows this put in the same transaction;
      // exceeding the limit aborts both the item and metadata update atomically.
      store.put(value);
      if (metadata) tx.objectStore('simulationMetadata').put(metadata);
      const count = store.count();
      count.onsuccess = () => {
        if (count.result > limit) { fail(new Error(`The limit is ${limit} saved ${name}. Remove an item before creating another.`)); return; }
        result(undefined);
      };
    });
  }
  private remove(name: string, id: string, withMetadata = false): Promise<void> {
    documentId(id);
    return this.transaction(withMetadata ? [name, 'simulationMetadata'] : [name], 'readwrite', (tx, result) => {
      tx.objectStore(name).delete(id);
      if (withMetadata) tx.objectStore('simulationMetadata').delete(id);
      result(undefined);
    });
  }
  private rename<T extends DocumentBase>(storeName: string, id: string, name: string, validate: (value: unknown) => T): Promise<T> {
    documentId(id);
    const nextName = validateName(name);
    return this.transaction([storeName], 'readwrite', (tx, result, fail) => {
      const store = tx.objectStore(storeName), request = store.get(id);
      request.onsuccess = () => {
        try {
          if (request.result === undefined) throw new Error('This item no longer exists. Refresh the list before trying again.');
          const current = validate(request.result);
          const next = { ...current, name: nextName, updatedAt: Math.max(Date.now(), current.updatedAt) };
          store.put(next);
          result(next);
        } catch (error) { fail(error); }
      };
    });
  }
  listGenomes(): Promise<GenomeDocument[]> { return this.list('genomes', validateGenomeDocument); }
  putGenome(value: GenomeDocument): Promise<void> { return this.put('genomes', validateGenomeDocument(value), MAX_LIBRARY_GENOMES); }
  renameGenome(id: string, name: string): Promise<GenomeDocument> { return this.rename('genomes', id, name, validateGenomeDocument); }
  deleteGenome(id: string): Promise<void> { return this.remove('genomes', id); }
  listEnvironments(): Promise<EnvironmentDocument[]> { return this.list('environments', validateEnvironmentDocument); }
  putEnvironment(value: EnvironmentDocument): Promise<void> { return this.put('environments', validateEnvironmentDocument(value), MAX_LIBRARY_ENVIRONMENTS); }
  renameEnvironment(id: string, name: string): Promise<EnvironmentDocument> { return this.rename('environments', id, name, validateEnvironmentDocument); }
  deleteEnvironment(id: string): Promise<void> { return this.remove('environments', id); }
  listSimulations(): Promise<SimulationRecord[]> { return this.list('simulations', validateSimulationRecord); }
  listSimulationSummaries(): Promise<SimulationMetadata[]> { return this.list('simulationMetadata', validateSimulationMetadata); }
  getSimulation(id: string): Promise<SimulationRecord | null> {
    documentId(id);
    return this.transaction(['simulations'], 'readonly', (tx, result, fail) => {
      const request = tx.objectStore('simulations').get(id);
      request.onsuccess = () => {
        try { result(request.result === undefined ? null : validateSimulationRecord(request.result)); }
        catch (error) { fail(error); }
      };
    });
  }
  putSimulation(value: SimulationRecord): Promise<void> {
    const checked = validateSimulationRecord(value);
    const { state: _state, ...metadata } = checked;
    return this.put('simulations', checked, MAX_LIBRARY_SIMULATIONS, metadata);
  }
  deleteSimulation(id: string): Promise<void> { return this.remove('simulations', id, true); }
}

function localRead(key: string): unknown {
  let text: string | null;
  try { text = localStorage.getItem(key); } catch (error) { throw storageError(error); }
  if (!text) return null;
  try { return JSON.parse(text); } catch { return null; }
}
function localWrite(key: string, value: unknown): void {
  try { localStorage.setItem(key, JSON.stringify(value)); } catch (error) { throw storageError(error); }
}
function localClear(key: string): void {
  try { localStorage.removeItem(key); } catch (error) { throw storageError(error); }
}
export function loadPreferences(): UiPreferences {
  const value = localRead(PREFERENCES_KEY);
  if (!value || typeof value !== 'object') return { ...DEFAULT_PREFERENCES };
  const saved = value as Partial<UiPreferences>;
  return {
    tab: ['simulation', 'environments', 'genomes'].includes(saved.tab ?? '') ? saved.tab! : DEFAULT_PREFERENCES.tab,
    simulationView: ['player', 'metrics', 'settings', 'genomes'].includes(saved.simulationView ?? '') ? saved.simulationView! : DEFAULT_PREFERENCES.simulationView,
    activeSimulationId: typeof saved.activeSimulationId === 'string' && /^[A-Za-z0-9:_-]{1,160}$/.test(saved.activeSimulationId) && !saved.activeSimulationId.startsWith('builtin:') ? saved.activeSimulationId : null,
    overlay: ['none', 'light', 'heat', 'flow', 'nutrients'].includes(saved.overlay ?? '') ? saved.overlay! : 'none',
    sensors: typeof saved.sensors === 'boolean' ? saved.sensors : true,
    trails: typeof saved.trails === 'boolean' ? saved.trails : false,
  };
}
export function savePreferences(value: UiPreferences): void { localWrite(PREFERENCES_KEY, value); }
export function loadGenomeDraft(): GenomeDraft | null {
  const value = localRead(GENOME_DRAFT_KEY);
  if (!value || typeof value !== 'object') return null;
  const draft = value as Partial<GenomeDraft>;
  return typeof draft.name === 'string' && draft.name.length <= 80 && typeof draft.source === 'string' && encoder.encode(draft.source).byteLength <= MAX_PROGRAM_BYTES
    ? { name: draft.name, source: draft.source } : null;
}
export function saveGenomeDraft(value: GenomeDraft): void {
  if (typeof value.name !== 'string' || value.name.length > 80 || typeof value.source !== 'string' || encoder.encode(value.source).byteLength > MAX_PROGRAM_BYTES) throw new Error('This genome draft is too large to save.');
  localWrite(GENOME_DRAFT_KEY, value);
}
export function clearGenomeDraft(): void { localClear(GENOME_DRAFT_KEY); }
export function loadEnvironmentDraft(): EnvironmentDraft | null {
  const value = localRead(ENVIRONMENT_DRAFT_KEY);
  if (!value || typeof value !== 'object') return null;
  const draft = value as Partial<EnvironmentDraft>;
  if (typeof draft.name !== 'string' || draft.name.length > 80) return null;
  try { return { name: draft.name, config: validateEnvironmentConfig(draft.config) }; } catch { return null; }
}
export function saveEnvironmentDraft(value: EnvironmentDraft): void {
  if (typeof value.name !== 'string' || value.name.length > 80) throw new Error('This environment name is too long.');
  localWrite(ENVIRONMENT_DRAFT_KEY, { name: value.name, config: validateEnvironmentConfig(value.config) });
}
export function clearEnvironmentDraft(): void { localClear(ENVIRONMENT_DRAFT_KEY); }
