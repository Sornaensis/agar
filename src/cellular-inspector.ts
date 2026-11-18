import type { BodyInspection, CellularAssembly, CellularRecipe, ProductRole } from './engine';
import { ORGANELLE_CATALOG, MECHANISM_CATALOG } from './organelles';
import { escapeHtml as esc } from './ui';

const ROLE_COLORS: Record<ProductRole, string> = {
  scaffold:'#bdc2b0',anchor:'#b4cdbb',motor:'#88cbd9',filament:'#99b5e0',pigment:'#9bd487',
  enzyme:'#dfbf7f',membrane:'#b9a1d9',transporter:'#7cd5b3',receptor:'#d5a0c3',
  regulator:'#ddac8a',replicase:'#b7aadf',constrictor:'#dfa4a0',
};
const mass = (value:number):string => value.toFixed(4);
const percent = (value:number):string => `${Math.round(Math.max(0,Math.min(1,value))*100)}%`;
const degrees = (value:number):string => `${Math.round(value*180/Math.PI)}°`;
const progress = (value:number):number => Math.max(0,Math.min(1,value));
const mechanism = (kind:string) => MECHANISM_CATALOG.find(item=>item.kind===kind);
const kindLabel = (kind:string):string => mechanism(kind)?.name ?? ORGANELLE_CATALOG.find(item=>item.kind===kind)?.name
  ?? ({membrane:'Membrane growth',replicator:'Genome replicator',constrictor:'Division ring'} as Record<string,string>)[kind] ?? kind;
const kindColor = (kind:string):string => mechanism(kind)?.color ?? ORGANELLE_CATALOG.find(item=>item.kind===kind)?.color
  ?? ({membrane:'#b9a1d9',replicator:'#b7aadf',constrictor:'#dfa4a0'} as Record<string,string>)[kind] ?? '#a8c9b7';

function components(recipe:CellularRecipe, assembly?:CellularAssembly):string {
  return `<div class="cellular-components">${recipe.components.map((part,index)=>{
    const installed = assembly?.components[index]?.mass ?? 0;
    return `<div class="cellular-component"><div><span><i style="background:${ROLE_COLORS[part.role]}"></i>${esc(part.name)}</span><strong>${mass(installed)} / ${mass(part.requiredMass)}</strong></div><div class="cellular-track"><span style="width:${progress(installed/Math.max(part.requiredMass,1e-12))*100}%;background:${ROLE_COLORS[part.role]}"></span></div><small>${esc(part.role)}</small></div>`;
  }).join('')}</div>`;
}

function recipeControls(recipe:CellularRecipe):string {
  return `<div class="cellular-machine-facts"><span>New starts <strong>${recipe.startAllowed===false?'Paused':'Allowed'}</strong></span><span>Recycle request <strong>${percent(recipe.recycleRequested ?? 0)}</strong></span><span>Recovered <strong>${mass(recipe.recycleRate ?? 0)} / s</strong></span></div>`;
}

function assemblyCard(assembly:CellularAssembly, recipe:CellularRecipe|undefined, expanded:ReadonlySet<string|undefined>, first:boolean):string {
  const key = `cellular-assembly-${assembly.id}`;
  const state = !assembly.complete ? 'Assembling' : assembly.functionalMass<=0 ? 'Damaged' : mechanism(assembly.kind)?.passive ? 'Present' : assembly.activity>0 ? 'Active' : 'Idle';
  return `<details class="cellular-machine" data-kind="${key}"${expanded.has(key)||first?' open':''}>
    <summary><i style="background:${kindColor(assembly.kind)}"></i><span>${esc(assembly.name)}<small>${esc(kindLabel(assembly.kind))} · #${assembly.id}</small></span><strong class="${assembly.complete?'':'building'}">${state}<small>${percent(assembly.mass/Math.max(assembly.requiredMass,1e-12))} built</small></strong></summary>
    <div class="cellular-machine-content">
      <p class="cellular-section-note"><code>${esc(assembly.kind)}</code> · ${esc(mechanism(assembly.kind)?.description ?? 'Legacy family machinery retains its original combined capabilities.')}</p>
      ${assembly.bottleneck?`<p class="cellular-bottleneck">Waiting for <strong>${esc(assembly.bottleneck)}</strong></p>`:''}
      ${components(assembly,assembly)}
      <div class="cellular-machine-facts"><span>Activity <strong>${percent(assembly.activity)}</strong></span><span>Integrity <strong>${percent(assembly.integrity)}</strong></span><span>Attachment <strong>${degrees(assembly.at)}</strong></span><span>Direction <strong>${degrees(assembly.direction)}</strong></span></div>
      ${recipe ? recipeControls(recipe) : `<p class="cellular-section-note">Inherited recipe. Its installed ingredients retain their original identity.</p>`}
    </div>
  </details>`;
}

/** Render only measured inventories. Unbuilt blueprints remain visibly distinct from machines. */
export function renderCellularBody(body:BodyInspection, expanded:ReadonlySet<string|undefined> = new Set(), firstPaint = false):string {
  const cellular = body.cellular;
  if (!cellular) return '';
  const finished = cellular.assemblies.filter(item=>item.complete).length;
  const stock = cellular.products.reduce((sum,product)=>sum+product.mass,0);
  const incomplete = cellular.assemblies.length-finished;
  const notStarted = cellular.recipes.filter(recipe=>!cellular.assemblies.some(assembly=>assembly.name===recipe.name && assembly.kind===recipe.kind));
  const firstAssembly = cellular.assemblies.find(item=>!item.complete) ?? cellular.assemblies[0];
  const meter = (name:string,value:number,caption:string):string=>`<div class="cellular-life-step"><div><span>${name}</span><strong>${percent(value)}</strong></div><progress max="1" value="${progress(value)}" aria-label="${name}"></progress><small>${caption}</small></div>`;
  return `<div class="cellular-body">
    <div class="cellular-body-heading"><div><span class="cellular-eyebrow">PRODUCTS → MACHINERY</span><h3>Living composition</h3></div><span class="cellular-mode-label">Cellular</span></div>
    <div class="cellular-totals"><div><strong>${mass(stock)}</strong><span>free product mass</span></div><div><strong>${finished}<small> / ${cellular.assemblies.length}</small></strong><span>complete machines</span></div><div><strong>${incomplete}</strong><span>in progress</span></div></div>
    <div class="cellular-work"><span>Synthesis <strong>${mass(body.synthesisRate)}</strong></span><span>Assembly <strong>${mass(body.constructionRate)}</strong></span><span>Recycling <strong>${mass(body.recyclingRate)}</strong></span><span>Translation work <strong>${mass(body.assemblyCapacity)}</strong></span><small>material / s · shared work capacity</small></div>
    <div class="cellular-life">${meter('Replication',cellular.replication.progress,`${mass(cellular.replication.mass)} / ${mass(cellular.replication.requiredMass)} material`)}${meter('Constriction',cellular.constriction.progress,'Growth and replication must finish')}</div>
    <details class="cellular-section" data-kind="cellular-products"${expanded.has('cellular-products')?' open':''}><summary><span>Products & expression</span><strong>${cellular.products.length}</strong></summary><p class="cellular-section-note">Pools are available stock. Concentration is mass per envelope area. Expression shows the last tick’s requested effort and achieved material per second, summed across genes for each product.</p>
      <table class="cellular-products"><thead><tr><th>Product</th><th>Pool / concentration</th><th>Expression</th></tr></thead><tbody>${cellular.products.map(product=>`<tr><td><span class="cellular-product-name"><i style="background:${ROLE_COLORS[product.role]}"></i>${esc(product.name)}</span><small>${esc(product.role)}${product.halfLife>0?` · t½ ${product.halfLife}s`:''}${product.current?'':' · inherited'}</small></td><td><strong>${mass(product.mass)}</strong><small>${mass(product.concentration)} / area</small></td><td><strong>${Math.round(product.expressionRequested*100)}%</strong><small>${mass(product.expressionRate)} / s</small></td></tr>`).join('')}</tbody></table>
    </details>
    <div class="cellular-machine-heading"><h4>Assemblies</h4><span>composition · activity · placement</span></div>
    <div class="cellular-machines">${cellular.assemblies.map(assembly=>assemblyCard(assembly,cellular.recipes.find(recipe=>recipe.name===assembly.name),expanded,firstPaint&&assembly===firstAssembly)).join('')}
      ${notStarted.map(recipe=>`<details class="cellular-machine cellular-blueprint" data-kind="cellular-recipe-${esc(recipe.name)}"${expanded.has(`cellular-recipe-${recipe.name}`)?' open':''}><summary><i style="background:${kindColor(recipe.kind)}"></i><span>${esc(recipe.name)}<small>${esc(kindLabel(recipe.kind))}</small></span><strong>${recipe.startAllowed===false?'Paused':'Blueprint'}<small>${mass(recipe.requiredMass)} material</small></strong></summary><div class="cellular-machine-content"><p class="cellular-section-note"><code>${esc(recipe.kind)}</code> · ${esc(mechanism(recipe.kind)?.description ?? 'Legacy family machinery retains its original combined capabilities.')}</p><p class="cellular-section-note">No components installed yet.</p>${components(recipe)}${recipeControls(recipe)}</div></details>`).join('')}
    </div>
    <p class="cellular-explanation">Genes make products; start conditions permit new assemblies. Existing partial machines continue paid installation. Activity runs complete machinery, while recycling spends work to recover its ingredients for later ticks.</p>
  </div>`;
}
