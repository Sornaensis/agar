import type { Cell, FoodPotential, RenderCell } from './engine';
import { COUNT_SCALE, clock } from './ui';

const amount = (value: number) => Number(value.toFixed(2)).toString();
const precise = (value: number) => Number(value.toPrecision(3)).toString();
const percent = (value: number) => Math.round(value * 100) + '%';

function reserve(key: string, label: string): string {
  return `<div class="organism-reserve" data-reserve="${key}"><div><span>${label}</span><strong data-value="${key}"></strong></div><meter aria-label="${label}" min="0" max="1" value="0"></meter></div>`;
}

const details = [
  ['area','Envelope area'], ['mass','Body mass'], ['growth','Area gained / s'], ['upkeep','Energy upkeep / s'],
  ['digestion','Substrate digested / s'], ['fixation','Carbon fixed / s'], ['respiration','Carbon respired / s'],
  ['carbon','Organic carbon reserve'], ['ligand','Inhibitory ligand carbon'], ['oxygen','Internal oxygen'], ['inorganic','Internal inorganic carbon'],
  ['exudate','Soluble gut energy'], ['tissue','Structural gut energy'], ['waste','Bound gut energy'],
] as const;

export function organismOverview(): string {
  return `<div class="organism-overview">
    <div class="organism-reserves">${reserve('energy','Available ATP')}${reserve('minerals','Minerals')}${reserve('integrity','Integrity')}${reserve('gut','Gut contents')}</div>
    <dl class="organism-facts inspector-metrics">
      ${[['feeding','Feeding'],['age','Age'],['maturity','Growth to division size'],['attachment','Connections']].map(([key,label]) => `<div><dt>${label}</dt><dd data-value="${key}"></dd></div>`).join('')}
      <div class="organism-signal" hidden><dt>Outgoing signal</dt><dd data-value="signal"></dd></div>
    </dl>
    <details class="organism-physiology" data-food-potential><summary>Food energy if consumed</summary>
      <dl class="organism-detail-facts">${[['body','Body tissue'],['reserve','Stored organic fuel'],['gut','Gut contents'],['atp','Remaining ATP'],['total','Total food energy']].map(([key,label]) =>
        `<div><dt>${label}</dt><dd data-food-pool="${key}">—</dd></div>`).join('')}</dl>
      <p class="muted">Body tissue includes the energy retained during construction. A consumer must digest and respire its meal to gain ATP.</p>
    </details>
    <details class="organism-physiology"><summary>Physiology</summary><dl class="organism-detail-facts">${details.map(([key,label]) => `<div><dt>${label}</dt><dd data-value="${key}"></dd></div>`).join('')}</dl></details>
  </div>`;
}

function feeding(cell: Cell): string {
  const capable = (kind: string) => cell.organelles.some(organ => organ.kind === kind && organ.capacity > 0);
  const solar = capable('photosynthetic'), digestive = capable('digestive');
  if (solar && digestive) return 'Light & organics';
  if (solar) return 'Light';
  if (digestive) return 'Organic matter';
  return 'No feeding organs';
}

function connections(cell?: RenderCell): string {
  if (!cell) return 'Unavailable';
  const parts: string[] = [];
  if (cell.colonySize) parts.push(`${cell.colonySize}-cell colony`);
  const feeding = cell.feedingNeighborIds?.length ?? 0;
  if (feeding) parts.push(`${feeding} feeding link${feeding === 1 ? '' : 's'}`);
  const contacts = cell.contactNeighborIds?.length ?? 0;
  if (contacts) parts.push(`${contacts} contact link${contacts === 1 ? '' : 's'}`);
  return parts.length ? parts.join(' · ') : 'Unattached';
}

export function updateOrganismOverview(root: HTMLElement, cell: Cell, visible?: RenderCell, food?: FoodPotential, counted=false): void {
  const quantity = (n:number) => counted ? Math.round(n*COUNT_SCALE).toLocaleString() : amount(n);
  const flux = (n:number) => precise(n*(counted ? COUNT_SCALE : 1));
  const values: Record<string, string> = {
    feeding: feeding(cell), age: clock(cell.age), maturity: cell.divisionReady ? 'Ready to divide' : percent(cell.maturity),
    attachment: connections(visible),
    signal: visible?.contactSignal === undefined ? '' : precise(visible.contactSignal),
    area: precise(cell.bodyArea), mass: precise(cell.bodyMass), growth: precise(cell.growthRate), upkeep: flux(cell.basalMetabolism+cell.memoryCost),
    digestion: flux(cell.digestionRate), fixation: flux(cell.carbonFixationRate), respiration: flux(cell.respirationRate),
    carbon: quantity(cell.organicCarbon), ligand: quantity(cell.ligandCarbon), oxygen: quantity(cell.internalOxygen), inorganic: quantity(cell.internalInorganicCarbon),
    exudate: quantity(cell.gutExudateEnergy), tissue: quantity(cell.gutTissueEnergy), waste: quantity(cell.gutWasteEnergy),
  };
  const stores: Record<string, [number,number]> = {
    energy:[cell.energy,cell.energyCapacity], minerals:[cell.nutrientStored,cell.nutrientCapacity],
    integrity:[cell.integrity,1], gut:[cell.gutEnergy,cell.gutCapacity],
  };
  for (const row of root.querySelectorAll<HTMLElement>('[data-reserve]')) {
    const key = row.dataset.reserve!, [value,capacity] = stores[key];
    row.hidden = key === 'gut' && capacity <= 0 && value <= 0;
    const meter = row.querySelector('meter')!;
    meter.max = Math.max(capacity, 1e-9);
    meter.value = value;
    const limit = counted ? Math.floor(capacity*COUNT_SCALE + 1e-8).toLocaleString() : amount(capacity);
    values[key] = key === 'integrity' ? percent(value) : quantity(value) + ' / ' + limit;
    meter.setAttribute('aria-valuetext',values[key]);
  }
  if (food) for (const field of root.querySelectorAll<HTMLElement>('[data-food-pool]')) {
    const text = quantity(food[field.dataset.foodPool as keyof FoodPotential]);
    if (field.textContent !== text) field.textContent = text;
  }
  root.querySelector<HTMLElement>('.organism-signal')!.hidden = visible?.contactSignal === undefined;
  for (const field of root.querySelectorAll<HTMLElement>('[data-value]')) {
    const text = values[field.dataset.value!];
    if (field.textContent !== text) field.textContent = text;
  }
}
