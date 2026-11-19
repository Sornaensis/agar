import type {GenomeLineageEntry, MetricsSnapshot} from './engine';
import {genomeColor} from './renderer';
import {icon} from './ui';

const mutationLabels = ['Root genome','Constant change','Expression change','Reference change','Gene silencing','Gene duplication','Placement change'];
const pageSize = 40;
const branchDepth = 20;
type Node = GenomeLineageEntry & {rootId:number; depth:number; children:number[]};
type Counts = MetricsSnapshot['genomes'][number];

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
  private roots:number[] = [];
  private rows = new Map<number,Row>();
  private opened = new Map<number,boolean>();
  private pages = new Map<number,number>();
  private anchor = 0;
  private visible:number[] = [];
  private shownCount = 0;
  private shownRoots = 0;
  private newestVariant = 0;
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

  update(target:HTMLElement, metrics:MetricsSnapshot):void {
    const mounted=target!==this.target, changed=metrics!==this.metrics;
    if (mounted) {
      this.target=target; this.rows.clear();
      target.innerHTML='<header class="lineage-heading"><div><h2>Genome lineages</h2><p data-lineage-summary></p></div><button type="button" class="button subtle small" data-lineage-newest hidden></button></header><nav class="lineage-context" aria-label="Branch navigation" hidden><button type="button" class="button subtle small" data-lineage-roots>All roots</button><span data-lineage-context-title></span><button type="button" class="button subtle small" data-lineage-earlier></button></nav><p class="lineage-status" role="status" hidden></p><ul class="lineage-forest" aria-label="Genome ancestry"></ul>';
      this.forest=target.querySelector('ul');
      target.addEventListener('click',this.click);
      target.addEventListener('keydown',this.keydown);
    }
    if (changed) {
      this.metrics=metrics;
      this.living=new Map(metrics.genomes.filter(row=>row.population>0).map(row=>[row.id,row]));
    }
    if (mounted || this.shownCount!==Math.min(this.nodes.size,metrics.genomeCount)) this.structure();
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
        this.nodes.set(entry.id,{...entry,rootId:parent?.rootId??entry.id,depth:parent?parent.depth+1:0,children:[]});
        (parent?.children??this.roots).push(entry.id);
      }
      this.structure();
    }).catch(error=>{this.error=error instanceof Error ? error.message : String(error);}).finally(()=>{
      this.fetching=false;this.status();
    });
  }

  private status():void {
    if (!this.target?.isConnected) return;
    const status=this.target.querySelector<HTMLElement>('.lineage-status')!;
    const message=this.error || (!this.nodes.size ? this.fetching ? 'Loading genome lineages…' : 'No genomes have been registered in this simulation.' : '');
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

  /** Keep existing buttons and focus when the native registry gains a branch. */
  private structure():void {
    if (!this.target?.isConnected || !this.forest) return;
    const visible:number[]=[];
    // The worker can register new offspring between a metrics sample and a registry
    // read. Show only IDs covered by this sample when stating their populations.
    this.shownCount=Math.min(this.nodes.size,this.metrics?.genomeCount??0);
    const countAtSample=(ids:number[]):number=>{
      if (!ids.length || ids[ids.length-1]<=this.shownCount) return ids.length;
      const next=ids.findIndex(id=>id>this.shownCount);
      return next<0?ids.length:next;
    };
    this.newestVariant=0;
    for (let id=this.shownCount;id>0;id--) {
      if (this.nodes.get(id)!.parentId) {this.newestVariant=id;break;}
    }
    const newest=this.target.querySelector<HTMLButtonElement>('[data-lineage-newest]')!;
    newest.hidden=!this.newestVariant;
    if (this.newestVariant) setText(newest,'Newest variant #'+this.newestVariant);
    this.shownRoots=countAtSample(this.roots);
    const anchorNode=this.nodes.get(this.anchor);
    const context=this.target.querySelector<HTMLElement>('.lineage-context')!;
    context.hidden=!anchorNode;
    if (anchorNode) {
      setText(context.querySelector('[data-lineage-context-title]')!,`${this.nameOf(anchorNode.rootId)} · branch from #${anchorNode.id}`);
      setText(context.querySelector('[data-lineage-earlier]')!,`Earlier ancestors · parent #${anchorNode.parentId}`);
    }
    const branch=(owner:number,group:HTMLUListElement,ids:number[],count:number)=>{
      const start=(this.pages.get(owner)??0)*pageSize, end=Math.min(start+pageSize,count);
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
      return {group,ids,index:start,end,more:controls};
    };
    const stack=[anchorNode?branch(-1,this.forest,[anchorNode.id],1):branch(0,this.forest,this.roots,this.shownRoots)];
    while (stack.length) {
      const group=stack[stack.length-1];
      if (group.index===group.end) {stack.pop();continue;}
      const id=group.ids[group.index++],node=this.nodes.get(id)!,row=this.row(node);
      if (row.item.parentElement!==group.group) group.group.insertBefore(row.item,group.more);
      visible.push(id);
      const children=countAtSample(node.children);
      if (children && !this.opened.has(id)) this.opened.set(id,this.shownCount<=pageSize || node.depth===0 && this.shownRoots<=8);
      const boundary=this.isDepthBoundary(node);
      const open=children>0 && !boundary && this.opened.get(id)===true;
      row.toggle.hidden=!children;
      row.toggle.setAttribute('aria-expanded',String(open));
      row.toggle.setAttribute('aria-label',boundary ? `Continue branch from genome ${id}` : `${open?'Collapse':'Expand'} ${children} child ${children===1?'variant':'variants'} of genome ${id}`);
      row.toggle.title=row.toggle.getAttribute('aria-label')!;
      row.group.hidden=!open;
      if (open) stack.push(branch(id,row.group,node.children,children));
    }
    this.visible=visible;
    this.updateReadouts();
  }

  private updateReadouts():void {
    if (!this.target?.isConnected) return;
    const variants=this.shownCount-this.shownRoots;
    const summary=this.target.querySelector<HTMLElement>('[data-lineage-summary]')!;
    setText(summary,this.shownCount ? `${this.shownRoots} ${this.shownRoots===1?'root':'roots'} · ${variants} ${variants===1?'variant':'variants'} · ${this.living.size} with living organisms` : '');
    for (const id of this.visible) {
      const row=this.rows.get(id)!,name=this.nameOf(id),count=this.living.get(id);
      if (row.name.textContent!==name) {
        setText(row.name,name); row.program.setAttribute('aria-label','Inspect '+name);
        row.graph.title='Graph of '+name; row.graph.setAttribute('aria-label',row.graph.title);
      }
      row.item.toggleAttribute('data-lineage-living',!!count);
      setText(row.stats,count ? `${count.population} ${count.population===1?'organism':'organisms'} · generation ${count.maxGeneration}` : 'No living organisms');
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
      const siblings=parent?.children??this.roots;
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
    if (button.hasAttribute('data-lineage-newest')) {this.reveal(this.newestVariant);return;}
    if (button.hasAttribute('data-lineage-roots')) {this.anchor=0;this.rebuild();this.focus(this.visible[0]);return;}
    if (button.hasAttribute('data-lineage-earlier')) {this.reveal(this.nodes.get(this.anchor)!.parentId);return;}
    if (button.hasAttribute('data-lineage-retry')) {this.error='';this.fetch();return;}
    if (button.dataset.lineageMore!==undefined || button.dataset.lineagePrevious!==undefined) {
      const owner=Number(button.dataset.lineageMore??button.dataset.lineagePrevious);
      const page=(this.pages.get(owner)??0)+(button.dataset.lineageMore!==undefined?1:-1);
      this.pages.set(owner,page);this.rebuild();this.focus((owner?this.nodes.get(owner)!.children:this.roots)[page*pageSize]);return;
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
      if (node.children.length && open!==right) {
        this.toggle(id); this.rows.get(id)?.toggle.focus(); return;
      }
      if (!right && id===this.anchor) {this.reveal(node.parentId);return;}
      this.rows.get(right ? node.children[(this.pages.get(id)??0)*pageSize] : node.parentId)?.program.focus(); return;
    }
    const index=this.visible.indexOf(id);
    const next=event.key==='Home' ? 0 : event.key==='End' ? this.visible.length-1 : index+(event.key==='ArrowDown'?1:-1);
    this.rows.get(this.visible[next])?.program.focus();
  };
}
