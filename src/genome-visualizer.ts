import type { GenomeGraph } from './engine';
import { buildGenomeNetwork, networkSlice, type GenomeNetwork, type NetworkNode, type NetworkEdge, type NetworkEdgeKind, type NetworkSlice } from './genome-network';

const SVG_NS = 'http://www.w3.org/2000/svg';
const WIDTH = 150, HEIGHT = 52, COLUMN = 178, ROW = 62, PAGE_SIZE = 40, VIEW_LIMIT = 112;
const COLORS: Record<NetworkEdgeKind, string> = { read: '#67d6e8', write: '#edbc72', clear: '#ee91ac', compute: '#6a8994' };
function element<K extends keyof HTMLElementTagNameMap>(tag: K, className = '', text = ''): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag); if (className) node.className = className; if (text) node.textContent = text; return node;
}
function svg<K extends keyof SVGElementTagNameMap>(tag: K, attributes: Record<string, string | number> = {}): SVGElementTagNameMap[K] {
  const node = document.createElementNS(SVG_NS, tag); for (const [name, value] of Object.entries(attributes)) node.setAttribute(name, String(value)); return node;
}
function button(text: string, className = 'secondary-button'): HTMLButtonElement {
  const node = element('button', className, text); node.type = 'button'; return node;
}
const short = (text: string, count: number): string => text.length > count ? text.slice(0, count - 1) + '…' : text;
const kindName = (kind: NetworkNode['kind']): string => kind === 'input' ? 'input signal' : kind === 'operation' ? 'builtin' : kind === 'register' ? 'register' : 'action';
const order = { register: 0, output: 1, input: 2, operation: 3 };
const displayName = (node: NetworkNode): string => node.kind === 'operation' && node.bindings.length ? node.label + ' · ' + node.bindings[0] : node.label;
interface Point { x: number; y: number }
interface NetworkLayout { points: Map<string, Point>; width: number; height: number; lanes: number }

/** Only displayed, non-temporal edges constrain the layout. */
export function layoutNetwork(slice: NetworkSlice, areaWidth = 1080, areaHeight = 570): NetworkLayout {
  const incoming = new Map<string, string[]>(), outgoing = new Map<string, string[]>();
  for (const edge of slice.edges) {
    if (edge.temporal) continue;
    const ins = incoming.get(edge.to) ?? []; ins.push(edge.from); incoming.set(edge.to, ins);
    const outs = outgoing.get(edge.from) ?? []; outs.push(edge.to); outgoing.set(edge.from, outs);
  }
  const offset = slice.nodes.some(node => node.kind === 'register') && slice.nodes.some(node => node.kind === 'input') ? 1 : 0;
  const ranks = new Map<string, number>();
  const sorted = [...slice.nodes].sort((a, b) => a.level - b.level || a.id.localeCompare(b.id, undefined, { numeric: true }));
  let maxOperation = offset;
  for (const node of sorted) {
    if (node.kind === 'register') ranks.set(node.id, 0);
    else if (node.kind === 'input') ranks.set(node.id, offset);
    else if (node.kind === 'operation') {
      const rank = Math.max(offset + 1, ...(incoming.get(node.id) ?? []).map(id => (ranks.get(id) ?? offset) + 1));
      ranks.set(node.id, rank); maxOperation = Math.max(maxOperation, rank);
    }
  }
  for (const node of sorted) if (node.kind === 'output') ranks.set(node.id, maxOperation + 1);
  const compact = new Map([...new Set(ranks.values())].sort((a,b) => a-b).map((value,index) => [value,index]));
  let columns = new Map<number, NetworkNode[]>();
  for (const node of sorted) {
    const column = compact.get(ranks.get(node.id) ?? 0) ?? 0;
    const items = columns.get(column) ?? []; items.push(node); columns.set(column, items);
  }
  const rows = new Map<string, number>();
  const rememberRows = (): void => { for (const nodes of columns.values()) nodes.forEach((node,index) => rows.set(node.id,(index+.5)/nodes.length)); };
  rememberRows();
  const rearrange = (column: number, neighbors: Map<string,string[]>): void => {
    const nodes = columns.get(column) ?? [], previous = new Map(nodes.map((node,index) => [node.id,index]));
    const center = (node: NetworkNode): number => {
      const related = (neighbors.get(node.id) ?? []).filter(id => rows.has(id));
      return related.length ? related.reduce((sum,id) => sum + rows.get(id)!,0)/related.length : rows.get(node.id) ?? 0;
    };
    nodes.sort((a,b) => center(a)-center(b) || previous.get(a.id)!-previous.get(b.id)!); rememberRows();
  };
  for (let sweep=0;sweep<3;sweep++) {
    for (let column=1;column<columns.size;column++) rearrange(column,incoming);
    for (let column=columns.size-2;column>=0;column--) rearrange(column,outgoing);
  }
  const lanes = Math.min(9,slice.edges.filter(edge => edge.temporal).length);
  // Parallel operations have no dependencies on one another, so tall ranks
  // can use adjacent columns without inventing an order or crossing an edge.
  const originalColumns = new Map([...columns].sort(([a], [b]) => a - b));
  let bestScale = 0;
  for (const maximum of [Infinity, 16, 12, 10, 8, 6]) {
    const candidate = new Map<number, NetworkNode[]>();
    for (const nodes of originalColumns.values()) {
      const count = Math.max(1, Math.ceil(nodes.length / maximum));
      const length = Math.ceil(nodes.length / count);
      for (let start = 0; start < nodes.length; start += length) candidate.set(candidate.size, nodes.slice(start, start + length));
    }
    const rows = Math.max(1, ...[...candidate.values()].map(nodes => nodes.length));
    const width = Math.max(430, Math.max(0, candidate.size - 1) * COLUMN + WIDTH + 52);
    const height = Math.max(230, rows * ROW + 76 + (lanes ? 36 + lanes * 10 : 0));
    const scale = Math.min(areaWidth / width, areaHeight / height, 1.25);
    if (scale > bestScale) { bestScale = scale; columns = candidate; }
  }
  const maxRows = Math.max(1,...[...columns.values()].map(nodes => nodes.length));
  const bodyHeight = maxRows * ROW + 24;
  const height = Math.max(230,bodyHeight + 52 + (lanes ? 36 + lanes * 10 : 0));
  const width = Math.max(430,Math.max(0,columns.size-1)*COLUMN + WIDTH + 52);
  const points = new Map<string,Point>();
  for (const [column,nodes] of columns) nodes.forEach((node,index) => points.set(node.id,{x:26+column*COLUMN,y:42+(bodyHeight-nodes.length*ROW)/2+index*ROW}));
  return { points, width, height, lanes };
}

/** Read-only network projection of the compiler graph. */
export class GenomeVisualizer {
  readonly dialog = element('dialog','genome-graph-dialog');
  private graph: GenomeGraph | null = null;
  private network: GenomeNetwork | null = null;
  private byId = new Map<string,NetworkNode>();
  private selected: string | null = null;
  private focusTrail: (string | null)[] = [null];
  private page = 0;
  private zoom = 1;
  private graphWidth = 600;
  private graphHeight = 300;
  private drawing: SVGSVGElement | null = null;
  private currentSlice: NetworkSlice | null = null;
  private readonly heading = element('h2','','Genome connections');
  private readonly summary = element('p','graph-summary');
  private readonly legend = element('div','graph-legend');
  private readonly legendItems = new Map<NetworkEdgeKind, HTMLElement>();
  private readonly memoryNote = element('span','graph-memory-note','Memory: read previous tick · write next tick');
  private readonly search = element('input');
  private readonly nodeList = element('div','graph-node-list');
  private readonly listCount = element('span');
  private readonly previous = button('Previous','text-button');
  private readonly next = button('Next','text-button');
  private readonly defaults = element('input');
  private readonly constants = element('input');
  private readonly options = element('details','graph-options-menu');
  private readonly overview = button('Overview');
  private readonly back = button('Back');
  private readonly focusButton = button('Focus');
  private readonly navigation = element('div','graph-navigation');
  private readonly scope = element('strong','graph-scope','Whole genome');
  private readonly zoomLabel = element('span','graph-zoom-label');
  private readonly viewport = element('div','graph-viewport');
  private readonly status = element('p','graph-limit');
  private readonly details = element('section','graph-details');
  private readonly resizeObserver: ResizeObserver;
  private fitted = true;

  constructor() {
    this.dialog.id = 'genome-graph-dialog'; this.heading.id = 'genome-graph-heading';
    this.dialog.setAttribute('aria-labelledby',this.heading.id);
    const heading = element('div','graph-heading'), title = element('div');
    title.append(this.heading,this.summary);
    const close = button('×','icon-button'); close.setAttribute('aria-label','Close genome visualizer');
    close.addEventListener('click',() => this.dialog.close()); heading.append(title,close);
    this.legend.setAttribute('aria-label','Connection colors');
    for (const [kind,label] of [['read','Read'],['write','Write'],['clear','Clear'],['compute','Compute']] as const) {
      const item = element('span','graph-legend-item graph-edge-'+kind,label), swatch=element('i');
      swatch.setAttribute('aria-hidden','true');item.prepend(swatch);this.legend.append(item);this.legendItems.set(kind,item);
    }
    this.memoryNote.title='Register reads use the previous tick’s values. Writes and clears commit together after evaluation; clear wins.';
    this.legend.append(this.memoryNote);
    const body = element('div','graph-body'), sidebar = element('aside','graph-sidebar');
    const label = element('label','graph-search-label','Find a node');
    this.search.type = 'search'; this.search.placeholder = 'Signal, builtin, register…'; this.search.maxLength = 96;
    this.search.setAttribute('aria-label','Find a signal, builtin, or register'); label.append(this.search);
    this.search.addEventListener('input',() => {this.page=0;this.renderList();});
    const filters = element('div','graph-options');
    for (const [input,text] of [[this.defaults,'Defaults'],[this.constants,'Constant outputs']] as const) {
      input.type='checkbox'; const filter=element('label','',text); filter.prepend(input); filters.append(filter);
      filter.title=input===this.defaults?'Include actions omitted from the program.':'Show fixed-value actions in the diagram; they always remain available in the list.';
      input.addEventListener('change',() => {this.page=0;this.renderList();this.renderGraph(true);});
    }
    this.options.append(element('summary','','Options'),filters);
    this.nodeList.setAttribute('aria-label','Genome inputs, builtins, registers, and actions');
    const pages = element('div','graph-list-pages');
    this.previous.addEventListener('click',() => {this.page--;this.renderList();if(this.previous.hidden)this.next.focus();});
    this.next.addEventListener('click',() => {this.page++;this.renderList();if(this.next.hidden)this.previous.focus();});
    pages.append(this.listCount,this.previous,this.next); sidebar.append(label,this.options,this.nodeList,pages);
    const main=element('section','graph-main'), toolbar=element('div','graph-tools');
    this.overview.addEventListener('click',() => {this.focusTrail=[null];this.selected=null;this.renderList();this.renderGraph(true);this.viewport.focus({preventScroll:true});});
    this.back.addEventListener('click',() => {if(this.focusTrail.length>1){this.focusTrail.pop();this.selected=this.focusTrail.at(-1)!;this.renderList();this.renderGraph(true);this.viewport.focus({preventScroll:true});}});
    this.focusButton.addEventListener('click',() => {if(this.selected){this.focusNode(this.selected);this.drawing?.querySelector<SVGGElement>('[data-network-node="'+this.selected+'"]')?.focus({preventScroll:true});}});
    this.focusButton.setAttribute('aria-label','Focus selected node');this.focusButton.title='Show nearby connections';
    this.overview.title='Show the whole genome and clear selection';this.back.title='Return to the previous focus';
    const out=button('−','icon-button'); out.setAttribute('aria-label','Zoom out'); out.addEventListener('click',() => this.setZoom(this.zoom/1.2));
    const inside=button('+','icon-button'); inside.setAttribute('aria-label','Zoom in'); inside.addEventListener('click',() => this.setZoom(this.zoom*1.2));
    const fit=button('Fit','text-button');fit.title='Fit all visible nodes';fit.addEventListener('click',() => this.fit());
    const zoomControls=element('div','graph-zoom-controls');zoomControls.setAttribute('aria-label','Graph zoom');
    this.navigation.append(this.back,this.overview);zoomControls.append(out,this.zoomLabel,inside,fit);
    toolbar.append(this.navigation,this.scope,this.focusButton,zoomControls);
    this.viewport.tabIndex=0;
    this.viewport.setAttribute('aria-label','Genome connection graph. Drag empty space to pan, or scroll. Tab to nodes and press Enter to select.');
    this.status.setAttribute('role','status'); this.status.setAttribute('aria-live','polite');
    this.details.setAttribute('aria-label','Selected connection details');
    main.append(toolbar,this.viewport,this.status,this.details); body.append(sidebar,main);
    this.dialog.append(heading,this.legend,body); document.body.append(this.dialog);
    this.installPan();
    this.resizeObserver=new ResizeObserver(() => {if(this.dialog.open&&this.fitted)this.fit();});
    this.resizeObserver.observe(this.viewport);
  }

  open(graph: GenomeGraph,title: string): void {
    this.graph=graph; this.network=buildGenomeNetwork(graph); this.byId=new Map(this.network.nodes.map(node=>[node.id,node]));
    this.selected=null;this.focusTrail=[null];this.page=0;this.search.value='';this.defaults.checked=false;this.constants.checked=false;this.options.open=false;
    this.heading.textContent=title;
    const count=(kind:NetworkNode['kind']):number=>this.network!.nodes.filter(node=>node.kind===kind).length;
    this.summary.textContent=([['input','inputs'],['operation','builtins'],['register','registers']] as const)
      .filter(([kind])=>count(kind)>0).map(([kind,label])=>count(kind)+' '+label).join(' · ')||'Constant actions';
    this.summary.title=this.network.expressionCount.toLocaleString()+' / 8,192 expressions; '+this.network.constantCount+' literal nodes and '+this.network.aliasCount+' aliases folded into labels; '+this.network.unusedCount+' unused expressions omitted.';
    for(const [kind,item] of this.legendItems)item.hidden=!this.network.edges.some(edge=>edge.kind===kind);
    this.memoryNote.hidden=count('register')===0;this.legend.hidden=!this.network.edges.length&&this.memoryNote.hidden;
    this.renderList();if(!this.dialog.open)this.dialog.showModal();this.renderGraph(true);
  }

  private eligibleNodes():NetworkNode[]{return this.network?.nodes.filter(node=>!node.implicit||this.defaults.checked)??[];}

  private renderList():void {
    const query=this.search.value.trim().toLowerCase();
    const matching=this.eligibleNodes().filter(node=>(node.label+' '+node.kind+' '+kindName(node.kind)+' '+node.type+' '+node.bindings.join(' ')).toLowerCase().includes(query));
    matching.sort((a,b)=>order[a.kind]-order[b.kind]||a.label.localeCompare(b.label)||a.id.localeCompare(b.id));
    const pages=Math.max(1,Math.ceil(matching.length/PAGE_SIZE));this.page=Math.max(0,Math.min(this.page,pages-1));
    const visible=matching.slice(this.page*PAGE_SIZE,(this.page+1)*PAGE_SIZE);this.nodeList.replaceChildren();let kind='';
    for(const node of visible){
      if(kind!==node.kind){kind=node.kind;this.nodeList.append(element('h3','graph-list-group',node.kind==='input'?'Inputs':node.kind==='operation'?'Builtins':node.kind==='register'?'Registers':'Actions'));}
      const control=button('','graph-list-node graph-kind-'+node.kind+(node.id===this.selected?' active':''));
      control.dataset.networkId=node.id;control.setAttribute('aria-pressed',String(node.id===this.selected));control.append(element('span','',displayName(node)));
      const meta=node.implicit?'default '+node.constants.map(value=>value.value).join(', '):node.kind==='output'&&node.constants.length&&!this.network!.edges.some(edge=>edge.to===node.id)
        ?'= '+node.constants.map(value=>value.value).join(', '):node.kind==='register'?node.type+' · previous → next':node.type;
      control.append(element('small','',short(meta,42)));control.title=displayName(node)+' · '+meta;
      control.addEventListener('click',()=>{
        this.selectNode(node.id);
        if(!this.currentSlice?.nodes.some(item=>item.id===node.id))this.focusNode(node.id);else this.revealNode(node.id);
        this.renderList();this.nodeList.querySelector<HTMLButtonElement>('[data-network-id="'+node.id+'"]')?.focus({preventScroll:true});
      });this.nodeList.append(control);
    }
    if(!visible.length)this.nodeList.append(element('p','graph-empty','No matching nodes. Try a signal or register name.'));
    this.listCount.textContent=pages>1?(this.page*PAGE_SIZE+1)+'–'+Math.min((this.page+1)*PAGE_SIZE,matching.length)+' of '+matching.length.toLocaleString():matching.length.toLocaleString()+(query?' matches':' nodes');
    this.previous.hidden=this.page===0;this.next.hidden=this.page+1>=pages;
  }

  private displayNetwork():{network:GenomeNetwork;hiddenConstants:number}{
    const network=this.network!,connected=new Set(network.edges.flatMap(edge=>[edge.from,edge.to])),focus=this.focusTrail.at(-1);let hiddenConstants=0;
    let nodes=this.eligibleNodes().filter(node=>{
      const hide=!this.constants.checked&&node.kind==='output'&&!connected.has(node.id)&&node.id!==focus;
      if(hide)hiddenConstants++;return !hide;
    });
    // Even a minimal constant-only program opens with a useful diagram.
    if(!nodes.length){nodes=this.eligibleNodes();hiddenConstants=0;}
    if(focus&&!nodes.some(node=>node.id===focus)){const node=this.byId.get(focus);if(node)nodes.push(node);}
    const ids=new Set(nodes.map(node=>node.id));
    return {network:{...network,nodes,edges:network.edges.filter(edge=>ids.has(edge.from)&&ids.has(edge.to))},hiddenConstants};
  }
  private focusNode(id:string):void{if(this.focusTrail.at(-1)!==id)this.focusTrail.push(id);this.selected=id;this.renderList();this.renderGraph(true);}
  private updateNavigation():void{
    const focused=this.focusTrail.at(-1);
    this.overview.hidden=focused===null&&this.selected===null;
    this.overview.textContent=focused===null?'Clear selection':'Overview';
    this.overview.title=focused===null?'Clear selection and fit the whole genome':'Return to the whole genome';
    // The first focus already has Overview as its return action.
    this.back.hidden=this.focusTrail.length<=2;
    this.navigation.hidden=this.overview.hidden&&this.back.hidden;
    this.focusButton.hidden=!this.selected||focused===this.selected;
  }
  private selectNode(id:string):void{
    this.selected=id;this.highlightSelection();this.renderDetails();this.updateNavigation();
    this.nodeList.querySelectorAll<HTMLButtonElement>('[data-network-id]').forEach(node=>{
      const selected=node.dataset.networkId===id;node.classList.toggle('active',selected);node.setAttribute('aria-pressed',String(selected));
    });
  }

  private renderGraph(fit:boolean):void{
    if(!this.network)return;
    const display=this.displayNetwork(),focused=this.focusTrail.at(-1)!;
    const slice=networkSlice(display.network,focused,VIEW_LIMIT,focused===null?8:3),layout=layoutNetwork(slice,this.viewport.clientWidth||1080,this.viewport.clientHeight||570);
    this.currentSlice=slice;this.graphWidth=layout.width;this.graphHeight=layout.height;
    const frontier=new Set(slice.frontier);
    const drawing=svg('svg',{class:'genome-network',viewBox:'0 0 '+layout.width+' '+layout.height,role:'group','aria-label':'Builtin and register connections'});
    const defs=svg('defs');
    for(const kind of ['read','write','clear','compute'] as const){
      const marker=svg('marker',{id:'genome-arrow-'+kind,viewBox:'0 0 10 10',refX:8,refY:5,markerWidth:6,markerHeight:6,orient:'auto'});
      marker.append(svg('path',{d:'M1 1L9 5L1 9Z',fill:COLORS[kind]}));defs.append(marker);
    }drawing.append(defs);
    const captions=svg('g',{class:'graph-column-labels','aria-hidden':'true'});
    const caption=(text:string,x:number):void=>{const label=svg('text',{x,y:22});label.textContent=text;captions.append(label);};
    if(slice.nodes.some(node=>node.kind==='input'||node.kind==='register'))caption(slice.nodes.some(node=>node.kind==='register')?'INPUTS & MEMORY':'INPUTS',26);
    const operations=slice.nodes.filter(node=>node.kind==='operation'),outputs=slice.nodes.filter(node=>node.kind==='output');
    if(operations.length)caption('BUILTINS',Math.min(...operations.map(node=>layout.points.get(node.id)!.x)));
    if(outputs.length)caption('ACTIONS',Math.min(...outputs.map(node=>layout.points.get(node.id)!.x)));
    drawing.append(captions);
    const edges=svg('g',{class:'graph-edges','aria-hidden':'true'}),temporal=slice.edges.filter(edge=>edge.temporal);let temporalIndex=0;
    for(const edge of slice.edges){
      const from=layout.points.get(edge.from),to=layout.points.get(edge.to);if(!from||!to)continue;
      const targetEdges=slice.edges.filter(item=>item.to===edge.to&&item.temporal===edge.temporal),index=targetEdges.indexOf(edge);
      const port=Math.max(-15,Math.min(15,(index-(targetEdges.length-1)/2)*10));let path:string,labelX:number,labelY:number;
      if(edge.temporal){
        const lane=temporalIndex++%Math.max(1,layout.lanes),y=layout.height-16-lane*10;
        const sx=from.x+WIDTH-14,sy=from.y+HEIGHT,tx=to.x+14+Math.max(0,port),ty=to.y+HEIGHT+3;
        if(edge.from===edge.to){path='M'+sx+','+sy+' C'+(sx+35)+','+(sy+36)+' '+(tx-35)+','+(ty+36)+' '+tx+','+ty;labelX=from.x+WIDTH/2;labelY=sy+36;}
        else{path='M'+sx+','+sy+' C'+sx+','+y+' '+sx+','+y+' '+(sx-22)+','+y+' L'+(tx+22)+','+y+' C'+tx+','+y+' '+tx+','+y+' '+tx+','+ty;labelX=(sx+tx)/2;labelY=y-5;}
      }else{
        const sx=from.x+WIDTH,sy=from.y+HEIGHT/2,tx=to.x-3,ty=to.y+HEIGHT/2+port,mid=(sx+tx)/2;
        path='M'+sx+','+sy+' C'+mid+','+sy+' '+mid+','+ty+' '+tx+','+ty;labelX=tx-7;labelY=ty-5;
      }
      const group=svg('g',{class:'graph-edge graph-edge-'+edge.kind+(edge.guard?' guarded':'')+(edge.temporal?' temporal':''),'data-edge-id':edge.id});
      const line=svg('path',{d:path,'marker-end':'url(#genome-arrow-'+edge.kind+')'}),title=svg('title');title.textContent=this.edgeDescription(edge);line.append(title);group.append(line);
      if(edge.guard||edge.temporal&&temporal.length<=10){
        const label=svg('text',{x:labelX,y:labelY,'text-anchor':edge.temporal?'middle':'end'});
        label.textContent=edge.temporal?(edge.kind==='clear'?'clear':'write')+(edge.guard?' when':'')+' · next tick':'when';group.append(label);
      }edges.append(group);
    }drawing.append(edges);
    const nodes=svg('g',{class:'graph-nodes'});
    for(const node of slice.nodes){
      const point=layout.points.get(node.id)!;
      const group=svg('g',{transform:'translate('+point.x+','+point.y+')',class:'graph-node graph-kind-'+node.kind+(frontier.has(node.id)?' frontier':'')+(node.implicit?' implicit':''),
        tabindex:0,role:'button','data-network-node':node.id,'aria-label':node.label+', '+kindName(node.kind)+', '+node.type+(frontier.has(node.id)?', more connections outside this view':'')});
      group.append(svg('rect',{width:WIDTH,height:HEIGHT,rx:node.kind==='register'?10:6}));
      const label=svg('text',{x:11,y:21,class:'graph-node-label'});label.textContent=short(node.label,19);
      const meta=svg('text',{x:11,y:39,class:'graph-node-meta'});
      const literal=node.constants.slice(0,2).map(value=>(value.rule?.startsWith('clear:')?'clear '+(value.port==='when'?'when ':''):value.port==='value'||value.port==='default'?'':value.port+' ')+value.value).join(' · ');
      meta.textContent=short(literal||(node.kind==='register'?'previous → next':node.bindings.join(', ')||kindName(node.kind)),25);
      const tooltip=svg('title');tooltip.textContent=node.label+'\n'+node.type+' · '+kindName(node.kind)+(node.bindings.length?'\nAliases: '+node.bindings.join(', '):'')
        +node.constants.map(value=>'\n'+(value.rule?value.rule+' ':'')+value.port+': '+value.value).join('');
      group.append(label,meta,tooltip);
      if(node.kind==='register'||node.kind==='input')group.append(svg('circle',{cx:WIDTH,cy:HEIGHT/2,r:3.5,class:'graph-read-port'}));
      if(node.kind==='register')group.append(svg('circle',{cx:14,cy:HEIGHT,r:3.5,class:'graph-write-port'}));
      if(frontier.has(node.id)){const more=svg('text',{x:WIDTH-8,y:12,'text-anchor':'end',class:'graph-frontier-mark'});more.textContent='…';group.append(more);}
      group.addEventListener('click',()=>this.selectNode(node.id));group.addEventListener('dblclick',()=>this.focusNode(node.id));
      group.addEventListener('keydown',event=>{if(event.key==='Enter'||event.key===' '){event.preventDefault();this.selectNode(node.id);}});
      nodes.append(group);
    }drawing.append(nodes);this.drawing=drawing;this.viewport.replaceChildren(drawing);
    this.updateNavigation();
    this.scope.textContent=focused?this.byId.get(focused)?.label??'Focused connections':'Overview';
    this.scope.title=focused?'Connections around '+this.scope.textContent:'All connected inputs, builtins, registers, and authored actions';
    const notes:string[]=[];
    if(slice.omitted)notes.push(slice.nodes.length+' of '+slice.total.toLocaleString()+' nodes. '+(slice.frontier.length?'Focus a dashed node to explore further.':'Find more nodes in the list.'));
    if(display.hiddenConstants)notes.push(display.hiddenConstants+' constant outputs in the list.');
    this.status.textContent=notes.join(' ');this.status.hidden=!notes.length;
    this.status.classList.toggle('clipped',slice.omitted>0);
    if(!slice.nodes.length)this.viewport.append(element('p','graph-empty-canvas','No nodes in this view. Find a node in the list or enable Defaults in Options.'));
    this.highlightSelection();this.renderDetails();if(fit)this.fit();else this.setZoom(this.zoom,false);
  }

  private fitScale():number{
    // The complete bounded slice must fit, including a long dependency chain.
    // Manual zoom can go down to this scale when it is below the usual 18%.
    return Math.min(Math.max(1,this.viewport.clientWidth-24)/this.graphWidth,Math.max(1,this.viewport.clientHeight-24)/this.graphHeight,1.25);
  }
  private setZoom(value:number,user=true):void{
    const old=this.zoom,width=this.viewport.clientWidth,height=this.viewport.clientHeight;
    const oldX=Math.max(0,(width-this.graphWidth*old)/2),oldY=Math.max(0,(height-this.graphHeight*old)/2);
    const cx=(this.viewport.scrollLeft+width/2-oldX)/old,cy=(this.viewport.scrollTop+height/2-oldY)/old;
    this.zoom=Math.max(Math.min(.18,this.fitScale()),Math.min(2.2,value));if(user)this.fitted=false;this.zoomLabel.textContent=Math.round(this.zoom*100)+'%';
    const scaledWidth=this.graphWidth*this.zoom,scaledHeight=this.graphHeight*this.zoom;
    const offsetX=Math.max(0,(width-scaledWidth)/2),offsetY=Math.max(0,(height-scaledHeight)/2);
    if(this.drawing){
      this.drawing.style.width=scaledWidth+'px';this.drawing.style.height=scaledHeight+'px';
      this.drawing.style.margin=offsetY+'px '+offsetX+'px';
    }
    if(user){this.viewport.scrollLeft=cx*this.zoom+offsetX-width/2;this.viewport.scrollTop=cy*this.zoom+offsetY-height/2;}
  }
  private fit():void{
    if(!this.drawing||!this.viewport.clientWidth||!this.viewport.clientHeight)return;
    this.fitted=true;this.setZoom(this.fitScale(),false);this.viewport.scrollLeft=0;this.viewport.scrollTop=0;
  }
  private revealNode(id:string):void{
    const group=this.drawing?.querySelector<SVGGElement>('[data-network-node="'+id+'"]');if(!group)return;
    const box=group.getBoundingClientRect(),parent=this.viewport.getBoundingClientRect();
    if(box.left<parent.left||box.right>parent.right)this.viewport.scrollLeft+=(box.left+box.right-parent.left-parent.right)/2;
    if(box.top<parent.top||box.bottom>parent.bottom)this.viewport.scrollTop+=(box.top+box.bottom-parent.top-parent.bottom)/2;
  }
  private highlightSelection():void{
    if(!this.drawing||!this.currentSlice)return;const ids=new Set<string>(),edges=new Set<string>();
    if(this.selected){
      ids.add(this.selected);
      for(const direction of ['up','down'] as const){
        const queue=[this.selected],seen=new Set(queue);
        for(let cursor=0;cursor<queue.length;cursor++){
          for(const edge of this.currentSlice.edges){
            if((direction==='up'?edge.to:edge.from)!==queue[cursor])continue;
            const target=direction==='up'?edge.from:edge.to;edges.add(edge.id);ids.add(target);
            // A memory endpoint ends this tick's path.
            if(seen.has(target)||this.byId.get(target)?.kind==='register')continue;seen.add(target);queue.push(target);
          }
        }
      }
    }
    this.drawing.classList.toggle('has-selection',this.selected!==null);
    this.drawing.querySelectorAll<SVGGElement>('[data-network-node]').forEach(node=>{
      node.classList.toggle('selected',node.dataset.networkNode===this.selected);node.classList.toggle('connected',ids.has(node.dataset.networkNode!));node.setAttribute('aria-pressed',String(node.dataset.networkNode===this.selected));
    });
    this.drawing.querySelectorAll<SVGGElement>('[data-edge-id]').forEach(edge=>edge.classList.toggle('connected',edges.has(edge.dataset.edgeId!)));
  }
  private edgeDescription(edge:NetworkEdge):string{
    return (this.byId.get(edge.from)?.label??edge.from)+' → '+(this.byId.get(edge.to)?.label??edge.to)+' · '+edge.kind+' '+edge.port+(edge.guard?' condition':'')+(edge.temporal?' · commits next tick':'');
  }
  private renderDetails():void{
    this.details.replaceChildren();const node=this.selected?this.byId.get(this.selected):null;
    if(!node||!this.network){
      this.details.classList.add('graph-details-empty');const note=element('p');
      note.append(element('strong','','Select a node '),document.createTextNode('to trace its inputs and outputs. Double-click to focus.'));
      this.details.append(note);return;
    }
    this.details.classList.remove('graph-details-empty');const header=element('div','graph-details-heading');
    header.append(element('h3','',node.label),element('span','graph-detail-type',kindName(node.kind)+' · '+node.type));
    const info=element('p');
    info.textContent=node.kind==='register'?'Reads the previous tick. Writes and clears apply next tick; clear wins. Empty reads do not populate memory.'
      :node.kind==='input'?'Sensor value sampled at the start of this tick.'
        :node.kind==='output'?(node.implicit?'Default value for an omitted action. ':'')+'Action request; energy and physical limits apply.'
          :node.label==='populated'||node.label.startsWith('populated(')?'Checks previous register occupancy, including stored zero, false, or empty text.'
            :'Pure operation, recomputed from its inputs each tick.';
    if(node.bindings.length)info.textContent+=' Aliases: '+node.bindings.join(', ')+'.';this.details.append(header,info);
    const literals=element('div','graph-literals');
    for(const value of node.constants){
      const entry=element('span'),role=value.rule?.startsWith('clear:')?'clear ':value.rule?.startsWith('write:')?'write ':'';
      entry.append(element('span','',role+(value.port==='clear'?'':value.port+' ')),element('code','',value.value));
      if(value.bindings?.length){
        const aliases=element('span','graph-constant-alias',' · aliases: '+value.bindings.slice(0,5).join(', ')+(value.bindings.length>5?' +'+(value.bindings.length-5)+' more':''));
        aliases.title=value.bindings.join(', ');entry.append(aliases);
      }
      literals.append(entry);
    }
    if(node.kind==='register'&&this.graph)for(const id of node.rootIds){
      const root=this.graph.roots.find(root=>root.id===id);if(!root)continue;
      literals.append(element('span',root.kind==='clear'?'graph-clear-rule':'graph-write-rule',(root.kind==='clear'?'Clear':'Write')+': '+(root.guard===null?'every tick':'when condition is true')));
    }
    if(literals.childElementCount)this.details.append(literals);
    const related=this.network.edges.filter(edge=>edge.from===node.id||edge.to===node.id),connections=element('div','graph-connections');
    for(const edge of related.slice(0,24)){
      const otherId=edge.from===node.id?edge.to:edge.from,other=this.byId.get(otherId);if(!other)continue;
      const control=button('','graph-connection graph-edge-'+edge.kind);
      control.append(element('span','',edge.kind+(edge.guard?' when':'')+(edge.temporal?' · next tick':'')),element('code','',edge.from===node.id?'→ '+other.label:other.label+' →'));
      control.title=this.edgeDescription(edge);control.addEventListener('click',()=>{
        this.selectNode(otherId);if(!this.currentSlice?.nodes.some(item=>item.id===otherId))this.focusNode(otherId);else this.revealNode(otherId);this.renderList();
      });connections.append(control);
    }
    if(related.length>24)connections.append(element('span','graph-extra-connections','+'+(related.length-24)+' connections. Focus this node to explore.'));
    if(connections.childElementCount)this.details.append(connections);
    else this.details.append(element('p','graph-no-connections',node.constants.length?'Uses the constant values shown above.':'No connected nodes.'));
  }
  private installPan():void{
    let drag:{x:number;y:number;left:number;top:number;pointer:number}|null=null;
    this.viewport.addEventListener('pointerdown',event=>{
      if(event.button!==0||(event.target as Element).closest('.graph-node'))return;
      drag={x:event.clientX,y:event.clientY,left:this.viewport.scrollLeft,top:this.viewport.scrollTop,pointer:event.pointerId};
      this.viewport.setPointerCapture(event.pointerId);this.viewport.classList.add('dragging');
    });
    this.viewport.addEventListener('pointermove',event=>{if(!drag)return;this.fitted=false;this.viewport.scrollLeft=drag.left-event.clientX+drag.x;this.viewport.scrollTop=drag.top-event.clientY+drag.y;});
    const end=():void=>{if(drag&&this.viewport.hasPointerCapture(drag.pointer))this.viewport.releasePointerCapture(drag.pointer);drag=null;this.viewport.classList.remove('dragging');};
    this.viewport.addEventListener('pointerup',end);this.viewport.addEventListener('pointercancel',end);
    this.viewport.addEventListener('wheel',event=>{if(!event.ctrlKey)return;event.preventDefault();this.setZoom(this.zoom*(event.deltaY<0?1.1:1/1.1));},{passive:false});
  }
}
