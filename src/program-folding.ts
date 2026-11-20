import { EditorState, type Text } from '@codemirror/state';
import { foldGutter, foldedRanges, foldService, unfoldEffect } from '@codemirror/language';
import { isCellularSource, tokenizeGenome } from './genome-language';
import type { SourceRange } from './program-navigation';

const cache = new WeakMap<Text, Map<number, SourceRange>>();

/** Ranges are keyed by header line so the gutter also works for split headers. */
function folds(doc: Text): Map<number, SourceRange> {
  const cached = cache.get(doc);
  if (cached) return cached;
  const ranges = new Map<number, SourceRange>();
  cache.set(doc, ranges);
  const source = doc.toString();
  if (!isCellularSource(source)) return ranges;
  const tokens = tokenizeGenome(source, true).filter(token => token.kind !== 'comment');
  let header = -1, openEnd = 0, depth = 0;
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (!depth && token.text === 'assembly' && tokens[i+2]?.text === ':') header = token.from;
    if (header < 0) continue;
    if (token.text === '{') {
      if (!depth) openEnd = token.to;
      depth++;
    } else if (token.text === '}' && depth) {
      if (--depth) continue;
      const line = doc.lineAt(header);
      if (token.from > line.to && !ranges.has(line.from)) {
        ranges.set(line.from, {from: Math.min(openEnd, line.to), to: token.from});
      }
      header = -1;
    } else if (token.text === ';' && !depth) header = -1;
  }
  return ranges;
}

export const assemblyFolding = [
  foldService.of((state, lineStart) => folds(state.doc).get(lineStart) ?? null),
  foldGutter(),
  // CodeMirror maps an existing fold through edits. Remove it if the edit
  // also removed its assembly header or changed which closing brace matches.
  EditorState.transactionExtender.of(tr => {
    if (!tr.docChanged) return null;
    const current = foldedRanges(tr.startState);
    if (!current.size) return null;
    const doc = tr.newDoc, ranges = folds(doc);
    const effects = [];
    for (let cursor = current.map(tr.changes).iter(); cursor.value; cursor.next()) {
      const expected = ranges.get(doc.lineAt(cursor.from).from);
      if (expected?.from !== cursor.from || expected.to !== cursor.to) {
        effects.push(unfoldEffect.of({from: cursor.from, to: cursor.to}));
      }
    }
    return effects.length ? {effects} : null;
  }),
];
