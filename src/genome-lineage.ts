import type {GenomeLineageEntry, MetricsSnapshot} from './engine';
import {genomeColor} from './renderer';
import {icon} from './ui';

const mutationLabels = ['Root genome','Constant change','Expression change','Reference change','Gene silencing','Gene duplication','Placement change'];
const pageSize = 40;
const branchDepth = 20;
type Node = GenomeLineageEntry & {rootId:number; depth:number};
type Counts = MetricsSnapshot['genomes'][number];

/** The native registry is append-only, with each parent preceding its children. */
export function projectLineage(nodes:ReadonlyMap<number,GenomeLineageEntry>, sampled:number, living:ReadonlySet<number>, showExtinct:boolean) {
  const retained=new Set<number>();
  for (const id of living) {
    if (id>sampled) continue;
    let next=id;
    while (next && !retained.has(next)) {
      retained.add(next);
      next=nodes.get(next)!.parentId;
    }
  }
  const branches=new Map<number,number[]>();
  let count=0, newest=0;
  for (let id=1;id<=sampled;id++) {
    if (!showExtinct && !retained.has(id)) continue;
    const {parentId}=nodes.get(id)!;
    let children=branches.get(parentId);
    if (!children) { children=[];branches.set(parentId,children); }
    children.push(id);
    if (parentId) newest=id;
    count++;
  }
  return {branches,retained,count,newest};
}

interface Row {
  item:HTMLLIElement; toggle:HTMLButtonElement; program:HTMLButtonElement; graph:HTMLButtonElement;
  name:HTMLElement; stats:HTMLElement; note:HTMLElement; group:HTMLUListElement;
}

const setText = (element:HTMLElement, value:string):void => {
  if (element.textContent!==value) element.textContent=value;
};

/** One view per simulation. Lineage links are immutable and resident in its worker registry. */
export class GenomeLineage {
  private nodes = new Map<number,Node>();
  private rows = new Map<number,Row>();
  private opened = new Map<number,boolean>();
  private pages = new Map<number,number>();
  private anchor = 0;
  private visible:number[] = [];
  private sampled = 0;
  private showExtinct = false;
  private projection = projectLineage(this.nodes,0,new Set(),false);
  private living = new Map<number,Counts>();
  private metrics:MetricsSnapshot|null = null;
  private target:HTMLElement|null = null;
  private forest:HTMLUListElement|null = null;
  private fetching = false;
  private error = '';

  constructor(private readonly names:Record<string,string>, private readonly load:(afterId:number)=>Promise<GenomeLineageEntry[]>) {}

  has(id:number):boolean { return this.nodes.has(id); }

  nameOf(id:number):string {
    const known=this.names[String(id)];
    if (known) return known;
    const node=this.nodes.get(id);
    const root=node?.parentId && this.names[String(node.rootId)];
    const suffix=' · '+id;
    return root ? root.slice(0,80-suffix.length)+suffix : 'Genome '+id;
  }

  refreshNames():void { this.updateReadouts(); }

  update(target:HTMLElement, metrics:MetricsSnapshot, showExtinct:boolean):void {
    const mounted=target!==this.target, changed=metrics!==this.metrics;
    if (mounted) {
      this.target=target; this.rows.clear();
      target.innerHTML='<header class="lineage-heading"><div><h2>Genome lineages</h2><p data-lineage-summary></p></div><div class="lineage-actions"><label><input type="checkbox" data-setting="showExtinct" data-lineage-extinct> Show Extinct</label><button type="button" class="button subtle small" data-lineage-newest hidden></button></div></header><nav class="lineage-context" aria-label="Branch navigation" hidden><button type="button" class="button subtle small" data-lineage-roots>All roots</button><span data-lineage-context-title></span><button type="button" class="button subtle small" data-lineage-earlier></button></nav><p class="lineage-status" role="status" hidden></p><ul class="lineage-forest" aria-label="Genome ancestry"></ul>';
      this.forest=target.querySelector('ul');
      target.addEventListener('click',this.click);
      target.addEventListener('keydown',this.keydown);
    }
    let livingChanged=false;
    if (changed) {
      this.metrics=metrics;
      const living=new Map(metrics.genomes.filter(row=>row.population>0).map(row=>[row.id,row]));
      livingChanged=living.size!==this.living.size || [...living.keys()].some(id=>!this.living.has(id));
      this.living=living;
    }
    if (mounted || livingChanged || showExtinct!==this.showExtinct || this.sampled!==Math.min(this.nodes.size,metrics.genomeCount)) {
      this.showExtinct=showExtinct;
      target.querySelector<HTMLInputElement>('[data-lineage-extinct]')!.checked=showExtinct;
      this.project();
    }
    else if (changed) this.updateReadouts();
    if (!this.error && !this.fetching && metrics.genomeCount>this.nodes.size) this.fetch();
  }

  private fetch():void {
    this.fetching=true; this.status();
    const start=this.nodes.size;
    void this.load(start).then(entries=>{
      if (!entries.length && (this.metrics?.genomeCount ?? 0)>start) throw new Error('The genome registry could not be read.');
      for (const entry of entries) {
        const parent=this.nodes.get(entry.parentId);
        this.nodes.set(entry.id,{...entry,rootId:parent?.rootId??entry.id,depth:parent?parent.depth+1:0});
      }
      this.project();
    }).catch(error=>{this.error=error instanceof Error ? error.message : String(error);}).finally(()=>{
      this.fetching=false;this.status();
    });
  }

  private status():void {
    if (!this.target?.isConnected) return;
    const status=this.target.querySelector<HTMLElement>('.lineage-status')!;
    const message=this.error || (this.projection.count ? '' : this.fetching || this.sampled<(this.metrics?.genomeCount??0)
      ? 'Loading genome lineages…' : this.nodes.size ? 'No living organisms in this simulation. Turn on Show Extinct to see its registered genomes.'
      : 'No genomes have been registered in this simulation.');
    if (status.dataset.message===message) return;
    status.dataset.message=message; status.textContent=message; status.hidden=!message;
    if (this.error) {
      const button=status.ownerDocument.createElement('button');
      button.type='button'; button.className='button subtle small'; button.dataset.lineageRetry=''; button.textContent='Retry';
      status.append(' ',button);
    }
  }

  private row(node:Node):Row {
    const cached=this.rows.get(node.id);
    if (cached) return cached;
    const item=this.target!.ownerDocument.createElement('li');
    item.dataset.lineageNode=String(node.id); item.dataset.lineageParent=String(node.parentId); item.dataset.mutationKind=String(node.mutationKind);
    if (!node.parentId) item.dataset.lineageRoot=String(node.id);
    const groupId='lineage-children-'+node.id;
    item.innerHTML=`<div class="lineage-row">
      <button type="button" class="lineage-toggle" data-lineage-toggle="${node.id}" aria-controls="${groupId}">${icon('chevron')}</button>
      <button type="button" class="lineage-program" data-action="live-program" data-genome="${node.id}"><i aria-hidden="true" class="genome-dot" style="background:${genomeColor(node.id)}"></i><span class="lineage-identity"><span class="lineage-name"></span><span class="lineage-note"></span></span></button>
      <span class="lineage-population" data-genome-stats="${node.id}"></span>
      <button type="button" class="icon-button lineage-graph" data-action="live-graph" data-genome="${node.id}">${icon('graph')}</button></div>
      <ul class="lineage-children${node.depth>=5?' lineage-compact':''}" id="${groupId}" aria-label="Child variants of genome ${node.id}" hidden></ul>`;
    const row:Row={item,toggle:item.querySelector('.lineage-toggle')!,program:item.querySelector('.lineage-program')!,
      graph:item.querySelector('.lineage-graph')!,name:item.querySelector('.lineage-name')!,stats:item.querySelector('[data-genome-stats]')!,
      note:item.querySelector('.lineage-note')!,group:item.querySelector('ul')!};
    setText(row.note,node.parentId ? `#${node.id} · ${mutationLabels[node.mutationKind]} · parent #${node.parentId}` : `#${node.id} · Root genome`);
    this.rows.set(node.id,row);
    return row;
  }

  private children(id:number):number[] { return this.projection.branches.get(id) ?? []; }

  private project():void {
    this.sampled=Math.min(this.nodes.size,this.metrics?.genomeCount??0);
    this.projection=projectLineage(this.nodes,this.sampled,new Set(this.living.keys()),this.showExtinct);
    while (this.anchor && (this.anchor>this.sampled || !this.showExtinct && !this.projection.retained.has(this.anchor))) {
      this.anchor=this.nodes.get(this.anchor)!.parentId;
    }
    if (this.anchor && !this.nodes.get(this.anchor)!.parentId) this.anchor=0;
    this.structure();
  }

  /** Reconcile by genome ID so live count updates retain the focused row and scroll position. */
  private structure():void {
    if (!this.target?.isConnected || !this.forest) return;
    const active=this.target.ownerDocument.activeElement as HTMLElement|null;
    const hadFocus=active && this.target.contains(active);
    const focusedId=Number(active?.closest<HTMLElement>('[data-lineage-node]')?.dataset.lineageNode);
    const visible:number[]=[];
    const newest=this.target.querySelector<HTMLButtonElement>('[data-lineage-newest]')!;
    newest.hidden=!this.projection.newest;
    if (!newest.hidden) setText(newest,'Newest variant #'+this.projection.newest);
    const roots=this.children(0);
    const anchorNode=this.nodes.get(this.anchor);
    const context=this.target.querySelector<HTMLElement>('.lineage-context')!;
    context.hidden=!anchorNode;
    if (anchorNode) {
      setText(context.querySelector('[data-lineage-context-title]')!,`${this.nameOf(anchorNode.rootId)} · branch from #${anchorNode.id}`);
      setText(context.querySelector('[data-lineage-earlier]')!,`Earlier ancestors · parent #${anchorNode.parentId}`);
    }
    const branch=(owner:number,group:HTMLUListElement,ids:number[])=>{
      const count=ids.length;
      const page=Math.min(this.pages.get(owner)??0,Math.max(0,Math.ceil(count/pageSize)-1));
      this.pages.set(owner,page);
      const start=page*pageSize, end=Math.min(start+pageSize,count);
      let controls=group.lastElementChild?.hasAttribute('data-lineage-page') ? group.lastElementChild as HTMLLIElement : null;
      if (count>pageSize) {
        if (!controls) {
          controls=group.ownerDocument.createElement('li'); controls.dataset.lineagePage='';
          controls.innerHTML=`<button type="button" class="button subtle small" data-lineage-previous="${owner}">Previous</button><span></span><button type="button" class="button subtle small" data-lineage-more="${owner}">Next</button>`;
          group.append(controls);
        }
        (controls.firstElementChild as HTMLButtonElement).disabled=start===0;
        (controls.lastElementChild as HTMLButtonElement).disabled=end===count;
        setText(controls.querySelector('span')!,`${start+1}–${end} of ${count} ${owner?'variants':'roots'}`);
      } else if (controls) { controls.remove(); controls=null; }
      return {group,ids,index:start,end,previous:null as Element|null};
    };
    const stack=[anchorNode?branch(-1,this.forest,[anchorNode.id]):branch(0,this.forest,roots)];
    while (stack.length) {
      const group=stack[stack.length-1];
      if (group.index===group.end) {stack.pop();continue;}
      const id=group.ids[group.index++],node=this.nodes.get(id)!,row=this.row(node);
      const next=group.previous ? group.previous.nextElementSibling : group.group.firstElementChild;
      if (row.item!==next) group.group.insertBefore(row.item,next);
      group.previous=row.item;
      visible.push(id);
      const children=this.children(id);
      if (children.length && !this.opened.has(id)) this.opened.set(id,this.projection.count<=pageSize || node.depth===0 && roots.length<=8);
      const boundary=this.isDepthBoundary(node);
      const open=children.length>0 && !boundary && this.opened.get(id)===true;
      row.toggle.hidden=!children.length;
      row.toggle.setAttribute('aria-expanded',String(open));
      row.toggle.setAttribute('aria-label',boundary ? `Continue branch from genome ${id}` : `${open?'Collapse':'Expand'} ${children.length} child ${children.length===1?'variant':'variants'} of genome ${id}`);
      row.toggle.title=row.toggle.getAttribute('aria-label')!;
      row.group.hidden=!open;
      if (open) stack.push(branch(id,row.group,children));
    }
    const painted=new Set(visible);
    for (const [id,row] of this.rows) {
      if (!painted.has(id)) { row.item.remove();this.rows.delete(id); }
    }
    if (hadFocus) {
      if (!active.isConnected || active.closest('[hidden]')) {
        let ancestor=focusedId;
        while (ancestor && !painted.has(ancestor)) ancestor=this.nodes.get(ancestor)?.parentId ?? 0;
        const fallback=this.rows.get(ancestor || visible[0])?.program ?? this.target.querySelector<HTMLInputElement>('[data-lineage-extinct]');
        fallback?.focus({preventScroll:true});
      } else if (this.target.ownerDocument.activeElement!==active) active.focus({preventScroll:true});
    }
    this.visible=visible;
    this.updateReadouts();
  }

  private updateReadouts():void {
    if (!this.target?.isConnected) return;
    const roots=this.children(0).length, shown=this.projection.count, variants=shown-roots;
    const summary=this.target.querySelector<HTMLElement>('[data-lineage-summary]')!;
    const living=this.metrics?.genomes.filter(row=>row.population>0 && row.id<=this.sampled).length??0;
    setText(summary,this.sampled ? `${shown} of ${this.sampled} registered · ${roots} ${roots===1?'root':'roots'} · ${variants} ${variants===1?'variant':'variants'} · ${living} with living organisms` : '');
    for (const id of this.visible) {
      const row=this.rows.get(id)!,name=this.nameOf(id),count=this.living.get(id);
      if (row.name.textContent!==name) {
        setText(row.name,name); row.program.setAttribute('aria-label','Inspect '+name);
        row.graph.title='Graph of '+name; row.graph.setAttribute('aria-label',row.graph.title);
      }
      row.item.toggleAttribute('data-lineage-living',!!count);
      setText(row.stats,count ? `${count.population} ${count.population===1?'organism':'organisms'} · generation ${count.maxGeneration}` : this.projection.retained.has(id) ? 'No living organisms · living descendants' : 'No living organisms');
    }
    this.status();
  }

  private isDepthBoundary(node:Node):boolean {
    return node.depth-(this.nodes.get(this.anchor)?.depth??0)>=branchDepth;
  }

  private rebuild():void {
    this.rows.clear();this.forest?.replaceChildren();this.structure();
  }

  private focus(id:number):void {
    const program=this.rows.get(id)?.program;
    program?.focus({preventScroll:true});program?.scrollIntoView({block:'nearest',inline:'nearest'});
  }

  /** Reveal a bounded section of the path. Every row retains its actual direct parent. */
  private reveal(id:number):void {
    const target=this.nodes.get(id);
    if (!target) return;
    let first=target;
    for (let depth=0;first.parentId && depth<branchDepth-1;depth++) first=this.nodes.get(first.parentId)!;
    const anchor=first.parentId ? first.id : 0;
    let rebuild=anchor!==this.anchor;
    this.anchor=anchor;
    let node:Node|undefined=target;
    while (node && node.id!==anchor) {
      const parent:Node|undefined=this.nodes.get(node.parentId);
      const siblings=this.children(node.parentId);
      const page=Math.floor(siblings.indexOf(node.id)/pageSize);
      if ((this.pages.get(node.parentId)??0)!==page) {this.pages.set(node.parentId,page);rebuild=true;}
      if (parent) this.opened.set(parent.id,true);
      node=parent;
    }
    if (rebuild) this.rebuild(); else this.structure();
    this.focus(id);
  }

  private toggle(id:number):void {
    const node=this.nodes.get(id)!;
    if (this.isDepthBoundary(node)) {
      this.anchor=id;this.opened.set(id,true);this.rebuild();this.focus(id);
    } else {this.opened.set(id,!this.opened.get(id));this.structure();}
  }

  private click=(event:MouseEvent):void=>{
    const button=(event.target as Element).closest<HTMLButtonElement>('button');
    if (!button) return;
    if (button.hasAttribute('data-lineage-newest')) {this.reveal(this.projection.newest);return;}
    if (button.hasAttribute('data-lineage-roots')) {this.anchor=0;this.rebuild();this.focus(this.visible[0]);return;}
    if (button.hasAttribute('data-lineage-earlier')) {this.reveal(this.nodes.get(this.anchor)!.parentId);return;}
    if (button.hasAttribute('data-lineage-retry')) {this.error='';this.fetch();return;}
    if (button.dataset.lineageMore!==undefined || button.dataset.lineagePrevious!==undefined) {
      const owner=Number(button.dataset.lineageMore??button.dataset.lineagePrevious);
      const page=(this.pages.get(owner)??0)+(button.dataset.lineageMore!==undefined?1:-1);
      this.pages.set(owner,page);this.rebuild();this.focus(this.children(owner)[page*pageSize]);return;
    }
    if (button.dataset.lineageToggle!==undefined) this.toggle(Number(button.dataset.lineageToggle));
  };

  private keydown=(event:KeyboardEvent):void=>{
    if (!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Home','End'].includes(event.key) || event.altKey||event.metaKey||event.ctrlKey) return;
    const id=Number((event.target as Element).closest<HTMLElement>('[data-lineage-node]')?.dataset.lineageNode),node=this.nodes.get(id);
    if (!node) return;
    event.preventDefault();
    if (event.key==='ArrowLeft' || event.key==='ArrowRight') {
      const right=event.key==='ArrowRight', open=this.rows.get(id)?.toggle.getAttribute('aria-expanded')==='true';
      const children=this.children(id);
      if (children.length && open!==right) {
        this.toggle(id); this.rows.get(id)?.toggle.focus(); return;
      }
      if (!right && id===this.anchor) {this.reveal(node.parentId);return;}
      this.rows.get(right ? children[(this.pages.get(id)??0)*pageSize] : node.parentId)?.program.focus(); return;
    }
    const index=this.visible.indexOf(id);
    const next=event.key==='Home' ? 0 : event.key==='End' ? this.visible.length-1 : index+(event.key==='ArrowDown'?1:-1);
    this.rows.get(this.visible[next])?.program.focus();
  };
}
