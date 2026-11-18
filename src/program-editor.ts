import { Compartment, EditorState, Transaction, type Extension } from '@codemirror/state';
import {
  EditorView, closeHoverTooltips, drawSelection, dropCursor, highlightActiveLine, highlightActiveLineGutter,
  hoverTooltip, keymap, lineNumbers, type ViewUpdate,
} from '@codemirror/view';
import {
  defaultKeymap, history, historyKeymap, isolateHistory, redo as redoCommand, redoDepth,
  undo as undoCommand, undoDepth,
} from '@codemirror/commands';
import { HighlightStyle, StreamLanguage, bracketMatching, indentOnInput, indentUnit, syntaxHighlighting } from '@codemirror/language';
import {
  autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap, pickedCompletion,
  startCompletion, type Completion, type CompletionContext, type CompletionResult,
} from '@codemirror/autocomplete';
import { lintGutter, nextDiagnostic, previousDiagnostic, setDiagnostics, setDiagnosticsEffect } from '@codemirror/lint';
import { searchKeymap } from '@codemirror/search';
import { tags } from '@lezer/highlight';
import type { Engine, GenomeAnalysis, GenomeBinding, GenomeDiagnostic, GenomeEvaluation } from './engine';
import {
  cellularDeclarations, genomeCompletions, genomeSignalCatalog, genomeSymbolDocumentation, isCellularSource,
  lookupCellularSymbol, lookupGenomeSymbol, tokenizeGenome, unwrapGenomeSelection,
  type GenomeSymbol, type GenomeToken, type GenomeTokenKind,
} from './genome-language';
import './program-editor.css';

export interface ProgramEditorOptions {
  parent: HTMLElement;
  source: string;
  readOnly?: boolean;
  compiler: () => Promise<Engine>;
  onChange?: (source: string) => void;
  onSave?: () => void;
  onStateChange?: () => void;
}

export interface ProgramEditorState {
  /** Dirty means the current text has not yet been checked, not that it was saved. */
  status: 'dirty' | 'checking' | 'valid' | 'error';
  diagnostics: readonly GenomeDiagnostic[];
  nodeCount: number | null;
  sourceBytes: number;
  readOnly: boolean;
}

interface LocalDeclaration { name: string; from: number; to: number; scopeFrom: number }
interface SymbolRange { from: number; to: number; symbol: GenomeSymbol }
interface StreamState { line: string; tokens: readonly GenomeToken[]; index: number }
interface EvaluatedPart { from: number; to: number; type: string; value: unknown; label?: string; note?: string }

const tokenStyles: Record<GenomeTokenKind, string> = {
  keyword: 'keyword', boolean: 'bool', number: 'number', string: 'string', comment: 'comment',
  operator: 'operator', punctuation: 'punctuation', identifier: 'variableName', signal: 'genomeSignal',
  function: 'genomeFunction', output: 'genomeOutput', register: 'genomeRegister', invalid: 'invalid',
};

// Genome strings and comments end at a physical line, so no cross-line lexer state is needed.
const genomeLanguage = StreamLanguage.define<StreamState>({
  name: 'agar-genome',
  startState: () => ({ line: '', tokens: [], index: 0 }),
  copyState: state => ({ ...state }),
  token(stream, state) {
    if (stream.sol() || state.line !== stream.string) {
      state.line = stream.string; state.tokens = tokenizeGenome(stream.string); state.index = 0;
    }
    while (state.index < state.tokens.length && state.tokens[state.index].to <= stream.pos) state.index++;
    const token = state.tokens[state.index];
    if (!token) { stream.skipToEnd(); return null; }
    if (token.from > stream.pos) { stream.pos = token.from; return null; }
    stream.pos = Math.max(stream.pos + 1, token.to);
    return tokenStyles[token.kind];
  },
  tokenTable: {
    genomeSignal: tags.special(tags.variableName),
    genomeFunction: tags.function(tags.variableName),
    genomeOutput: tags.propertyName,
    genomeRegister: tags.special(tags.propertyName),
  },
  languageData: { commentTokens: { line: '#' }, closeBrackets: { brackets: ['(', '{', '"'] }, wordChars: '.' },
});

const genomeHighlight = HighlightStyle.define([
  { tag: tags.keyword, color: '#c4a5e9' },
  { tag: tags.bool, color: '#e6b080' },
  { tag: tags.number, color: '#dfbc87' },
  { tag: tags.string, color: '#b3d79b' },
  { tag: tags.comment, color: '#7e9092', fontStyle: 'italic' },
  { tag: tags.operator, color: '#a8b9bf' },
  { tag: tags.punctuation, color: '#99abad' },
  { tag: tags.variableName, color: '#e3e8e6' },
  { tag: tags.special(tags.variableName), color: '#8fced8' },
  { tag: tags.function(tags.variableName), color: '#c4b3ed' },
  { tag: tags.propertyName, color: '#9ce0bb' },
  { tag: tags.special(tags.propertyName), color: '#edb88a' },
  { tag: tags.invalid, color: '#f0a598', textDecoration: 'underline wavy #dc8177' },
]);

const editorTheme = EditorView.theme({
  '&': { color: '#e0e7e4', backgroundColor: '#11191b' },
  '&.cm-focused': { outline: 'none' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: '#b4ead2' },
  '.cm-content': { caretColor: '#b4ead2' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': { backgroundColor: '#355347' },
  '.cm-activeLine': { backgroundColor: '#b4ead208' },
  '.cm-gutters': { backgroundColor: '#11191b', color: '#63797b', borderRight: '1px solid #263236' },
  '.cm-activeLineGutter': { backgroundColor: '#22332e', color: '#b4d8c9' },
  '.cm-matchingBracket': { color: '#d7f4e6', backgroundColor: '#375447', outline: '1px solid #769c88' },
  '.cm-nonmatchingBracket': { color: '#f5b7a8', backgroundColor: '#5a302d' },
  '.cm-tooltip': { backgroundColor: '#202b2e', color: '#dfe8e4', border: '1px solid #40504d' },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': { backgroundColor: '#365746', color: '#e1f5e9' },
  '.cm-searchMatch': { backgroundColor: '#b5cf7930', outline: '1px solid #b5cf7955' },
  '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: '#b5cf7960' },
  '.cm-panels': { backgroundColor: '#1c2729', color: '#d7e0dc' },
}, { dark: true });

let editorSequence = 0;
const encoder = new TextEncoder();
const errorMessage = (error: unknown): string => error instanceof Error ? error.message : String(error);

function symbolHelp(symbol: GenomeSymbol, compact = false): HTMLElement {
  const element = document.createElement('div');
  element.className = compact ? 'program-symbol-help compact' : 'program-symbol-help';
  const signature = document.createElement('code'); signature.textContent = symbol.signature;
  const description = document.createElement('p'); description.textContent = symbol.description;
  element.append(signature, description);
  return element;
}

function evaluationValue(value: unknown, type: string): string {
  if (type === 'vec2' && Array.isArray(value)) return `vec(${value.map(component => evaluationValue(component, 'scalar')).join(', ')})`;
  if (typeof value === 'number') return Number.isInteger(value) ? String(value) : Number(value.toPrecision(7)).toString();
  if (typeof value === 'string') return JSON.stringify(value);
  return String(value);
}

function evaluationHelp(part: EvaluatedPart, source: string, compact = false): HTMLElement {
  const element = document.createElement('div'); element.className = `program-evaluation-help${compact ? ' compact' : ''}`;
  const expression = document.createElement('code'); expression.className = 'program-evaluation-expression';
  expression.textContent = part.label ?? source.slice(part.from, part.to).trim();
  expression.title = expression.textContent;
  const result = document.createElement('div'); result.className = 'program-evaluation-result';
  const value = document.createElement('code'); value.textContent = evaluationValue(part.value, part.type);
  const type = document.createElement('span'); type.textContent = part.type;
  result.append(value, type); element.append(expression, result);
  if (part.note) { const note = document.createElement('p'); note.textContent = part.note; element.append(note); }
  return element;
}

/** An isolated editor: changing or checking code never writes the catalog or a live world. */
export class ProgramEditor {
  readonly element: HTMLElement;
  private readonly view: EditorView;
  private readonly readOnlyMode = new Compartment();
  private readonly diagnosticsElement: HTMLUListElement;
  private readonly helpElement: HTMLElement;
  private readonly statusElement: HTMLElement;
  private readonly positionElement: HTMLElement;
  private readonly sizeElement: HTMLElement;
  private readonly referenceElement: HTMLElement;
  private readonly readSignalSelect: HTMLSelectElement;
  private readonly writeSignalSelect: HTMLSelectElement;
  private readonly evaluationElement: HTMLElement;
  private readonly evaluationTimeElement: HTMLElement;
  private readonly languageElement: HTMLElement;
  private referenceSymbol: GenomeSymbol | null = null;
  private evaluation: GenomeEvaluation | null = null;
  private evaluationUnavailable: string | null = null;
  private source: string;
  private readOnly: boolean;
  private destroyed = false;
  private revision = 0;
  private validationSerial = 0;
  private validationTimer = 0;
  private pending: { revision: number; serial: number; promise: Promise<boolean> } | null = null;
  private analysis: GenomeAnalysis | null = null;
  private diagnostics: GenomeDiagnostic[] = [];
  private status: ProgramEditorState['status'] = 'dirty';
  private sourceBytes: number;
  private tokens: GenomeToken[] = [];
  private codeTokens: GenomeToken[] = [];
  private declarations: LocalDeclaration[] = [];
  private declarationByFrom = new Map<number, LocalDeclaration>();
  private bindingByFrom = new Map<number, GenomeBinding>();
  private helpKey = '';

  constructor(private readonly options: ProgramEditorOptions) {
    this.source = options.source;
    this.readOnly = options.readOnly ?? false;
    this.sourceBytes = encoder.encode(this.source).length;
    this.refreshTokens();
    const identifier = `program-editor-${++editorSequence}`;
    this.element = document.createElement('div');
    this.element.className = 'program-editor';
    this.languageElement = document.createElement('div'); this.languageElement.className = 'program-language-summary';
    const referenceBar = document.createElement('div'); referenceBar.className = 'program-reference-bar';
    this.readSignalSelect = this.createSignalSelect('read');
    this.writeSignalSelect = this.createSignalSelect('write');
    this.evaluationTimeElement = document.createElement('span'); this.evaluationTimeElement.className = 'program-evaluation-time';
    this.evaluationTimeElement.hidden = true;
    const completionHint = document.createElement('span'); completionHint.className = 'program-completion-hint';
    completionHint.textContent = 'Ctrl Space'; completionHint.title = 'Open contextual autocomplete with Control or Command + Space';
    referenceBar.append(this.readSignalSelect, this.writeSignalSelect, completionHint, this.evaluationTimeElement);
    this.referenceElement = document.createElement('div'); this.referenceElement.className = 'program-reference-panel'; this.referenceElement.hidden = true;
    const host = document.createElement('div'); host.className = 'program-editor-code';
    this.evaluationElement = document.createElement('div'); this.evaluationElement.className = 'program-evaluation'; this.evaluationElement.hidden = true;
    this.helpElement = document.createElement('div');
    this.helpElement.className = 'program-context';
    this.helpElement.id = `${identifier}-context`;
    this.helpElement.setAttribute('aria-live', 'polite');
    this.helpElement.setAttribute('aria-atomic', 'true');
    this.diagnosticsElement = document.createElement('ul');
    this.diagnosticsElement.className = 'program-diagnostics';
    this.diagnosticsElement.setAttribute('aria-label', 'Compiler diagnostics');
    this.diagnosticsElement.hidden = true;
    const statusBar = document.createElement('div'); statusBar.className = 'program-status';
    this.statusElement = document.createElement('span');
    this.statusElement.className = 'program-check-state';
    this.statusElement.setAttribute('role', 'status');
    this.sizeElement = document.createElement('span'); this.sizeElement.className = 'program-size';
    this.positionElement = document.createElement('span'); this.positionElement.className = 'program-position';
    statusBar.append(this.statusElement, this.sizeElement, this.positionElement);
    const shortcuts = document.createElement('span');
    shortcuts.className = 'program-shortcuts'; shortcuts.id = `${identifier}-shortcuts`;
    shortcuts.textContent = 'Tab leaves the editor. Control or Command plus Space opens completions, Enter checks the program, S saves, Z undoes, and Shift Z redoes. Control or Command plus bracket indents. F8 goes to the next error.';
    this.element.append(this.languageElement, referenceBar, this.referenceElement, host, this.evaluationElement, this.helpElement, this.diagnosticsElement, statusBar, shortcuts);
    options.parent.append(this.element);
    this.view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: this.source,
        extensions: [
          genomeLanguage, syntaxHighlighting(genomeHighlight), editorTheme,
          lineNumbers(), highlightActiveLineGutter(), drawSelection(), dropCursor(), highlightActiveLine(),
          history(), indentUnit.of('  '), EditorState.tabSize.of(2), indentOnInput(), bracketMatching(), closeBrackets(), lintGutter(),
          this.readOnlyMode.of(this.readOnlyExtensions()),
          EditorView.contentAttributes.of({
            role: 'textbox', 'aria-label': 'Genome program', 'aria-multiline': 'true',
            'aria-describedby': `${identifier}-shortcuts ${identifier}-context`,
            spellcheck: 'false', autocapitalize: 'off', autocomplete: 'off', autocorrect: 'off', tabindex: '0',
          }),
          autocompletion({ override: [context => this.complete(context)], activateOnTypingDelay: 120, maxRenderedOptions: 40 }),
          hoverTooltip((_view, position, side) => {
            const match = this.symbolNear(position);
            const evaluated = this.evaluatedPart(position, position);
            const source = this.source;
            const range = evaluated ?? match;
            if (!range || (position === range.from && side < 0) || (position === range.to && side > 0)) return null;
            return { pos: range.from, end: range.to, above: true, create: () => {
              const dom = document.createElement('div');
              const render = (): void => {
                const snapshot = this.evaluation;
                const part = snapshot?.source === source ? this.evaluatedPart(position, position) : null;
                dom.replaceChildren();
                if (part && snapshot) dom.append(evaluationHelp({ ...part,
                  note: `${part.note ? `${part.note} ` : ''}Snapshot at ${snapshot.time.toFixed(2)} s · organism ${snapshot.cellId}.`,
                }, source));
                if (match) dom.append(symbolHelp(match.symbol));
              };
              render();
              return { dom, update: render };
            } };
          }, { hoverTime: 350, hideOnChange: true }),
          // Clear marks in the same transaction as edits instead of leaving an old error attached to new code.
          EditorState.transactionExtender.of(transaction => transaction.docChanged
            ? { effects: setDiagnosticsEffect.of([]) } : null),
          EditorView.updateListener.of(update => this.updated(update)),
          keymap.of([
            { key: 'Mod-s', run: () => { if (!this.readOnly) this.options.onSave?.(); return true; }, preventDefault: true },
            { key: 'Mod-Enter', run: () => { void this.check(); return true; }, preventDefault: true },
            { key: 'Mod-Space', run: view => !this.readOnly && startCompletion(view), preventDefault: true },
            { key: 'Mod-Shift-z', run: () => this.redo(), preventDefault: true },
            { key: 'F8', run: nextDiagnostic, shift: previousDiagnostic, preventDefault: true },
            ...closeBracketsKeymap, ...completionKeymap,
            ...historyKeymap.map(binding => ({
              ...binding,
              run: (view: EditorView) => !this.readOnly && Boolean(binding.run?.(view)),
              shift: binding.shift ? (view: EditorView) => !this.readOnly && Boolean(binding.shift?.(view)) : undefined,
            })),
            ...searchKeymap, ...defaultKeymap,
          ]),
        ],
      }),
    });
    this.view.dispatch(setDiagnostics(this.view.state, []));
    this.renderStatus(); this.renderContext(); this.refreshLanguageReference();
    this.scheduleCheck();
    queueMicrotask(() => { if (!this.destroyed) this.options.onStateChange?.(); });
  }

  get value(): string { return this.source; }
  get canUndo(): boolean { return !this.destroyed && !this.readOnly && undoDepth(this.view.state) > 0; }
  get canRedo(): boolean { return !this.destroyed && !this.readOnly && redoDepth(this.view.state) > 0; }
  get state(): ProgramEditorState {
    return {
      status: this.status, diagnostics: this.diagnostics.map(item => ({ ...item })),
      nodeCount: this.analysis?.nodeCount ?? null, sourceBytes: this.sourceBytes, readOnly: this.readOnly,
    };
  }

  /** Programmatic changes (for example mutations) remain possible while input is temporarily locked. */
  setValue(source: string): void {
    if (this.destroyed || source === this.source) return;
    let from = 0;
    while (from < this.source.length && from < source.length && this.source[from] === source[from]) from++;
    let oldTo = this.source.length, newTo = source.length;
    while (oldTo > from && newTo > from && this.source[oldTo - 1] === source[newTo - 1]) { oldTo--; newTo--; }
    // Never split a Unicode surrogate pair while finding the smallest text edit.
    if (from > 0 && /[\uDC00-\uDFFF]/.test(source[from] ?? this.source[from] ?? '')) from--;
    if (oldTo < this.source.length && /[\uDC00-\uDFFF]/.test(this.source[oldTo])) oldTo++;
    if (newTo < source.length && /[\uDC00-\uDFFF]/.test(source[newTo])) newTo++;
    this.view.dispatch({
      changes: { from, to: oldTo, insert: source.slice(from, newTo) },
      selection: { anchor: from, head: newTo },
      annotations: [isolateHistory.of('full'), Transaction.userEvent.of('input.replace')],
      scrollIntoView: true,
    });
  }

  undo(): boolean { return this.canUndo ? undoCommand(this.view) : false; }
  redo(): boolean { return this.canRedo ? redoCommand(this.view) : false; }

  setReadOnly(readOnly: boolean): void {
    if (this.destroyed || this.readOnly === readOnly) return;
    this.readOnly = readOnly;
    this.view.dispatch({ effects: this.readOnlyMode.reconfigure(this.readOnlyExtensions()) });
    this.element.classList.toggle('is-readonly', readOnly);
    if (this.referenceSymbol) this.showReference(this.referenceSymbol);
    this.options.onStateChange?.();
  }

  focus(): void { if (!this.destroyed) this.view.focus(); }

  /** Pure previews are supplied by the current organism's WASM instance, never inferred in JS. */
  setEvaluation(evaluation: GenomeEvaluation | null): void {
    if (this.destroyed) return;
    if (evaluation && evaluation.source !== this.source) {
      this.setEvaluationUnavailable('The program changed. Refresh this organism to evaluate its current source.'); return;
    }
    this.evaluation = evaluation; this.evaluationUnavailable = null;
    this.renderEvaluation(); this.renderContext();
    // Existing hover views receive the same fresh snapshot as the persistent value panel.
    this.view.dispatch(evaluation ? {} : { effects: closeHoverTooltips });
  }

  setEvaluationUnavailable(message: string): void {
    if (this.destroyed) return;
    this.evaluation = null; this.evaluationUnavailable = message;
    this.renderEvaluation(); this.renderContext();
    this.view.dispatch({ effects: closeHoverTooltips });
  }

  /** A result applies only to the exact revision requested; a changed/destroyed editor returns false. */
  check(): Promise<boolean> {
    if (this.destroyed) return Promise.resolve(false);
    window.clearTimeout(this.validationTimer);
    if (this.analysis) return Promise.resolve(this.analysis.valid);
    if (this.pending?.revision === this.revision) return this.pending.promise;
    const source = this.source, revision = this.revision, serial = ++this.validationSerial;
    const current = (): boolean => !this.destroyed && revision === this.revision && serial === this.validationSerial;
    this.status = 'checking'; this.renderStatus(); this.options.onStateChange?.();
    const promise = (async (): Promise<boolean> => {
      try {
        const compiler = await this.options.compiler();
        if (!current()) return false;
        const analysis = compiler.analyze(source);
        if (!current()) return false;
        this.analysis = analysis;
        this.bindingByFrom = new Map(analysis.bindings.map(binding => [binding.from, binding]));
        this.diagnostics = analysis.diagnostics.map(item => ({ ...item }));
        this.sourceBytes = analysis.sourceBytes;
        this.status = analysis.valid ? 'valid' : 'error';
        this.applyDiagnostics(); this.renderStatus(); this.renderContext(); this.options.onStateChange?.();
        return analysis.valid;
      } catch (error) {
        if (!current()) return false;
        this.analysis = null;
        this.bindingByFrom.clear();
        this.diagnostics = [{ severity: 'error', message: `Type checking unavailable: ${errorMessage(error)}`, line: 1, column: 1, from: 0, to: 0 }];
        this.status = 'error';
        this.applyDiagnostics(); this.renderStatus(); this.renderContext(); this.options.onStateChange?.();
        return false;
      } finally {
        if (this.pending?.serial === serial) this.pending = null;
      }
    })();
    this.pending = { revision, serial, promise };
    return promise;
  }

  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true; this.validationSerial++;
    window.clearTimeout(this.validationTimer);
    this.pending = null; this.analysis = null; this.tokens = []; this.codeTokens = []; this.declarations = [];
    this.evaluation = null; this.referenceSymbol = null;
    this.declarationByFrom.clear(); this.bindingByFrom.clear();
    this.view.destroy(); this.element.remove();
  }

  private readOnlyExtensions(): Extension[] {
    return [EditorState.readOnly.of(this.readOnly), EditorView.editable.of(!this.readOnly)];
  }

  private updated(update: ViewUpdate): void {
    if (this.destroyed) return;
    if (update.docChanged) {
      this.source = update.state.doc.toString(); this.revision++; this.validationSerial++;
      this.analysis = null; this.diagnostics = []; this.status = 'dirty';
      if (this.evaluation) { this.evaluation = null; this.evaluationUnavailable = 'The source changed. Values are unavailable for this draft.'; }
      this.bindingByFrom.clear();
      this.sourceBytes = encoder.encode(this.source).length;
      this.refreshTokens(); this.refreshLanguageReference(); this.renderDiagnostics(); this.renderStatus();
      this.scheduleCheck(); this.options.onChange?.(this.source); this.options.onStateChange?.();
    }
    if (update.docChanged || update.selectionSet) { this.renderContext(); this.renderEvaluation(); }
  }

  private createSignalSelect(direction: 'read' | 'write'): HTMLSelectElement {
    const select = document.createElement('select'); select.className = `program-signal-select ${direction}`;
    this.populateSignalSelect(select, direction);
    select.addEventListener('change', () => {
      const selected = (isCellularSource(this.source) ? lookupCellularSymbol(select.value, this.source) : undefined) ?? lookupGenomeSymbol(select.value);
      if (!selected) { this.referenceElement.hidden = true; this.referenceSymbol = null; return; }
      (direction === 'read' ? this.writeSignalSelect : this.readSignalSelect).value = '';
      this.showReference(selected);
    });
    return select;
  }

  private populateSignalSelect(select: HTMLSelectElement, direction: 'read' | 'write'): void {
    const cellular = isCellularSource(this.source), selected = select.value;
    select.setAttribute('aria-label', cellular ? direction === 'read' ? 'Cellular input reference' : 'Cellular declaration reference' : direction === 'read' ? 'Read signal reference' : 'Write signal reference');
    const prompt = document.createElement('option'); prompt.value = '';
    prompt.textContent = cellular ? direction === 'read' ? 'Inputs & stocks' : 'Products & recipes' : direction === 'read' ? 'Read signals' : 'Write signals';
    select.replaceChildren(prompt);
    const groups = new Map<string, HTMLOptGroupElement>();
    for (const item of genomeSignalCatalog(direction, this.source)) {
      const groupName = cellular && direction === 'write' ? item.type === 'mechanism' ? 'Mechanisms' : item.category === 'function' ? 'Product roles' : 'Declarations & placement'
        : /^(pool|concentration|built)\./.test(item.name) ? 'This cell’s products & assemblies'
        : item.name.startsWith('sensed.') ? 'Named receptor readings'
        : item.category === 'register' ? 'Memory registers'
        : direction === 'write' ? /^(target|build|recycle)\./.test(item.name) ? 'Organelle construction' : item.name.startsWith('scan.') ? 'Physical scan' : 'Actions'
          : /^(object|contact)\./.test(item.name) ? 'Physical observations'
            : /^(light|heat|chemical|flow)\./.test(item.name) ? 'Environment sensors'
              : item.name.startsWith('organelle.') ? 'Actual organelles' : item.name.startsWith('body.') ? 'Body structure' : 'Cell state';
      let group = groups.get(groupName);
      if (!group) { group = document.createElement('optgroup'); group.label = groupName; groups.set(groupName, group); select.append(group); }
      const option = document.createElement('option'); option.value = item.name;
      option.textContent = `${item.name}${item.category === 'register' ? ` … ${item.name.slice(0, 2)}999` : ''} · ${item.type}`;
      group.append(option);
    }
    select.value = selected;
  }

  private refreshLanguageReference(): void {
    const cellular = isCellularSource(this.source);
    this.languageElement.hidden = !cellular;
    this.element.classList.toggle('cellular-program', cellular);
    if (cellular) {
      const declarations = cellularDeclarations(this.source);
      const label = document.createElement('strong'); label.textContent = 'Cellular program';
      const detail = document.createElement('span');
      const mechanisms = declarations.assemblies.filter(item=>item.kind.includes('(')).length;
      detail.textContent = `${declarations.products.length} products · ${declarations.genes.length} genes · ${mechanisms ? `${mechanisms} mechanisms${mechanisms<declarations.assemblies.length?` · ${declarations.assemblies.length-mechanisms} family recipes`:''}` : `${declarations.assemblies.length} recipes`}${declarations.functions.length ? ` · ${declarations.functions.length} functions` : ''}`;
      this.languageElement.replaceChildren(label, detail);
    }
    this.populateSignalSelect(this.readSignalSelect, 'read'); this.populateSignalSelect(this.writeSignalSelect, 'write');
  }

  private showReference(symbol: GenomeSymbol): void {
    this.referenceSymbol = symbol; this.referenceElement.hidden = false;
    const content = document.createElement('div'); content.className = 'program-reference-content';
    const summary = symbolHelp(symbol); summary.classList.add('program-reference-summary');
    const usage = document.createElement('div'); usage.className = 'program-reference-usage';
    const liveValue = document.createElement('div'); liveValue.className = 'program-reference-live-value'; liveValue.hidden = true; usage.append(liveValue);
    const documentation = genomeSymbolDocumentation(symbol);
    const example = document.createElement('pre'); example.className = 'program-reference-example'; example.textContent = documentation.example;
    usage.append(example);
    for (const text of documentation.notes) { const note = document.createElement('p'); note.textContent = text; summary.append(note); }
    content.append(summary, usage);
    const controls = document.createElement('div'); controls.className = 'program-reference-controls';
    if (!this.readOnly) {
      const insert = document.createElement('button'); insert.type = 'button'; insert.className = 'program-reference-insert';
      const cellular = isCellularSource(this.source);
      insert.textContent = cellular ? this.writeSignalSelect.value ? 'Insert declaration' : 'Insert input' : symbol.category === 'output' || this.writeSignalSelect.value ? 'Insert write' : 'Insert read';
      insert.addEventListener('click', () => {
        if (this.destroyed || this.readOnly) return;
        const writing = symbol.category === 'output' || Boolean(this.writeSignalSelect.value);
        const text = cellular ? symbol.insertText ?? (symbol.category === 'function' ? `${symbol.name}(${symbol.name === 'regulator' ? '8' : ''})` : symbol.name)
          : writing ? `${symbol.name} = ` : symbol.name;
        const selection = this.view.state.selection.main;
        this.view.dispatch({ changes: { from: selection.from, to: selection.to, insert: text }, selection: { anchor: selection.from + text.length },
          annotations: Transaction.userEvent.of('input.complete'), scrollIntoView: true });
        this.view.focus();
      });
      controls.append(insert);
    }
    const close = document.createElement('button'); close.type = 'button'; close.className = 'program-reference-close'; close.textContent = 'Close';
    close.setAttribute('aria-label', 'Close signal reference');
    close.addEventListener('click', () => { this.referenceElement.hidden = true; this.referenceSymbol = null; this.readSignalSelect.value = ''; this.writeSignalSelect.value = ''; });
    controls.append(close);
    this.referenceElement.replaceChildren(content, controls);
    this.renderReferenceValue();
  }

  private renderReferenceValue(): void {
    const container = this.referenceElement.querySelector<HTMLElement>('.program-reference-live-value');
    if (!container) return;
    const symbol = this.referenceSymbol, evaluation = this.evaluation;
    const value = symbol?.category === 'signal' ? evaluation?.signals.find(value => value.name === symbol.name)
      : symbol?.category === 'output' ? evaluation?.outputs.find(value => value.name === symbol.name) : undefined;
    container.hidden = !value;
    if (!value || !evaluation) { container.replaceChildren(); return; }
    const label = document.createElement('span'); label.textContent = `${symbol?.category === 'output' ? 'Clamped request' : 'Current value'} · ${evaluation.time.toFixed(2)} s`;
    const code = document.createElement('code'); code.textContent = evaluationValue(value.value, value.type);
    container.replaceChildren(label, code);
  }

  private evaluatedPart(from: number, to: number): EvaluatedPart | null {
    const evaluation = this.evaluation;
    if (!evaluation || evaluation.source !== this.source) return null;
    const selection = from !== to;
    let start = from, end = to;
    if (selection) {
      const normalized = unwrapGenomeSelection(this.source, from, to, this.codeTokens);
      start = normalized.from; end = normalized.to;
    }
    if (!selection) {
      const token = this.tokenAt(from);
      if (token) {
        if (token.kind === 'comment') return null;
        // An operator belongs to its enclosing expression, never to the leaf ending at its left edge.
        start = token.from; end = token.to;
        const declaration = evaluation.bindings.find(binding => binding.from === token.from && binding.to === token.to);
        if (declaration) return { ...declaration, label: declaration.name, note: 'Local binding value.' };
        if (evaluation.mode==='cellular') {
          const declarations=cellularDeclarations(this.source);
          const gene=declarations.genes.find(item=>item.from===token.from && item.to===token.to);
          const assembly=declarations.assemblies.find(item=>item.from===token.from && item.to===token.to);
          const name=gene?`gene.${gene.name}`:assembly?`activity.${assembly.name}`:null;
          const request=name?evaluation.outputs.find(item=>item.name===name):undefined;
          if (request) return {from:token.from,to:token.to,type:request.type,value:request.value,label:request.name,
            note:gene?'Requested expression effort. Actual synthesis shares finite core work, minerals, energy and space.':'Requested activity. Only completed functional machinery can act.'};
          const product=declarations.products.find(item=>item.name===token.text);
          const stock=product?evaluation.signals.find(item=>item.name===`pool.${product.name}`):undefined;
          if (stock) return {from:token.from,to:token.to,type:stock.type,value:stock.value,label:stock.name,note:'Available physical product mass at this snapshot.'};
        }
        const output = token.kind === 'output' ? evaluation.outputs.find(output => output.name === token.text) : undefined;
        if (output) return { from: token.from, to: token.to, type: output.type, value: output.value, label: output.name,
          note: 'Action request after engine clamping. Capability and resource checks still apply.' };
        const write = evaluation.writes.find(write => write.from === token.from && write.to === token.to);
        if (write) return { ...write, value: write.kind === 'clear' ? write.applies : write.value,
          type: write.kind === 'clear' ? 'boolean' : write.type, label: write.kind === 'clear' ? `clear ${write.name}` : `${write.name} write`,
          note: write.applies ? 'This rule would apply. Previewing does not change memory.' : write.guard ? 'This write is suppressed by an active clear.' : 'The when condition is false; this rule would not apply.' };
      }
    }
    let found: (typeof evaluation.expressions)[number] | undefined;
    for (const expression of evaluation.expressions) {
      if (expression.from > start || expression.to < end || expression.from === expression.to) continue;
      if (!found || expression.to - expression.from < found.to - found.from) found = expression;
    }
    return found ? { ...found, ...(selection && (found.from !== start || found.to !== end) ? { note: 'Value of the enclosing complete expression.' } : {}) } : null;
  }

  private renderEvaluation(): void {
    const evaluation = this.evaluation;
    this.renderReferenceValue();
    this.evaluationElement.hidden = !evaluation && !this.evaluationUnavailable;
    this.evaluationTimeElement.hidden = !evaluation;
    this.element.classList.toggle('has-evaluation', Boolean(evaluation));
    if (!evaluation) {
      this.evaluationTimeElement.textContent = '';
      const hint = document.createElement('span'); hint.className = 'program-context-empty'; hint.textContent = this.evaluationUnavailable ?? '';
      this.evaluationElement.replaceChildren(hint); return;
    }
    this.evaluationTimeElement.textContent = `Snapshot ${evaluation.time.toFixed(2)} s`;
    this.evaluationTimeElement.title = `Organism ${evaluation.cellId} · tick ${evaluation.tick}. Current signals and memory; this preview does not commit writes or advance the world.`;
    const selection = this.view.state.selection.main;
    const part = this.evaluatedPart(selection.from, selection.to);
    const content = part ? evaluationHelp(part, this.source, true) : document.createElement('span');
    if (!part) { content.className = 'program-context-empty'; content.textContent = 'Click, hover, or select an expression to inspect its value.'; }
    const note = document.createElement('span'); note.className = 'program-evaluation-note';
    note.textContent = 'Current signals and memory · preview does not commit writes';
    note.title = 'Values are calculated with the same evaluator as the simulation. All pure branches can be inspected, including a branch an if expression does not select. Commands may still be limited by the body and available resources.';
    this.evaluationElement.replaceChildren(content, note);
  }

  private scheduleCheck(): void {
    window.clearTimeout(this.validationTimer);
    this.validationTimer = window.setTimeout(() => { if (!this.destroyed) void this.check(); }, 300);
  }

  private refreshTokens(): void {
    this.tokens = tokenizeGenome(this.source);
    this.codeTokens = this.tokens.filter(token => token.kind !== 'comment');
    this.declarations = [];
    for (let index = 0; index < this.codeTokens.length; index++) {
      if (this.codeTokens[index].text !== 'let') continue;
      const name = this.codeTokens[index + 1];
      if (!name || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(name.text) || this.codeTokens[index + 2]?.text !== '=') continue;
      let end = index + 3;
      while (end < this.codeTokens.length && this.codeTokens[end].text !== ';') end++;
      this.declarations.push({ name: name.text, from: name.from, to: name.to, scopeFrom: this.codeTokens[end]?.to ?? Infinity });
      index = end;
    }
    this.declarationByFrom = new Map(this.declarations.map(declaration => [declaration.from, declaration]));
  }

  private visibleBindings(position: number, declaration = false): GenomeBinding[] {
    return (this.analysis?.bindings ?? []).filter(binding => {
      const item = this.declarationByFrom.get(binding.from);
      return item && (item.scopeFrom <= position || (declaration && item.from <= position && position <= item.to));
    });
  }

  private localSymbol(declaration: LocalDeclaration): GenomeSymbol {
    const binding = this.bindingByFrom.get(declaration.from);
    return binding ? lookupGenomeSymbol(binding.name, [binding])! : {
      name: declaration.name, category: 'binding', type: 'not checked', signature: `${declaration.name}: not checked`,
      description: 'Immutable local expression. Its type appears after the current program is checked.',
    };
  }

  private tokenAt(position: number): GenomeToken | undefined {
    let low = 0, high = this.tokens.length;
    while (low < high) {
      const middle = (low + high) >>> 1;
      if (this.tokens[middle].to < position) low = middle + 1; else high = middle;
    }
    const token = this.tokens[low];
    // Prefer the token on the right at an exact boundary, so a dot/name remains one semantic target.
    if (token?.to === position && this.tokens[low + 1]?.from === position) return this.tokens[low + 1];
    return token && token.from <= position && position <= token.to ? token : undefined;
  }

  private symbolNear(position: number): SymbolRange | null {
    const token = this.tokenAt(position);
    if (!token || ['string', 'comment', 'number', 'operator', 'punctuation', 'invalid'].includes(token.kind)) return null;
    const bindings = this.visibleBindings(position, true);
    const local = this.declarations.find(item => item.name === token.text && (item.scopeFrom <= position || item.from === token.from));
    const symbol = (local ? this.localSymbol(local) : undefined)
      ?? (isCellularSource(this.source) ? lookupCellularSymbol(token.text, this.source) : undefined)
      ?? lookupGenomeSymbol(token.text, bindings);
    return symbol ? { from: token.from, to: token.to, symbol } : null;
  }

  private callAt(position: number): GenomeSymbol | undefined {
    let depth = 0;
    for (let index = this.codeTokens.length - 1; index >= 0; index--) {
      const token = this.codeTokens[index];
      if (token.from >= position || token.kind === 'string') continue;
      if (token.text === ')') { depth++; continue; }
      if (token.text === '(') {
        if (depth) { depth--; continue; }
        const previous = this.codeTokens[index - 1];
        if (previous?.kind === 'function') return (isCellularSource(this.source) ? lookupCellularSymbol(previous.text, this.source) : undefined) ?? lookupGenomeSymbol(previous.text);
      }
      if (token.text === ';' && !depth) break;
    }
    return undefined;
  }

  private renderContext(): void {
    if (this.destroyed) return;
    const position = this.view.state.selection.main.head;
    const line = this.view.state.doc.lineAt(position);
    this.positionElement.textContent = `Ln ${line.number}, Col ${position - line.from + 1}`;
    const token = this.tokenAt(position);
    const left = token?.from === position && (token.kind === 'punctuation' || token.kind === 'operator')
      ? this.symbolNear(Math.max(0, position - 1))?.symbol : undefined;
    const symbol = this.symbolNear(position)?.symbol ?? left ?? this.callAt(position);
    this.helpElement.hidden = Boolean(this.evaluation && !symbol);
    const key = symbol ? `${symbol.signature}\n${symbol.description}` : '';
    if (key === this.helpKey && this.helpElement.childNodes.length) return;
    this.helpKey = key;
    if (symbol) this.helpElement.replaceChildren(symbolHelp(symbol, true));
    else {
      const hint = document.createElement('span'); hint.className = 'program-context-empty';
      hint.textContent = 'Place the cursor on a name to see its type or signature.';
      this.helpElement.replaceChildren(hint);
    }
  }

  private complete(context: CompletionContext): CompletionResult | null {
    if (this.destroyed || this.readOnly) return null;
    const token = this.tokenAt(context.pos);
    if (token?.kind === 'comment' || (token?.kind === 'string' && context.pos > token.from
      && (context.pos < token.to || !token.text.endsWith('"') || token.text.length === 1))) return null;
    const word = context.matchBefore(/[A-Za-z_][A-Za-z0-9_.]*/);
    if (!word && !context.explicit) return null;
    const prefix = word?.text ?? '', from = word?.from ?? context.pos;
    const bindings: { name: string; type: string; from: number; to: number }[] = this.visibleBindings(context.pos);
    for (const item of this.declarations) {
      if (item.scopeFrom <= context.pos && !bindings.some(binding => binding.name === item.name)) bindings.push({ ...item, type: 'not checked' });
    }
    const options: Completion[] = genomeCompletions(this.source, context.pos, prefix, bindings).map(symbol => ({
      label: symbol.name,
      type: symbol.category === 'binding' || symbol.category === 'register' ? 'variable'
        : symbol.category === 'signal' || symbol.category === 'output' ? 'property' : symbol.category,
      detail: symbol.type,
      info: () => symbolHelp(symbol),
      boost: symbol.category === 'binding' ? 10 : 0,
      apply: (view, completion, start, end) => {
        if (view.state.readOnly) return;
        let insert = symbol.insertText ?? symbol.name, anchor = start + insert.length;
        if (insert.endsWith(' = ') && /^\s*=/.test(view.state.sliceDoc(end, Math.min(end + 32, view.state.doc.length)))) {
          insert = symbol.name; anchor = start + insert.length;
        }
        if (symbol.category === 'function' && !symbol.insertText && !/^\s*\(/.test(view.state.sliceDoc(end, Math.min(end + 32, view.state.doc.length)))) {
          insert += '()'; anchor++;
        }
        view.dispatch({
          changes: { from: start, to: end, insert }, selection: { anchor },
          annotations: [pickedCompletion.of(completion), Transaction.userEvent.of('input.complete')],
        });
      },
    }));
    // Requery after every edit: a prior revision's inferred binding types must never be reused.
    return { from, options };
  }

  private applyDiagnostics(): void {
    this.view.dispatch(setDiagnostics(this.view.state, this.diagnostics.map(item => ({
      from: Math.max(0, Math.min(this.source.length, item.from)),
      to: Math.max(0, Math.min(this.source.length, item.to)), severity: item.severity,
      message: item.message, source: 'Genome compiler',
    }))));
    this.renderDiagnostics();
  }

  private renderDiagnostics(): void {
    this.diagnosticsElement.replaceChildren();
    this.diagnosticsElement.hidden = this.diagnostics.length === 0;
    for (const diagnostic of this.diagnostics) {
      const item = document.createElement('li');
      const button = document.createElement('button'); button.type = 'button';
      const location = document.createElement('span'); location.className = 'program-diagnostic-location';
      location.textContent = `${diagnostic.line}:${diagnostic.column}`;
      const text = document.createElement('span'); text.textContent = diagnostic.message;
      button.setAttribute('aria-label', `Line ${diagnostic.line}, column ${diagnostic.column}: ${diagnostic.message}`);
      button.append(location, text);
      button.addEventListener('click', () => {
        if (this.destroyed) return;
        const from = Math.min(this.source.length, diagnostic.from), to = Math.min(this.source.length, diagnostic.to);
        this.view.dispatch({ selection: { anchor: from, head: to }, scrollIntoView: true }); this.view.focus();
      });
      item.append(button); this.diagnosticsElement.append(item);
    }
  }

  private renderStatus(): void {
    this.element.dataset.state = this.status;
    this.element.classList.toggle('is-readonly', this.readOnly);
    this.statusElement.textContent = this.status === 'valid' ? 'Valid program'
      : this.status === 'checking' ? 'Checking…'
        : this.status === 'error' ? `${this.diagnostics.length} ${this.diagnostics.length === 1 ? 'error' : 'errors'}` : 'Not checked';
    this.sizeElement.textContent = this.analysis?.valid ? `${this.analysis.nodeCount.toLocaleString()} nodes` : '';
  }
}
