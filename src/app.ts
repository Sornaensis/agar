import './style.css';
import { Engine, type Cell, type WorldSnapshot } from './engine';
import { Renderer, genomeColor } from './renderer';
import { GenomeVisualizer } from './genome-visualizer';
import { BUILTIN_GENOMES, BUILTIN_ENVIRONMENTS, BUILTIN_SIMULATIONS, isBuiltin, type BuiltinSimulation } from './catalog';
import { Storage, loadPreferences, savePreferences, newGenome, validateName, parseGenomeImport, parseEnvironmentImport,
  exportGenome, exportEnvironment, GENOME_LANGUAGE_VERSION, MAX_GENOME_IMPORT_BYTES, MAX_ENVIRONMENT_IMPORT_BYTES,
  type GenomeDocument, type EnvironmentDocument, type EnvironmentConfig, type SimulationMetadata, type UiPreferences } from './storage';
import { CatalogUI } from './catalog-ui';
import { ProgramEditor } from './program-editor';
import { SimulationSession, claimSimulation, summarize } from './session';
import { icon, escapeHtml as esc, createDialog, setDialogError, downloadFile, inputNumber, clock, readableError } from './ui';
import { showGuide } from './guide';
import { ORGANELLE_CATALOG } from './organelles';

const $ = <T extends HTMLElement = HTMLElement>(id: string): T => document.getElementById(id) as T;
let preferenceWarning: unknown;
let prefs: UiPreferences;
try { prefs = loadPreferences(); }
catch (error) {
  preferenceWarning = error;
  prefs = { tab: 'simulation', simulationView: 'player', activeSimulationId: null, overlay: 'none', sensors: true, trails: false };
}
let storage: Storage;
let savedGenomes: GenomeDocument[] = [], savedEnvironments: EnvironmentDocument[] = [], simulations: SimulationMetadata[] = [];
let active: SimulationSession | null = null, renderer: Renderer | null = null;
let inspectorProgram: ProgramEditor | null = null;
const expandedInspections = new Set<{ session: SimulationSession; cellId: number; editor: ProgramEditor }>();
const inspectedRevisions = new WeakMap<ProgramEditor, { session: SimulationSession; cellId: number; revision: number; tick: number }>();
let ready = false, transitioning = false, saveFailed = false;
let lifecycleFlushes = 0;
let selectedId: number | null = null;
let inspectorView: 'overview' | 'body' | 'memory' | 'program' = 'overview';
let memoryFilter = '', memoryPage = 0, catalogQuery = '', simulationOpenError = '';
let lastFrame = 0, lastUi = 0, accumulator = 0, toastTimer = 0;
const genomeVisualizer = new GenomeVisualizer();
const compiler = (): Promise<Engine> => Engine.load();
const saveIssues = new Map<string, unknown>();
const pendingSaves = new Set<string>();
const libraryChannel = typeof BroadcastChannel === 'function' ? new BroadcastChannel('cellarium:catalog') : null;
const allGenomes = (): GenomeDocument[] => [...savedGenomes, ...BUILTIN_GENOMES];
const allEnvironments = (): EnvironmentDocument[] => [...savedEnvironments, ...BUILTIN_ENVIRONMENTS];
const findGenome = (id: string): GenomeDocument | undefined => allGenomes().find(item => item.id === id);
const findEnvironment = (id: string): EnvironmentDocument | undefined => allEnvironments().find(item => item.id === id);

document.querySelector('#app')!.innerHTML = `
<div class="app-shell"><header class="app-header">
  <button class="brand" aria-label="agar simulations" data-action="home"><span class="brand-symbol">${icon('cell')}</span><span>agar</span></button>
  <nav class="primary-tabs" aria-label="Sections" role="tablist">
    <button role="tab" data-tab="simulation">Simulation</button><button role="tab" data-tab="environments">Environments</button><button role="tab" data-tab="genomes">Genomes</button>
  </nav><div class="header-actions"><span id="save-state" class="save-indicator" role="status" aria-live="polite">Loading…</span><button id="save-retry" class="icon-button" aria-label="Retry save" title="Retry save" data-action="save" hidden>${icon('refresh')}</button><button class="icon-button help-button" aria-label="Guide" title="Guide" data-action="guide">${icon('help')}</button></div>
</header><main class="workspace" id="workspace"><div class="empty-state">Opening agar…</div></main></div>
<div id="toast" class="toast" role="status" aria-live="polite"></div><input id="import-file" type="file" accept=".json,.cell" hidden>`;

function toast(message: string): void {
  $('toast').textContent = message; $('toast').classList.add('visible');
  window.clearTimeout(toastTimer); toastTimer = window.setTimeout(() => $('toast').classList.remove('visible'), 4500);
}
function saveStatus(state: 'saving' | 'saved' | 'error', error?: unknown, scope = 'simulation'): void {
  if (state === 'saving') { pendingSaves.add(scope); saveIssues.delete(scope); }
  else pendingSaves.delete(scope);
  if (state === 'error') {
    saveIssues.set(scope, error);
    if (scope === 'simulation' && active) { active.metadata.paused = true; active.touch(); updatePlayback(); }
    toast(readableError(error));
  }
  if (state === 'saved') saveIssues.delete(scope);
  saveFailed = saveIssues.has('simulation');
  $('save-state').classList.toggle('error', saveIssues.size > 0);
  $('save-state').classList.toggle('saving', pendingSaves.size > 0);
  $('save-state').textContent = saveIssues.size ? 'Not saved' : pendingSaves.size ? 'Saving…' : 'Saved';
  $('save-retry').hidden = !saveFailed;
  $('save-state').title = saveFailed ? readableError(saveIssues.get('simulation')) + ' Use Retry save.'
    : saveIssues.size ? 'Retry the failed save in its editor.' : pendingSaves.size ? 'Saving changes' : 'Changes saved';
}
function persistPreferences(): void { try { savePreferences(prefs); } catch (error) { toast(readableError(error)); } }
function upsertMetadata(metadata: SimulationMetadata): void {
  simulations = [metadata, ...simulations.filter(item => item.id !== metadata.id)].sort((a, b) => b.updatedAt - a.updatedAt);
}
let catalogRefreshSerial = 0;
async function refreshCatalogs(render = true): Promise<void> {
  const serial = ++catalogRefreshSerial;
  const [genomes, environments, runs] = await Promise.all([storage.listGenomes(), storage.listEnvironments(), storage.listSimulationSummaries()]);
  if (serial !== catalogRefreshSerial) return;
  savedGenomes = genomes; savedEnvironments = environments; simulations = runs;
  if (!render || !ready || transitioning) return;
  if (prefs.tab !== 'simulation') renderCatalogCards();
  else if (!active) renderView();
}
async function persistGenome(document: GenomeDocument): Promise<void> {
  saveStatus('saving', undefined, 'catalog');
  try { await storage.putGenome(document); savedGenomes = await storage.listGenomes(); saveStatus('saved', undefined, 'catalog'); libraryChannel?.postMessage('changed'); if (prefs.tab === 'genomes') renderView(); }
  catch (error) { saveStatus('error', error, 'catalog'); throw error; }
}
async function persistEnvironment(document: EnvironmentDocument): Promise<void> {
  saveStatus('saving', undefined, 'catalog');
  try { await storage.putEnvironment(document); savedEnvironments = await storage.listEnvironments(); saveStatus('saved', undefined, 'catalog'); libraryChannel?.postMessage('changed'); if (prefs.tab === 'environments') renderView(); }
  catch (error) { saveStatus('error', error, 'catalog'); throw error; }
}
async function graphSource(source: string, name: string): Promise<void> {
  const engine = await compiler(), id = engine.compile(source);
  genomeVisualizer.open(engine.getGenomeGraph(id), name);
}
const catalog = new CatalogUI({ compiler, saveGenome: persistGenome, saveEnvironment: persistEnvironment,
  graph: graphSource, startSimulation: id => newSimulationDialog(undefined, id), notify: toast });

function menu(kind: 'genome' | 'environment' | 'simulation', id: string, name: string, builtin = false): string {
  return `<details class="item-menu"><summary aria-label="Actions for ${esc(name)}" title="Actions">${icon('more')}</summary><div class="menu-popover">
    ${kind !== 'simulation' ? `<button data-action="copy-${kind}" data-id="${esc(id)}">${icon('copy')}Edit a copy</button><button data-action="export-${kind}" data-id="${esc(id)}">${icon('download')}Export</button>` : ''}
    ${!builtin ? `<button data-action="rename-${kind}" data-id="${esc(id)}">Rename</button><button class="danger" data-action="delete-${kind}" data-id="${esc(id)}">Delete</button>` : ''}
  </div></details>`;
}
function environmentThumb(environment: EnvironmentDocument): string {
  const colors = ['#e8c578', '#ef9380', '#84b7ed', '#7cdbad'];
  return `<svg class="environment-thumb" viewBox="0 0 1200 760" aria-hidden="true"><rect width="1200" height="760" fill="#101d22"/>${environment.config.sources.map(source =>
    `<circle cx="${source.x}" cy="${source.y}" r="${source.radius}" fill="${colors[source.kind]}" opacity=".08"/><circle cx="${source.x}" cy="${source.y}" r="10" fill="${colors[source.kind]}"/>`).join('')}</svg>`;
}
function renderSimulationList(): string {
  return `<div class="page library-page"><header class="page-header"><h1>Simulations</h1><button class="button primary" data-action="new-simulation">${icon('plus')}New simulation</button></header>
    ${simulationOpenError ? `<p class="compatibility-notice" role="alert">${esc(simulationOpenError)}${simulationOpenError.includes('preserved') ? '' : ' Your saved simulation remains unchanged.'}</p>` : ''}
    ${simulations.length ? `<section class="catalog-section"><h2 class="catalog-section-heading">Saved</h2><div class="catalog-grid simulation-grid">${simulations.map(item => `<article class="catalog-card simulation-card"><button class="card-open" data-action="open-simulation" data-id="${esc(item.id)}" aria-label="Open simulation ${esc(item.name)}"><span class="card-identity">${icon('cell')}</span><span class="card-copy"><span class="card-title">${esc(item.name)}</span><span class="card-description">${esc(item.environmentName)}</span><span class="simulation-card-stats"><span>${item.summary.population} ${item.summary.population===1?'organism':'organisms'}</span><span>${clock(item.summary.time)}</span></span></span><span class="card-chevron">${icon('chevron')}</span></button>${menu('simulation',item.id,item.name)}</article>`).join('')}</div></section>` : ''}
    <section class="catalog-section"><h2 class="catalog-section-heading">Start from a preset</h2><div class="catalog-grid preset-grid">${BUILTIN_SIMULATIONS.map(item => `<article class="catalog-card preset-card"><button class="card-open" data-action="start-template" data-id="${item.id}" aria-label="Start ${esc(item.name)}"><span class="card-identity">${icon('play')}</span><span class="card-copy"><span class="card-title">${esc(item.name)}</span><span class="card-description">${esc(item.description)}</span></span><span class="card-chevron">${icon('chevron')}</span></button></article>`).join('')}</div></section></div>`;
}
function renderCatalogCards(): void {
  const target = $('catalog-items'); if (!target) return;
  const isGenome = prefs.tab === 'genomes';
  const items = (isGenome ? allGenomes() : allEnvironments()).filter(item => (item.name + ' ' + (item.description || '')).toLowerCase().includes(catalogQuery.toLowerCase()));
  const card = (item: GenomeDocument | EnvironmentDocument): string => {
    const kind = isGenome ? 'genome' : 'environment';
    return `<article class="catalog-card ${kind}-card"><button class="card-open" data-action="open-${kind}" data-id="${esc(item.id)}" aria-label="Open ${kind} ${esc(item.name)}">${isGenome ? `<span class="card-identity">${icon('dna')}</span>` : environmentThumb(item as EnvironmentDocument)}<span class="card-copy"><span class="card-title">${esc(item.name)}</span><span class="card-description">${esc(isGenome && (item as GenomeDocument).languageVersion !== GENOME_LANGUAGE_VERSION ? 'Older language · view or export the preserved source' : item.description || (isGenome ? '' : `${(item as EnvironmentDocument).config.sources.length} sources · ${(item as EnvironmentDocument).config.foodRate} organic particles / s`))}</span></span><span class="card-chevron">${icon('chevron')}</span></button>${menu(kind,item.id,item.name,isBuiltin(item.id))}</article>`;
  };
  target.innerHTML = [false,true].map(builtin => {
    const group = items.filter(item => isBuiltin(item.id) === builtin);
    return group.length ? `<section class="catalog-section"><h2 class="catalog-section-heading">${builtin?'Built-in':'Saved'}</h2><div class="catalog-grid ${isGenome?'genome':'environment'}-grid">${group.map(card).join('')}</div></section>` : '';
  }).join('') || '<div class="empty-state">No matches. Try a different search.</div>';
}
function renderCatalog(): string {
  const genome = prefs.tab === 'genomes', kind = genome ? 'genome' : 'environment';
  return `<div class="page library-page"><header class="page-header"><h1>${genome ? 'Genomes' : 'Environments'}</h1><div class="toolbar"><label class="search-field">${icon('search')}<input class="search-input" id="catalog-search" type="search" placeholder="Search" aria-label="Search ${genome ? 'genomes' : 'environments'}" value="${esc(catalogQuery)}"></label><button class="icon-button import-button" aria-label="Import ${kind}" title="Import ${kind}" data-action="import-${kind}">${icon('upload')}</button><button class="button primary" data-action="new-${kind}">${icon('plus')}New ${kind}</button></div></header><div id="catalog-items"></div></div>`;
}
function renderView(): void {
  inspectorProgram?.destroy(); inspectorProgram = null;
  renderer?.dispose(); renderer = null;
  for (const button of document.querySelectorAll<HTMLButtonElement>('[data-tab]')) {
    const selected = button.dataset.tab === prefs.tab;
    button.classList.toggle('active', selected); button.setAttribute('aria-selected', String(selected)); button.tabIndex = selected ? 0 : -1;
  }
  if (!ready) return;
  if (prefs.tab !== 'simulation') { $('workspace').innerHTML = renderCatalog(); renderCatalogCards(); return; }
  if (!active) { $('workspace').innerHTML = renderSimulationList(); return; }
  $('workspace').innerHTML = `<div class="simulation-shell"><header class="simulation-bar"><div class="simulation-title"><button class="icon-button" data-action="close-simulation" aria-label="All simulations">${icon('back')}</button><h1>${esc(active.metadata.name)}</h1></div>
    <nav class="simulation-tabs" role="tablist" aria-label="Simulation views">${(['player', 'metrics', 'settings', 'genomes'] as const).map(view => `<button role="tab" data-view="${view}" tabindex="${prefs.simulationView === view ? 0 : -1}" aria-selected="${prefs.simulationView === view}" class="${prefs.simulationView === view ? 'active' : ''}">${view[0].toUpperCase() + view.slice(1)}</button>`).join('')}</nav>
    </header><div class="simulation-content">${prefs.simulationView === 'player' ? playerMarkup() : prefs.simulationView === 'metrics' ? metricsMarkup() : prefs.simulationView === 'settings' ? settingsMarkup() : '<div class="page active-genomes-page"><div class="genome-list" id="active-genomes"></div></div>'}</div></div>`;
  if (prefs.simulationView === 'player') {
    const canvas = $<HTMLCanvasElement>('world'); renderer = new Renderer(canvas);
    canvas.addEventListener('click', event => {
      if (!active || !renderer || transitioning) return;
      selectedId = renderer.pickCell(active.world, event.clientX, event.clientY)?.id ?? null;
      inspectorView = 'overview'; memoryPage = 0; memoryFilter = ''; renderInspector();
    });
    canvas.addEventListener('keydown', event => {
      if (!active || transitioning) return;
      if (event.code === 'Space') { event.preventDefault(); void togglePlayback(); }
      else if (event.key === 'Escape') { selectedId = null; renderInspector(); }
      else if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
        event.preventDefault();
        const cells = active.world.cells, index = cells.findIndex(cell => cell.id === selectedId), direction = event.key === 'ArrowRight' ? 1 : -1;
        selectedId = cells.length ? cells[index < 0 ? (direction > 0 ? 0 : cells.length - 1) : (index + direction + cells.length) % cells.length].id : null; renderInspector();
      }
    });
    renderInspector(); updatePlayback(); renderer.render(active.world, selectedId, prefs);
  }
  updateSimulationUi();
}
async function navigate(tab: UiPreferences['tab']): Promise<void> {
  if (!ready || transitioning) return;
  transitioning = true;
  try { if (active) await active.flush(); await refreshCatalogs(false); prefs.tab = tab; catalogQuery = ''; accumulator = 0; persistPreferences(); renderView(); }
  catch (error) { toast(readableError(error)); } finally { transitioning = false; }
}
function applyEnvironment(engine: Engine, config: EnvironmentConfig): void {
  engine.setEnvironment(config.ambientTemperature, config.ambientLight, config.currentStrength);
  engine.setInitialNutrients(config.initialNutrients); engine.setFoodRate(config.foodRate); engine.clearSources();
  for (const source of config.sources) if (!engine.addSource(source.kind, source.x, source.y, source.strength, source.radius)) throw new Error('Could not create an environment source.');
}
function makeSession(engine: Engine, metadata: SimulationMetadata): SimulationSession {
  return new SimulationSession(engine, metadata, storage, saveStatus, upsertMetadata);
}
async function openSimulation(id: string): Promise<void> {
  if (transitioning) return;
  if (active?.metadata.id === id) { prefs.tab = 'simulation'; renderView(); return; }
  transitioning = true; let release: (() => void) | null = null;
  try {
    release = await claimSimulation(id);
    const record = await storage.getSimulation(id); if (!record) throw new Error('This simulation no longer exists.');
    const engine = await Engine.load(); engine.restoreState(record.state);
    if (active) await active.flush();
    const { state: _state, ...metadata } = record; metadata.paused = true;
    const next = makeSession(engine, metadata); next.release = release;
    active?.release(); active = next; release = null;
    prefs.activeSimulationId = id; prefs.tab = 'simulation'; prefs.simulationView = 'player';
    selectedId = null; accumulator = 0; persistPreferences(); renderView();
  } catch (error) {
    release?.();
    if (!active) { simulationOpenError = readableError(error); prefs.activeSimulationId = null; persistPreferences(); renderView(); }
    else toast(readableError(error));
  } finally { transitioning = false; }
}
async function closeSimulation(): Promise<void> {
  if (!active || transitioning) return;
  transitioning = true;
  try {
    active.metadata.paused = true; active.touch(); await active.flush(); active.release(); active = null;
    selectedId = null; prefs.activeSimulationId = null; simulations = await storage.listSimulationSummaries(); persistPreferences(); renderView();
  } catch (error) { toast(readableError(error)); } finally { transitioning = false; }
}
function genomeOptions(selected?: string): string {
  return allGenomes().map(genome => `<option value="${esc(genome.id)}"${selected === genome.id ? ' selected' : ''}${genome.languageVersion !== GENOME_LANGUAGE_VERSION ? ' disabled' : ''}>${esc(genome.name)}${genome.languageVersion !== GENOME_LANGUAGE_VERSION ? ' · older language' : isBuiltin(genome.id) ? '' : ' · saved'}</option>`).join('');
}
function newSimulationDialog(template?: BuiltinSimulation, environmentId?: string): void {
  if (!ready || transitioning || document.querySelector('.new-simulation-dialog[open]')) return;
  const populations = template?.populations.map(item => ({ ...item })) ?? [{ genomeId: BUILTIN_GENOMES[0].id, count: 22 }];
  const chosenEnvironment = environmentId || template?.environmentId || BUILTIN_ENVIRONMENTS[0].id;
  const dialog = createDialog('New simulation', `<form>
    <div class="form-field"><label for="simulation-name">Name</label><input id="simulation-name" maxlength="80" required value="${esc(template?.name || 'New simulation')}"></div>
    <div class="form-row"><div class="form-field"><label for="simulation-environment">Environment</label><select id="simulation-environment">${allEnvironments().map(item => `<option value="${esc(item.id)}"${item.id === chosenEnvironment ? ' selected' : ''}>${esc(item.name)}</option>`).join('')}</select></div><div class="form-field"><label for="simulation-seed">Seed</label><input id="simulation-seed" type="number" min="0" max="4294967295" step="1" value="${template?.seed ?? 42}"></div></div>
    <h3>Starting organisms</h3><div class="population-labels" aria-hidden="true"><span>Genome</span><span>Count</span></div><div id="starting-populations"></div><button type="button" class="button subtle" id="add-population">${icon('plus')}Add genome</button>
    <p class="form-error" role="alert" hidden></p><footer class="dialog-actions"><button class="button primary" type="submit">Start simulation ${icon('arrow')}</button></footer></form>`, 'new-simulation-dialog');
  const rows = dialog.querySelector<HTMLElement>('#starting-populations')!;
  const drawRows = (): void => { rows.innerHTML = populations.map((item, index) => `<div class="population-row"><select data-population="${index}" aria-label="Genome ${index + 1}">${genomeOptions(item.genomeId)}</select><input type="number" min="0" step="1" data-count="${index}" value="${item.count}" aria-label="Organism count ${index + 1}"><button class="icon-button" type="button" data-remove-population="${index}" aria-label="Remove genome ${index + 1}">${icon('close')}</button></div>`).join(''); };
  drawRows();
  const syncPopulation = (event: Event): void => {
    const target = event.target as HTMLInputElement;
    if (target.dataset.population !== undefined) {
      const index = Number(target.dataset.population); populations[index].genomeId = target.value;
    }
    if (target.dataset.count !== undefined) populations[Number(target.dataset.count)].count = target.valueAsNumber;
  };
  rows.addEventListener('input', syncPopulation);
  rows.addEventListener('change', syncPopulation);
  rows.addEventListener('click', event => {
    const button = (event.target as Element).closest<HTMLButtonElement>('[data-remove-population]');
    if (button) { populations.splice(Number(button.dataset.removePopulation), 1); drawRows(); }
  });
  dialog.querySelector('#add-population')!.addEventListener('click', () => { if (populations.length < 16) { populations.push({ genomeId: BUILTIN_GENOMES[0].id, count: 1 }); drawRows(); } });
  dialog.querySelector('form')!.addEventListener('submit', event => {
    event.preventDefault();
    if (transitioning) return;
    void (async () => {
      const controls = [...dialog.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLButtonElement>('input,select,button')]
        .map(control => ({ control, disabled: control.disabled }));
      for (const item of controls) item.control.disabled = true;
      transitioning = true; dialog.dataset.saving = 'true';
      let release: (() => void) | null = null;
      try {
        const name = validateName(dialog.querySelector<HTMLInputElement>('#simulation-name')!.value);
        const seed = inputNumber(dialog.querySelector('#simulation-seed')!, 'Seed', 0, 4294967295);
        if (!Number.isInteger(seed)) throw new Error('Seed must be a whole number.');
        const founders = populations.map((_, index) => ({
          genomeId: rows.querySelector<HTMLSelectElement>(`[data-population="${index}"]`)!.value,
          count: rows.querySelector<HTMLInputElement>(`[data-count="${index}"]`)!.valueAsNumber,
        }));
        if (founders.some(item => !Number.isSafeInteger(item.count) || item.count < 0)) throw new Error('Organism counts must be nonnegative whole numbers.');
        const environment = findEnvironment(dialog.querySelector<HTMLSelectElement>('#simulation-environment')!.value);
        if (!environment) throw new Error('Choose an environment.');
        const environmentConfig = structuredClone(environment.config), environmentName = environment.name;
        const startingGenomes = founders.map(item => {
          const genome = findGenome(item.genomeId); if (!genome) throw new Error('A starting genome no longer exists.');
          if (genome.languageVersion !== GENOME_LANGUAGE_VERSION) throw new Error('This genome uses the older investment language. Edit a new copy before introducing it.');
          return { count: item.count, source: genome.source, name: genome.name };
        });
        const engine = await Engine.load(); applyEnvironment(engine, environmentConfig); engine.setMutationRate(0.1); engine.resetEmpty(seed, environmentConfig.foodRate > 0 ? 360 : 0);
        const names: Record<string, string> = {};
        for (const item of startingGenomes) {
          const id = engine.compile(item.source); names[String(id)] = item.name;
          if (engine.seed(id, item.count) !== item.count) throw new Error('Could not create the starting population.');
        }
        if (active) await active.flush();
        const now = Date.now(), id = 'simulation:' + crypto.randomUUID(); release = await claimSimulation(id);
        const next = makeSession(engine, { id, name, environmentName, createdAt: now, updatedAt: now, seed, paused: false, speed: 1, genomeNames: names, history: [], summary: summarize(engine.snapshot()) });
        next.refresh(); next.touch(); await next.checkpoint(); next.release = release; active?.release(); active = next; release = null;
        prefs.activeSimulationId = id; prefs.tab = 'simulation'; prefs.simulationView = 'player'; selectedId = null; accumulator = 0;
        simulationOpenError = ''; persistPreferences(); dialog.close(); renderView();
      } catch (error) { release?.(); setDialogError(dialog, error); }
      finally { for (const item of controls) item.control.disabled = item.disabled; transitioning = false; delete dialog.dataset.saving; }
    })();
  });
  dialog.addEventListener('cancel', event => { if (dialog.dataset.saving) event.preventDefault(); });
}

function playerMarkup(): string {
  return `<section class="player-stage" aria-label="Simulation player"><canvas id="world" tabindex="0" role="img" aria-label="Simulation. Click an organism to inspect it. Arrow keys select organisms; Space plays or pauses."></canvas>
    <div class="player-toolbar"><div class="playback"><button class="play-button" id="play-pause" data-action="play"></button><select class="playback-speed" id="playback-speed" aria-label="Playback speed" title="Playback speed">${[1,2,5,10].map(speed => `<option value="${speed}">${speed}×</option>`).join('')}</select></div><button class="button primary" data-action="add-organism">${icon('plus')}Add organism</button></div>
    <div class="player-caption" id="player-caption"></div><aside class="cell-inspector" id="cell-inspector" aria-label="Organism inspector" hidden></aside></section>`;
}
function updatePlayback(): void {
  const button = $('play-pause'); if (!active || !button) return;
  button.innerHTML = icon(active.metadata.paused ? 'play' : 'pause');
  button.setAttribute('aria-label', active.metadata.paused ? 'Play simulation' : 'Pause simulation');
  button.title = (active.metadata.paused ? 'Play' : 'Pause') + ' · Space';
  $<HTMLSelectElement>('playback-speed').value = String(active.metadata.speed);
}
async function togglePlayback(): Promise<void> {
  if (!active || transitioning) return;
  const session = active;
  session.metadata.paused = !session.metadata.paused; accumulator = 0; session.touch(); updatePlayback();
  const pausing = session.metadata.paused;
  if (pausing) transitioning = true;
  try { if (pausing) await session.flush(); else await session.checkpoint(); }
  catch { /* Save status keeps the failure visible. */ }
  finally { if (pausing) transitioning = false; }
}
function genomeName(id: number): string {
  if (!active) return 'Genome ' + id;
  const names = active.metadata.genomeNames;
  if (transitioning || lifecycleFlushes) return names[String(id)] || 'Genome ' + id;
  if (!names[String(id)]) {
    const record = active.engine.getGenome(id);
    const original = allGenomes().find(item => item.source === record.source);
    names[String(id)] = (original?.name || (record.parentId ? (names[String(record.parentId)] || 'Variant').split(' · ')[0] + ' · ' + id : 'Genome ' + id)).slice(0,80);
    active.touch();
  }
  return names[String(id)];
}
function renderInspector(): void {
  inspectorProgram?.destroy(); inspectorProgram = null;
  const panel = $('cell-inspector'); if (!panel || !active) return;
  panel.hidden = selectedId === null; if (selectedId === null) return;
  const cell = active.world.cells.find(item => item.id === selectedId);
  panel.innerHTML = `<header class="inspector-header"><div><h2>Organism ${selectedId}</h2><span class="muted">${cell ? esc(genomeName(cell.genomeId)) : 'No longer alive'}</span></div><button class="icon-button" data-action="close-inspector" aria-label="Close organism inspector">${icon('close')}</button></header>
    ${cell ? `<nav class="inspector-tabs" aria-label="Organism details" role="tablist">${(['overview','body','memory','program'] as const).map(view => `<button role="tab" data-inspector-view="${view}" tabindex="${inspectorView === view ? 0 : -1}" aria-selected="${inspectorView === view}" class="${inspectorView === view ? 'active' : ''}">${view[0].toUpperCase()+view.slice(1)}</button>`).join('')}</nav><div id="inspector-content"></div>` : '<p class="muted">This organism has died.</p>'}`;
  if (!cell) return;
  if (inspectorView === 'memory') {
    $('inspector-content').innerHTML = `<div class="memory-summary" id="memory-summary"></div><input id="memory-filter" class="search-input memory-filter" type="search" placeholder="Find a register" aria-label="Find a register" value="${esc(memoryFilter)}"><table class="memory-table"><thead><tr><th>Address</th><th>Value</th></tr></thead><tbody id="memory-values"></tbody></table><div class="memory-footer"><button class="button subtle" data-action="memory-previous">Previous</button><span id="memory-page"></span><button class="button subtle" data-action="memory-next">Next</button></div>`;
  } else if (inspectorView === 'program') {
    const source = active.engine.getGenome(cell.genomeId).source;
    $('inspector-content').innerHTML = `<div class="program-toolbar"><button class="button small subtle" data-action="cell-graph">${icon('graph')}Graph</button><button class="icon-button" data-action="cell-program" aria-label="Expand program" title="Expand program">${icon('expand')}</button></div><div id="organism-program" class="organism-program-editor"></div>`;
    inspectorProgram = new ProgramEditor({ parent: $('organism-program'), source, readOnly: true, compiler });
  }
  updateInspector();
}
function metricRow(label: string, value: string): string { return `<div class="metric-row"><span>${label}</span><strong>${value}</strong></div>`; }
function trophicRole(cell: Cell): 'Autotroph' | 'Heterotroph' | 'Mixotroph' | 'Non-feeding' {
  const photosynthesis = cell.organelles.some(organ => organ.kind === 'photosynthetic' && organ.capacity > 0),
    digestion = cell.organelles.some(organ => organ.kind === 'digestive' && organ.capacity > 0);
  return photosynthesis && digestion ? 'Mixotroph' : photosynthesis ? 'Autotroph' : digestion ? 'Heterotroph' : 'Non-feeding';
}
function memoryValue(value: unknown): string {
  if (typeof value === 'number') return Number.isInteger(value) ? String(value) : Number(value.toPrecision(6)).toString();
  if (Array.isArray(value)) return '[' + value.map(memoryValue).join(', ') + ']';
  return JSON.stringify(value) ?? String(value);
}
function updateProgramEvaluation(editor: ProgramEditor, session: SimulationSession, cellId: number): void {
  const previous = inspectedRevisions.get(editor);
  if (previous?.session === session && previous.cellId === cellId
    && previous.revision === session.revision && previous.tick === session.world.tick) return;
  inspectedRevisions.set(editor, { session, cellId, revision: session.revision, tick: session.world.tick });
  if (!session.world.cells.some(cell => cell.id === cellId)) {
    editor.setEvaluationUnavailable(`Organism ${cellId} has died. Its program remains available; live values do not.`);
    return;
  }
  try { editor.setEvaluation(session.engine.getCellEvaluation(cellId)); }
  catch (error) { editor.setEvaluationUnavailable(readableError(error)); }
}
function updateInspector(): void {
  if (!active || selectedId === null || !$('inspector-content')) return;
  const cell = active.world.cells.find(item => item.id === selectedId);
  if (!cell) { renderInspector(); return; }
  if (inspectorView === 'overview') {
    $('inspector-content').innerHTML = `<div class="inspector-metrics">${metricRow('Capabilities',trophicRole(cell))}${metricRow('Energy',cell.energy.toFixed(1)+' / '+cell.energyCapacity.toFixed(1))}${metricRow('Envelope area',cell.bodyArea.toFixed(3))}${metricRow('Material mass',cell.bodyMass.toFixed(3))}${metricRow('Division',cell.divisionReady ? 'Ready' : Math.round(cell.maturity*100)+'% grown')}${metricRow('Growth',cell.growthRate.toFixed(3)+' area / s')}${metricRow('Gut',cell.gutEnergy.toFixed(1)+' / '+cell.gutCapacity.toFixed(1))}${metricRow('Digestion',cell.digestionRate.toFixed(2)+' energy / s')}${metricRow('Photosynthesis',cell.photosynthesisRate.toFixed(2)+' energy / s')}${metricRow('Integrity',Math.round(cell.integrity*100)+'%')}${metricRow('Minerals',cell.nutrientStored.toFixed(2)+' / '+cell.nutrientCapacity.toFixed(2))}${metricRow('Age',clock(cell.age)+' / '+clock(cell.lifespan))}${metricRow('Generation',String(cell.generation))}${metricRow('Upkeep',(cell.basalMetabolism+cell.memoryCost).toFixed(2)+' energy / s')}</div>`;
  } else if (inspectorView === 'body') {
    const target = $('inspector-content'), expanded = new Set([...target.querySelectorAll<HTMLDetailsElement>('details[open]')].map(item => item.dataset.kind));
    const focusedFamily = target.contains(document.activeElement) ? document.activeElement?.closest<HTMLDetailsElement>('.organelle-detail')?.dataset.kind : undefined;
    try {
      const body = active.engine.cellBody(cell.id);
      const meter = (name: string, used: number, capacity: number): string => `<div class="body-meter"><div><span>${name}</span><strong>${used.toFixed(3)} / ${capacity.toFixed(3)}</strong></div><meter min="0" max="${Math.max(capacity,0.000001)}" value="${used}" aria-label="${name}"></meter></div>`;
      target.innerHTML = `<div class="body-capacity">${meter('Internal space',body.spaceUsed,body.spaceCapacity)}${meter('Membrane attachments',body.surfaceUsed,body.surfaceCapacity)}</div><div class="body-work"><span>Building <strong>${body.constructionRate.toFixed(3)}</strong></span><span>Recycling <strong>${body.recyclingRate.toFixed(3)}</strong></span><span>Repair <strong>${body.repairRate.toFixed(3)}</strong></span><small>material / s</small></div><div class="organelle-list">${ORGANELLE_CATALOG.map(definition => {
        const summary = cell.organelles.find(item => item.kind === definition.kind)!;
        const request = body.requested.find(item => item.kind === definition.kind)!;
        const organs = body.organs.filter(item => item.kind === definition.kind);
        if (!organs.length && !request.count && !request.build && !request.recycle) return '';
        return `<details class="organelle-detail" data-kind="${definition.kind}"${expanded.has(definition.kind) ? ' open' : ''}><summary><i style="background:${definition.color}"></i><span>${esc(definition.name)}<small>${summary.mass.toFixed(3)} material${summary.building > 0 ? ' · building' : ''}${summary.damaged > 0 ? ' · damaged' : ''}</small></span><strong title="Actual instances / last requested count">${summary.count}<small> / ${request.count}</small></strong></summary><div class="organelle-content"><p>${esc(definition.description)}</p><div class="organelle-request">${metricRow('Last size target',request.size.toFixed(3))}${metricRow('Build / recycle effort',Math.round(request.build*100)+'% / '+Math.round(request.recycle*100)+'%')}${metricRow('Functional capacity',summary.capacity.toFixed(3))}${metricRow('Damaged material',summary.damaged.toFixed(3))}</div>${organs.length ? `<table class="organelle-table"><thead><tr><th>Unit</th><th>Built</th><th>Integrity</th><th>State</th></tr></thead><tbody>${organs.map(organ => `<tr><td>#${organ.id}</td><td>${organ.mass.toFixed(3)}</td><td>${Math.round(organ.integrity*100)}%</td><td>${organ.functional ? organ.mass + 1e-9 < request.size ? 'Growing' : 'Functional' : organ.mass < organ.minimumMass ? 'Building' : 'Damaged'}</td></tr>`).join('')}</tbody></table>` : '<p class="muted">No physical units yet.</p>'}</div></details>`;
      }).join('')}</div><p class="body-explanation">Targets request a body. Only built, functional organs provide capabilities; incomplete and damaged organs still occupy space.</p>`;
      if (focusedFamily) target.querySelector<HTMLElement>(`[data-kind="${focusedFamily}"] > summary`)?.focus({preventScroll:true});
    } catch (error) { target.textContent = readableError(error); }
  } else if (inspectorView === 'memory') {
    const entries = active.engine.getMemory(cell.id), matching = entries.filter(entry => (entry.name+' '+entry.type).toLowerCase().includes(memoryFilter.toLowerCase()));
    const pageCount = Math.max(1,Math.ceil(matching.length/50)); memoryPage = Math.max(0,Math.min(memoryPage,pageCount-1));
    $('memory-summary').textContent = entries.length+' registers · '+cell.memoryCost.toFixed(4)+' energy / s';
    $('memory-values').innerHTML = matching.slice(memoryPage*50,memoryPage*50+50).map(entry => `<tr><td title="${esc(entry.type)}"><code>${esc(entry.name)}</code></td><td title="${esc(JSON.stringify(entry.value) ?? String(entry.value))}"><code>${esc(memoryValue(entry.value))}</code></td></tr>`).join('') || `<tr><td colspan="2">${entries.length ? 'No matching registers.' : 'Memory is empty.'}</td></tr>`;
    $('memory-page').textContent = (memoryPage+1)+' / '+pageCount;
    $('memory-page').closest<HTMLElement>('.memory-footer')!.hidden = pageCount === 1;
    document.querySelector<HTMLButtonElement>('[data-action="memory-previous"]')!.disabled = memoryPage === 0;
    document.querySelector<HTMLButtonElement>('[data-action="memory-next"]')!.disabled = memoryPage === pageCount-1;
  } else if (inspectorView === 'program' && inspectorProgram) {
    updateProgramEvaluation(inspectorProgram, active, cell.id);
  }
}
function openLiveProgram(id: number, cellId?: number): void {
  if (!active) return;
  const session = active, engine = session.engine, name = genomeName(id), source = engine.getGenome(id).source;
  const title = cellId === undefined ? name : `Organism ${cellId} · ${name}`;
  const dialog = createDialog(title, `<div class="live-program-editor"></div><footer class="dialog-actions"><button class="button subtle" id="program-graph">${icon('graph')}Graph</button><button class="button primary" id="program-save">${icon('copy')}Save to Genomes</button></footer>`, 'editor-dialog live-program-dialog');
  const editor = new ProgramEditor({ parent: dialog.querySelector<HTMLElement>('.live-program-editor')!, source, readOnly: true, compiler });
  const inspection = cellId === undefined ? null : { session, cellId, editor };
  if (inspection) { expandedInspections.add(inspection); updateProgramEvaluation(editor, session, inspection.cellId); }
  dialog.addEventListener('close', () => {
    if (inspection) expandedInspections.delete(inspection);
    editor.destroy();
  }, { once: true });
  dialog.querySelector('#program-graph')!.addEventListener('click', () => { try { genomeVisualizer.open(engine.getGenomeGraph(id), name); } catch (error) { setDialogError(dialog,error); } });
  dialog.querySelector('#program-save')!.addEventListener('click', () => {
    const button = dialog.querySelector<HTMLButtonElement>('#program-save')!; button.disabled = true;
    void persistGenome(newGenome(name,source)).then(() => { button.textContent = 'Saved to Genomes'; toast('Genome saved.'); }).catch(error => { button.disabled=false; setDialogError(dialog,error); });
  });
}
function addOrganismDialog(): void {
  if (!active || transitioning) return;
  const session = active;
  const dialog = createDialog('Add organism', '<input class="search-input" id="organism-search" type="search" placeholder="Find a genome" aria-label="Find a genome"><div class="picker-list" id="organism-options"></div><p class="form-error" role="alert" hidden></p>', 'organism-picker');
  const options = dialog.querySelector<HTMLElement>('#organism-options')!;
  const draw = (): void => {
    const query = dialog.querySelector<HTMLInputElement>('#organism-search')!.value.toLowerCase();
    const matches = allGenomes().filter(item => item.languageVersion === GENOME_LANGUAGE_VERSION && (item.name+' '+(item.description||'')).toLowerCase().includes(query));
    options.innerHTML = [false,true].map(builtin => {
      const group=matches.filter(item=>isBuiltin(item.id)===builtin);
      return group.length ? `<div class="picker-group"><h3>${builtin?'Built-in':'Saved'}</h3>${group.map(item => `<button type="button" class="picker-item" data-add-id="${esc(item.id)}" aria-label="Add ${esc(item.name)}" title="${esc(item.description||item.name)}">${icon('dna')}<span>${esc(item.name)}</span>${icon('plus')}</button>`).join('')}</div>` : '';
    }).join('') || '<p class="muted picker-empty">No matching current-language genomes.</p>';
  };
  draw(); dialog.querySelector('#organism-search')!.addEventListener('input',draw);
  options.addEventListener('click', event => {
    const button = (event.target as Element).closest<HTMLButtonElement>('[data-add-id]');
    if (!button || active !== session || transitioning) return;
    const genome = findGenome(button.dataset.addId!); if (!genome) return;
    try {
      const id = session.engine.compile(genome.source);
      if (session.engine.seed(id,1) !== 1) throw new Error('The organism could not be introduced.');
      session.metadata.genomeNames[String(id)] = genome.name; session.refresh(); session.touch();
      dialog.close(); void session.checkpoint().catch(() => {}); updateSimulationUi();
      toast('One '+genome.name+' organism added.');
    } catch (error) { setDialogError(dialog,error); }
  });
  dialog.querySelector<HTMLInputElement>('#organism-search')!.focus();
}
const metricDefinitions = [
  ['population','Organisms'],['food','Organic particles'],['births','Births'],['deaths','Deaths'],
  ['generation','Generation'],['mutations','Mutated births'],['predation','Predation deaths'],['attacks','Bites'],
] as const;
function metricsMarkup(): string {
  const cards = (definitions: ReadonlyArray<readonly [string,string]>): string => definitions.map(([key,label]) => `<article class="metric-card"><span>${label}</span><strong id="metric-${key}">—</strong></article>`).join('');
  return `<div class="page metrics-page"><div class="metrics-grid">${cards(metricDefinitions.slice(0,4))}</div>
    <section class="chart-panel"><div class="section-heading"><h2>Population</h2><span id="history-duration"></span></div><svg class="metrics-chart" id="history-chart" viewBox="0 0 1000 240" preserveAspectRatio="none" role="img" aria-label="Population history"></svg></section>
    <section class="trophic-panel"><div class="section-heading"><h2>Food chain</h2><span title="Based on functional organs that have actually been built.">Current capabilities</span></div><div class="metrics-grid">${cards([['autotrophs','Autotrophs'],['heterotrophs','Heterotrophs'],['mixotrophs','Mixotrophs'],['hunters','Attack-capable']])}</div><div class="energy-flow">${[['production','Photosynthesis'],['digestion','Digestion'],['exudation','Organic release']].map(([key,label])=>`<div><span>${label}</span><strong id="flow-${key}">—</strong></div>`).join('')}</div><p class="muted">Light → producers → organic material → consumers. Minerals return to the water through decay.</p></section>
    <section class="nutrient-panel"><h2>Minerals</h2><div class="metrics-grid nutrient-metrics">${[['water','Water'],['food','Organic particles'],['cells','Organisms & guts'],['total','Total']].map(([key,label])=>`<div class="metric-card"><span>${label}</span><strong id="nutrient-${key}">—</strong></div>`).join('')}</div></section>
    <details class="metrics-secondary"><summary>Resource accounting</summary><div class="inspector-metrics">${metricRow('Material introduced','<span id="introduced-material">—</span>')}${metricRow('Organic energy introduced','<span id="introduced-energy">—</span>')}${metricRow('Energy currently held','<span id="held-energy">—</span>')}${metricRow('Light energy collected','<span id="light-energy">—</span>')}</div></details>
    <details class="metrics-secondary"><summary>Evolution & interactions</summary><div class="metrics-grid">${cards(metricDefinitions.slice(4))}</div></details></div>`;
}
function renderMetrics(world: WorldSnapshot): void {
  const values: Record<string,number> = { population:world.cells.length,food:world.food.length,births:world.births,deaths:world.deaths,generation:world.cells.reduce((max,cell)=>Math.max(max,cell.generation),0),mutations:world.mutatedBirths,predation:world.predationCount,attacks:world.attackCount };
  for (const [key] of metricDefinitions) $('metric-'+key).textContent = String(values[key]);
  const trophic = { autotrophs: 0, heterotrophs: 0, mixotrophs: 0, hunters: 0 };
  const flow = { production: 0, digestion: 0, exudation: 0 };
  for (const cell of world.cells) {
    const role = trophicRole(cell);
    if (role === 'Autotroph') trophic.autotrophs++;
    if (role === 'Heterotroph') trophic.heterotrophs++;
    if (role === 'Mixotroph') trophic.mixotrophs++;
    if (cell.organelles.some(organ => organ.kind === 'piercer' && organ.capacity > 0)) trophic.hunters++;
    flow.production += cell.photosynthesisRate; flow.digestion += cell.digestionRate; flow.exudation += cell.exudationRate;
  }
  for (const [key,value] of Object.entries(trophic)) $('metric-'+key).textContent = String(value);
  for (const [key,value] of Object.entries(flow)) $('flow-'+key).textContent = value.toFixed(1)+' energy / s';
  for (const [key,value] of Object.entries({water:world.nutrientDissolvedTotal,food:world.nutrientFoodTotal,cells:world.nutrientCellTotal,total:world.nutrientTotal})) $('nutrient-'+key).textContent=value.toFixed(1);
  $('introduced-material').textContent = world.nutrientInputTotal.toFixed(2);
  $('introduced-energy').textContent = world.energyInputTotal.toFixed(2);
  $('held-energy').textContent = world.energyTotal.toFixed(2);
  $('light-energy').textContent = world.photosynthesisTotal.toFixed(2);
  $('nutrient-total').title = world.nutrientInputTotal.toFixed(1)+' mineral units introduced since this simulation began.';
  const history = [...active!.metadata.history.filter(point=>point.time<world.time),{time:world.time,population:world.cells.length}];
  const first = history[0]?.time ?? 0, span=Math.max(1,world.time-first);
  const max=Math.ceil(Math.max(10,...history.map(point=>point.population))*1.1/10)*10;
  const points=history.map(point=>`${((point.time-first)/span*930+50).toFixed(2)},${(214-point.population/max*190).toFixed(2)}`);
  if(points.length===1) points.push('980,'+(214-history[0].population/max*190).toFixed(2));
  const ticks=[0,max/2,max].map(value=>{const y=214-value/max*190;return `<path d="M50 ${y}H980" stroke="#29373b" stroke-dasharray="3 7"/><text x="36" y="${y+4}" text-anchor="end" fill="#809697" font-size="11">${value}</text>`;}).join('');
  $('history-chart').innerHTML = `${ticks}${points.length?`<path d="M50 214L${points.join(' L')}L${points[points.length-1].split(',')[0]} 214Z" fill="rgba(130,226,192,.06)"/><path d="M${points.join(' L')}" fill="none" stroke="#8be2c5" stroke-width="2.5" vector-effect="non-scaling-stroke"/>`:''}`;
  $('history-duration').textContent = clock(first)+' – '+clock(world.time);
}
function rangeField(key: string,label: string,value: number,min: number,max: number,step: number,suffix=''): string {
  return `<div class="form-field"><label for="setting-${key}">${label}<output id="value-${key}">${value}${suffix}</output></label><input id="setting-${key}" data-setting="${key}" type="range" min="${min}" max="${max}" step="${step}" value="${value}"></div>`;
}
function settingsMarkup(): string {
  const world=active!.world,environment=world.environment;
  return `<div class="page"><div class="settings-grid">
    <section class="settings-panel"><h2>Simulation</h2><div class="form-field"><label for="setting-name">Name</label><input id="setting-name" data-setting="name" maxlength="80" value="${esc(active!.metadata.name)}"></div>
    ${rangeField('food','External organic input / s',world.foodRate,0,60,1)}${rangeField('mutation','Mutation chance',Math.round(world.mutationRate*100),0,100,1,'%')}
    <p class="muted">Seed ${active!.metadata.seed} · ${esc(active!.metadata.environmentName)}</p></section>
    <section class="settings-panel"><h2>Display</h2><div class="form-field"><label for="setting-overlay">Field</label><select id="setting-overlay" data-setting="overlay">${[['none','None'],['light','Light'],['heat','Temperature'],['flow','Water flow'],['nutrients','Dissolved nutrients']].map(([value,label])=>`<option value="${value}"${prefs.overlay===value?' selected':''}>${label}</option>`).join('')}</select></div>
    <label class="toggle-field"><span>Organelle highlights</span><input type="checkbox" role="switch" data-setting="sensors"${prefs.sensors?' checked':''}></label><label class="toggle-field"><span>Movement trails</span><input type="checkbox" role="switch" data-setting="trails"${prefs.trails?' checked':''}></label></section>
    <section class="settings-panel"><h2>Environment</h2>${rangeField('light','Ambient light',Math.round(environment.ambientLight*100),0,100,1,'%')}${rangeField('temperature','Temperature',Math.round(environment.ambientTemperature*100)/100,0,1,.01)}${rangeField('current','Current strength',Math.round(environment.currentStrength),0,30,1)}
    </section>
  </div></div>`;
}
function renderActiveGenomes(): void {
  if (!active) return;
  const target=$('active-genomes'); if(!target) return;
  const counts=new Map<number,Cell[]>();
  for(const cell of active.world.cells) { const cells=counts.get(cell.genomeId)??[];cells.push(cell);counts.set(cell.genomeId,cells); }
  const entries=[...counts.entries()].sort(([a],[b])=>a-b),signature=entries.map(([id])=>id).join(',');
  if(target.dataset.signature!==signature || !target.childElementCount) {
    target.dataset.signature=signature;
    target.innerHTML=entries.map(([id])=>`<article class="genome-row"><button class="genome-open" data-action="live-program" data-genome="${id}" aria-label="Inspect ${esc(genomeName(id))}"><i class="genome-dot" style="background:${genomeColor(id)}"></i><span class="genome-row-title"><span class="card-title">${esc(genomeName(id))}</span><span class="muted" data-genome-stats="${id}"></span></span><span class="card-chevron">${icon('chevron')}</span></button><button class="icon-button" data-action="live-graph" data-genome="${id}" aria-label="Graph of ${esc(genomeName(id))}" title="Graph">${icon('graph')}</button></article>`).join('') || '<div class="empty-state">No living organisms. Add an organism from the player.</div>';
  }
  for(const [id,cells] of entries) {
    const target=document.querySelector<HTMLElement>(`[data-genome-stats="${id}"]`)!;
    target.textContent=cells.length+(cells.length===1?' organism':' organisms')+' · generation '+Math.max(...cells.map(cell=>cell.generation));
  }
}
function updateSimulationUi(): void {
  if(!active||prefs.tab!=='simulation') return;
  for (const inspection of expandedInspections) updateProgramEvaluation(inspection.editor, inspection.session, inspection.cellId);
  if(prefs.simulationView==='player') {
    $('player-caption').textContent=active.world.cells.length+' organisms · '+clock(active.world.time);
    updateInspector();
  } else if(prefs.simulationView==='metrics') renderMetrics(active.world);
  else if(prefs.simulationView==='genomes') renderActiveGenomes();
}


async function renameItem(kind: 'genome'|'environment'|'simulation', id: string): Promise<void> {
  const item=kind==='genome'?findGenome(id):kind==='environment'?findEnvironment(id):simulations.find(item=>item.id===id);
  if(!item||isBuiltin(id)) return;
  const dialog=createDialog('Rename',`<form><div class="form-field"><label for="rename-value">Name</label><input id="rename-value" maxlength="80" required value="${esc(item.name)}"></div><p class="form-error" role="alert" hidden></p><footer class="dialog-actions"><button class="button primary" type="submit">Rename</button></footer></form>`,'confirm-dialog');
  dialog.querySelector('form')!.addEventListener('submit',event=>{
    event.preventDefault();
    void(async()=>{
      const button=dialog.querySelector<HTMLButtonElement>('[type="submit"]')!;button.disabled=true;
      let release:(()=>void)|null=null;
      try{
        const name=validateName(dialog.querySelector<HTMLInputElement>('#rename-value')!.value);
        saveStatus('saving',undefined,'catalog');
        if(kind==='genome') await storage.renameGenome(id,name);
        else if(kind==='environment') await storage.renameEnvironment(id,name);
        else{
          release=await claimSimulation(id);
          const record=await storage.getSimulation(id);if(!record)throw new Error('This simulation no longer exists.');
          await storage.putSimulation({...record,name,updatedAt:Math.max(Date.now(),record.updatedAt)});
        }
        await refreshCatalogs();libraryChannel?.postMessage('changed');saveStatus('saved',undefined,'catalog');
        dialog.close();
      }catch(error){saveStatus('error',error,'catalog');setDialogError(dialog,error);}finally{release?.();button.disabled=false;}
    })();
  });
}
function deleteItem(kind:'genome'|'environment'|'simulation',id:string):void{
  const item=kind==='genome'?findGenome(id):kind==='environment'?findEnvironment(id):simulations.find(item=>item.id===id);
  if(!item||isBuiltin(id))return;
  const dialog=createDialog('Delete '+item.name,`<p>${kind==='simulation'?'This removes the saved simulation.':'Existing simulations keep their own copy.'}</p><p class="form-error" role="alert" hidden></p><footer class="dialog-actions"><button class="button subtle" id="cancel-delete">Cancel</button><button class="button danger" id="confirm-delete">Delete</button></footer>`,'confirm-dialog');
  dialog.querySelector('#cancel-delete')!.addEventListener('click',()=>dialog.close());
  dialog.querySelector('#confirm-delete')!.addEventListener('click',()=>{
    const button=dialog.querySelector<HTMLButtonElement>('#confirm-delete')!;button.disabled=true;
    void(async()=>{
      let release:(()=>void)|null=null;
      try{
        saveStatus('saving',undefined,'catalog');
        if(kind==='genome'){await storage.deleteGenome(id);savedGenomes=await storage.listGenomes();}
        else if(kind==='environment'){await storage.deleteEnvironment(id);savedEnvironments=await storage.listEnvironments();}
        else{
          release=await claimSimulation(id);await storage.deleteSimulation(id);simulations=await storage.listSimulationSummaries();
          if(prefs.activeSimulationId===id){prefs.activeSimulationId=null;persistPreferences();}
        }
        libraryChannel?.postMessage('changed');saveStatus('saved',undefined,'catalog');dialog.close();renderView();
      }catch(error){saveStatus('error',error,'catalog');setDialogError(dialog,error);}finally{release?.();button.disabled=false;}
    })();
  });
}
let importKind:'genome'|'environment'='genome';
function requestImport(kind:'genome'|'environment'):void{
  importKind=kind;const input=$<HTMLInputElement>('import-file');input.value='';input.accept=kind==='genome'?'.json,.cell':'.json';input.click();
}
$<HTMLInputElement>('import-file').addEventListener('change',()=>{
  const file=$<HTMLInputElement>('import-file').files?.[0],kind=importKind;if(!file)return;
  void(async()=>{
    try{
      if(file.size>(kind==='genome'?MAX_GENOME_IMPORT_BYTES:MAX_ENVIRONMENT_IMPORT_BYTES))throw new Error('This import file is too large.');
      const text=await file.text();
      if(kind==='genome'){
        const document=file.name.toLowerCase().endsWith('.cell')?newGenome(file.name.replace(/\.cell$/i,'').slice(0,80),text):parseGenomeImport(text);
        (await compiler()).compile(document.source);await persistGenome(document);await navigate('genomes');toast('Genome imported.');
      }else{await persistEnvironment(parseEnvironmentImport(text));await navigate('environments');toast('Environment imported.');}
    }catch(error){toast(readableError(error));}
  })();
});
async function action(name:string,id?:string,genomeId?:number):Promise<void>{
  if(name==='guide'){showGuide();return;}
  if(!ready||transitioning)return;
  if(name==='home'){await navigate('simulation');await closeSimulation();return;}
  if(name==='save'){
    if(saveIssues.has('catalog'))toast('Retry the failed save in its editor.');
    if(active){transitioning=true;try{await active.flush();}catch(error){saveStatus('error',error);}finally{transitioning=false;}}
    return;
  }
  if(name==='new-simulation'){newSimulationDialog();return;}
  if(name==='start-template'){newSimulationDialog(BUILTIN_SIMULATIONS.find(item=>item.id===id));return;}
  if(name==='open-simulation'&&id){await openSimulation(id);return;}
  if(name==='close-simulation'){await closeSimulation();return;}
  if(name==='play'){await togglePlayback();return;}
  if(name==='add-organism'){await refreshCatalogs(false);addOrganismDialog();return;}
  if(name==='close-inspector'){selectedId=null;renderInspector();return;}
  if(name==='memory-previous'||name==='memory-next'){memoryPage+=name==='memory-next'?1:-1;updateInspector();return;}
  if(name==='cell-program'||name==='cell-graph'){
    const cell=active?.world.cells.find(item=>item.id===selectedId);if(!cell||!active)return;
    if(name==='cell-program')openLiveProgram(cell.genomeId, cell.id);else genomeVisualizer.open(active.engine.getGenomeGraph(cell.genomeId),genomeName(cell.genomeId));return;
  }
  if(name==='live-program'&&genomeId){openLiveProgram(genomeId);return;}
  if(name==='live-graph'&&genomeId&&active){genomeVisualizer.open(active.engine.getGenomeGraph(genomeId),genomeName(genomeId));return;}
  if(name==='new-genome'){catalog.openGenome(undefined,true);return;}
  if(name==='new-environment'){catalog.openEnvironment(undefined,true);return;}
  if(name==='open-genome'||name==='copy-genome'){const item=findGenome(id!);if(item)catalog.openGenome(item,name==='copy-genome');return;}
  if(name==='open-environment'||name==='copy-environment'){const item=findEnvironment(id!);if(item)catalog.openEnvironment(item,name==='copy-environment');return;}
  if(name==='import-genome'||name==='import-environment'){requestImport(name==='import-genome'?'genome':'environment');return;}
  if(name==='export-genome'){const item=findGenome(id!);if(item)downloadFile(item.name.replace(/[^a-zA-Z0-9_-]/g,'-')+'.genome.json',exportGenome(item));return;}
  if(name==='export-environment'){const item=findEnvironment(id!);if(item)downloadFile(item.name.replace(/[^a-zA-Z0-9_-]/g,'-')+'.environment.json',exportEnvironment(item));return;}
  if(name.startsWith('rename-')&&id){await renameItem(name.slice(7) as 'genome'|'environment'|'simulation',id);return;}
  if(name.startsWith('delete-')&&id)deleteItem(name.slice(7) as 'genome'|'environment'|'simulation',id);
}
document.addEventListener('click',event=>{
  const target=(event.target as Element).closest<HTMLButtonElement>('button');if(!target)return;
  if(transitioning&&(target.dataset.tab||target.dataset.view||target.dataset.inspectorView||target.dataset.action)){event.preventDefault();return;}
  if(target.dataset.tab){void navigate(target.dataset.tab as UiPreferences['tab']);return;}
  if(target.dataset.view&&active){
    prefs.simulationView=target.dataset.view as UiPreferences['simulationView'];persistPreferences();renderView();
    document.querySelector<HTMLButtonElement>('[data-view="'+prefs.simulationView+'"]')?.focus({preventScroll:true});return;
  }
  if(target.dataset.inspectorView){
    inspectorView=target.dataset.inspectorView as typeof inspectorView;renderInspector();
    document.querySelector<HTMLButtonElement>('[data-inspector-view="'+inspectorView+'"]')?.focus({preventScroll:true});return;
  }
  if(target.dataset.action){
    target.closest('details')?.removeAttribute('open');
    void action(target.dataset.action,target.dataset.id,Number(target.dataset.genome)).catch(error=>toast(readableError(error)));
  }
});
document.addEventListener('input',event=>{
  const input=event.target as HTMLInputElement;
  if(input.id==='catalog-search'){catalogQuery=input.value;renderCatalogCards();}
  if(input.id==='memory-filter'){memoryFilter=input.value;memoryPage=0;updateInspector();}
  if(input.dataset.setting&&input.type==='range'){
    const output=$('value-'+input.dataset.setting);
    if(output)output.textContent=input.value+(input.dataset.setting==='light'||input.dataset.setting==='mutation'?'%':'');
  }
});
document.addEventListener('change',event=>{
  const input=event.target as HTMLInputElement,key=input.dataset.setting;
  if(input.id==='playback-speed'){
    if(!active||transitioning){updatePlayback();return;}
    const speed=Number(input.value);
    if(![1,2,5,10].includes(speed)){updatePlayback();return;}
    active.metadata.speed=speed as 1|2|5|10;active.touch();updatePlayback();void active.checkpoint().catch(()=>{});return;
  }
  if(!key||!active||transitioning)return;
  try{
    if(key==='overlay'){prefs.overlay=input.value as UiPreferences['overlay'];persistPreferences();return;}
    if(key==='sensors'||key==='trails'){prefs[key]=input.checked;persistPreferences();return;}
    if(key==='name'){
      active.metadata.name=validateName(input.value);
      const heading=document.querySelector('.simulation-title h1');if(heading)heading.textContent=active.metadata.name;
    }else if(key==='food')active.engine.setFoodRate(inputNumber(input,'External organic input',0,60));
    else if(key==='mutation')active.engine.setMutationRate(inputNumber(input,'Mutation chance',0,100)/100);
    else{
      const environment=active.world.environment;
      active.engine.setEnvironment(key==='temperature'?inputNumber(input,'Temperature',0,1):environment.ambientTemperature,
        key==='light'?inputNumber(input,'Light',0,100)/100:environment.ambientLight,
        key==='current'?inputNumber(input,'Current strength',0,30):environment.currentStrength);
    }
    active.refresh();active.touch();void active.checkpoint().catch(()=>{});
  }catch(error){toast(readableError(error));renderView();}
});
document.addEventListener('keydown',event=>{
  if(event.key==='Escape')for(const details of document.querySelectorAll<HTMLDetailsElement>('.item-menu[open]')){details.open=false;details.querySelector<HTMLElement>('summary')?.focus();}
  const current=(event.target as HTMLElement).closest<HTMLButtonElement>('button[role="tab"]');
  if(!current||transitioning||!['ArrowLeft','ArrowRight','Home','End'].includes(event.key))return;
  const tabs=[...current.closest('[role="tablist"]')!.querySelectorAll<HTMLButtonElement>('button[role="tab"]')],index=tabs.indexOf(current);
  if(!tabs.length)return;
  event.preventDefault();const next=tabs[event.key==='Home'?0:event.key==='End'?tabs.length-1:(index+(event.key==='ArrowRight'?1:tabs.length-1))%tabs.length];
  if(next.dataset.tab)void navigate(next.dataset.tab as UiPreferences['tab']).then(()=>document.querySelector<HTMLButtonElement>('[data-tab="'+next.dataset.tab+'"]')?.focus());
  else {const selector=next.dataset.view?'[data-view="'+next.dataset.view+'"]':'[data-inspector-view="'+next.dataset.inspectorView+'"]';next.click();document.querySelector<HTMLButtonElement>(selector)?.focus();}
});
document.addEventListener('click',event=>{
  for(const details of document.querySelectorAll<HTMLDetailsElement>('.item-menu[open]'))if(!details.contains(event.target as Node))details.open=false;
});
function frame(now:number):void{
  const elapsed=Math.max(0,Math.min((now-lastFrame)/1000,.2));lastFrame=now;
  try{
    if(active&&prefs.tab==='simulation'){
      if(!active.metadata.paused&&!transitioning&&!lifecycleFlushes&&!document.hidden){
        accumulator=Math.min(accumulator+elapsed*active.metadata.speed,1);
        const steps=Math.min(30,Math.floor(accumulator*30));
        if(steps){active.advance(steps);accumulator-=steps/30;}
      }
      if(renderer&&prefs.simulationView==='player')renderer.render(active.world,selectedId,prefs);
      if(!transitioning&&!lifecycleFlushes&&now-lastUi>250){updateSimulationUi();lastUi=now;}
    }
  }catch(error){
    if(active){active.metadata.paused=true;active.touch();updatePlayback();}
    toast(readableError(error));
  }
  requestAnimationFrame(frame);
}
window.setInterval(()=>{
  if(active&&!transitioning&&!saveFailed&&!active.pending&&active.revision!==active.savedRevision)void active.checkpoint().catch(()=>{});
},2000);
async function flushLifecycle(session: SimulationSession): Promise<void> {
  lifecycleFlushes++;
  try { await session.flush(); }
  catch { /* Session save status exposes failures. */ }
  finally { lifecycleFlushes--; accumulator = 0; lastFrame = performance.now(); }
}
document.addEventListener('visibilitychange',()=>{
  accumulator=0;lastFrame=performance.now();
  if(document.hidden&&active)void flushLifecycle(active);
  if(!document.hidden&&ready&&!transitioning)void refreshCatalogs().catch(error=>toast(readableError(error)));
});
window.addEventListener('pagehide',event=>{
  accumulator=0;
  if(active){const session=active;void flushLifecycle(session).finally(()=>{if(!event.persisted)session.release();});}
});
window.addEventListener('pageshow',()=>{lastFrame=performance.now();accumulator=0;renderer?.resize();});
libraryChannel?.addEventListener('message',()=>{if(ready&&!transitioning)void refreshCatalogs().catch(error=>toast(readableError(error)));});
window.addEventListener('focus',()=>{if(ready&&!transitioning)void refreshCatalogs().catch(error=>toast(readableError(error)));});
async function recoverPreviousDraft():Promise<void>{
  let source:string|null=null;
  try{source=localStorage.getItem('cellarium.genome.v1');}catch{return;}
  if(!source)return;
  if(allGenomes().some(item=>item.source===source)){try{localStorage.removeItem('cellarium.genome.v1');}catch{}return;}
  try{
    (await compiler()).compile(source);await persistGenome(newGenome('Recovered genome',source));
    localStorage.removeItem('cellarium.genome.v1');toast('Your previous genome was added to Genomes.');
  }catch{toast('Your previous draft remains saved; it could not be imported automatically.');}
}
async function start():Promise<void>{
  try{
    storage=await Storage.open();
    [savedGenomes,savedEnvironments,simulations]=await Promise.all([storage.listGenomes(),storage.listEnvironments(),storage.listSimulationSummaries()]);
    await recoverPreviousDraft();
    ready=true;saveStatus('saved');renderView();if(preferenceWarning)toast('Preferences could not be loaded. '+readableError(preferenceWarning));
    const tab=prefs.tab,view=prefs.simulationView,id=prefs.activeSimulationId;
    if(id&&simulations.some(item=>item.id===id)){
      await openSimulation(id);prefs.tab=tab;prefs.simulationView=view;persistPreferences();renderView();
    }else{prefs.activeSimulationId=null;persistPreferences();}
    lastFrame=performance.now();requestAnimationFrame(frame);
  }catch(error){
    $('save-state').textContent='Unavailable';
    $('workspace').innerHTML=`<div class="empty-state"><h1>agar could not open</h1><p>${esc(readableError(error))}</p><button class="button" id="retry-start">Try again</button></div>`;
    $('retry-start').addEventListener('click',()=>location.reload());
  }
}
void start();

