import { cellularDeclarations, tokenizeGenome, type GenomeToken } from './genome-language';

type SymbolKind = 'product' | 'assembly' | 'gene' | 'function' | 'parameter' | 'binding';
export interface SourceRange { from: number; to: number }
export interface ProgramDeclaration extends SourceRange {
  name: string;
  kind: SymbolKind;
  references: SourceRange[];
}
interface Occurrence extends SourceRange {
  declaration: ProgramDeclaration;
  tokenIndex: number;
  token: SourceRange;
}

const namePattern = /^[A-Za-z_][A-Za-z_0-9]*$/;
const recipeControls = new Set(['activity', 'start_when', 'engage_when', 'recycle', 'at', 'direction']);

/** Source ranges for authored declarations and their uses. Each dotted reading links
 * just its product/assembly component. Rust still decides whether an edit compiles. */
export class ProgramNavigation {
  readonly declarations: ProgramDeclaration[] = [];
  private readonly occurrences: Occurrence[] = [];
  private readonly identities = new Map<number, number>();

  constructor(source: string, input: readonly GenomeToken[] = tokenizeGenome(source)) {
    const tokens = input.filter(token => token.kind !== 'comment');
    const byStart = new Map(tokens.map((token, index) => [token.from, index]));
    const declarationIndices = new Map<ProgramDeclaration, number>();
    const definitions = new Map<number, ProgramDeclaration>();
    const products = new Map<string, ProgramDeclaration>();
    const assemblies = new Map<string, ProgramDeclaration>();
    const functions = new Map<string, ProgramDeclaration>();
    const bindings = new Map<string, ProgramDeclaration>();
    const params = new Map<number, Map<string, ProgramDeclaration>>();
    const bind = (index: number, target: ProgramDeclaration | undefined, offset = 0): void => {
      if (!target) return;
      const from = tokens[index].from + offset, to = from + target.name.length;
      this.occurrences.push({ from, to, declaration: target, tokenIndex: index, token: tokens[index] });
      this.identities.set(index, declarationIndices.get(target)!);
      if (from !== target.from) target.references.push({ from, to });
    };
    const declare = (index: number, kind: SymbolKind): ProgramDeclaration => {
      const {text: name, from, to} = tokens[index];
      const declaration = {name, from, to, kind, references: []};
      this.declarations.push(declaration);
      declarationIndices.set(declaration, index);
      definitions.set(index, declaration);
      bind(index, declaration);
      return declaration;
    };
    const catalog = cellularDeclarations(source);
    for (const {from, name} of catalog.products) products.set(name, declare(byStart.get(from)!, 'product'));
    for (const {from, name} of catalog.assemblies) assemblies.set(name, declare(byStart.get(from)!, 'assembly'));
    for (const {from} of catalog.genes) declare(byStart.get(from)!, 'gene');
    for (const {from, parameterRanges} of catalog.functions) {
      const index = byStart.get(from)!;
      declare(index, 'function');
      const names = new Map<string, ProgramDeclaration>();
      for (const parameter of parameterRanges) names.set(parameter.name, declare(byStart.get(parameter.from)!, 'parameter'));
      params.set(index, names);
    }

    let start = 0, inAssembly = false, expression = false;
    // Bindings/functions become visible after their statement. Product and recipe
    // names are available to the whole program, including before declaration.
    for (let i = 0; i < tokens.length; i++) {
      const word = tokens[i].text, head = tokens[start].text;
      if (word === '{' || word === '}') {
        inAssembly = word === '{'; start = i + 1; expression = false; continue;
      }
      if (word === ';') {
        const target = definitions.get(start + 1);
        if (target?.kind === 'binding') bindings.set(target.name, target);
        if (target?.kind === 'function') functions.set(target.name, target);
        start = i + 1; expression = false; continue;
      }
      if (head === 'let' && i === start + 1 && namePattern.test(word) && tokens[i + 1]?.text === '=') {
        declare(i, 'binding'); continue;
      }
      if (definitions.has(i)) continue;
      if (inAssembly && i === start) {
        const literalIngredient = word === 'engage_when' && tokens[i+2]?.kind === 'number' && tokens[i+3]?.text === ';';
        if (!recipeControls.has(word) || literalIngredient) bind(i, products.get(word));
      }
      if (!expression) {
        if (word === '=' || word === 'when' || inAssembly && word === ':') expression = true;
        else if (head === 'gene' && i === start + 3) bind(i, products.get(word));
        continue;
      }
      if (head === 'product') {
        if (tokens[i-1]?.text === '(' && tokens[i-2]?.text === 'adhesin') bind(i, products.get(word));
        continue;
      }
      const [prefix, name, field] = word.split('.');
      if (name && (prefix === 'pool' || prefix === 'concentration')) bind(i, products.get(name), prefix.length + 1);
      else if (name && (prefix === 'built' || prefix === 'sensed' && field)) bind(i, assemblies.get(name), prefix.length + 1);
      else if (namePattern.test(word)) {
        const parameter = head === 'fn' ? params.get(start + 1)?.get(word) : undefined;
        bind(i, tokens[i+1]?.text === '(' ? functions.get(word) : parameter ?? bindings.get(word));
      }
    }
    this.declarations.sort((a,b) => a.from - b.from);
    this.occurrences.sort((a,b) => a.tokenIndex - b.tokenIndex);
  }

  at(position: number): ProgramDeclaration | undefined {
    return this.occurrences.find(item => item.token.from <= position && position <= item.token.to)?.declaration;
  }

  rename(source: string, target: ProgramDeclaration, name: string): {from:number;to:number;insert:string}[] {
    if (!namePattern.test(name) || name.length > 64) throw Error('Use up to 64 letters, digits or underscores, starting with a letter or underscore.');
    const edits = [target, ...target.references].map(({from,to}) => ({from,to,insert:name})).sort((a,b) => a.from - b.from);
    let candidate = '', end = 0;
    for (const edit of edits) {
      candidate += source.slice(end, edit.from) + name;
      end = edit.to;
    }
    const next = new ProgramNavigation(candidate + source.slice(end));
    // Any changed binding, including capture of an otherwise built-in input, changes meaning.
    if (this.identities.size !== next.identities.size || [...this.identities].some(([index, id]) => next.identities.get(index) !== id))
      throw Error('That name would change which declaration a reference uses. Choose another name.');
    return edits;
  }
}
