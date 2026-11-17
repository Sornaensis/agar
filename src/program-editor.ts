import { Compartment, EditorState, Transaction, type Extension } from '@codemirror/state';
import {
  EditorView, drawSelection, dropCursor, highlightActiveLine, highlightActiveLineGutter,
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
import type { Engine, GenomeAnalysis, GenomeBinding, GenomeDiagnostic } from './engine';
import {
  lookupGenomeSymbol, registerCompletions, searchGenomeSymbols, tokenizeGenome,
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
  languageData: { commentTokens: { line: '#' }, closeBrackets: { brackets: ['(', '"'] }, wordChars: '.' },
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
    const host = document.createElement('div'); host.className = 'program-editor-code';
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
    this.element.append(host, this.helpElement, this.diagnosticsElement, statusBar, shortcuts);
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
            if (!match || (position === match.from && side < 0) || (position === match.to && side > 0)) return null;
            return { pos: match.from, end: match.to, above: true, create: () => ({ dom: symbolHelp(match.symbol) }) };
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
    this.renderStatus(); this.renderContext();
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
    const position = Math.min(this.view.state.selection.main.head, source.length);
    this.view.dispatch({
      changes: { from: 0, to: this.view.state.doc.length, insert: source },
      selection: { anchor: position },
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
    this.options.onStateChange?.();
  }

  focus(): void { if (!this.destroyed) this.view.focus(); }

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
      this.bindingByFrom.clear();
      this.sourceBytes = encoder.encode(this.source).length;
      this.refreshTokens(); this.renderDiagnostics(); this.renderStatus();
      this.scheduleCheck(); this.options.onChange?.(this.source); this.options.onStateChange?.();
    }
    if (update.docChanged || update.selectionSet) this.renderContext();
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
      if (name?.kind !== 'identifier' || this.codeTokens[index + 2]?.text !== '=') continue;
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
    const symbol = lookupGenomeSymbol(token.text, bindings) ?? (local ? this.localSymbol(local) : undefined);
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
        if (previous?.kind === 'function') return lookupGenomeSymbol(previous.text);
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
    const bindings = this.visibleBindings(context.pos);
    const symbols = new Map(searchGenomeSymbols(prefix, bindings, 160).map(item => [item.name, item]));
    for (const item of registerCompletions(prefix, this.source, 30)) symbols.set(item.name, item);
    for (const item of this.declarations) {
      if (item.scopeFrom <= context.pos && item.name.toLowerCase().startsWith(prefix.toLowerCase())) symbols.set(item.name, this.localSymbol(item));
      if (symbols.size >= 200) break;
    }
    const options: Completion[] = [...symbols.values()].map(symbol => ({
      label: symbol.name,
      type: symbol.category === 'binding' || symbol.category === 'register' ? 'variable'
        : symbol.category === 'signal' || symbol.category === 'output' ? 'property' : symbol.category,
      detail: symbol.type,
      info: () => symbolHelp(symbol),
      boost: symbol.category === 'binding' ? 10 : 0,
      apply: (view, completion, start, end) => {
        if (view.state.readOnly) return;
        let insert = symbol.insertText ?? symbol.name, anchor = start + insert.length;
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
