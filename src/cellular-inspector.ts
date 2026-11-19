import type { BodyInspection, CellularAssembly, CellularRecipe, GenomeSignalEvaluation, ProductRole } from './engine';
import { ORGANELLE_CATALOG, MECHANISM_CATALOG } from './organelles';
import { escapeHtml as esc } from './ui';

const ROLE_COLORS: Record<ProductRole, string> = {
  scaffold:'#bdc2b0',anchor:'#b4cdbb',motor:'#88cbd9',filament:'#99b5e0',pigment:'#9bd487',
  enzyme:'#dfbf7f',membrane:'#b9a1d9',transporter:'#7cd5b3',receptor:'#d5a0c3',
  regulator:'#ddac8a',replicase:'#b7aadf',constrictor:'#dfa4a0',adhesin:'#b893d4',
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

function assemblyCard(assembly:CellularAssembly, recipe:CellularRecipe|undefined, expanded:ReadonlySet<string|undefined>):string {
  const key = `cellular-assembly-${assembly.id}`;
  const state = !assembly.complete ? 'Assembling' : assembly.functionalMass<=0 ? 'Damaged' : mechanism(assembly.kind)?.passive ? 'Present' : assembly.activity>0 ? 'Active' : 'Idle';
  return `<details class="cellular-machine" data-kind="${key}"${expanded.has(key)?' open':''}>
    <summary><i style="background:${kindColor(assembly.kind)}"></i><span>${esc(assembly.name)}<small>${esc(kindLabel(assembly.kind))}</small></span><strong class="${assembly.complete?'':'building'}">${state}${assembly.complete ? '' : `<small>${percent(assembly.mass/Math.max(assembly.requiredMass,1e-12))} built</small>`}</strong></summary>
    <div class="cellular-machine-content">
      <p class="cellular-section-note"><code>${esc(assembly.kind)}</code> · ${esc(mechanism(assembly.kind)?.description ?? 'Legacy family machinery retains its original combined capabilities.')}</p>
      ${assembly.bottleneck?`<p class="cellular-bottleneck">Waiting for <strong>${esc(assembly.bottleneck)}</strong></p>`:''}
      ${components(assembly,assembly)}
      <div class="cellular-machine-facts"><span>Activity <strong>${percent(assembly.activity)}</strong></span><span>Integrity <strong>${percent(assembly.integrity)}</strong></span><span>Attachment <strong>${degrees(assembly.at)}</strong></span><span>Direction <strong>${degrees(assembly.direction)}</strong></span></div>
      ${recipe ? recipeControls(recipe) : `<p class="cellular-section-note">Inherited recipe. Its installed ingredients retain their original identity.</p>`}
    </div>
  </details>`;
}

function colonyExchange(body:BodyInspection):string {
  const state = body.colony;
  if (!state || !(state.neighbors || state.energyNet || state.mineralNet || state.carbonNet)) return '';
  const signed = (value:number) => value === 0 ? '0' : (value > 0 ? '+' : '−') + Number(Math.abs(value).toPrecision(3));
  return `<section class="colony-exchange"><h4>Colony exchange</h4>
    <p>${state.neighbors === 0 ? 'No current bonds' : 'Directly bonded to ' + state.neighborIds.map(id=>'#'+id).join(', ')}</p>
    ${state.neighbors || state.energyNet || state.mineralNet || state.carbonNet ? `<dl>${[['Energy',state.energyNet],['Minerals',state.mineralNet],['Organic carbon',state.carbonNet]].map(([label,value])=>`<div><dt>${label}</dt><dd>${signed(Number(value))}</dd></div>`).join('')}</dl><small>Last tick · + received · − donated</small>` : ''}
  </section>`;
}

function receptionPanel(recipes:CellularRecipe[], expanded:ReadonlySet<string|undefined>, readings?:GenomeSignalEvaluation[]):string {
  const junctions = recipes.filter(item=>item.kind==='junction(signal)');
  if (!junctions.length) return '';
  const names = new Map(readings?.map(signal=>[signal.name,signal.value]));
  const content = readings ? junctions.map(recipe=>{
    const prefix = 'sensed.' + recipe.name + '.', count = names.get(prefix+'count'), value = names.get(prefix+'value');
    const valid = names.get(prefix+'valid'), available = names.get(prefix+'available');
    const parts = [
      typeof count === 'number' ? `${count} signaling neighbor${count === 1 ? '' : 's'}` : '',
      valid === false || count === 0 ? 'No incoming signal' : typeof value === 'number' ? `${valid === true || (typeof count==='number' && count>0) ? 'Mean incoming signal' : 'Signal readout'} ${Number(value.toPrecision(3))}` : valid === true ? 'Receiving' : '',
    ].filter(Boolean);
    const hardware = available === true ? 'Junction available' : available === false ? 'Junction unavailable' : 'No readings used by this program';
    return `<div class="junction-reading"><strong>${esc(recipe.name)}</strong><span>${parts.join(' · ') || hardware}</span></div>`;
  }).join('') : '<p class="cellular-section-note">Reading junctions…</p>';
  return `<details class="cellular-section" data-kind="colony-reception"${expanded.has('colony-reception')?' open':''}><summary><span>Incoming colony signals</span></summary>${content}</details>`;
}

/** Render only measured inventories. Unbuilt blueprints remain visibly distinct from machines. */
export function renderCellularBody(body:BodyInspection, expanded:ReadonlySet<string|undefined> = new Set(), readings?:GenomeSignalEvaluation[]):string {
  const cellular = body.cellular;
  if (!cellular) return '';
  const finished = cellular.assemblies.filter(item=>item.complete).length;
  const stock = cellular.products.reduce((sum,product)=>sum+product.mass,0);
  const incomplete = cellular.assemblies.length-finished;
  const notStarted = cellular.recipes.filter(recipe=>!cellular.assemblies.some(assembly=>assembly.name===recipe.name && assembly.kind===recipe.kind));
  const meter = (name:string,value:number):string => `<div class="cellular-life-step"><div><span>${name}</span><strong>${percent(value)}</strong></div><progress max="1" value="${progress(value)}" aria-label="${name}"></progress></div>`;
  return `<div class="cellular-body">
    ${colonyExchange(body)}
    ${receptionPanel(cellular.recipes.filter(recipe=>cellular.assemblies.some(a=>a.name===recipe.name && a.kind===recipe.kind && a.complete && a.functionalMass>0)),expanded,readings)}
    <div class="body-inventory-title"><h3>Cell machinery</h3><span>${finished} complete · ${incomplete} developing</span></div>
    <div class="cellular-machines">${cellular.assemblies.map(assembly=>assemblyCard(assembly,cellular.recipes.find(recipe=>recipe.name===assembly.name),expanded)).join('') || '<p class="body-empty">No cellular machinery built yet.</p>'}</div>
    <details class="cellular-section" data-kind="cellular-division"${expanded.has('cellular-division')?' open':''}><summary><span>Cell division</span></summary><div class="cellular-life">${meter('Replication',cellular.replication.progress)}${meter('Constriction',cellular.constriction.progress)}</div></details>
    <details class="cellular-section" data-kind="cellular-products"${expanded.has('cellular-products')?' open':''}><summary><span>Products & expression</span><strong>${cellular.products.length}</strong></summary>
    <p class="cellular-section-note">${mass(stock)} free product mass · Synthesis and assembly share a work budget.</p>
    <div class="cellular-work">${[['Synthesis',body.synthesisRate],['Assembly',body.constructionRate],['Recycling',body.recyclingRate],['Translation work',body.assemblyCapacity]].map(([label,value])=>`<span>${label}<strong>${mass(Number(value))}</strong></span>`).join('')}<small>Material per second</small></div>
      <table class="cellular-products"><thead><tr><th>Product</th><th>Pool / concentration</th><th>Expression</th></tr></thead><tbody>${cellular.products.map(product=>`<tr><td><span class="cellular-product-name"><i style="background:${ROLE_COLORS[product.role]}"></i>${esc(product.name)}</span><small>${esc(product.role)}${product.precursor ? ` from ${esc(product.precursor)}` : ''}${product.halfLife>0?` · t½ ${product.halfLife}s`:''}${product.current?'':' · inherited'}</small></td><td><strong>${mass(product.mass)}</strong><small>${mass(product.concentration)} / area</small></td><td><strong>${Math.round(product.expressionRequested*100)}%</strong><small>${mass(product.expressionRate)} / s${product.precursor ? ' finished' : ''}</small></td></tr>`).join('')}</tbody></table>
    </details>
    ${notStarted.length ? `<details class="cellular-section" data-kind="cellular-blueprints"${expanded.has('cellular-blueprints')?' open':''}><summary><span>Unbuilt recipes</span><strong>${notStarted.length}</strong></summary><div class="cellular-machines">
      ${notStarted.map(recipe=>`<details class="cellular-machine cellular-blueprint" data-kind="cellular-recipe-${esc(recipe.name)}"${expanded.has(`cellular-recipe-${recipe.name}`)?' open':''}><summary><i style="background:${kindColor(recipe.kind)}"></i><span>${esc(recipe.name)}<small>${esc(kindLabel(recipe.kind))}</small></span><strong>${recipe.startAllowed===false?'Paused':'Blueprint'}<small>${mass(recipe.requiredMass)} material</small></strong></summary><div class="cellular-machine-content"><p class="cellular-section-note"><code>${esc(recipe.kind)}</code> · ${esc(mechanism(recipe.kind)?.description ?? 'Legacy family machinery retains its original combined capabilities.')}</p><p class="cellular-section-note">No components installed yet.</p>${components(recipe)}${recipeControls(recipe)}</div></details>`).join('')}
    </div></details>` : ''}
  </div>`;
}
