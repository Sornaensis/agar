import { Engine, type MutationKind, type SourceKind } from './engine';
import { SOURCE_NAMES, SOURCE_FIELDS, defaultSource, defaultBarrier, toolOptions, toolName, sourceFields, barrierFields, readObjectFields, objectList, cameraControls, type EnvironmentTool, type ObjectSelection, type SourceConfig, type BarrierConfig } from './environment-controls';
import { WorkerEngine } from './worker-engine';
import type { RenderWorldSnapshot } from './session';
import { Renderer, type FieldOverlay } from './renderer';
import {
  GENOME_LANGUAGE_VERSION, clearEnvironmentDraft, clearGenomeDraft, loadEnvironmentDraft, loadGenomeDraft,
  newEnvironment, newGenome, saveEnvironmentDraft, saveGenomeDraft, validateEnvironmentConfig,
  type EnvironmentConfig, type EnvironmentDocument, type GenomeDocument,
} from './storage';
import { createDialog, escapeHtml, icon, inputNumber, setDialogError } from './ui';
import { ProgramEditor } from './program-editor';
import { PRESETS } from './presets';
import { isCellularSource } from './genome-language';
import { gasFields, readGasConfig } from './gas-controls';

export interface CatalogCallbacks {
  compiler: () => Promise<Engine>;
  saveGenome: (document: GenomeDocument) => Promise<void>;
  saveEnvironment: (document: EnvironmentDocument) => Promise<void>;
  graph: (source: string, name: string) => Promise<void>;
  startSimulation: (environmentId: string) => void;
  notify: (message: string) => void;
}

const DEFAULT_ENVIRONMENT: EnvironmentConfig = {
  width: 1200, height: 760, foodRate: 6, ambientTemperature: 0.5, ambientLight: 0, currentStrength: 0, initialNutrients: 0.3, sources: [], barriers: [],
  gases: { initialOxygen: 1, initialCarbon: 1, mode: 'sealed', exchangeRate: 0.02, oxygenEquilibrium: 1, carbonEquilibrium: 1 },
};
// Use the same authored construction program as the built-in producer.
const DEFAULT_GENOME = PRESETS.find(preset => preset.name === 'Phototroph')!.source;
const alive = (dialog: HTMLDialogElement): boolean => dialog.open && dialog.isConnected;
const message = (error: unknown): string => error instanceof Error ? error.message : String(error);
const copyName = (name: string): string => `${name.slice(0, 74)} copy`;
const query = <T extends Element = HTMLElement>(dialog: Element, selector: string): T => dialog.querySelector<T>(selector)!;

function clearError(dialog: HTMLDialogElement): void {
  const error = dialog.querySelector<HTMLElement>('.form-error');
  if (error) { error.textContent = ''; error.hidden = true; }
}

/** A save captures one draft; keep its controls and close actions stable until it completes. */
function operations(dialog: HTMLDialogElement, onSettled?: () => void): (task: () => Promise<void>) => void {
  let busy = false;
  dialog.addEventListener('cancel', event => { if (busy) event.preventDefault(); });
  return task => {
    if (busy || !alive(dialog)) return;
    busy = true;
    clearError(dialog);
    const controls = [...dialog.querySelectorAll<HTMLInputElement | HTMLButtonElement | HTMLSelectElement | HTMLTextAreaElement>(
      'input,button,select,textarea',
    )].map(control => ({ control, disabled: control.disabled }));
    for (const item of controls) item.control.disabled = true;
    dialog.setAttribute('aria-busy', 'true');
    dialog.dataset.saving = 'true';
    void task().catch(error => { if (alive(dialog)) setDialogError(dialog, error); }).finally(() => {
      busy = false;
      dialog.removeAttribute('aria-busy');
      delete dialog.dataset.saving;
      for (const item of controls) item.control.disabled = item.disabled;
      onSettled?.();
    });
  };
}

/** Catalog editing never touches a running world's engine or a saved source document. */
export class CatalogUI {
  constructor(private readonly callbacks: CatalogCallbacks) {}

  openGenome(document?: GenomeDocument, edit = false): void {
    const editable = edit || !document;
    let draftError: unknown;
    let draft: ReturnType<typeof loadGenomeDraft> = null;
    if (!document) {
      try { draft = loadGenomeDraft(); } catch (error) { draftError = error; }
    }
    const name = document ? (editable ? copyName(document.name) : document.name) : (draft?.name ?? 'Untitled genome');
    const source = document?.source ?? draft?.source ?? DEFAULT_GENOME;
    const title = editable ? (document ? 'Edit copy' : 'New genome') : document!.name;
    const dialog = createDialog(title, `
      ${editable ? `<div class="editor-name-row"><label for="genome-name">Name</label><input id="genome-name" name="name" maxlength="80" value="${escapeHtml(name)}" autocomplete="off"></div>` : ''}
      ${!editable && document?.description ? `<p class="editor-description">${escapeHtml(document.description)}</p>` : ''}
      ${document && document.languageVersion !== GENOME_LANGUAGE_VERSION ? '<p class="compatibility-notice">This source uses the older investment language. The original stays preserved; update a copy to organelle targets and physical sensing before running it.</p>' : ''}
      <div class="editor-toolbar">
        <div class="toolbar">
          ${editable ? `<div class="editor-history" role="group" aria-label="Edit history"><button type="button" class="icon-button" data-undo aria-label="Undo" title="Undo (Ctrl/Cmd+Z)" disabled>${icon('undo')}</button><button type="button" class="icon-button" data-redo aria-label="Redo" title="Redo (Ctrl/Cmd+Shift+Z)" disabled>${icon('redo')}</button></div>` : ''}
          <button type="button" class="button subtle" data-graph>${icon('graph')}Graph</button>
          ${editable ? `<button type="button" class="button subtle" data-check title="Check types (Ctrl/Cmd+Enter)">${icon('check')}Check</button>` : ''}
        </div>
        ${editable ? `<details class="mutation-menu"><summary class="button subtle">${icon('dna')}Mutate</summary><div class="mutation-options">${['Any change','Constants','Expressions','References','Silence a gene','Duplicate a gene','Change placement'].map((label, kind) => `<button type="button" data-mutation-kind="${kind}"${kind >= 4 && !isCellularSource(source) ? ' hidden' : ''}>${label}</button>`).join('')}</div></details>` : ''}
      </div>
      <div id="genome-source" class="program-editor-host"></div>
      <footer class="editor-footer dialog-actions">
        <div class="editor-feedback">${editable ? '<span class="editor-save-state">Unsaved</span>' : ''}<p class="form-success" role="status" data-program-status hidden></p><p class="form-error" role="alert" hidden></p></div>
        ${editable ? `<button type="button" class="button primary" data-save title="Save (Ctrl/Cmd+S)">${icon('save')}${document ? 'Save copy' : 'Save genome'}</button>` : `<button type="button" class="button primary" data-copy>${icon('copy')}Edit copy</button>`}
      </footer>
    `, 'editor-dialog genome-editor-dialog');
    const nameInput = editable ? query<HTMLInputElement>(dialog, '#genome-name') : null;
    const status = query<HTMLElement>(dialog, '[data-program-status]');
    let saved = false;
    let draftTimer = 0;
    let editor: ProgramEditor;
    const updateActions = (): void => {
      if (!editable || !editor || !alive(dialog)) return;
      const busy = dialog.getAttribute('aria-busy') === 'true';
      query<HTMLButtonElement>(dialog, '[data-undo]').disabled = busy || !editor.canUndo;
      query<HTMLButtonElement>(dialog, '[data-redo]').disabled = busy || !editor.canRedo;
      dialog.querySelectorAll<HTMLButtonElement>('[data-mutation-kind="4"],[data-mutation-kind="5"],[data-mutation-kind="6"]').forEach(button => { button.hidden = !isCellularSource(editor.value); });
    };
    const run = operations(dialog, updateActions);
    const withEditor = (task: () => Promise<void>): void => run(async () => {
      editor.setReadOnly(true);
      try { await task(); }
      finally { if (alive(dialog)) editor.setReadOnly(!editable); }
    });
    const persistDraft = (): void => {
      window.clearTimeout(draftTimer);
      if (editable && !saved) saveGenomeDraft({ name: nameInput!.value, source: editor.value });
    };
    const changed = (): void => {
      status.textContent = ''; status.hidden = true;
      clearError(dialog);
      window.clearTimeout(draftTimer);
      draftTimer = window.setTimeout(() => {
        if (!alive(dialog)) return;
        try { persistDraft(); } catch (error) { setDialogError(dialog, error); }
      }, 250);
      updateActions();
    };
    const save = (): void => {
      if (!editable) return;
      withEditor(async () => {
        if (!(await editor.check()) || !alive(dialog)) return;
        const next = newGenome(nameInput!.value, editor.value, document?.description);
        await this.callbacks.saveGenome(next);
        saved = true;
        let suffix = '';
        try { clearGenomeDraft(); } catch (error) { suffix = ` The saved draft could not be cleared: ${message(error)}`; }
        if (alive(dialog)) dialog.close();
        this.callbacks.notify(`${next.name} saved.${suffix}`);
      });
    };
    editor = new ProgramEditor({
      parent: query<HTMLElement>(dialog, '#genome-source'), source, readOnly: !editable,
      compiler: this.callbacks.compiler, onChange: changed, onSave: save, onStateChange: updateActions,
    });
    dialog.addEventListener('close', () => {
      window.clearTimeout(draftTimer);
      if (editable && !saved) {
        try { persistDraft(); } catch (error) { this.callbacks.notify(`Draft could not be saved: ${message(error)}`); }
      }
      editor.destroy();
    }, { once: true });
    query<HTMLButtonElement>(dialog, '[data-graph]').addEventListener('click', () => withEditor(async () => {
      if (!(await editor.check()) || !alive(dialog)) return;
      const currentName = nameInput?.value.trim() || document?.name || 'Genome';
      await this.callbacks.graph(editor.value, currentName);
    }));
    if (!editable) {
      query<HTMLButtonElement>(dialog, '[data-copy]').addEventListener('click', () => {
        dialog.close(); this.openGenome(document, true);
      });
      return;
    }
    nameInput!.addEventListener('input', changed);
    query<HTMLButtonElement>(dialog, '[data-undo]').addEventListener('click', () => { editor.undo(); editor.focus(); });
    query<HTMLButtonElement>(dialog, '[data-redo]').addEventListener('click', () => { editor.redo(); editor.focus(); });
    query<HTMLButtonElement>(dialog, '[data-check]').addEventListener('click', () => withEditor(async () => { await editor.check(); }));
    const mutationMenu = query<HTMLDetailsElement>(dialog, '.mutation-menu');
    mutationMenu.querySelectorAll<HTMLButtonElement>('[data-mutation-kind]').forEach(button => button.addEventListener('click', () => {
      const kind = Number(button.dataset.mutationKind) as MutationKind;
      mutationMenu.open = false;
      withEditor(async () => {
        if (!(await editor.check()) || !alive(dialog)) return;
        const compiler = await this.callbacks.compiler();
        if (!alive(dialog)) return;
        const id = compiler.compile(editor.value);
        const seed = crypto.getRandomValues(new Uint32Array(1))[0];
        const result = compiler.mutate(id, seed, kind);
        if (result.changed) editor.setValue(compiler.getGenome(result.id).source);
        status.textContent = result.changed ? 'Mutation applied · Undo to revert' : (result.reason || 'No legal change was available.');
        status.hidden = false;
        persistDraft();
      });
    }));
    dialog.addEventListener('click', event => { if (!mutationMenu.contains(event.target as Node)) mutationMenu.open = false; });
    dialog.addEventListener('keydown', event => {
      if (event.key === 'Escape' && mutationMenu.open) {
        event.preventDefault(); event.stopPropagation(); mutationMenu.open = false;
        query<HTMLElement>(mutationMenu, 'summary').focus();
      }
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's' && !event.defaultPrevented) {
        event.preventDefault(); save();
      }
    });
    query<HTMLButtonElement>(dialog, '[data-save]').addEventListener('click', save);
    if (draftError) setDialogError(dialog, draftError);
    nameInput!.focus(); nameInput!.select();
  }

  openEnvironment(document?: EnvironmentDocument, edit = false): void {
    const editable = edit || !document;
    let draftError: unknown;
    let draft: ReturnType<typeof loadEnvironmentDraft> = null;
    if (!document) { try { draft = loadEnvironmentDraft(); } catch (error) { draftError = error; } }
    const name = document ? (editable ? copyName(document.name) : document.name) : (draft?.name ?? 'Untitled environment');
    let config = validateEnvironmentConfig(document?.config ?? draft?.config ?? DEFAULT_ENVIRONMENT);
    const numberField = (key: string, label: string, value: number, min: number, max: number, step: string): string =>
      `<div class="form-field"><label for="environment-${key}">${label}</label><input type="number" id="environment-${key}" value="${value}" min="${min}" max="${max}" step="${step}"${editable ? '' : ' readonly'}></div>`;
    const dialog = createDialog(editable ? (document ? 'Edit copy' : 'New environment') : document!.name, `
      ${editable ? `<div class="editor-name-row"><label for="environment-name">Name</label><input id="environment-name" maxlength="80" value="${escapeHtml(name)}" autocomplete="off"></div>` : ''}
      <div class="environment-editor">
        <div class="environment-preview">
          <div class="preview-toolbar">
            ${editable ? `<label class="environment-tool-select"><span class="visually-hidden">Environment tool</span><select data-environment-tool aria-label="Environment tool">${toolOptions()}</select></label>` : ''}
            <select id="preview-overlay" aria-label="Preview field"><option value="light">Light</option><option value="heat">Temperature</option><option value="flow">Water flow</option><option value="nutrients">Minerals</option><option value="oxygen">Oxygen</option><option value="carbon">Inorganic carbon</option><option value="none">No overlay</option></select>
            ${cameraControls('preview')}
          </div>
          <canvas class="environment-canvas" aria-label="Environment preview. Select an object to edit it." tabindex="0"></canvas>
          <p class="preview-status" role="status">Loading preview…</p><p class="preview-caption"></p>
          <p class="environment-navigation-hint">Scroll to zoom · Shift-drag or right-drag to pan · Fit to see the whole world</p>
        </div>
        <aside class="environment-rail">
          <section class="environment-form environment-size"><h3>World size</h3>
            ${numberField('width', 'Width', config.width, 400, 4800, '20')}${numberField('height', 'Height', config.height, 400, 3040, '20')}
          </section>
          <section class="source-section"><h3>Objects</h3><div class="source-list" aria-label="Environment objects"></div><div class="source-detail"></div></section>
          <details class="environment-baseline gas-settings"><summary>Gases & atmosphere</summary>${gasFields(config.gases, editable, true)}</details>
          <details class="environment-baseline"><summary>Background & starting conditions</summary><div class="environment-form">
            ${numberField('food', 'Organic input / s', config.foodRate, 0, 60, '0.5')}
            ${numberField('light', 'Ambient light', config.ambientLight, 0, 1, '0.05')}
            ${numberField('temperature', 'Temperature', config.ambientTemperature, 0, 1, '0.05')}
            ${numberField('current', 'Background flow', config.currentStrength, 0, 30, '1')}
            ${numberField('nutrients', 'Starting nutrients', config.initialNutrients, 0, 5, '0.1')}
          </div></details>
        </aside>
      </div>
      <footer class="editor-footer dialog-actions"><div class="editor-feedback"><p class="form-error" role="alert" hidden></p></div>
        ${editable ? `<button type="button" class="button primary" data-save>${document ? 'Save copy' : 'Save environment'}</button>` : `<button type="button" class="button subtle" data-copy>${icon('copy')}Edit copy</button><button type="button" class="button primary" data-use>${icon('play')}Use environment</button>`}
      </footer>`, 'editor-dialog environment-editor-dialog');
    const run = operations(dialog), nameInput = editable ? query<HTMLInputElement>(dialog, '#environment-name') : null;
    const canvas = query<HTMLCanvasElement>(dialog, '.environment-canvas'), list = query<HTMLElement>(dialog, '.source-list');
    const detail = query<HTMLElement>(dialog, '.source-detail'), overlay = query<HTMLSelectElement>(dialog, '#preview-overlay');
    const status = query<HTMLElement>(dialog, '.preview-status'), caption = query<HTMLElement>(dialog, '.preview-caption');
    const toolInput = dialog.querySelector<HTMLSelectElement>('[data-environment-tool]');
    let selected: ObjectSelection | null = config.sources.length ? { kind: 'source', id: 0 } : config.barriers.length ? { kind: 'barrier', id: 0 } : null;
    let tool: EnvironmentTool = 'select';
    const sourceDefaults = SOURCE_NAMES.map((_, kind) => defaultSource(kind as SourceKind, 0, 0));
    let barrierDefault = defaultBarrier(false, 0, 0);
    let engine: WorkerEngine | null = null, renderer: Renderer | null = null, snapshot: RenderWorldSnapshot | null = null;
    let busy = false, revision = 0, visibleRevision = -1, refreshTimer = 0, saved = false;
    const collect = (): EnvironmentConfig => {
      if (!editable) return structuredClone(config);
      const next = structuredClone(config);
      next.width = inputNumber(query(dialog, '#environment-width'), 'Width', 400, 4800);
      next.height = inputNumber(query(dialog, '#environment-height'), 'Height', 400, 3040);
      if (selected?.kind === 'source') next.sources[selected.id] = readObjectFields(detail, next.sources[selected.id], Math.max(config.width, next.width), Math.max(config.height, next.height));
      if (selected?.kind === 'barrier') next.barriers[selected.id] = readObjectFields(detail, next.barriers[selected.id], Math.max(config.width, next.width), Math.max(config.height, next.height));
      for (const object of [...next.sources, ...next.barriers]) { object.x %= next.width; object.y %= next.height; }
      next.foodRate = inputNumber(query(dialog, '#environment-food'), 'Organic input', 0, 60);
      next.ambientLight = inputNumber(query(dialog, '#environment-light'), 'Ambient light', 0, 1);
      next.ambientTemperature = inputNumber(query(dialog, '#environment-temperature'), 'Temperature', 0, 1);
      next.currentStrength = inputNumber(query(dialog, '#environment-current'), 'Background flow', 0, 30);
      next.initialNutrients = inputNumber(query(dialog, '#environment-nutrients'), 'Starting nutrients', 0, 5);
      next.gases = readGasConfig(dialog);
      return validateEnvironmentConfig(next);
    };
    const persist = (): void => {
      if (!editable || saved) return;
      saveEnvironmentDraft({ name: nameInput!.value, config });
    };
    const highlight = (): void => {
      const object = selected && snapshot && visibleRevision === revision ? (selected.kind === 'source' ? snapshot.environment.sources[selected.id] : snapshot.environment.barriers[selected.id]) : null;
      renderer?.setEnvironmentSelection(object && selected ? { kind: selected.kind, id: object.id } : null);
    };
    const draw = (): void => { if (renderer && snapshot) { highlight(); renderer.render(snapshot, null, { overlay: overlay.value as FieldOverlay, sensors: false, trails: false }); } };
    const refresh = (): void => {
      if (!alive(dialog) || !engine) return;
      revision++; highlight();
      if (busy) return;
      status.hidden = false; status.textContent = 'Updating preview…';
      busy = true;
      const worker = engine;
      void (async () => {
        while (alive(dialog) && engine === worker) {
          const stamp = revision, next = structuredClone(config);
          await worker.clearBarriers();
          if (!await worker.setWorldSize(next.width, next.height)) throw new Error('The world size could not be changed.');
          await worker.setEnvironment(next.ambientTemperature, next.ambientLight, next.currentStrength);
          await worker.setInitialNutrients(next.initialNutrients); await worker.setFoodRate(next.foodRate);
          if (!await worker.setInitialGases(next.gases.initialOxygen, next.gases.initialCarbon) || !await worker.setGasExchange(next.gases)) throw new Error('The gas settings could not be applied.');
          if (!await worker.replaceSources(next.sources)) throw new Error('The emitters could not be added.');
          // Settle the fields without moving authored geometry. The preview
          // then restores each motion flag at the exact saved coordinates.
          const barrierIds: number[] = [];
          for (const barrier of next.barriers) {
            const id = await worker.addBarrier(barrier.x, barrier.y, barrier.length, barrier.width, barrier.angle, false);
            if (!id) throw new Error('A barrier could not be added.');
            barrierIds.push(id);
          }
          await worker.resetEmpty(42, 0);
          const settled = await worker.advance(60, { frame: false });
          if (settled.error) throw new Error(settled.error);
          if (!alive(dialog) || engine !== worker) return;
          if (stamp !== revision) continue;
          for (let index = 0; index < next.barriers.length; index++) {
            const barrier = next.barriers[index];
            if (barrier.floating && !await worker.updateBarrier(barrierIds[index], barrier.x, barrier.y, barrier.length, barrier.width, barrier.angle, true)) throw new Error('The preview barrier could not be restored.');
          }
          const presentation = await worker.presentation({ environment: true });
          if (presentation.error) throw new Error(presentation.error);
          if (!presentation.frame || !presentation.environment || !('samples' in presentation.environment)) throw new Error('The preview snapshot is incomplete.');
          if (!alive(dialog) || engine !== worker) return;
          if (stamp !== revision) continue;
          snapshot = { ...presentation.frame, environment: presentation.environment }; visibleRevision = stamp;
          draw(); status.textContent = ''; status.hidden = true; return;
        }
      })().catch(error => { if (alive(dialog)) { status.hidden = false; status.textContent = 'Preview unavailable'; setDialogError(dialog, error); } }).finally(() => { busy = false; });
    };
    const renderList = (): void => { list.innerHTML = objectList(config.sources, config.barriers, selected); };
    const renderDetail = (): void => {
      if (tool !== 'select') {
        const sourceKind = tool.startsWith('source:') ? Number(tool.split(':')[1]) as SourceKind : null;
        detail.innerHTML = `<div class="source-detail-heading"><h4>Place ${toolName(tool).toLowerCase()}</h4></div>` + (sourceKind !== null ? sourceFields(sourceDefaults[sourceKind], config.width, config.height, true, false) : barrierFields(barrierDefault, config.width, config.height, true, false));
      } else if (selected) {
        const source = selected.kind === 'source' ? config.sources[selected.id] : null;
        const barrier = selected.kind === 'barrier' ? config.barriers[selected.id] : null;
        const label = source ? SOURCE_NAMES[source.kind] : barrier!.floating ? 'Floating barrier' : 'Fixed barrier';
        detail.innerHTML = `<div class="source-detail-heading"><h4>${label}</h4>${editable ? `<button type="button" class="icon-button source-remove" data-remove-object aria-label="Remove selected object">${icon('trash')}</button>` : ''}</div>`
          + (source ? sourceFields(source, config.width, config.height, editable) : barrierFields(barrier!, config.width, config.height, editable));
      } else detail.innerHTML = '<p class="environment-hint">Select an emitter or barrier to tune it.</p>';
      caption.textContent = tool === 'select' ? `${config.width} × ${config.height} world · ${config.sources.length + config.barriers.length} objects` : `Click the world to place ${toolName(tool).toLowerCase()}. Enter places it at the view center.`;
      if (toolInput) toolInput.value = tool;
      canvas.classList.toggle('placing-object', tool !== 'select');
      highlight();
    };
    const flush = (): boolean => {
      window.clearTimeout(refreshTimer); const next = collect();
      const modified = JSON.stringify(next) !== JSON.stringify(config);
      config = next; persist(); return modified;
    };
    const changed = (): void => {
      if (!editable || dialog.getAttribute('aria-busy') === 'true') return;
      window.clearTimeout(refreshTimer);
      refreshTimer = window.setTimeout(() => {
        if (!alive(dialog)) return;
        try { const oldWidth = config.width, oldHeight = config.height, modified = flush(); renderList(); if (oldWidth !== config.width || oldHeight !== config.height) renderDetail(); if (modified) refresh(); clearError(dialog); }
        catch (error) { setDialogError(dialog, error); }
      }, 220);
    };
    const choose = (next: ObjectSelection | null): void => {
      if (dialog.getAttribute('aria-busy') === 'true') return;
      try {
        const modified = flush(); selected = next; tool = 'select';
        if (selected?.kind === 'source') overlay.value = SOURCE_FIELDS[config.sources[selected.id].kind];
        renderList(); renderDetail(); if (modified) refresh(); else draw(); clearError(dialog);
      } catch (error) { setDialogError(dialog, error); }
    };
    const place = (x: number, y: number): void => {
      if (!editable || tool === 'select' || busy || dialog.getAttribute('aria-busy') === 'true') return;
      try {
        flush();
        const candidate = structuredClone(config);
        let nextSelection: ObjectSelection;
        if (tool.startsWith('source:')) {
          if (candidate.sources.length >= 64) throw new Error('An environment can contain at most 64 emitters.');
          const kind = Number(tool.split(':')[1]) as SourceKind;
          const source = readObjectFields(detail, sourceDefaults[kind], config.width, config.height);
          candidate.sources.push({ ...source, x, y }); nextSelection = { kind: 'source', id: candidate.sources.length - 1 };
          const validated = validateEnvironmentConfig(candidate);
          sourceDefaults[kind] = source; config = validated;
        } else {
          if (candidate.barriers.length >= 64) throw new Error('An environment can contain at most 64 barriers.');
          const barrier = readObjectFields(detail, barrierDefault, config.width, config.height);
          candidate.barriers.push({ ...barrier, x, y }); nextSelection = { kind: 'barrier', id: candidate.barriers.length - 1 };
          const validated = validateEnvironmentConfig(candidate);
          barrierDefault = barrier; config = validated;
        }
        selected = nextSelection; tool = 'select'; persist(); renderList(); renderDetail(); refresh(); clearError(dialog);
      } catch (error) { setDialogError(dialog, error); }
    };
    list.addEventListener('click', event => {
      const button = (event.target as Element).closest<HTMLButtonElement>('[data-object-kind]');
      if (button) choose({ kind: button.dataset.objectKind as 'source' | 'barrier', id: Number(button.dataset.objectId) });
    });
    detail.addEventListener('input', () => { if (tool === 'select') changed(); });
    detail.addEventListener('change', () => { if (tool === 'select') changed(); });
    detail.addEventListener('click', event => {
      if (!(event.target as Element).closest('[data-remove-object]') || !selected || !editable) return;
      try {
        // Removing an object remains possible even when its draft fields are invalid.
        window.clearTimeout(refreshTimer);
        if (selected.kind === 'source') config.sources.splice(selected.id, 1); else config.barriers.splice(selected.id, 1);
        selected = null; persist(); renderList(); renderDetail(); refresh(); clearError(dialog);
      } catch (error) { setDialogError(dialog, error); }
    });
    toolInput?.addEventListener('change', () => {
      try {
        const next = toolInput.value as EnvironmentTool, modified = flush(); selected = null; tool = next;
        if (tool.startsWith('source:')) overlay.value = SOURCE_FIELDS[Number(tool.split(':')[1])];
        else if (tool !== 'select') { barrierDefault = defaultBarrier(tool === 'barrier:floating', 0, 0); overlay.value = 'flow'; }
        renderList(); renderDetail(); if (modified) refresh(); else draw(); clearError(dialog);
      } catch (error) { toolInput.value = tool; setDialogError(dialog, error); }
    });
    canvas.addEventListener('click', event => {
      if (!renderer || !snapshot || renderer.wasDragging() || busy) return;
      if (tool !== 'select') { const position = renderer.toWorld(event.clientX, event.clientY); if (position) place(position.x, position.y); return; }
      if (busy) return;
      const source = renderer.pickSource(snapshot, event.clientX, event.clientY);
      const barrier = source ? null : renderer.pickBarrier(snapshot, event.clientX, event.clientY);
      choose(source ? { kind: 'source', id: snapshot.environment.sources.findIndex(item => item.id === source.id) } : barrier ? { kind: 'barrier', id: snapshot.environment.barriers.findIndex(item => item.id === barrier.id) } : null);
    });
    canvas.addEventListener('keydown', event => {
      if (event.key === 'Enter' && tool !== 'select') { event.preventDefault(); const center = renderer?.getCamera(); place(center?.x ?? config.width / 2, center?.y ?? config.height / 2); }
      if (event.key === 'Escape' && tool !== 'select') { event.preventDefault(); tool = 'select'; renderDetail(); }
    });
    dialog.querySelectorAll<HTMLButtonElement>('[data-camera]').forEach(button => button.addEventListener('click', () => {
      if (button.dataset.camera === 'preview:fit') renderer?.fitWorld(); else renderer?.zoomBy(button.dataset.camera === 'preview:in' ? 1.25 : .8);
    }));
    overlay.addEventListener('change', draw);
    dialog.addEventListener('close', () => {
      window.clearTimeout(refreshTimer); renderer?.dispose(); engine?.dispose(); renderer = null; engine = null; snapshot = null;
      if (editable && !saved) { try { config = collect(); persist(); } catch (error) { this.callbacks.notify(`Draft could not be saved: ${message(error)}`); } }
    }, { once: true });
    renderList(); renderDetail();
    void WorkerEngine.load().then(worker => {
      if (!alive(dialog)) { worker.dispose(); return; }
      engine = worker; renderer = new Renderer(canvas); refresh();
    }).catch(error => { if (alive(dialog)) { status.textContent = 'Preview unavailable'; setDialogError(dialog, error); } });
    if (!editable) {
      query<HTMLButtonElement>(dialog, '[data-copy]').addEventListener('click', () => { dialog.close(); this.openEnvironment(document, true); });
      query<HTMLButtonElement>(dialog, '[data-use]').addEventListener('click', () => { try { this.callbacks.startSimulation(document!.id); dialog.close(); } catch (error) { setDialogError(dialog, error); } });
      return;
    }
    nameInput!.addEventListener('input', changed);
    dialog.querySelectorAll<HTMLInputElement>('.environment-form input').forEach(input => input.addEventListener('input', changed));
    dialog.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-gas-field]').forEach(input => input.addEventListener('change', changed));
    query<HTMLButtonElement>(dialog, '[data-save]').addEventListener('click', () => run(async () => {
      config = collect();
      const next = newEnvironment(nameInput!.value, config, document?.description);
      await this.callbacks.saveEnvironment(next); saved = true;
      let suffix = ''; try { clearEnvironmentDraft(); } catch (error) { suffix = ` The saved draft could not be cleared: ${message(error)}`; }
      if (alive(dialog)) dialog.close(); this.callbacks.notify(`${next.name} saved.${suffix}`);
    }));
    if (draftError) setDialogError(dialog, draftError);
    nameInput!.focus(); nameInput!.select();
  }
}
