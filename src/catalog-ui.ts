import { Engine, type MutationKind, type SourceKind } from './engine';
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

export interface CatalogCallbacks {
  compiler: () => Promise<Engine>;
  saveGenome: (document: GenomeDocument) => Promise<void>;
  saveEnvironment: (document: EnvironmentDocument) => Promise<void>;
  graph: (source: string, name: string) => Promise<void>;
  startSimulation: (environmentId: string) => void;
  notify: (message: string) => void;
}

const SOURCE_NAMES = ['Light', 'Heat / cool', 'Vortex', 'Nutrient spring'] as const;
const SOURCE_TOOLS = ['Light', 'Heat', 'Vortex', 'Spring'] as const;
const SOURCE_ICONS = ['light', 'heat', 'flow', 'plus'] as const;
const SOURCE_FIELDS: FieldOverlay[] = ['light', 'heat', 'flow', 'nutrients'];
const DEFAULT_ENVIRONMENT: EnvironmentConfig = {
  foodRate: 6, ambientTemperature: 0.5, ambientLight: 0, currentStrength: 0, initialNutrients: 0.3, sources: [],
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
    if (!document) {
      try { draft = loadEnvironmentDraft(); } catch (error) { draftError = error; }
    }
    const name = document ? (editable ? copyName(document.name) : document.name) : (draft?.name ?? 'Untitled environment');
    let config = validateEnvironmentConfig(document?.config ?? draft?.config ?? DEFAULT_ENVIRONMENT);
    const title = editable ? (document ? 'Edit copy' : 'New environment') : document!.name;
    const numberField = (key: string, label: string, value: number, min: number, max: number, step: string): string => `
      <div class="form-field"><label for="environment-${key}">${label}</label><input type="number" id="environment-${key}" value="${value}" min="${min}" max="${max}" step="${step}" ${editable ? '' : 'readonly'}></div>`;
    const dialog = createDialog(title, `
      ${editable ? `<div class="editor-name-row"><label for="environment-name">Name</label><input id="environment-name" name="name" maxlength="80" value="${escapeHtml(name)}" autocomplete="off"></div>` : ''}
      <div class="environment-editor">
        <div class="environment-preview">
          <div class="preview-toolbar">
            ${editable ? `<div class="source-toolbar" role="group" aria-label="Source tools"><button type="button" data-source-tool="select">Select</button>${SOURCE_TOOLS.map((label, kind) => `<button type="button" data-source-kind="${kind}" title="${SOURCE_NAMES[kind]}">${icon(SOURCE_ICONS[kind])}${label}</button>`).join('')}</div>` : ''}
            <div class="preview-field"><label class="visually-hidden" for="preview-overlay">Preview field</label><select id="preview-overlay" aria-label="Preview field"><option value="light">Light</option><option value="heat">Temperature</option><option value="flow">Water flow</option><option value="nutrients">Nutrients</option><option value="none">No overlay</option></select></div>
          </div>
          <canvas class="environment-canvas" aria-label="Environment preview. Click a source to select it." tabindex="0"></canvas>
          <p class="preview-status" role="status">Loading preview…</p>
          <p class="preview-caption"></p>
        </div>
        <aside class="environment-rail">
          <section class="environment-form"><h3>Habitat</h3>
            ${numberField('food', 'External organic input / s', config.foodRate, 0, 60, '0.5')}
            ${numberField('light', 'Ambient light', config.ambientLight, 0, 1, '0.05')}
            ${numberField('temperature', 'Temperature', config.ambientTemperature, 0, 1, '0.05')}
            ${numberField('current', 'Current', config.currentStrength, 0, 30, '0.5')}
            ${numberField('nutrients', 'Nutrients', config.initialNutrients, 0, 5, '0.1')}
          </section>
          <section class="source-section"><h3>Sources</h3><div class="source-list" aria-label="Environment sources"></div><div class="source-detail"></div></section>
        </aside>
      </div>
      <footer class="editor-footer dialog-actions">
        <div class="editor-feedback"><p class="form-error" role="alert" hidden></p></div>
        ${editable ? `<button type="button" class="button primary" data-save>${document ? 'Save copy' : 'Save environment'}</button>` : `<button type="button" class="button subtle" data-copy>${icon('copy')}Edit copy</button><button type="button" class="button primary" data-use>${icon('play')}Use environment</button>`}
      </footer>
    `, 'editor-dialog environment-editor-dialog');
    const run = operations(dialog);
    const nameInput = editable ? query<HTMLInputElement>(dialog, '#environment-name') : null;
    const canvas = query<HTMLCanvasElement>(dialog, '.environment-canvas');
    const sourceList = query<HTMLElement>(dialog, '.source-list');
    const sourceDetail = query<HTMLElement>(dialog, '.source-detail');
    const overlayInput = query<HTMLSelectElement>(dialog, '#preview-overlay');
    const previewStatus = query<HTMLElement>(dialog, '.preview-status');
    const caption = query<HTMLElement>(dialog, '.preview-caption');
    let selectedSource: number | null = config.sources.length ? 0 : null;
    let placingKind: SourceKind | null = editable && !config.sources.length ? 0 : null;
    const placement = SOURCE_NAMES.map(() => ({ strength: '0.7', radius: '200' }));
    let previewEngine: WorkerEngine | null = null;
    let renderer: Renderer | null = null;
    let previewSnapshot: RenderWorldSnapshot | null = null;
    let previewBusy = false, previewRevision = 0;
    let resizeObserver: ResizeObserver | null = null;
    let frame = 0;
    let refreshTimer = 0;
    let saved = false;
    overlayInput.value = selectedSource !== null ? SOURCE_FIELDS[config.sources[selectedSource].kind] : 'light';

    const collectConfig = (skipSource = -1): EnvironmentConfig => {
      const nextSources = config.sources.flatMap((source, index) => {
        if (index === skipSource) return [];
        if (!editable || index !== selectedSource) return [{ ...source }];
        return [{
          kind: source.kind,
          x: inputNumber(query<HTMLInputElement>(sourceDetail, '[data-source-x]'), 'Source x', 0, 1199.9999999999),
          y: inputNumber(query<HTMLInputElement>(sourceDetail, '[data-source-y]'), 'Source y', 0, 759.9999999999),
          strength: inputNumber(query<HTMLInputElement>(sourceDetail, '[data-source-strength]'), 'Source strength', source.kind === 0 || source.kind === 3 ? 0 : -1, 1),
          radius: inputNumber(query<HTMLInputElement>(sourceDetail, '[data-source-radius]'), 'Source radius', 40, 400),
        }];
      });
      return validateEnvironmentConfig({
        foodRate: inputNumber(query<HTMLInputElement>(dialog, '#environment-food'), 'External organic input', 0, 60),
        ambientLight: inputNumber(query<HTMLInputElement>(dialog, '#environment-light'), 'Ambient light', 0, 1),
        ambientTemperature: inputNumber(query<HTMLInputElement>(dialog, '#environment-temperature'), 'Temperature', 0, 1),
        currentStrength: inputNumber(query<HTMLInputElement>(dialog, '#environment-current'), 'Current strength', 0, 30),
        initialNutrients: inputNumber(query<HTMLInputElement>(dialog, '#environment-nutrients'), 'Starting nutrients', 0, 5),
        sources: nextSources,
      });
    };
    const draw = (): void => {
      if (!alive(dialog) || !renderer || !previewSnapshot) return;
      renderer.render(previewSnapshot, null, { overlay: overlayInput.value as FieldOverlay, sensors: false, trails: false });
    };
    const refreshPreview = (): void => {
      if (!alive(dialog) || !previewEngine) return;
      previewRevision++;
      if (previewBusy) return;
      previewBusy = true;
      const engine = previewEngine;
      void (async () => {
        // Coalesce rapid edits while the worker computes a preview. Only the
        // newest complete configuration becomes visible.
        while (alive(dialog) && previewEngine === engine) {
          const revision = previewRevision, next = structuredClone(config);
          await engine.setEnvironment(next.ambientTemperature, next.ambientLight, next.currentStrength);
          await engine.setInitialNutrients(next.initialNutrients);
          await engine.setFoodRate(next.foodRate);
          await engine.clearSources();
          for (const source of next.sources) await engine.addSource(source.kind, source.x, source.y, source.strength, source.radius);
          await engine.resetEmpty(42, 0);
          const presentation = await engine.advance(60, { environment: true });
          if (presentation.error) throw new Error(presentation.error);
          if (!alive(dialog) || previewEngine !== engine) return;
          if (revision !== previewRevision) continue;
          previewSnapshot = { ...presentation.frame, environment: presentation.environment! };
          draw(); previewStatus.textContent = ''; previewStatus.hidden = true;
          return;
        }
      })().catch(error => {
        if (alive(dialog)) { previewStatus.hidden = false; previewStatus.textContent = 'Preview unavailable'; setDialogError(dialog, error); }
      }).finally(() => { previewBusy = false; });
    };
    const persistDraft = (): void => {
      if (!editable || saved) return;
      config = collectConfig();
      saveEnvironmentDraft({ name: nameInput!.value, config });
    };
    const syncTools = (): void => {
      dialog.querySelectorAll<HTMLButtonElement>('[data-source-kind],[data-source-tool]').forEach(button => {
        const selected = button.dataset.sourceTool === 'select' ? placingKind === null : Number(button.dataset.sourceKind) === placingKind;
        button.classList.toggle('active', selected); button.setAttribute('aria-pressed', String(selected));
      });
      caption.textContent = editable && placingKind !== null ? `Click to place ${['a light', 'heat or cooling', 'a vortex', 'a nutrient spring'][placingKind]}.` : '';
      canvas.setAttribute('aria-label', placingKind !== null
        ? `Environment preview. Click to place ${SOURCE_NAMES[placingKind].toLowerCase()}, or press Enter to place it in the center.`
        : 'Environment preview. Click a source to select it.');
    };
    const renderSources = (): void => {
      sourceList.innerHTML = config.sources.map((source, index) => `<button type="button" class="source-item${selectedSource === index ? ' active' : ''}" data-select-source="${index}" aria-pressed="${selectedSource === index}"><span>${icon(SOURCE_ICONS[source.kind])}${SOURCE_NAMES[source.kind]}</span><small>${Math.round(source.x)}, ${Math.round(source.y)}</small></button>`).join('') || '<p class="source-empty">No sources</p>';
    };
    const changed = (): void => {
      clearError(dialog);
      window.clearTimeout(refreshTimer);
      refreshTimer = window.setTimeout(() => {
        if (!alive(dialog)) return;
        try { persistDraft(); renderSources(); refreshPreview(); } catch (error) { setDialogError(dialog, error); }
      }, 200);
    };
    const renderSourceDetail = (): void => {
      sourceDetail.hidden = selectedSource === null && placingKind === null;
      if (placingKind !== null) {
        const kind = placingKind, values = placement[kind];
        sourceDetail.innerHTML = `<div class="source-detail-heading"><h4>Place ${SOURCE_TOOLS[kind].toLowerCase()}</h4></div><div class="source-fields">
          <label>Strength<input id="source-strength" type="number" min="${kind === 0 || kind === 3 ? 0 : -1}" max="1" step="0.05" value="${escapeHtml(values.strength)}"></label>
          <label>Radius<input id="source-radius" type="number" min="40" max="400" step="10" value="${escapeHtml(values.radius)}"></label></div>`;
        query<HTMLInputElement>(sourceDetail, '#source-strength').addEventListener('input', event => { values.strength = (event.target as HTMLInputElement).value; });
        query<HTMLInputElement>(sourceDetail, '#source-radius').addEventListener('input', event => { values.radius = (event.target as HTMLInputElement).value; });
        return;
      }
      if (selectedSource === null) { sourceDetail.innerHTML = ''; return; }
      const source = config.sources[selectedSource];
      const field = (key: string, label: string, value: number, min: number, max: number, step: string): string => `<label>${label}<input type="number" data-source-${key} aria-label="Selected source ${label}" value="${value}" min="${min}" max="${max}" step="${step}"${editable ? '' : ' readonly'}></label>`;
      sourceDetail.innerHTML = `<div class="source-detail-heading"><h4>${SOURCE_NAMES[source.kind]}</h4>${editable ? `<button type="button" class="icon-button source-remove" data-remove-source aria-label="Remove selected source">${icon('trash')}</button>` : ''}</div><div class="source-fields">${field('x', 'X', source.x, 0, 1199.99, 'any')}${field('y', 'Y', source.y, 0, 759.99, 'any')}${field('strength', 'Strength', source.strength, source.kind === 0 || source.kind === 3 ? 0 : -1, 1, '0.05')}${field('radius', 'Radius', source.radius, 40, 400, '10')}</div>`;
      if (!editable) return;
      sourceDetail.querySelectorAll<HTMLInputElement>('input').forEach(input => input.addEventListener('input', changed));
      query<HTMLButtonElement>(sourceDetail, '[data-remove-source]').addEventListener('click', () => {
        if (selectedSource === null) return;
        try {
          window.clearTimeout(refreshTimer);
          const index = selectedSource;
          config = collectConfig(index);
          selectedSource = config.sources.length ? Math.min(index, config.sources.length - 1) : null;
          renderSources(); renderSourceDetail(); syncTools(); persistDraft(); refreshPreview(); clearError(dialog);
        } catch (error) { setDialogError(dialog, error); }
      });
    };
    const chooseSource = (index: number | null): void => {
      if (dialog.getAttribute('aria-busy') === 'true') return;
      try {
        window.clearTimeout(refreshTimer);
        if (editable) config = collectConfig();
        selectedSource = index; placingKind = null;
        if (index !== null) overlayInput.value = SOURCE_FIELDS[config.sources[index].kind];
        renderSources(); renderSourceDetail(); syncTools(); persistDraft(); refreshPreview(); clearError(dialog);
      } catch (error) { setDialogError(dialog, error); }
    };
    sourceList.addEventListener('click', event => {
      const button = (event.target as Element).closest<HTMLButtonElement>('[data-select-source]');
      if (!button) return;
      const index = Number(button.dataset.selectSource);
      chooseSource(index);
      sourceList.querySelector<HTMLButtonElement>(`[data-select-source="${index}"]`)?.focus({ preventScroll: true });
    });
    renderSources(); renderSourceDetail(); syncTools();
    overlayInput.addEventListener('change', draw);
    canvas.addEventListener('click', event => {
      if (!renderer || !previewSnapshot) return;
      if (placingKind !== null) {
        const position = renderer.toWorld(event.clientX, event.clientY);
        if (position) placeSource(position.x, position.y);
      } else {
        // The visible source IDs/indexes belong to the last completed preview.
        // After removal/reordering they cannot index the pending configuration.
        if (previewBusy) return;
        const source = renderer.pickSource(previewSnapshot, event.clientX, event.clientY);
        chooseSource(source ? previewSnapshot.environment.sources.findIndex(item => item.id === source.id) : null);
      }
    });
    canvas.addEventListener('keydown', event => {
      if (event.key === 'Enter' && placingKind !== null) { event.preventDefault(); placeSource(600, 380); }
    });
    dialog.addEventListener('close', () => {
      window.clearTimeout(refreshTimer);
      window.cancelAnimationFrame(frame);
      resizeObserver?.disconnect(); renderer?.dispose(); previewEngine?.dispose();
      renderer = null; previewEngine = null; previewSnapshot = null;
      if (editable && !saved) {
        try { persistDraft(); } catch (error) { this.callbacks.notify(`Draft could not be saved: ${message(error)}`); }
      }
    }, { once: true });
    void WorkerEngine.load().then(engine => {
      if (!alive(dialog)) { engine.dispose(); return; }
      previewEngine = engine; renderer = new Renderer(canvas);
      resizeObserver = new ResizeObserver(() => {
        window.cancelAnimationFrame(frame);
        frame = window.requestAnimationFrame(() => { renderer?.resize(); draw(); });
      });
      resizeObserver.observe(canvas); refreshPreview();
    }).catch(error => {
      if (alive(dialog)) { previewStatus.textContent = 'Preview unavailable'; setDialogError(dialog, error); }
    });

    if (!editable) {
      query<HTMLButtonElement>(dialog, '[data-copy]').addEventListener('click', () => { dialog.close(); this.openEnvironment(document, true); });
      query<HTMLButtonElement>(dialog, '[data-use]').addEventListener('click', () => {
        try { this.callbacks.startSimulation(document!.id); dialog.close(); } catch (error) { setDialogError(dialog, error); }
      });
      return;
    }
    nameInput!.addEventListener('input', changed);
    dialog.querySelectorAll<HTMLInputElement>('.environment-form input').forEach(input => input.addEventListener('input', changed));
    query<HTMLButtonElement>(dialog, '[data-source-tool="select"]').addEventListener('click', () => chooseSource(null));
    dialog.querySelectorAll<HTMLButtonElement>('[data-source-kind]').forEach(button => button.addEventListener('click', () => {
      try {
        window.clearTimeout(refreshTimer);
        config = collectConfig();
        placingKind = Number(button.dataset.sourceKind) as SourceKind; selectedSource = null;
        overlayInput.value = SOURCE_FIELDS[placingKind];
        renderSources(); renderSourceDetail(); syncTools(); persistDraft(); refreshPreview(); clearError(dialog);
      } catch (error) { setDialogError(dialog, error); }
    }));
    const placeSource = (x: number, y: number): void => {
      if (!alive(dialog) || !previewEngine || placingKind === null || dialog.getAttribute('aria-busy') === 'true') return;
      try {
        window.clearTimeout(refreshTimer);
        config = collectConfig();
        if (config.sources.length >= 16) throw new Error('An environment can contain at most 16 sources. Remove a source before adding another.');
        const kind = placingKind;
        const strength = inputNumber(query<HTMLInputElement>(sourceDetail, '#source-strength'), 'Source strength', kind === 0 || kind === 3 ? 0 : -1, 1);
        const radius = inputNumber(query<HTMLInputElement>(sourceDetail, '#source-radius'), 'Source radius', 40, 400);
        config.sources.push({ kind, x, y, strength, radius });
        selectedSource = config.sources.length - 1; placingKind = null;
        renderSources(); renderSourceDetail(); syncTools(); persistDraft(); refreshPreview(); clearError(dialog);
      } catch (error) { setDialogError(dialog, error); }
    };
    query<HTMLButtonElement>(dialog, '[data-save]').addEventListener('click', () => run(async () => {
      const next = newEnvironment(nameInput!.value, collectConfig(), document?.description);
      await this.callbacks.saveEnvironment(next);
      saved = true;
      let suffix = '';
      try { clearEnvironmentDraft(); } catch (error) { suffix = ` The saved draft could not be cleared: ${message(error)}`; }
      if (alive(dialog)) dialog.close();
      this.callbacks.notify(`${next.name} saved.${suffix}`);
    }));
    if (draftError) setDialogError(dialog, draftError);
    nameInput!.focus(); nameInput!.select();
  }
}
