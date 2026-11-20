import './style.css';
import './player-view.css';
import './observation.css';
import './ecology-metrics.css';
import { ecologyMarkup, budgetStatusMarkup, renderEcology, renderBudgetStatus, recordedEnergyLoss } from './ecology-metrics';
import './sim-tools.css';
import './genome-lineage.css';
import { GenomeLineage } from './genome-lineage';
import { cellEditActions, editReserves } from './sim-tools';
import { PlayerView, PlayerFrameLoop, type PlayerCloseReason } from './player-view';
import { SimulationLoop, type SimulationWork } from './simulation-loop';
import { Engine, type Cell, type BodyInspection, type RegisterEntry, type GenomeRecord, type GenomeSignalEvaluation, type MetricsSnapshot, type SourceKind } from './engine';
import { WorkerEngine, type PresentationOptions } from './worker-engine';
import { Renderer, genomeColor } from './renderer';
import { GenomeVisualizer } from './genome-visualizer';
import { BUILTIN_GENOMES, BUILTIN_ENVIRONMENTS, BUILTIN_SIMULATIONS, simulationStartSettings, isBuiltin, type BuiltinSimulation } from './catalog';
import { Storage, loadPreferences, savePreferences, newGenome, upgradePreviousGenomeDraft, validateName, parseGenomeImport, parseEnvironmentImport,
  exportGenome, exportEnvironment, GENOME_LANGUAGE_VERSION, MAX_GENOME_IMPORT_BYTES, MAX_ENVIRONMENT_IMPORT_BYTES,
  type GenomeDocument, type EnvironmentDocument, type EnvironmentConfig, type SimulationMetadata, type UiPreferences } from './storage';
import { CatalogUI } from './catalog-ui';
import { SOURCE_NAMES, fieldOptions, arrowNudge, defaultSource, defaultBarrier, toolOptions, toolName, sourceFields, barrierFields, readObjectFields, objectList, cameraControls, displayedObjectValue, readEnvironmentNumber, type EnvironmentTool, type ObjectSelection, type SourceConfig, type BarrierConfig } from './environment-controls';
import { ProgramEditor } from './program-editor';
import { SimulationSession, claimSimulation } from './session';
import { COUNT_SCALE, icon, escapeHtml as esc, createDialog, setDialogError, downloadFile, inputNumber, clock, readableError } from './ui';
import { showGuide } from './guide';
import { ORGANELLE_CATALOG } from './organelles';
import { renderCellularBody } from './cellular-inspector';
import { organismOverview, updateOrganismOverview } from './organism-overview';
import { gasFields, readGasExchange } from './gas-controls';

const $ = <T extends HTMLElement = HTMLElement>(id: string): T => (document.getElementById(id) ?? playerView.document?.getElementById(id)) as T;
const queryUi = <T extends Element = HTMLElement>(selector: string): T | null => playerView.document?.querySelector<T>(selector) ?? document.querySelector<T>(selector);
let preferenceWarning: unknown;
let prefs: UiPreferences;
try { prefs = loadPreferences(); }
catch (error) {
  preferenceWarning = error;
  prefs = { tab: 'simulation', simulationView: 'player', activeSimulationId: null, overlay: 'none', builderOverlay: 'light', sensors: true, trails: false, colonyActivity: true, showExtinct: false };
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
type CellTool = { kind:'place'; genome:GenomeDocument; repeat:boolean; placed:number } | { kind:'move'; id:number; colony:boolean };
let cellTool: CellTool | null = null, cellEditPending = false;
let liveGasesOpen = false;
let environmentOpen = false, environmentSelection: ObjectSelection | null = null;
let environmentTool: EnvironmentTool = 'select';
let liveSourceDraft: SourceConfig | null = null, liveBarrierDraft: BarrierConfig | null = null;
let livePlacementPending = false, environmentEditSequence = 0;
const pendingEnvironmentInputs = new WeakMap<Element, number>();
let inspectorView: 'overview' | 'body' | 'memory' | 'program' = 'overview';
let memoryFilter = '', memoryPage = 0, catalogQuery = '', simulationOpenError = '';
let lastUi = 0, toastTimer = 0;
let lastPresentation = 0, lastMetrics = 0, lastEnvironment = 0, lastCheckpoint = 0;
let toastWindow: Window = window;
let playerElement: HTMLElement | null = null, restoringPlayer = false, unloading = false;
const playerView = new PlayerView(window, { bindDocument: bindUiDocument, changed: playerWindowChanged, visibilityChanged: playerVisibilityChanged });
const playerHost = (): Window => playerView.visible ? playerView.window! : !document.hidden ? window : playerView.window ?? window;
const frameLoop = new PlayerFrameLoop(playerHost, frame, undefined, () => playerView.isOpen);
const simulationLoop = new SimulationLoop(playerHost, simulationWork, advanceSimulation, runError);
let inspectorEpoch = 0, inspectorPending = false, inspectorStamp = '';
let cellInspection: { session: SimulationSession; cell: Cell; body?: BodyInspection; memory?: RegisterEntry[]; reception?: GenomeSignalEvaluation[] } | null = null;
const lineageViews = new WeakMap<SimulationSession,GenomeLineage>();
const genomeRecords = new WeakMap<SimulationSession, Map<number, Promise<GenomeRecord>>>();
const evaluatingEditors = new WeakSet<ProgramEditor>();
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
  const targets = [$('toast'), playerView.document?.getElementById('player-window-toast')].filter((item): item is HTMLElement => Boolean(item));
  for (const target of targets) { target.textContent = message; target.classList.add('visible'); }
  toastWindow.clearTimeout(toastTimer); toastWindow = playerView.window ?? window;
  toastTimer = toastWindow.setTimeout(() => { for (const target of targets) target.classList.remove('visible'); }, 4500);
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
async function readSavedGenomes(): Promise<GenomeDocument[]> {
  const documents = await storage.listGenomes();
  let converter: Engine | undefined;
  for (let index = 0; index < documents.length; index++) {
    const item = documents[index];
    if (item.languageVersion !== 3) continue;
    let source = item.source;
    if (source.includes('avoid_message') || source.includes('adhere_message')) {
      converter ??= await compiler();
      try { source = converter.upgradeContactSource(source); }
      catch { continue; } // Keep unconvertible source available for editing and export.
    }
    documents[index] = { ...item, source, languageVersion: GENOME_LANGUAGE_VERSION };
  }
  return documents;
}
let catalogRefreshSerial = 0;
async function refreshCatalogs(render = true): Promise<void> {
  const serial = ++catalogRefreshSerial;
  const [genomes, environments, runs] = await Promise.all([readSavedGenomes(), storage.listEnvironments(), storage.listSimulationSummaries()]);
  if (serial !== catalogRefreshSerial) return;
  savedGenomes = genomes; savedEnvironments = environments; simulations = runs;
  if (!render || !ready || transitioning) return;
  if (prefs.tab !== 'simulation') renderCatalogCards();
  else if (!active) renderView();
}
async function persistGenome(document: GenomeDocument): Promise<void> {
  saveStatus('saving', undefined, 'catalog');
  try { await storage.putGenome(document); savedGenomes = await readSavedGenomes(); saveStatus('saved', undefined, 'catalog'); libraryChannel?.postMessage('changed'); if (prefs.tab === 'genomes') renderView(); }
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
const catalog = new CatalogUI({ compiler,
  builderOverlay:()=>prefs.builderOverlay, setBuilderOverlay:value=>{prefs.builderOverlay=value;persistPreferences();}, saveGenome: persistGenome, saveEnvironment: persistEnvironment,
  graph: graphSource, startSimulation: id => newSimulationDialog(undefined, id), notify: toast });

function menu(kind: 'genome' | 'environment' | 'simulation', id: string, name: string, builtin = false): string {
  return `<details class="item-menu"><summary aria-label="Actions for ${esc(name)}" title="Actions">${icon('more')}</summary><div class="menu-popover">
    ${kind !== 'simulation' ? `<button data-action="copy-${kind}" data-id="${esc(id)}">${icon('copy')}Edit a copy</button><button data-action="export-${kind}" data-id="${esc(id)}">${icon('download')}Export</button>` : ''}
    ${!builtin ? `<button data-action="rename-${kind}" data-id="${esc(id)}">Rename</button><button class="danger" data-action="delete-${kind}" data-id="${esc(id)}">Delete</button>` : ''}
  </div></details>`;
}
function environmentThumb(environment: EnvironmentDocument): string {
  const config = environment.config, colors = ['#e8c578', '#ef9380', '#84b7ed', '#7cdbad', '#bd9cdd', '#84b7ed', '#84b7ed', '#84b7ed', '#65bef7', '#65bef7', '#d6a5f2', '#d6a5f2', '#a5c4f7'];
  return `<svg class="environment-thumb" viewBox="0 0 ${config.width} ${config.height}" aria-hidden="true"><rect width="${config.width}" height="${config.height}" fill="#101d22"/>${config.sources.map(source =>
    `<circle cx="${source.x}" cy="${source.y}" r="${source.radius}" fill="${colors[source.kind]}" opacity=".12"/><circle cx="${source.x}" cy="${source.y}" r="10" fill="${colors[source.kind]}"/>`).join('')}${config.barriers.map(barrier => `<rect x="${barrier.x-barrier.length/2}" y="${barrier.y-barrier.width/2}" width="${barrier.length}" height="${barrier.width}" rx="8" transform="rotate(${barrier.angle*180/Math.PI} ${barrier.x} ${barrier.y})" fill="${barrier.floating ? '#b9c5ca' : '#61717b'}"/>`).join('')}</svg>`;
}

function renderSimulationList(): string {
  return `<div class="page library-page"><header class="page-header"><h1>Simulations</h1><button class="button primary" data-action="new-simulation">${icon('plus')}New simulation</button></header>
    ${simulationOpenError ? `<p class="compatibility-notice" role="alert">${esc(simulationOpenError)}${simulationOpenError.includes('preserved') ? '' : ' Your saved simulation remains unchanged.'}</p>` : ''}
    ${simulations.length ? `<section class="catalog-section"><h2 class="catalog-section-heading">Saved</h2><div class="catalog-grid simulation-grid">${simulations.map(item => `<article class="catalog-card simulation-card"><button class="card-open" data-action="open-simulation" data-id="${esc(item.id)}" aria-label="Open simulation ${esc(item.name)}"><span class="card-identity">${icon('cell')}</span><span class="card-copy"><span class="card-title">${esc(item.name)}</span><span class="card-description">${esc(item.environmentName)}</span><span class="simulation-card-stats"><span>${item.summary.population} ${item.summary.population===1?'organism':'organisms'}</span><span>${clock(item.summary.time)}</span></span></span><span class="card-chevron">${icon('chevron')}</span></button>${menu('simulation',item.id,item.name)}</article>`).join('')}</div></section>` : ''}
    <section class="catalog-section"><h2 class="catalog-section-heading">Start from a preset</h2><div class="catalog-grid preset-grid">${BUILTIN_SIMULATIONS.map(item => `<article class="catalog-card preset-card"><button class="card-open" data-action="start-template" data-id="${item.id}" aria-label="Start ${esc(item.name)}"><span class="card-identity">${icon('play')}</span><span class="card-copy"><span class="card-title">${esc(item.name)}</span><span class="card-description">${esc(item.description)}</span><span class="preset-founders">${item.populations.map(row => `${row.count} × ${esc(BUILTIN_GENOMES.find(genome => genome.id === row.genomeId)!.name)}`).join(' · ')}</span></span><span class="card-chevron">${icon('chevron')}</span></button></article>`).join('')}</div></section></div>`;
}
function renderCatalogCards(): void {
  const target = $('catalog-items'); if (!target) return;
  const isGenome = prefs.tab === 'genomes';
  const items = (isGenome ? allGenomes() : allEnvironments()).filter(item => (item.name + ' ' + (item.description || '')).toLowerCase().includes(catalogQuery.toLowerCase()));
  const card = (item: GenomeDocument | EnvironmentDocument): string => {
    const kind = isGenome ? 'genome' : 'environment';
    return `<article class="catalog-card ${kind}-card"><button class="card-open" data-action="open-${kind}" data-id="${esc(item.id)}" aria-label="Open ${kind} ${esc(item.name)}">${isGenome ? `<span class="card-identity">${icon('dna')}</span>` : environmentThumb(item as EnvironmentDocument)}<span class="card-copy"><span class="card-title">${esc(item.name)}</span><span class="card-description">${esc(isGenome && (item as GenomeDocument).languageVersion !== GENOME_LANGUAGE_VERSION ? 'Older language · view or export the preserved source' : item.description || (isGenome ? '' : `${(item as EnvironmentDocument).config.sources.length} emitter${(item as EnvironmentDocument).config.sources.length===1?'':'s'} · ${(item as EnvironmentDocument).config.barriers.length} barrier${(item as EnvironmentDocument).config.barriers.length===1?'':'s'}`))}</span></span><span class="card-chevron">${icon('chevron')}</span></button>${menu(kind,item.id,item.name,isBuiltin(item.id))}</article>`;
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
  const retainPlayer = (playerView.isOpen || restoringPlayer) && Boolean(playerElement && renderer);
  if (!retainPlayer) {
    if (prefs.tab !== 'simulation' || prefs.simulationView !== 'player') cellTool = null;
    inspectorEpoch++;
    inspectorProgram?.destroy(); inspectorProgram = null;
    renderer?.dispose(); renderer = null; playerElement = null;
  }
  for (const button of document.querySelectorAll<HTMLButtonElement>('[data-tab]')) {
    const selected = button.dataset.tab === prefs.tab;
    button.classList.toggle('active', selected); button.setAttribute('aria-selected', String(selected)); button.tabIndex = selected ? 0 : -1;
  }
  if (!ready) return;
  if (prefs.tab !== 'simulation') { $('workspace').innerHTML = renderCatalog(); renderCatalogCards(); return; }
  if (!active) { $('workspace').innerHTML = renderSimulationList(); return; }
  $('workspace').innerHTML = `<div class="simulation-shell"><header class="simulation-bar"><div class="simulation-title"><button class="icon-button" data-action="close-simulation" aria-label="All simulations">${icon('back')}</button><h1>${esc(active.metadata.name)}</h1></div>
    <nav class="simulation-tabs" role="tablist" aria-label="Simulation views">${(['player', 'metrics', 'settings', 'genomes'] as const).map(view => `<button role="tab" data-view="${view}" tabindex="${prefs.simulationView === view ? 0 : -1}" aria-selected="${prefs.simulationView === view}" class="${prefs.simulationView === view ? 'active' : ''}">${view[0].toUpperCase() + view.slice(1)}</button>`).join('')}</nav>
    </header><div class="simulation-content">${prefs.simulationView === 'player' ? playerView.isOpen ? playerPlaceholderMarkup() : retainPlayer ? '' : playerMarkup() : prefs.simulationView === 'metrics' ? metricsMarkup() : prefs.simulationView === 'settings' ? settingsMarkup() : '<div class="page active-genomes-page"><div class="genome-list" id="active-genomes"></div></div>'}</div></div>`;
  if (prefs.simulationView === 'player' && !playerView.isOpen && retainPlayer) {
    document.querySelector('.simulation-content')!.append(playerElement!);
    renderer!.reparent(); inspectorProgram?.reparent();
    updatePlayback(); renderLiveEnvironment();
  } else if (prefs.simulationView === 'player' && !playerView.isOpen) {
    playerElement = document.querySelector<HTMLElement>('.player-stage');
    const canvas = $<HTMLCanvasElement>('world'); renderer = new Renderer(canvas, {
      preferEnvironment: () => environmentOpen,
      canDrag: target => !transitioning && !cellEditPending && !livePlacementPending && (target.kind === 'cell'
        ? !environmentOpen && target.object.id === selectedId && (!cellTool || cellTool.kind === 'move')
        : !cellTool && environmentTool === 'select'),
      drop: (target,x,y,colony) => {
        const id = target.object.id;
        if (target.kind === 'cell') {
          const tool = cellTool?.kind === 'move' && cellTool.id === id ? cellTool : {kind:'move' as const,id,colony};
          void useCellTool(tool,x,y);
        } else if (active) {
          const selection = {kind:target.kind,id};
          selectEnvironmentObject(selection);
          void updateEnvironmentObject(active,selection,{x,y}).catch(error=>toast(readableError(error)));
        }
      },
    });
    canvas.addEventListener('click', event => {
      if (!active || !renderer || transitioning || renderer.wasDragging()) return;
      if (cellTool) {
        const destination = renderer.toWorld(event.clientX,event.clientY);
        if (destination) void useCellTool(cellTool,destination.x,destination.y);
        else toast('Choose a point inside the world.');
        return;
      }
      if (environmentOpen && environmentTool !== 'select') {
        const position = renderer.toWorld(event.clientX,event.clientY);
        if (position) void placeLiveEnvironmentObject(position.x,position.y).catch(error=>toast(readableError(error)));
        return;
      }
      const hit = renderer.pickTarget(active.world,event.clientX,event.clientY,environmentOpen);
      if (hit && hit.kind !== 'cell') { selectEnvironmentObject({kind:hit.kind,id:hit.object.id}); return; }
      if (environmentOpen) {
        if (!hit) { selectEnvironmentObject(null); return; }
        environmentOpen=false; environmentSelection=null; renderLiveEnvironment();
      }
      selectedId = hit?.object.id ?? null;
      if (selectedId === null) {
        const food = renderer.pickFood(active.world,event.clientX,event.clientY);
        if (food) {
          const quantity = (value: number) => active!.counted
            ? Math.round(value * COUNT_SCALE).toLocaleString() : Number(value.toPrecision(4));
          toast(`${['Exudate','Tissue','Waste'][food.kind]} particle · ${quantity(food.energy)} food energy · ${quantity(food.nutrients)} minerals`);
        }
      }
      inspectorView = 'overview'; memoryPage = 0; memoryFilter = ''; renderInspector();
    });
    canvas.addEventListener('keydown', event => {
      if (!active || transitioning) return;
      const nudge=environmentOpen && environmentSelection ? arrowNudge(event) : null;
      if (cellTool && event.key === 'Enter' && !event.repeat) {
        event.preventDefault(); const point = renderer!.getCamera(); void useCellTool(cellTool,point.x,point.y);
      } else if (cellTool && event.key === 'Escape') { event.preventDefault(); clearCellTool(); }
      else if (event.key === 'Enter' && environmentOpen && environmentTool !== 'select') { event.preventDefault(); const center=renderer?.getCamera(); void placeLiveEnvironmentObject(center?.x ?? active.world.width/2,center?.y ?? active.world.height/2).catch(error=>toast(readableError(error))); }
      else if (event.code === 'Space') { event.preventDefault(); void togglePlayback(); }
      else if (event.key === 'Escape') { if (environmentOpen) { environmentTool='select'; environmentSelection=null; renderLiveEnvironment(); } selectedId = null; renderInspector(); }
      else if (environmentSelection && nudge) {
        event.preventDefault(); if (event.repeat) return;
        void updateEnvironmentObject(active,environmentSelection,{},nudge).catch(error=>toast(readableError(error)));
      }
      else if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
        event.preventDefault();
        if(environmentOpen){environmentOpen=false;environmentTool='select';environmentSelection=null;renderLiveEnvironment();}
        if (cellTool) clearCellTool();
        const cells = active.world.cells, index = cells.findIndex(cell => cell.id === selectedId), direction = event.key === 'ArrowRight' ? 1 : -1;
        selectedId = cells.length ? cells[index < 0 ? (direction > 0 ? 0 : cells.length - 1) : (index + direction + cells.length) % cells.length].id : null; renderInspector();
      }
    });
    bindLiveEnvironment(); renderLiveEnvironment(); renderCellTool();
    canvas.closest('.player-stage')!.querySelectorAll<HTMLButtonElement>('[data-camera]').forEach(button=>button.addEventListener('click',()=>{if(button.dataset.camera==='player:fit')renderer?.fitWorld();else renderer?.zoomBy(button.dataset.camera==='player:in'?1.25:.8);}));
    renderInspector(); updatePlayback(); renderer.render(active.world, selectedId, prefs, cellInspection?.session === active && cellInspection.cell.id === selectedId ? cellInspection.cell : null, cellInspection?.session === active && cellInspection.cell.id === selectedId ? cellInspection.body ?? null : null);
  }
  if (playerView.isOpen) renderLiveEnvironment();
  updateSimulationUi();
}
async function navigate(tab: UiPreferences['tab']): Promise<void> {
  if (!ready || transitioning) return;
  transitioning = true;
  try {
    if (active) {
      await active.flush();
    }
    await refreshCatalogs(false); prefs.tab = tab; catalogQuery = ''; simulationLoop.reset(); persistPreferences(); renderView();
    if (active) await active.refresh(viewPresentationOptions());
  }
  catch (error) { toast(readableError(error)); } finally { transitioning = false; simulationLoop.reset(); }
}
async function applyEnvironment(engine: WorkerEngine, config: EnvironmentConfig): Promise<void> {
  if (!await engine.setWorldSize(config.width, config.height)) throw new Error('The environment world size is invalid.');
  await engine.setEnvironment(config.ambientTemperature, config.ambientLight, config.currentStrength);
  await engine.setInitialNutrients(config.initialNutrients); await engine.clearBarriers();
  if (!await engine.setInitialGases(config.gases.initialOxygen, config.gases.initialCarbon) || !await engine.setGasExchange(config.gases)) throw new Error('The gas settings could not be applied.');
  if (!await engine.replaceSources(config.sources)) throw new Error('Could not create the environment sources.');
  for (const barrier of config.barriers) if (!await engine.addBarrier(barrier.x, barrier.y, barrier.length, barrier.width, barrier.angle, barrier.floating)) throw new Error('Could not create an environment barrier.');
}

async function makeSession(engine: WorkerEngine, metadata: SimulationMetadata): Promise<SimulationSession> {
  const initial = await engine.presentation({ metrics: true, environment: true });
  return new SimulationSession(engine, metadata, initial, storage, saveStatus, upsertMetadata, await engine.usesCountChemistry());
}
function disposeSession(session: SimulationSession): void {
  if (session === active) { cellTool = null; cellEditPending = false; playerView.close(unloading ? 'unload' : 'session'); }
  for (const inspection of expandedInspections) if (inspection.session === session) {
    inspection.editor.setEvaluationUnavailable('This simulation has been closed.');
    expandedInspections.delete(inspection);
  }
  if (cellInspection?.session === session) cellInspection = null;
  genomeRecords.delete(session); lineageViews.delete(session);
  session.dispose();
}
function runError(session: SimulationSession, error: unknown): void {
  if (active !== session) return;
  session.metadata.paused = true; session.touch(); updatePlayback();
  toast(readableError(error));
}
async function openSimulation(id: string): Promise<void> {
  if (transitioning) return;
  if (active?.metadata.id === id) { prefs.tab = 'simulation'; renderView(); return; }
  transitioning = true; let release: (() => void) | null = null; let candidate: WorkerEngine | null = null;
  try {
    release = await claimSimulation(id);
    const record = await storage.getSimulation(id); if (!record) throw new Error('This simulation no longer exists.');
    const engine = candidate = await WorkerEngine.load(); await engine.restoreState(record.state);
    if (active) await active.flush();
    const { state: _state, ...metadata } = record; metadata.paused = true;
    const next = await makeSession(engine, metadata); next.release = release;
    if (active) disposeSession(active); active = next; environmentSelection = null; environmentTool = 'select'; release = null; candidate = null;
    prefs.activeSimulationId = id; prefs.tab = 'simulation'; prefs.simulationView = 'player';
    selectedId = null; simulationLoop.reset(); persistPreferences(); renderView();
  } catch (error) {
    candidate?.dispose(); release?.();
    if (!active) { simulationOpenError = readableError(error); prefs.activeSimulationId = null; persistPreferences(); renderView(); }
    else toast(readableError(error));
  } finally { transitioning = false; }
}
async function closeSimulation(): Promise<void> {
  if (!active || transitioning) return;
  transitioning = true;
  try {
    active.metadata.paused = true; active.touch(); await active.flush(); disposeSession(active); active = null;
    selectedId = null; prefs.activeSimulationId = null; simulations = await storage.listSimulationSummaries(); persistPreferences(); renderView();
  } catch (error) { toast(readableError(error)); } finally { transitioning = false; }
}
function genomeOptions(selected?: string): string {
  return allGenomes().map(genome => `<option value="${esc(genome.id)}"${selected === genome.id ? ' selected' : ''}${genome.languageVersion !== GENOME_LANGUAGE_VERSION ? ' disabled' : ''}>${esc(genome.name)}${genome.languageVersion !== GENOME_LANGUAGE_VERSION ? ' · older language' : isBuiltin(genome.id) ? '' : ' · saved'}</option>`).join('');
}
function newSimulationDialog(template?: BuiltinSimulation, environmentId?: string): void {
  if (!ready || transitioning || document.querySelector('.new-simulation-dialog[open]')) return;
  const startingTemplate = template ?? (environmentId
    ? BUILTIN_SIMULATIONS.find(item => item.environmentId === environmentId)
    : BUILTIN_SIMULATIONS[0]);
  const chosenEnvironment = environmentId ?? startingTemplate?.environmentId ?? BUILTIN_ENVIRONMENTS[0].id;
  const populations = startingTemplate?.populations.map(item => ({ ...item })) ?? [{
    genomeId: 'builtin:genome:phototroph',
    count: 1,
  }];
  const dialog = createDialog('New simulation', `<form>
    <div class="form-field"><label for="simulation-name">Name</label><input id="simulation-name" maxlength="80" required value="${esc(template?.name || 'New simulation')}"></div>
    <div class="form-row"><div class="form-field"><label for="simulation-environment">Environment</label><select id="simulation-environment">${allEnvironments().map(item => `<option value="${esc(item.id)}"${item.id === chosenEnvironment ? ' selected' : ''}>${esc(item.name)}</option>`).join('')}</select></div><div class="form-field"><label for="simulation-seed">Seed</label><input id="simulation-seed" type="number" min="0" max="4294967295" step="1" value="${startingTemplate?.seed ?? 42}"></div></div>
    <div class="form-field"><label for="resource-arithmetic">Resource arithmetic</label><select id="resource-arithmetic"><option value="counts" ${simulationStartSettings().countChemistry ? 'selected':''}>Whole counts · 32-bit</option><option value="continuous" ${simulationStartSettings().countChemistry ? '':'selected'}>Continuous</option></select></div>
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
      let release: (() => void) | null = null; let candidate: WorkerEngine | null = null;
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
        const { mutationRate } = simulationStartSettings();
        const engine = candidate = await WorkerEngine.load();
        await applyEnvironment(engine, environmentConfig);
        await engine.setMutationRate(mutationRate);
        await engine.resetEmpty(seed);
        await engine.setCountChemistry(dialog.querySelector<HTMLSelectElement>('#resource-arithmetic')!.value === 'counts');
        const names: Record<string, string> = {};
        for (const item of startingGenomes) {
          const id = await engine.compile(item.source); names[String(id)] = item.name;
          if (await engine.seed(id, item.count) !== item.count) throw new Error('Could not create the starting population.');
        }
        if (active) await active.flush();
        const now = Date.now(), id = 'simulation:' + crypto.randomUUID(); release = await claimSimulation(id);
        const next = await makeSession(engine, { id, name, environmentName, createdAt: now, updatedAt: now, seed, paused: false, speed: 1, genomeNames: names, history: [], summary: await engine.summary() });
        next.touch(); await next.checkpoint(); next.release = release; if (active) disposeSession(active); active = next; environmentSelection = null; environmentTool = 'select'; release = null; candidate = null;
        prefs.activeSimulationId = id; prefs.tab = 'simulation'; prefs.simulationView = 'player'; selectedId = null; simulationLoop.reset();
        simulationOpenError = ''; persistPreferences(); dialog.close(); renderView();
      } catch (error) { candidate?.dispose(); release?.(); setDialogError(dialog, error); }
      finally { for (const item of controls) item.control.disabled = item.disabled; transitioning = false; delete dialog.dataset.saving; }
    })();
  });
  dialog.addEventListener('cancel', event => { if (dialog.dataset.saving) event.preventDefault(); });
}

function playerPlaceholderMarkup(): string {
  return `<section class="player-window-placeholder"><h2>Player is popped out</h2><p>${playerView.mode === 'picture-in-picture' ? 'The simulation stays visible and keeps running while you use the rest of agar or another tab.' : 'The simulation keeps running in its separate window. This browser cannot keep it always on top.'}</p><div class="toolbar"><button class="button subtle" data-action="focus-player">Show player</button><button class="button primary" data-action="return-player">Return player here</button></div><p>Keep this tab open to keep the player running.</p></section>`;
}
async function openPlayerWindow(): Promise<void> {
  if (!active || !playerElement || !renderer || transitioning) return;
  if (playerView.isOpen) { playerView.focus(); return; }
  const session = active, element = playerElement, drawing = renderer;
  await playerView.open(element, session.metadata.name, () => !unloading && active === session && renderer === drawing && playerElement === element);
}
function playerWindowChanged(reason?: PlayerCloseReason): void {
  simulationLoop.reset();
  if (unloading || reason === 'unload' || reason === 'session') { frameLoop.refresh(); return; }
  if (playerView.isOpen) {
    renderer?.reparent(); inspectorProgram?.reparent(); renderView();
  } else if (active && playerElement && renderer) {
    prefs.tab = 'simulation'; prefs.simulationView = 'player'; persistPreferences();
    restoringPlayer = true;
    try { renderView(); } finally { restoringPlayer = false; }
    $('world')?.focus({ preventScroll: true });
  }
  if (active) {
    const session = active;
    void session.refresh({ metrics: true, environment: true }).catch(error => runError(session, error));
  }
  frameLoop.refresh();
}

function playerMarkup(): string {
  return `<section class="player-stage" aria-label="Simulation player"><canvas id="world" tabindex="0" role="img" aria-label="Simulation. Tap an organism, emitter or barrier to inspect it. Drag an environment object or a selected organism to move it. Tap a food particle to read its energy and minerals. Scroll or pinch to zoom. Shift-drag or swipe the background to pan. Arrow keys move a selected environment object or select organisms; Space plays or pauses."></canvas>
    <div class="player-toolbar"><div class="playback"><button class="play-button" id="play-pause" data-action="play"></button><select class="playback-speed" id="playback-speed" aria-label="Playback speed" title="Playback speed">${[1,2,5,10].map(speed => `<option value="${speed}">${speed}×</option>`).join('')}</select></div><button class="button subtle" data-action="toggle-environment" aria-label="Environment" title="Environment" aria-pressed="${environmentOpen}">${icon('flow')}<span>Environment</span></button><button class="button primary" data-action="add-organism" aria-label="Add organism" title="Add organism">${icon('plus')}<span>Add organism</span></button><button type="button" class="player-popout" data-action="popout-player" title="${playerView.supportsAlwaysOnTop ? 'Open an always-on-top player' : 'Open a separate player window; always-on-top is unavailable in this browser'}">${icon('expand')}${playerView.supportsAlwaysOnTop ? 'Pop out' : 'Window'}</button><button type="button" class="player-dock" data-action="return-player" aria-label="Return to tab" title="Return to tab">${icon('back')}<span>Return to tab</span></button><button type="button" class="button small subtle player-hide" data-player-hide aria-label="Hide controls" title="Hide controls">Hide</button></div>
    <div class="cell-tool-bar" id="cell-tool-bar" hidden><span id="cell-tool-status" title="Tap or click a destination, or press Enter to use the center of the view." role="status" aria-live="polite"></span><label><input type="checkbox" data-repeat-placement> Place several</label><button type="button" class="button small subtle" data-action="cancel-cell-tool">Done</button></div>
    <div class="player-view-controls"><label class="player-field"><span>Field</span> <select id="player-overlay" data-setting="overlay" aria-label="Field overlay">${fieldOptions(prefs.overlay)}</select></label>${cameraControls('player')}<span>Scroll to zoom · Shift-drag to pan</span></div><aside class="live-environment" id="live-environment" aria-label="Environment controls" hidden></aside><div class="player-caption" id="player-caption"></div><aside class="cell-inspector" id="cell-inspector" aria-label="Organism inspector" hidden></aside></section>`;
}
function renderLiveEnvironment(): void {
  const panel = $('live-environment');
  if (!panel || !active) return;
  panel.hidden = !environmentOpen;
  const environment = active.world.environment;
  if (environmentSelection && !(environmentSelection.kind === 'source' ? environment.sources : environment.barriers).some(object => object.id === environmentSelection!.id)) environmentSelection = null;
  renderer?.setEnvironmentSelection(environmentOpen ? environmentSelection : null);
  queryUi('[data-action="toggle-environment"]')?.setAttribute('aria-pressed', String(environmentOpen));
  if (!environmentOpen) { $('world')?.classList.remove('placing-object'); return; }
  const source = environmentSelection?.kind === 'source' ? environment.sources.find(item => item.id === environmentSelection!.id) : null;
  const barrier = environmentSelection?.kind === 'barrier' ? environment.barriers.find(item => item.id === environmentSelection!.id) : null;
  let detail = '<p class="environment-hint">Choose an object in the list or click it in the world to tune it.</p>';
  if (environmentTool !== 'select') detail = `<h3>Place ${toolName(environmentTool).toLowerCase()}</h3>` + (environmentTool.startsWith('source:') ? sourceFields(liveSourceDraft!, environment.width, environment.height, true, false, active.counted) : barrierFields(liveBarrierDraft!, environment.width, environment.height, true, false));
  else if (source || barrier) detail = `<div class="source-detail-heading"><h3 data-object-title>${source ? SOURCE_NAMES[source.kind] : barrier!.floating ? 'Floating barrier' : 'Fixed barrier'}</h3><button type="button" class="icon-button" data-live-remove aria-label="Remove selected object">${icon('trash')}</button></div>`
    + (source ? sourceFields(source, environment.width, environment.height, true, true, active.counted) : barrierFields(barrier!, environment.width, environment.height));
  panel.innerHTML = `<header class="inspector-header"><h2>Environment</h2><button class="icon-button" data-action="close-environment" aria-label="Close environment controls">${icon('close')}</button></header>
    <div class="live-environment-tools"><label class="environment-tool-select"><span class="visually-hidden">Environment tool</span><select data-live-tool aria-label="Environment tool">${toolOptions(environmentTool)}</select></label></div>
    <p class="environment-hint environment-placement-hint">${environmentTool === 'select' ? 'Drag an object to move it, enter X and Y, or use the arrow keys on the world.' : 'Click the world to place this object. Enter places it at the view center.'}</p>
    <div class="source-list" aria-label="Live environment objects">${objectList(environment.sources, environment.barriers, environmentSelection, true)}</div>
    <div class="source-detail">${detail}</div>
    <details class="environment-baseline gas-settings"${liveGasesOpen ? ' open' : ''}><summary>Gases & atmosphere</summary>${gasFields(environment.gases, true, false, active.counted)}<button type="button" class="button subtle small" data-live-gases>Apply gas boundary</button><p class="environment-hint" data-live-gas-readout></p></details>
    <details class="environment-baseline"><summary data-live-size-summary>World size · ${environment.width} × ${environment.height}</summary><div class="source-fields"><label>Width<input type="number" data-live-width value="${environment.width}" min="400" max="4800" step="20"></label><label>Height<input type="number" data-live-height value="${environment.height}" min="400" max="3040" step="20"></label></div><button class="button subtle small" data-live-resize>Apply size</button></details>
    <p class="environment-hint environment-accounting">Minerals absorbed${active.counted ? ' (counts)' : ''}: <span data-absorbed-total></span></p>`;
  const gasPanel=panel.querySelector<HTMLDetailsElement>('.gas-settings')!;
  gasPanel.addEventListener('toggle',()=>{
    if (!active || !gasPanel.isConnected || gasPanel.open===liveGasesOpen) return;
    liveGasesOpen=gasPanel.open;
    if (liveGasesOpen) void active.refresh({frame:false,metrics:true}).then(updateLiveEnvironmentReadout).catch(error=>toast(readableError(error)));
  });
  $('world')?.classList.toggle('placing-object', environmentTool !== 'select');
  updateLiveEnvironmentReadout();
}
function updateLiveEnvironmentReadout(): void {
  const panel = $('live-environment');
  if (!active || !environmentOpen || !panel || panel.hidden) return;
  const environment = active.world.environment;
  const absorbed = panel.querySelector('[data-absorbed-total]'); if (absorbed) absorbed.textContent = active.counted ? String(Math.round(environment.nutrientRemovedTotal * COUNT_SCALE)) : environment.nutrientRemovedTotal.toFixed(2);
  const gases = panel.querySelector('[data-live-gas-readout]');
  if (gases && liveGasesOpen) {
    const patches=environment.columns*environment.rows;
    const oxygen=active.metrics.oxygenDissolvedTotal/patches, carbon=active.metrics.inorganicCarbonTotal/patches;
    gases.textContent = active.counted
      ? `Mean counts/patch: ${(oxygen * COUNT_SCALE).toFixed(1)} oxygen · ${(carbon * COUNT_SCALE).toFixed(1)} inorganic carbon`
      : `Average in water: ${oxygen.toFixed(3)} oxygen · ${carbon.toFixed(3)} inorganic carbon`;
  }
  const list = panel.querySelector<HTMLElement>('.source-list');
  const signature = environment.sources.map(object=>`s${object.id}:${object.kind}`).join(',') + '|' + environment.barriers.map(object=>`b${object.id}:${object.floating}`).join(',') + `|${environmentSelection?.kind}:${environmentSelection?.id}`;
  if (list && list.dataset.signature !== signature) { list.innerHTML = objectList(environment.sources, environment.barriers, environmentSelection, true); list.dataset.signature = signature; }
  const sizeSummary = panel.querySelector('[data-live-size-summary]'); if (sizeSummary) sizeSummary.textContent = `World size · ${environment.width} × ${environment.height}`;
  if (environmentTool !== 'select' || !environmentSelection) return;
  const current = (environmentSelection.kind === 'source' ? environment.sources : environment.barriers).find(object=>object.id===environmentSelection!.id);
  if (!current) { environmentSelection = null; renderLiveEnvironment(); return; }
  const title = panel.querySelector('[data-object-title]');
  if (title) title.textContent = 'kind' in current ? SOURCE_NAMES[current.kind] : current.floating ? 'Floating barrier' : 'Fixed barrier';
  if ('floating' in current) {
    const hint = panel.querySelector('.source-detail .environment-hint');
    if (hint) hint.textContent = current.floating ? 'Drifts with the local current and obstructs organisms, particles and field transport.' : 'Stays in place and obstructs organisms, particles and field transport.';
  }
  panel.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-object-field]').forEach(input => {
    const key = input.dataset.objectField!;
    if (key === 'x') (input as HTMLInputElement).max = String(environment.width - Number.EPSILON * environment.width);
    if (key === 'y') (input as HTMLInputElement).max = String(environment.height - Number.EPSILON * environment.height);
    if (input.ownerDocument.activeElement === input || pendingEnvironmentInputs.has(input) || input.getAttribute('aria-invalid') === 'true') return;
    const value = (current as unknown as Record<string, number | boolean>)[key];
    if (value === undefined) return;
    input.value = typeof value === 'boolean' ? String(value) : displayedObjectValue(input as HTMLInputElement, key, value);
    input.dataset.presentedValue = input.value;
  });
}

function selectEnvironmentObject(selection: ObjectSelection | null): void {
  environmentSelection = selection; environmentTool = 'select'; environmentOpen = true;
  selectedId = null; renderInspector(); renderLiveEnvironment();
}
async function placeLiveEnvironmentObject(x: number, y: number): Promise<void> {
  if (!active || environmentTool === 'select' || transitioning || livePlacementPending) return;
  const session = active, tool = environmentTool;
  livePlacementPending = true;
  try {
    const detail = $('live-environment').querySelector('.source-detail')!;
    const environment = session.world.environment;
    const source = tool.startsWith('source:') ? { ...readObjectFields(detail, liveSourceDraft!, environment.width, environment.height), x, y } : null;
    const barrier = !tool.startsWith('source:') ? { ...readObjectFields(detail, liveBarrierDraft!, environment.width, environment.height), x, y } : null;
    const id = await session.mutate(async engine => {
      const created = tool.startsWith('source:')
        ? await engine.addSource(source!.kind, source!.x, source!.y, source!.strength, source!.radius, source!.angle, source!.length)
        : await engine.addBarrier(barrier!.x, barrier!.y, barrier!.length, barrier!.width, barrier!.angle, barrier!.floating);
      if (!created) throw new Error('The environment object could not be added. Check the object limit and its dimensions.');
      return created;
    });
    if (active === session) {
      if (environmentOpen && environmentTool === tool && !environmentSelection) selectEnvironmentObject({ kind: tool.startsWith('source:') ? 'source' : 'barrier', id });
      else updateLiveEnvironmentReadout();
    }
    await session.checkpoint();
  } finally { livePlacementPending = false; }
}

async function updateEnvironmentObject(session:SimulationSession, selection:ObjectSelection, changes:Record<string,number|boolean>, nudge?:{x:number;y:number}):Promise<void> {
  await session.mutate(async engine => {
    // Read inside the command so a moving barrier keeps its latest position on a non-position edit.
    const env = await engine.environmentMetadata();
    const current = (selection.kind==='source' ? env.sources : env.barriers).find(object=>object.id===selection.id);
    if (!current) throw new Error('This environment object no longer exists.');
    const next = {...current,...changes};
    if (nudge) {
      next.x = (current.x+nudge.x+env.width)%env.width;
      next.y = (current.y+nudge.y+env.height)%env.height;
    }
    const updated = 'strength' in next
      ? await engine.updateSource(next.id,next.kind,next.x,next.y,next.strength,next.radius,next.angle,next.length)
      : await engine.updateBarrier(next.id,next.x,next.y,next.length,next.width,next.angle,next.floating);
    if (!updated) throw new Error('The environment object could not be updated.');
  });
  if (active===session) updateLiveEnvironmentReadout();
  await session.checkpoint();
}

function bindLiveEnvironment(): void {
  const panel = $('live-environment');
  panel.addEventListener('change', event => {
    if (!active || transitioning) return;
    const input = event.target as HTMLInputElement | HTMLSelectElement;
    if (input.matches('[data-live-tool]')) {
      environmentTool = input.value as EnvironmentTool; environmentSelection = null;
      if (environmentTool.startsWith('source:')) liveSourceDraft = defaultSource(Number(environmentTool.split(':')[1]) as SourceKind,0,0);
      else if (environmentTool !== 'select') liveBarrierDraft = defaultBarrier(environmentTool === 'barrier:floating',0,0);
      renderLiveEnvironment(); return;
    }
    const key = input.dataset.objectField;
    if (!key) return;
    input.removeAttribute('aria-invalid');
    const environment = active.world.environment;
    if (environmentTool !== 'select') {
      try {
        const detail = panel.querySelector('.source-detail')!;
        if (environmentTool.startsWith('source:')) liveSourceDraft = readObjectFields(detail, liveSourceDraft!, environment.width, environment.height);
        else liveBarrierDraft = readObjectFields(detail, liveBarrierDraft!, environment.width, environment.height);
      } catch (error) { input.setAttribute('aria-invalid','true'); toast(readableError(error)); }
      return;
    }
    const selection = environmentSelection, session = active;
    if (!selection) return;
    try {
      const value = key === 'floating' ? input.value === 'true' : readEnvironmentNumber(input as HTMLInputElement, Number((input as HTMLInputElement).min), Number((input as HTMLInputElement).max)) * (key === 'angle' ? Math.PI / 180 : 1);
      const edit = ++environmentEditSequence; pendingEnvironmentInputs.set(input, edit);
      void updateEnvironmentObject(session,selection,{[key]:value}).catch(error=>toast(readableError(error))).finally(() => {
        if (pendingEnvironmentInputs.get(input) === edit) pendingEnvironmentInputs.delete(input);
        if (active === session) updateLiveEnvironmentReadout();
      });
    } catch (error) { input.setAttribute('aria-invalid','true'); toast(readableError(error)); }
  });
  panel.addEventListener('click', event => {
    if (!active || transitioning) return;
    const target = (event.target as Element).closest<HTMLButtonElement>('button'); if (!target) return;
    if (target.dataset.objectKind) { selectEnvironmentObject({ kind: target.dataset.objectKind as 'source' | 'barrier', id: Number(target.dataset.objectId) }); return; }
    const session = active;
    if (target.hasAttribute('data-live-gases')) {
      try {
        const config = readGasExchange(panel);
        target.disabled = true;
        void session.mutate(async engine => { if (!await engine.setGasExchange(config)) throw new Error('The gas boundary could not be updated.'); })
          .then(() => session.checkpoint()).catch(error => toast(readableError(error))).finally(() => { target.disabled = false; });
      } catch (error) { toast(readableError(error)); }
      return;
    }
    if (target.hasAttribute('data-live-remove') && environmentSelection) {
      const selection = environmentSelection;
      void session.mutate(async engine => {
        if (selection.kind === 'source') await engine.removeSource(selection.id); else await engine.removeBarrier(selection.id);
      }).then(async () => {
        if (active === session) {
          if(environmentSelection?.kind===selection.kind && environmentSelection.id===selection.id)selectEnvironmentObject(null);
          else updateLiveEnvironmentReadout();
        }
        await session.checkpoint();
      }).catch(error => toast(readableError(error)));
    }
    if (target.hasAttribute('data-live-resize')) {
      try {
        const width = inputNumber(panel.querySelector('[data-live-width]')!, 'Width', 400, 4800);
        const height = inputNumber(panel.querySelector('[data-live-height]')!, 'Height', 400, 3040);
        if (width % 20 || height % 20) throw new Error('World dimensions must be multiples of 20.');
        void session.mutate(async engine => { if (!await engine.setWorldSize(width, height)) throw new Error('The world size could not be changed. Every barrier must fit inside the new dimensions.'); })
          .then(async () => {
            if (active === session) {
              renderer?.fitWorld(); updateLiveEnvironmentReadout();
              const currentPanel=$('live-environment');
              for(const [key,value] of [['width',width],['height',height]] as const){
                const field=currentPanel?.querySelector<HTMLInputElement>(`[data-live-${key}]`);
                if(field && field.ownerDocument.activeElement!==field)field.value=String(value);
              }
            }
            await session.checkpoint();
          }).catch(error => toast(readableError(error)));
      } catch (error) { toast(readableError(error)); }
    }
  });
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
  session.metadata.paused = !session.metadata.paused; simulationLoop.reset(); session.touch(); updatePlayback();
  const pausing = session.metadata.paused;
  if (pausing) {
    transitioning = true;
    saveStatus('saving', undefined, 'pause-checkpoint');
  }
  try {
    if (pausing) {
      // Finish any accepted batch, then refresh the slower projections once so
      // a paused overlay and metrics describe the final rendered tick.
      try { await session.refresh(viewPresentationOptions()); }
      finally { await session.flush(); }
    } else await session.checkpoint();
  }
  catch (error) { runError(session, error); }
  finally {
    if (pausing) {
      transitioning = false;
      saveStatus('saved', undefined, 'pause-checkpoint');
    }
    simulationLoop.reset();
  }
}
function getGenome(session: SimulationSession, id: number): Promise<GenomeRecord> {
  let records = genomeRecords.get(session);
  if (!records) { records = new Map(); genomeRecords.set(session, records); }
  let pending = records.get(id);
  if (!pending) {
    pending = session.inspect(async engine => {
      const record = await engine.getGenome(id);
      if (active === session && !session.metadata.genomeNames[String(id)]) {
        const original = allGenomes().find(item => item.source === record.source);
        const names = session.metadata.genomeNames;
        const tree=lineageViews.get(session);
        const derivedName=record.parentId && tree?.has(id) ? tree.nameOf(id) : undefined;
        names[String(id)] = (derivedName || original?.name || (record.parentId ? (names[String(record.parentId)] || 'Variant').split(' · ')[0] + ' · ' + id : 'Genome ' + id)).slice(0,80);
        session.touch(); tree?.refreshNames();
      }
      return record;
    });
    records.set(id, pending);
    void pending.catch(() => { records!.delete(id); });
  }
  return pending;
}
function genomeName(id: number): string {
  if (!active) return 'Genome ' + id;
  const name = active.metadata.genomeNames[String(id)];
  const tree=lineageViews.get(active);
  if (!name && tree?.has(id)) return tree.nameOf(id);
  if (!name && !transitioning && !lifecycleFlushes) void getGenome(active,id).catch(() => {});
  return name || 'Genome ' + id;
}
function renderInspector(): void {
  inspectorEpoch++; inspectorStamp = '';
  inspectorProgram?.destroy(); inspectorProgram = null;
  const panel = $('cell-inspector'); if (!panel || !active) return;
  panel.hidden = selectedId === null; if (selectedId === null) return;
  const cell = active.world.cells.find(item => item.id === selectedId);
  panel.innerHTML = `<header class="inspector-header"><div class="organism-title"><h2>${cell ? `<i aria-hidden="true" style="background:${genomeColor(cell.genomeId)}"></i><span data-inspected-genome>${esc(genomeName(cell.genomeId))}</span>` : 'Organism ' + selectedId}</h2>${cell ? `<span class="muted">Organism #${selectedId} · Generation ${cell.generation}</span>` : ''}</div><button class="icon-button" data-action="close-inspector" aria-label="Close organism inspector">${icon('close')}</button></header>
    ${cell ? cellEditActions(Boolean(cell.attachedNeighbors)) : ''}
    <p id="inspector-error" class="compatibility-notice" role="status" hidden></p>
    ${cell ? `<nav class="inspector-tabs" aria-label="Organism details" role="tablist">${(['overview','body','memory','program'] as const).map(view => `<button role="tab" data-inspector-view="${view}" tabindex="${inspectorView === view ? 0 : -1}" aria-selected="${inspectorView === view}" class="${inspectorView === view ? 'active' : ''}">${view[0].toUpperCase()+view.slice(1)}</button>`).join('')}</nav><div id="inspector-content"></div>` : '<p class="muted">This organism has died.</p>'}`;
  if (!cell) return;
  if (inspectorView === 'overview') {
    const content = $('inspector-content');
    content.innerHTML = organismOverview();
    content.querySelector('[data-food-potential]')!.addEventListener('toggle', () => {
      inspectorEpoch++; inspectorStamp = ''; updateInspector();
    });
  } else if (inspectorView === 'memory') {
    $('inspector-content').innerHTML = `<div class="memory-summary" id="memory-summary"></div><input id="memory-filter" class="search-input memory-filter" type="search" placeholder="Find a register" aria-label="Find a register" value="${esc(memoryFilter)}"><table class="memory-table"><thead><tr><th>Address</th><th>Value</th></tr></thead><tbody id="memory-values"></tbody></table><div class="memory-footer"><button class="button subtle" data-action="memory-previous">Previous</button><span id="memory-page"></span><button class="button subtle" data-action="memory-next">Next</button></div>`;
  } else if (inspectorView === 'program') {
    $('inspector-content').innerHTML = `<div class="program-toolbar"><button class="button small subtle" data-action="cell-graph">${icon('graph')}Graph</button><button class="icon-button" data-action="cell-program" aria-label="Expand program" title="Expand program">${icon('expand')}</button></div><div id="organism-program" class="organism-program-editor"><p class="muted">Loading program…</p></div>`;
  }
  updateInspector();
}
function metricRow(label: string, value: string): string { return `<div class="metric-row"><span>${label}</span><strong>${value}</strong></div>`; }
function memoryValue(value: unknown): string {
  if (typeof value === 'number') return Number.isInteger(value) ? String(value) : Number(value.toPrecision(6)).toString();
  if (Array.isArray(value)) return '[' + value.map(memoryValue).join(', ') + ']';
  return JSON.stringify(value) ?? String(value);
}
function updateProgramEvaluation(editor: ProgramEditor, session: SimulationSession, cellId: number): void {
  if (!editor.visible) return;
  const previous = inspectedRevisions.get(editor);
  if (evaluatingEditors.has(editor) || (previous?.session === session && previous.cellId === cellId
    && previous.revision === session.stateVersion && previous.tick === session.tick)) return;
  if (session.world.tick === session.tick && !session.world.cells.some(cell => cell.id === cellId)) {
    editor.setEvaluationUnavailable(`Organism ${cellId} has died. Its program remains available; live values do not.`);
    inspectedRevisions.set(editor, { session, cellId, revision: session.stateVersion, tick: session.tick });
    return;
  }
  const requestedVersion = session.stateVersion;
  evaluatingEditors.add(editor);
  void session.evaluation(cellId).then(({ evaluation, stateVersion }) => {
    if (stateVersion !== session.stateVersion || evaluation.tick !== session.tick || !editor.visible || active !== session || (editor !== inspectorProgram && ![...expandedInspections].some(item => item.editor === editor))) return;
    editor.setEvaluation(evaluation);
    inspectedRevisions.set(editor, { session, cellId, revision: stateVersion, tick: evaluation.tick });
  }).catch(error => {
    if (requestedVersion === session.stateVersion && editor.visible && active === session && (editor === inspectorProgram || [...expandedInspections].some(item => item.editor === editor))) editor.setEvaluationUnavailable(readableError(error));
  }).finally(() => { evaluatingEditors.delete(editor); });
}
function updateInspector(): void {
  const content = $('inspector-content');
  if (!active || cellTool || selectedId === null || !content || content.ownerDocument.hidden) return;
  const session = active, id = selectedId, mode = inspectorView, epoch = inspectorEpoch;
  if (!session.world.cells.some(item => item.id === id)) { cellInspection = null; renderInspector(); return; }
  const readReception = mode === 'body' && !!content.querySelector('[data-kind="colony-reception"][open]');
  const readFood = mode === 'overview' && Boolean(content.querySelector('[data-food-potential][open]'));
  const stamp = `${session.stateVersion}:${session.tick}:${id}:${mode}:${readReception}:${readFood}`;
  if (inspectorPending || inspectorStamp === stamp) return;
  inspectorPending = true;
  void (async () => {
    const detail = await session.inspect(async engine => {
      const stateVersion = session.stateVersion, tick = session.tick;
      const cell = await engine.cellSnapshot(id);
      if (!cell) return null;
      const needsBody = mode === 'body' || mode === 'memory' || readFood;
      const body = needsBody ? await engine.cellBody(id) : undefined;
      const memory = mode === 'memory' ? await engine.getMemory(id) : undefined;
      const reception = readReception ? (await engine.getCellEvaluation(id)).signals : undefined;
      return { cell, body, memory, reception, stamp: `${stateVersion}:${tick}:${id}:${mode}:${readReception}:${readFood}` };
    });
    if (!detail) return;
    const { cell, body, memory, reception } = detail;
    const source = mode === 'program' ? (await getGenome(session, cell.genomeId)).source : undefined;
    if (active !== session || selectedId !== id || inspectorView !== mode || inspectorEpoch !== epoch || content.ownerDocument.hidden) return;
    cellInspection = { session, cell, body, memory, reception }; inspectorStamp = detail.stamp;
    const errorNotice = $('inspector-error');
    if (errorNotice) { errorNotice.hidden = true; errorNotice.textContent = ''; }
    if (source !== undefined && !inspectorProgram && $('organism-program')) {
      $('organism-program').replaceChildren();
      inspectorProgram = new ProgramEditor({ parent: $('organism-program'), source, readOnly: true, compiler });
    }
    paintInspector();
  })().catch(error => {
    const errorNotice = $('inspector-error');
    if (active === session && selectedId === id && inspectorEpoch === epoch && errorNotice) {
      errorNotice.textContent = readableError(error); errorNotice.hidden = false;
    }
  }).finally(() => { inspectorPending = false; });
}
function paintInspector(): void {
  if (!active || selectedId === null || !$('inspector-content')) return;
  const cell = cellInspection?.session === active && cellInspection.cell.id === selectedId ? cellInspection.cell : null;
  if (!cell) return;
  const visible = active.world.cells.find(item => item.id === cell.id);
  const moveColony = queryUi<HTMLButtonElement>('[data-action="move-colony"]');
  if (moveColony) moveColony.disabled = !visible?.attachedNeighbors;
  const name = queryUi<HTMLElement>('[data-inspected-genome]');
  if (name) name.textContent = genomeName(cell.genomeId);
  if (inspectorView === 'overview') {
    updateOrganismOverview($('inspector-content'), cell, visible, cellInspection?.body?.foodPotential, active!.counted);
  } else if (inspectorView === 'body') {
    const target = $('inspector-content'), expanded = new Set([...target.querySelectorAll<HTMLDetailsElement>('details[open]')].map(item => item.dataset.kind));
    const focusedFamily = target.contains(target.ownerDocument.activeElement) ? target.ownerDocument.activeElement?.closest<HTMLDetailsElement>('details[data-kind]')?.dataset.kind : undefined;
    try {
      const body = cellInspection?.body; if (!body) return;
      const meter = (name: string, used: number, capacity: number): string => `<div class="body-meter"><div><span>${name}</span><strong>${used.toFixed(3)} / ${capacity.toFixed(3)}</strong></div><meter min="0" max="${Math.max(capacity,0.000001)}" value="${used}" aria-label="${name}"></meter></div>`;
      if (body.cellular) {
        target.innerHTML = renderCellularBody(body, expanded, cellInspection?.reception, active!.counted) + `<details class="cellular-section" data-kind="cellular-budgets"${expanded.has('cellular-budgets') ? ' open' : ''}><summary><span>Structure & capacity</span></summary><div class="body-capacity">${meter('Internal space',body.spaceUsed,body.spaceCapacity)}${meter('Membrane attachments',body.surfaceUsed,body.surfaceCapacity)}</div><p class="cellular-section-note">All products, incomplete assemblies and installed machinery occupy physical space.</p><table class="organelle-table"><thead><tr><th>Inherited organ</th><th>Mass</th><th>Integrity</th></tr></thead><tbody>${body.organs.map(organ=>`<tr><td>${esc(organ.kind)} #${organ.id}</td><td>${organ.mass.toFixed(4)}</td><td>${Math.round(organ.integrity*100)}%</td></tr>`).join('')}</tbody></table></details>`;
      } else {
      target.innerHTML = `<div class="body-capacity">${meter('Internal space',body.spaceUsed,body.spaceCapacity)}${meter('Membrane attachments',body.surfaceUsed,body.surfaceCapacity)}</div><div class="body-work"><span>Expression <strong>${body.synthesisRate.toFixed(3)}</strong></span><span>Assembly <strong>${body.constructionRate.toFixed(3)}</strong></span><span title="Shared work budget: protein synthesis costs 0.6 work per material, and assembly costs 0.4.">Core capacity <strong>${body.assemblyCapacity.toFixed(3)}</strong></span><span>Recycling <strong>${body.recyclingRate.toFixed(3)}</strong></span><span>Repair <strong>${body.repairRate.toFixed(3)}</strong></span><small>material / s · synthesis and assembly share core work</small></div><div class="organelle-list">${ORGANELLE_CATALOG.map(definition => {
        const summary = cell.organelles.find(item => item.kind === definition.kind)!;
        const request = body.requested.find(item => item.kind === definition.kind)!;
        const organs = body.organs.filter(item => item.kind === definition.kind);
        if (!organs.length && !request.count && !request.build && !request.recycle && request.proteinMass < 0.000001) return '';
        return `<details class="organelle-detail" data-kind="${definition.kind}"${expanded.has(definition.kind) ? ' open' : ''}><summary><i style="background:${definition.color}"></i><span>${esc(definition.name)}<small>${summary.mass.toFixed(3)} material${summary.building > 0 ? ' · assembling' : ''}${summary.damaged > 0 ? ' · damaged' : ''}</small></span><strong title="Actual instances / last requested count">${summary.count}<small> / ${request.count}</small></strong></summary><div class="organelle-content"><p>${esc(definition.description)}</p><div class="organelle-request">${metricRow('Assembly protein',request.proteinMass.toFixed(4)+' material')}${metricRow('Expression / recycle effort',Math.round(request.build*100)+'% / '+Math.round(request.recycle*100)+'%')}${metricRow('Unit size target',request.size.toFixed(3))}${metricRow('Functional capacity',summary.capacity.toFixed(3))}${metricRow('Damaged material',summary.damaged.toFixed(3))}</div>${organs.length ? `<table class="organelle-table"><thead><tr><th>Unit</th><th>Built</th><th>Integrity</th><th>State</th></tr></thead><tbody>${organs.map(organ => `<tr><td>#${organ.id}</td><td>${organ.mass.toFixed(3)}</td><td>${Math.round(organ.integrity*100)}%</td><td>${organ.functional ? organ.mass + 1e-9 < request.size ? 'Growing' : 'Functional' : organ.mass < organ.minimumMass ? 'Assembling' : 'Damaged'}</td></tr>`).join('')}</tbody></table>` : '<p class="muted">No physical units yet.</p>'}</div></details>`;
      }).join('')}</div><p class="body-explanation">Expression produces family-specific proteins. Targets assemble organs from existing stock. Unused proteins occupy space, consume energy and decay.</p>`;
      }
      target.querySelector('details[data-kind="colony-reception"]')?.addEventListener('toggle', updateInspector);
      if (focusedFamily) target.querySelector<HTMLElement>(`[data-kind="${focusedFamily}"] > summary`)?.focus({preventScroll:true});
    } catch (error) { target.textContent = readableError(error); }
  } else if (inspectorView === 'memory') {
    const entries = cellInspection?.memory; if (!entries) return;
    const cellular = cellInspection?.body?.cellular;
    const regulators = cellular?.products.filter(product => product.role === 'regulator') ?? [];
    const query = memoryFilter.toLowerCase();
    const matching = [...entries, ...regulators].filter(item => `${item.name} ${'role' in item ? item.role : item.type}`.toLowerCase().includes(query));
    const pageCount = Math.max(1,Math.ceil(matching.length/50)); memoryPage = Math.max(0,Math.min(memoryPage,pageCount-1));
    $('memory-summary').textContent = `${entries.length} populated registers · ${cell.memoryCost.toFixed(4)} energy / s${regulators.length ? ` · ${regulators.length} regulatory products` : ''}`;
    const filter=$<HTMLInputElement>('memory-filter'); filter.placeholder=cellular?'Find a register or regulator':'Find a register';filter.setAttribute('aria-label',filter.placeholder);
    $('memory-values').closest('table')!.querySelector('thead')!.innerHTML=regulators.length?'<tr><th>Register or regulator</th><th>Value or stock</th></tr>':'<tr><th>Address</th><th>Value</th></tr>';
    $('memory-values').innerHTML = matching.slice(memoryPage*50,memoryPage*50+50).map(entry => 'role' in entry
      ? `<tr><td><code>${esc(entry.name)}</code></td><td><code>${entry.mass.toFixed(4)} material</code><small class="regulator-memory-note">${entry.concentration.toFixed(4)} / area · half-life ${entry.halfLife}s</small></td></tr>`
      : `<tr><td title="${esc(entry.type)}"><code>${esc(entry.name)}</code></td><td title="${esc(JSON.stringify(entry.value) ?? String(entry.value))}"><code>${esc(memoryValue(entry.value))}</code></td></tr>`).join('')
      || `<tr><td colspan="2">${entries.length || regulators.length ? 'No matching memory entries.' : 'Memory is empty.'}</td></tr>`;
    $('memory-page').textContent = (memoryPage+1)+' / '+pageCount;
    $('memory-page').closest<HTMLElement>('.memory-footer')!.hidden = pageCount === 1;
    queryUi<HTMLButtonElement>('[data-action="memory-previous"]')!.disabled = memoryPage === 0;
    queryUi<HTMLButtonElement>('[data-action="memory-next"]')!.disabled = memoryPage === pageCount-1;
  } else if (inspectorView === 'program' && inspectorProgram) {
    updateProgramEvaluation(inspectorProgram, active, cell.id);
  }
}
async function openLiveProgram(id: number, cellId?: number): Promise<void> {
  if (!active) return;
  const session = active, engine = session.engine, source = (await getGenome(session,id)).source;
  if (active !== session) return;
  const name = genomeName(id);
  const title = cellId === undefined ? name : `Organism ${cellId} · ${name}`;
  const dialog = createDialog(title, `<div class="live-program-editor"></div><footer class="dialog-actions"><button class="button subtle" id="program-graph">${icon('graph')}Graph</button><button class="button primary" id="program-save">${icon('copy')}Save to Genomes</button></footer>`, 'editor-dialog live-program-dialog');
  const editor = new ProgramEditor({ parent: dialog.querySelector<HTMLElement>('.live-program-editor')!, source, readOnly: true, compiler });
  const inspection = cellId === undefined ? null : { session, cellId, editor };
  if (inspection) { expandedInspections.add(inspection); updateProgramEvaluation(editor, session, inspection.cellId); }
  dialog.addEventListener('close', () => {
    if (inspection) expandedInspections.delete(inspection);
    editor.destroy();
  }, { once: true });
  dialog.querySelector('#program-graph')!.addEventListener('click', () => { void engine.getGenomeGraph(id).then(graph => { if(dialog.open) genomeVisualizer.open(graph,name); }).catch(error => { if(dialog.open) setDialogError(dialog,error); }); });
  dialog.querySelector('#program-save')!.addEventListener('click', () => {
    const button = dialog.querySelector<HTMLButtonElement>('#program-save')!; button.disabled = true;
    void persistGenome(newGenome(name,source)).then(() => { button.textContent = 'Saved to Genomes'; toast('Genome saved.'); }).catch(error => { button.disabled=false; setDialogError(dialog,error); });
  });
}
function renderCellTool(): void {
  const bar = $('cell-tool-bar');
  if (!bar || !playerElement) return;
  bar.hidden = !cellTool;
  playerElement.toggleAttribute('data-cell-tool',Boolean(cellTool));
  if (!cellTool) return;
  $('cell-tool-status').textContent = cellTool.kind === 'place'
    ? `Place ${cellTool.genome.name} · tap the world${cellTool.placed ? ' · ' + cellTool.placed + ' added' : ''}`
    : `Move ${cellTool.colony ? 'linked group' : 'cell ' + cellTool.id} · tap a destination`;
  bar.querySelector('label')!.hidden = cellTool.kind !== 'place';
  bar.querySelector<HTMLInputElement>('[data-repeat-placement]')!.checked = cellTool.kind === 'place' && cellTool.repeat;
  bar.querySelector('button')!.textContent = cellTool.kind === 'place' && cellTool.placed ? 'Done' : 'Cancel';
}
function clearCellTool(): void { cellTool = null; renderCellTool(); }
function startCellTool(tool: CellTool): void {
  environmentOpen = false; environmentSelection = null; environmentTool = 'select'; renderLiveEnvironment();
  cellTool = tool; renderCellTool();
  $('world')?.focus({preventScroll:true});
}
async function useCellTool(tool: CellTool, x: number, y: number): Promise<void> {
  const session = active;
  if (!session || transitioning || cellEditPending) return;
  cellEditPending = true;
  const previous = cellTool;
  try {
    let id: number, message: string;
    if (tool.kind === 'place') {
      id = await session.mutate(async engine => {
        const genomeId = await engine.compile(tool.genome.source);
        const cellId = await engine.placeCell(genomeId,x,y);
        session.metadata.genomeNames[String(genomeId)] = tool.genome.name;
        return cellId;
      });
      tool.placed++;
      message = `${tool.genome.name} placed · organism ${id}.`;
    } else {
      const count = await session.mutate(engine => engine.moveCell(tool.id,x,y,tool.colony));
      id = tool.id;
      message = count === 1 ? `Moved organism ${id}.` : `Moved ${count} linked organisms.`;
    }
    void session.checkpoint().catch(() => {});
    if (active !== session || previous !== cellTool) return;
    selectedId = id; inspectorView = 'overview'; memoryPage = 0; memoryFilter = '';
    if (tool.kind === 'place' && tool.repeat) renderCellTool(); else clearCellTool();
    renderInspector(); updateSimulationUi(); toast(message);
  } catch (error) {
    if (active === session) {
      if (tool.kind === 'move' && !session.world.cells.some(cell => cell.id === tool.id)) { clearCellTool(); renderInspector(); }
      toast(readableError(error));
    }
  } finally { if (active === session) cellEditPending = false; }
}
function addOrganismDialog(): void {
  if (!active || transitioning) return;
  const session = active;
  const dialog = createDialog('Add organism', '<input class="search-input" id="organism-search" type="search" placeholder="Find a genome" aria-label="Find a genome"><div class="picker-list" id="organism-options"></div><p class="form-error" role="alert" hidden></p>', 'organism-picker', playerElement?.ownerDocument);
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
    const genome = findGenome(button.dataset.addId!);
    if (!genome) return;
    dialog.close();
    startCellTool({kind:'place',genome,repeat:false,placed:0});
  });
  dialog.querySelector<HTMLInputElement>('#organism-search')!.focus();
}
const metricDefinitions = [
  ['population','Organisms'],['food','Organic particles'],['births','Births'],['deaths','Deaths'],
  ['generation','Generation'],['mutations','Mutated births'],['predation','Predation deaths'],['attacks','Bites'],
] as const;
function metricsMarkup(): string {
  const cards = (definitions: ReadonlyArray<readonly [string,string]>): string => definitions.map(([key,label]) => `<article class="metric-card"><span>${label}</span><strong id="metric-${key}">—</strong>${key === 'food' ? '<small id="mean-particle-carbon" title="Organic carbon remaining across all particles divided by their count. Particles lose carbon as they decay."></small>' : ''}</article>`).join('');
  return `<div class="page metrics-page"><div class="metrics-grid">${cards(metricDefinitions.slice(0,4))}</div>
    ${ecologyMarkup()}
    ${budgetStatusMarkup()}
    <section class="trophic-panel"><div class="section-heading"><h2>Feeding capabilities</h2><span>Living organisms</span></div><div class="metrics-grid">${cards([['autotrophs','Light feeders'],['heterotrophs','Organic feeders'],['mixotrophs','Mixed feeders'],['hunters','Can attack']])}</div></section>
    ${active?.counted ? `<details class="metrics-secondary"><summary>Exact count budgets</summary>
      <div class="count-budget-scroll"><table class="count-budget-table"><thead><tr><th>Pool</th><th>Held</th><th>Added</th><th>Removed</th><th>Remainder</th></tr></thead>
      <tbody>${(['mineral','carbon','oxygen','energy'] as const).map((key,i) => `<tr><th scope="row">${['Minerals','Carbon','Oxygen','Energy'][i]}</th>${['held','added','removed','difference'].map(field => `<td id="count-${key}-${field}">—</td>`).join('')}</tr>`).join('')}</tbody></table></div>
      <p class="muted">The remainder is added minus held and removed. Minerals, carbon and oxygen should each have zero remainder. The energy remainder should equal the recorded losses under Where energy went. Oxygen flows include photosynthesis, respiration and decay.</p></details>` : ''}
    <details class="metrics-secondary"><summary>Food & energy flow</summary><div class="energy-flow">${[['production','Energy fixed from light'],['digestion','Assimilated substrate'],['exudation','Organic release']].map(([key,label])=>`<div><span>${label}</span><strong id="flow-${key}">—</strong></div>`).join('')}</div><div class="energy-flow">${[['exudate','Exudate'],['tissue','Cell tissue'],['waste','Waste']].map(([key,label])=>`<div><span>${label}</span><strong id="organic-${key}">—</strong></div>`).join('')}</div></details>
    <details class="metrics-secondary"><summary>Gases & minerals${active?.counted ? ' (counts)' : ''}</summary><div class="metrics-grid">${cards([['oxygen-water','Oxygen in water'],['carbon-water','Inorganic carbon in water'],['carbon-cells','Organic carbon in organisms'],['carbon-food','Organic carbon in particles']])}</div><div class="metrics-grid nutrient-metrics">${[['water','Dissolved minerals'],['food','Minerals in particles'],['cells','Minerals in organisms'],['total','Total minerals']].map(([key,label])=>`<div class="metric-card"><span>${label}</span><strong id="nutrient-${key}">—</strong></div>`).join('')}</div></details>
    <details class="metrics-secondary"><summary>Carbon & oxygen accounting${active?.counted ? ' (counts)' : ''}</summary><div class="inspector-metrics">${[
      ['carbon-total','Carbon currently held'],['carbon-input','Carbon introduced'],['carbon-removed','Carbon removed'],['carbon-legacy','Carbon in imported legacy bodies'],['carbon-fixed','Carbon fixed by photosynthesis'],['carbon-respired','Carbon respired by organisms'],['carbon-decayed','Carbon respired by background decay'],['carbon-balance','Carbon balance error'],['oxygen-total','Oxygen in water & organisms'],['oxygen-input','Oxygen introduced'],['oxygen-removed','Oxygen removed'],['oxygen-balance','Oxygen balance error'],
    ].map(([key,label])=>metricRow(label,`<span id="gas-${key}">—</span>`)).join('')}</div><p class="muted">Carbon balance: present + removed − introduced. Oxygen balance also includes production by fixation and consumption by respiration and decay. Atmospheric inflow and outflow are recorded.</p></details>
    <details class="metrics-secondary"><summary>Resource accounting${active?.counted ? ' (counts)' : ''}</summary><div class="inspector-metrics">${[
      ['introduced-material','Minerals introduced'],['removed-material','Minerals absorbed'],['withdrawn-material','Minerals withdrawn from cells'],
      ['introduced-energy','Energy introduced from outside'],['light-energy','Energy fixed from light'],['withdrawn-energy','ATP withdrawn from cells'],
      ['held-energy','Total energy still held'],['packet-energy','Held in food particles'],['dissipated-energy','<span id="dissipated-energy-label">Dissipated energy</span>'],
    ].map(([id,label])=>metricRow(label,`<span id="${id}">—</span>`)).join('')}</div><p class="muted">Energy supplied from outside and fixed from light is either still held, withdrawn, or dissipated through construction, metabolism and decay.</p></details>
    <details class="metrics-secondary"><summary>Evolution & interactions</summary><div class="metrics-grid">${cards(metricDefinitions.slice(4))}</div></details></div>`;
}
function renderMetrics(world: MetricsSnapshot): void {
  const audit = world.countAudit;
  const mineralAbsorbed = world.mineralAbsorbedTotal ?? active!.world.environment.nutrientRemovedTotal;
  const amount = (value: number, digits: number): string => audit
    ? String(Math.round(value * COUNT_SCALE)) : value.toFixed(digits);
  const residual = (entry: { held: string; added: string; removed: string }): bigint =>
    BigInt(entry.added) - BigInt(entry.removed) - BigInt(entry.held);
  if (audit) {
    for (const key of ['mineral', 'carbon', 'oxygen', 'energy'] as const) {
      const entry = audit[key];
      for (const field of ['held', 'added', 'removed'] as const) $('count-' + key + '-' + field).textContent = entry[field];
      $('count-' + key + '-difference').textContent = residual(entry).toString();
    }
  }
  const values: Record<string,number> = { population:world.population,food:world.food,births:world.births,deaths:world.deaths,generation:world.generation,mutations:world.mutatedBirths,predation:world.predationCount,attacks:world.attackCount };
  for (const [key] of metricDefinitions) $('metric-'+key).textContent = String(values[key]);
  for (const [key,value] of Object.entries(world.trophic)) $('metric-'+key).textContent = String(value);
  for (const [key,value] of Object.entries(world.flow)) $('flow-'+key).textContent = amount(value,1) + (audit ? ' energy counts/s' : ' energy / s');
  $('mean-particle-carbon').textContent = world.food ? (audit ? (world.organicFoodTotal * COUNT_SCALE / world.food).toFixed(1) + ' C counts / particle' : (world.organicFoodTotal / world.food).toFixed(3) + ' organic C / particle') : '';
  const organicCounts = world.organicCounts;
  ['exudate','tissue','waste'].forEach((kind,index) => { $('organic-'+kind).textContent = organicCounts[index]+' particles'; });
  for (const [key,value] of Object.entries({water:world.nutrientDissolvedTotal,food:world.nutrientFoodTotal,cells:world.nutrientCellTotal,total:world.nutrientTotal})) $('nutrient-'+key).textContent=key==='total' && audit ? audit.mineral.held : amount(value,1);
  for (const [key,value] of Object.entries({ 'oxygen-water':world.oxygenDissolvedTotal,'carbon-water':world.inorganicCarbonTotal,'carbon-cells':world.organicCellTotal,'carbon-food':world.organicFoodTotal })) $('metric-'+key).textContent=amount(value,2);
  const oxygenTotal = world.oxygenDissolvedTotal + world.oxygenCellTotal;
  const carbonBalance = world.carbonTotal + world.carbonRemovedTotal - world.carbonInputTotal;
  const oxygenBalance = oxygenTotal + world.oxygenRemovedTotal + world.carbonRespiredTotal + world.decayRespiredTotal - world.oxygenInputTotal - world.carbonFixedTotal;
  for (const [key,value] of Object.entries({
    'carbon-total':world.carbonTotal,'carbon-input':world.carbonInputTotal,'carbon-removed':world.carbonRemovedTotal,'carbon-legacy':world.legacyCarbonImported,
    'carbon-fixed':world.carbonFixedTotal,'carbon-respired':world.carbonRespiredTotal,'carbon-decayed':world.decayRespiredTotal,'carbon-balance':carbonBalance,
    'oxygen-total':oxygenTotal,'oxygen-input':world.oxygenInputTotal,'oxygen-removed':world.oxygenRemovedTotal,'oxygen-balance':oxygenBalance,
  })) {
    const exact = audit && ({
      'carbon-total': audit.carbon.held, 'carbon-input': audit.carbon.added, 'carbon-removed': audit.carbon.removed,
      'carbon-balance': String(-residual(audit.carbon)), 'oxygen-total': audit.oxygen.held, 'oxygen-balance': String(-residual(audit.oxygen)),
    } as Record<string, string>)[key];
    $('gas-'+key).textContent = exact ?? (key.endsWith('balance') && !audit
      ? Math.abs(value) < 1e-7 ? '0.000000' : value.toExponential(3) : amount(value,3));
  }
  $('introduced-material').textContent = audit?.mineral.added ?? amount(world.nutrientInputTotal,2);
  $('removed-material').textContent = amount(mineralAbsorbed, 2);
  $('withdrawn-material').textContent = amount(world.mineralWithdrawnTotal,2);
  $('withdrawn-energy').textContent = audit?.energy.removed ?? amount(world.energyWithdrawnTotal,2);
  $('introduced-energy').textContent = amount(world.energyInputTotal,2);
  $('held-energy').textContent = audit?.energy.held ?? amount(world.energyTotal,2);
  $('packet-energy').textContent = amount(world.packetEnergyTotal,2);
  const dissipated = world.energyInputTotal + world.photosynthesisTotal - world.energyWithdrawnTotal - world.energyTotal;
  if (world.energyLoss) {
    $('dissipated-energy-label').textContent = BigInt(world.energyLoss.opening)
      ? 'Dissipated energy (includes opening balance)' : 'Dissipated energy (measured)';
    $('dissipated-energy').textContent = recordedEnergyLoss(world.energyLoss).toString();
  } else {
    $('dissipated-energy-label').textContent = 'Dissipated energy (by difference)';
    $('dissipated-energy').textContent = audit ? residual(audit.energy).toString()
      : (Math.abs(dissipated) < 1e-7 ? 0 : dissipated).toFixed(2);
  }
  $('light-energy').textContent = amount(world.photosynthesisTotal,2);
  $('nutrient-total').title = (audit ? audit.mineral.added + ' mineral counts' : world.nutrientInputTotal.toFixed(1) + ' mineral units') + ' introduced since this simulation began.';
  renderEcology($('ecology-panel'), world, active!.metadata);
  renderBudgetStatus($('ecology-balance'), world, mineralAbsorbed);
}
function rangeField(key: string,label: string,value: number,min: number,max: number,step: number,suffix=''): string {
  return `<div class="form-field"><label for="setting-${key}">${label}<output id="value-${key}">${value}${suffix}</output></label><input id="setting-${key}" data-setting="${key}" type="range" min="${min}" max="${max}" step="${step}" value="${value}"></div>`;
}
function settingsMarkup(): string {
  const world=active!.world;
  return `<div class="page"><div class="settings-grid">
    <section class="settings-panel"><h2>Simulation</h2><div class="form-field"><label for="setting-name">Name</label><input id="setting-name" data-setting="name" maxlength="80" value="${esc(active!.metadata.name)}"></div>
    ${rangeField('mutation','Mutation chance',Math.round(world.mutationRate*100),0,100,1,'%')}
    <p class="muted">Seed ${active!.metadata.seed} · ${esc(active!.metadata.environmentName)}</p></section>
    <section class="settings-panel"><h2>Display</h2><div class="form-field"><label for="setting-overlay">Field</label><select id="setting-overlay" data-setting="overlay">${fieldOptions(prefs.overlay)}</select></div>
    <label class="toggle-field"><span>Organelle highlights</span><input type="checkbox" role="switch" data-setting="sensors"${prefs.sensors?' checked':''}></label><label class="toggle-field"><span>Cell connections</span><input type="checkbox" role="switch" data-setting="colonyActivity"${prefs.colonyActivity?' checked':''}></label><p class="muted">Teal bars show colony bonds retained at division. Amber rings show feeding links. Thin gray lines show other contact links. Signal dots are violet for positive, amber for negative and gray for zero.</p><label class="toggle-field"><span>Movement trails</span><input type="checkbox" role="switch" data-setting="trails"${prefs.trails?' checked':''}></label></section>
    <section class="settings-panel"><h2>Environment</h2><p class="muted">${world.width} × ${world.height} · ${world.environment.sources.length} emitters · ${world.environment.barriers.length} barriers</p><p class="muted">Select emitters and barriers in the player to adjust their strength, size, direction and position.</p><button class="button subtle" data-action="edit-live-environment">${icon('flow')}Edit environment</button></section>
  </div></div>`;
}
function renderActiveGenomes(): void {
  const session=active, target=$('active-genomes');
  if (!session || !target) return;
  let tree=lineageViews.get(session);
  if (!tree) {
    tree=new GenomeLineage(session.metadata.genomeNames, afterId=>session.inspect(engine=>engine.getGenomeLineage(afterId)));
    lineageViews.set(session,tree);
  }
  tree.update(target,session.metrics,prefs.showExtinct);
}
function updateSimulationUi(): void {
  if(!active) return;
  for (const inspection of expandedInspections) updateProgramEvaluation(inspection.editor, inspection.session, inspection.cellId);
  if(playerView.visible || !playerView.isOpen && !document.hidden && prefs.tab==='simulation' && prefs.simulationView==='player') {
    $('player-caption').textContent=active.metadata.summary.population+(active.metadata.summary.population === 1 ? ' organism · ' : ' organisms · ')+clock(active.metadata.summary.time);
    updateLiveEnvironmentReadout();
    updateInspector();
  }
  if(!document.hidden && prefs.tab==='simulation' && prefs.simulationView==='metrics') renderMetrics(active.metrics);
  else if(!document.hidden && prefs.tab==='simulation' && prefs.simulationView==='genomes') renderActiveGenomes();
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
        if(kind==='genome'){await storage.deleteGenome(id);savedGenomes=await readSavedGenomes();}
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
        const raw=file.name.toLowerCase().endsWith('.cell');
        const document=raw?newGenome(file.name.replace(/\.cell$/i,'').slice(0,80),text):parseGenomeImport(text);
        const engine=await compiler();
        if(raw || document.languageVersion===3) document.source=engine.upgradeContactSource(document.source);
        engine.compile(document.source);
        document.languageVersion=GENOME_LANGUAGE_VERSION;
        await persistGenome(document);await navigate('genomes');toast('Genome imported.');
      }else{await persistEnvironment(parseEnvironmentImport(text));await navigate('environments');toast('Environment imported.');}
    }catch(error){toast(readableError(error));}
  })();
});
async function action(name:string,id?:string,genomeId?:number):Promise<void>{
  if(name==='popout-player'){await openPlayerWindow();return;}
  if(name==='return-player'){playerView.close();return;}
  if(name==='focus-player'){playerView.focus();return;}
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
  if(name==='cancel-cell-tool'){clearCellTool();$('world')?.focus({preventScroll:true});return;}
  if(name==='move-cell'||name==='move-colony'){
    const colony=name==='move-colony';
    if(active&&selectedId!==null&&(!colony||active.world.cells.find(c=>c.id===selectedId)?.attachedNeighbors)){
      startCellTool({kind:'move',id:selectedId,colony});
    }
    return;
  }
  if(name==='edit-reserves'&&active&&selectedId!==null&&playerElement){
    const session=active,id=selectedId;
    if(await editReserves(session,id,playerElement.ownerDocument,()=>session===active && selectedId===id) && session===active){
      inspectorStamp=''; updateSimulationUi(); toast('Reserves updated for organism '+id+'.');
    }
    return;
  }
  if(name==='toggle-environment'||name==='close-environment'||name==='edit-live-environment'){
    clearCellTool();
    environmentOpen=name==='edit-live-environment'||name==='toggle-environment'&&!environmentOpen;
    if(!environmentOpen){environmentTool='select';environmentSelection=null;$('world')?.classList.remove('placing-object');}
    if(name==='edit-live-environment'){prefs.simulationView='player';persistPreferences();renderView();}
    else{if(environmentOpen){selectedId=null;renderInspector();}renderLiveEnvironment();}
    return;
  }
  if(name==='add-organism'){await refreshCatalogs(false);addOrganismDialog();return;}
  if(name==='close-inspector'){selectedId=null;renderInspector();return;}
  if(name==='memory-previous'||name==='memory-next'){memoryPage+=name==='memory-next'?1:-1;paintInspector();return;}
  if(name==='cell-program'||name==='cell-graph'){
    const cell=active?.world.cells.find(item=>item.id===selectedId);if(!cell||!active)return;
    if(name==='cell-program')await openLiveProgram(cell.genomeId, cell.id);else { const session=active; const graph=await session.engine.getGenomeGraph(cell.genomeId); if(active===session)genomeVisualizer.open(graph,genomeName(cell.genomeId)); }return;
  }
  if(name==='live-program'&&genomeId){await openLiveProgram(genomeId);return;}
  if(name==='live-graph'&&genomeId&&active){const session=active;const graph=await session.engine.getGenomeGraph(genomeId);if(active===session)genomeVisualizer.open(graph,genomeName(genomeId));return;}
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
function handleUiClick(event: MouseEvent): void {
  const target=(event.target as Element).closest<HTMLButtonElement>('button');if(!target)return;
  if(transitioning&&(target.dataset.tab||target.dataset.view||target.dataset.inspectorView||target.dataset.action)){event.preventDefault();return;}
  if(target.dataset.tab){void navigate(target.dataset.tab as UiPreferences['tab']);return;}
  if(target.dataset.view&&active){
    prefs.simulationView=target.dataset.view as UiPreferences['simulationView'];persistPreferences();renderView();
    const session=active;
    void session.refresh(viewPresentationOptions())
      .then(()=>{if(active===session)updateSimulationUi();}).catch(error=>runError(session,error));
    document.querySelector<HTMLButtonElement>('[data-view="'+prefs.simulationView+'"]')?.focus({preventScroll:true});return;
  }
  if(target.dataset.inspectorView){
    inspectorView=target.dataset.inspectorView as typeof inspectorView;renderInspector();
    queryUi<HTMLButtonElement>('[data-inspector-view="'+inspectorView+'"]')?.focus({preventScroll:true});return;
  }
  if(target.dataset.action){
    if ((event.target as Element).ownerDocument !== document && ['cell-program','cell-graph','live-program','live-graph'].includes(target.dataset.action)) window.focus();
    target.closest('details')?.removeAttribute('open');
    void action(target.dataset.action,target.dataset.id,Number(target.dataset.genome)).catch(error=>toast(readableError(error)));
  }
}
function handleUiInput(event: Event): void {
  const input=event.target as HTMLInputElement;
  if(input.id==='catalog-search'){catalogQuery=input.value;renderCatalogCards();}
  if(input.id==='memory-filter'){memoryFilter=input.value;memoryPage=0;paintInspector();}
  if(input.dataset.setting&&input.type==='range'){
    const output=$('value-'+input.dataset.setting);
    if(output)output.textContent=input.value+(input.dataset.setting==='light'||input.dataset.setting==='mutation'?'%':'');
  }
}
function handleUiChange(event: Event): void {
  const input=event.target as HTMLInputElement,key=input.dataset.setting;
  if(input.hasAttribute('data-repeat-placement') && cellTool?.kind==='place'){cellTool.repeat=input.checked;return;}
  if(input.id==='playback-speed'){
    if(!active||transitioning){updatePlayback();return;}
    const speed=Number(input.value);
    if(![1,2,5,10].includes(speed)){updatePlayback();return;}
    active.metadata.speed=speed as 1|2|5|10;simulationLoop.reset();active.touch();updatePlayback();void active.checkpoint().catch(()=>{});return;
  }
  if(!key||!active||transitioning)return;
  const session=active;
  try{
    if(key==='overlay'){
      prefs.overlay=input.value as UiPreferences['overlay'];persistPreferences();
      for (const id of ['setting-overlay','player-overlay']) { const select=$<HTMLSelectElement>(id); if(select)select.value=prefs.overlay; }
      if(playerIsVisible())void session.refresh({frame:false,environment:prefs.overlay!=='none' ? true : 'metadata'}).catch(error=>runError(session,error));
      return;
    }
    if(key==='showExtinct'){prefs.showExtinct=input.checked;persistPreferences();renderActiveGenomes();return;}
    if(key==='sensors'||key==='trails'||key==='colonyActivity'){prefs[key]=input.checked;persistPreferences();return;}
    if(key==='name'){
      session.metadata.name=validateName(input.value);
      const heading=document.querySelector('.simulation-title h1');if(heading)heading.textContent=session.metadata.name;
      session.touch();void session.checkpoint().catch(()=>{});return;
    }
    if(key!=='mutation')return;
    const value=inputNumber(input,'Mutation chance',0,100)/100;
    void session.mutate(async engine=>{await engine.setMutationRate(value);})
      .then(()=>session.checkpoint()).catch(error=>runError(session,error));
  }catch(error){toast(readableError(error));renderView();}
}
function handleUiKeydown(event: KeyboardEvent): void {
  if(event.key==='Escape' && cellTool && playerElement?.contains(event.target as Node)){
    event.preventDefault(); clearCellTool(); $('world')?.focus({preventScroll:true}); return;
  }
  if(event.key==='Escape')for(const details of document.querySelectorAll<HTMLDetailsElement>('.item-menu[open]')){details.open=false;details.querySelector<HTMLElement>('summary')?.focus();}
  const current=(event.target as HTMLElement).closest<HTMLButtonElement>('button[role="tab"]');
  if(!current||transitioning||!['ArrowLeft','ArrowRight','Home','End'].includes(event.key))return;
  const tabs=[...current.closest('[role="tablist"]')!.querySelectorAll<HTMLButtonElement>('button[role="tab"]')],index=tabs.indexOf(current);
  if(!tabs.length)return;
  event.preventDefault();const next=tabs[event.key==='Home'?0:event.key==='End'?tabs.length-1:(index+(event.key==='ArrowRight'?1:tabs.length-1))%tabs.length];
  if(next.dataset.tab)void navigate(next.dataset.tab as UiPreferences['tab']).then(()=>document.querySelector<HTMLButtonElement>('[data-tab="'+next.dataset.tab+'"]')?.focus());
  else {const selector=next.dataset.view?'[data-view="'+next.dataset.view+'"]':'[data-inspector-view="'+next.dataset.inspectorView+'"]';next.click();queryUi<HTMLButtonElement>(selector)?.focus();}
}
function handleUiDismissMenus(event: MouseEvent): void {
  for(const details of document.querySelectorAll<HTMLDetailsElement>('.item-menu[open]'))if(!details.contains(event.target as Node))details.open=false;
}
function bindUiDocument(doc: Document): () => void {
  doc.addEventListener('click', handleUiClick);
  doc.addEventListener('input', handleUiInput);
  doc.addEventListener('change', handleUiChange);
  doc.addEventListener('keydown', handleUiKeydown);
  doc.addEventListener('click', handleUiDismissMenus);
  return () => {
    doc.removeEventListener('click', handleUiClick); doc.removeEventListener('input', handleUiInput);
    doc.removeEventListener('change', handleUiChange); doc.removeEventListener('keydown', handleUiKeydown);
    doc.removeEventListener('click', handleUiDismissMenus);
  };
}
bindUiDocument(document);
function playerIsVisible(): boolean {
  return playerView.isOpen ? playerView.visible : !document.hidden && prefs.tab === 'simulation' && prefs.simulationView === 'player';
}
function viewPresentationOptions(): PresentationOptions {
  const player = playerIsVisible();
  const metrics = player && environmentOpen && liveGasesOpen || !document.hidden && prefs.tab === 'simulation' && (prefs.simulationView === 'metrics' || prefs.simulationView === 'genomes');
  return { frame: player, metrics, environment: player && prefs.overlay !== 'none' ? true
    : player || metrics && prefs.simulationView === 'metrics' ? 'metadata' : false };
}
function simulationWork(): SimulationWork<SimulationSession> | null {
  if (!active || unloading || transitioning || lifecycleFlushes || active.metadata.paused
    || prefs.tab !== 'simulation' && !playerView.isOpen || document.hidden && !playerView.isOpen) return null;
  return { target: active, speed: active.metadata.speed, busy: active.busy };
}
function advanceSimulation(session: SimulationSession, steps: number, now: number): Promise<void> {
  const visible = viewPresentationOptions();
  const frame = Boolean(visible.frame && now - lastPresentation >= 1000 / 30);
  const metrics = Boolean(visible.metrics && now - lastMetrics >= 250);
  const environment = visible.environment && now - lastEnvironment >= (playerIsVisible() ? 100 : 250) ? visible.environment : false;
  if (frame) lastPresentation = now;
  if (metrics) lastMetrics = now;
  if (environment) lastEnvironment = now;
  return session.advance(steps, { frame, metrics, environment });
}
function frame(now:number):void{
  try{
    simulationLoop.refresh();
    if(active&&(prefs.tab==='simulation'||playerView.isOpen)){
      const session=active, playerVisible=playerIsVisible();
      const uiVisible=!document.hidden||playerView.visible;
      if(renderer&&playerVisible)renderer.render(active.world,selectedId,prefs,cellInspection?.session === active && cellInspection.cell.id === selectedId ? cellInspection.cell : null,cellInspection?.session === active && cellInspection.cell.id === selectedId ? cellInspection.body ?? null : null);
      playerView.setTitle(session.metadata.name);
      if(now-lastCheckpoint>=2000){lastCheckpoint=now;if(!transitioning&&!saveFailed&&!session.pending&&session.revision!==session.savedRevision)void session.checkpoint().catch(()=>{});}
      if(uiVisible&&!transitioning&&!lifecycleFlushes&&now-lastUi>250){updateSimulationUi();lastUi=now;}
    }
  }catch(error){
    if(active){active.metadata.paused=true;active.touch();updatePlayback();}
    toast(readableError(error));
  }
}
async function flushLifecycle(session: SimulationSession): Promise<void> {
  lifecycleFlushes++;
  try {
    await session.refresh(viewPresentationOptions());
    await session.flush();
  }
  catch (error) { runError(session, error); }
  finally { lifecycleFlushes--; simulationLoop.reset(); }
}
function playerVisibilityChanged(): void {
  simulationLoop.reset();frameLoop.refresh();
  if(document.hidden&&!playerView.visible&&active)void flushLifecycle(active);
  else if(active){
    // Hidden stepping omits field projections. Refresh them even if playback
    // paused in the background, so returning never leaves a stale overlay.
    const session=active;
    void session.refresh(viewPresentationOptions()).catch(error=>runError(session,error));
  }
}
document.addEventListener('visibilitychange',()=>{
  playerVisibilityChanged();
  if(!document.hidden&&ready&&!transitioning)void refreshCatalogs().catch(error=>toast(readableError(error)));
});
window.addEventListener('pagehide',event=>{
  unloading=true;simulationLoop.stop();frameLoop.stop();playerView.close('unload');
  if(active){const session=active;void flushLifecycle(session).finally(()=>{if(!event.persisted)disposeSession(session);});}
});
window.addEventListener('pageshow',()=>{unloading=false;if(ready){renderView();simulationLoop.start();frameLoop.start();}renderer?.resize();});
libraryChannel?.addEventListener('message',()=>{if(ready&&!transitioning)void refreshCatalogs().catch(error=>toast(readableError(error)));});
window.addEventListener('focus',()=>{if(ready&&!transitioning)void refreshCatalogs().catch(error=>toast(readableError(error)));});
async function recoverPreviousDraft():Promise<void>{
  let source:string|null=null;
  try{source=localStorage.getItem('cellarium.genome.v1');}catch{return;}
  if(!source)return;
  if(allGenomes().some(item=>item.source===source)){try{localStorage.removeItem('cellarium.genome.v1');}catch{}return;}
  try{
    const engine=await compiler();source=engine.upgradeContactSource(source);
    engine.compile(source);await persistGenome(newGenome('Recovered genome',source));
    localStorage.removeItem('cellarium.genome.v1');toast('Your previous genome was added to Genomes.');
  }catch{toast('Your previous draft remains saved; it could not be imported automatically.');}
}
async function start():Promise<void>{
  try{
    storage=await Storage.open();
    [savedGenomes,savedEnvironments,simulations]=await Promise.all([readSavedGenomes(),storage.listEnvironments(),storage.listSimulationSummaries()]);
    try { await upgradePreviousGenomeDraft(async source => (await compiler()).upgradeContactSource(source)); }
    catch { /* Preserve incomplete drafts so the editor can show their diagnostics. */ }
    await recoverPreviousDraft();
    ready=true;saveStatus('saved');if(preferenceWarning)toast('Preferences could not be loaded. '+readableError(preferenceWarning));
    const tab=prefs.tab,view=prefs.simulationView,id=prefs.activeSimulationId;
    if(id&&simulations.some(item=>item.id===id)){
      await openSimulation(id);prefs.tab=tab;prefs.simulationView=view;persistPreferences();
    }else{prefs.activeSimulationId=null;persistPreferences();}
    renderView();simulationLoop.start();frameLoop.start();
  }catch(error){
    $('save-state').textContent='Unavailable';
    $('workspace').innerHTML=`<div class="empty-state"><h1>agar could not open</h1><p>${esc(readableError(error))}</p><button class="button" id="retry-start">Try again</button></div>`;
    $('retry-start').addEventListener('click',()=>location.reload());
  }
}
void start();

