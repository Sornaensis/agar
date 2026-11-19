import type { Cell, RenderCell } from './engine';
import { clock } from './ui';

const amount = (value: number) => Number(value.toFixed(2)).toString();
const precise = (value: number) => Number(value.toPrecision(3)).toString();
const percent = (value: number) => Math.round(value * 100) + '%';

function reserve(key: string, label: string): string {
  return `<div class="organism-reserve" data-reserve="${key}"><div><span>${label}</span><strong data-value="${key}"></strong></div><meter aria-label="${label}" min="0" max="1" value="0"></meter></div>`;
}

const details = [
  ['area','Envelope area'], ['mass','Body mass'], ['growth','Area gained / s'], ['upkeep','Energy upkeep / s'],
  ['digestion','Substrate digested / s'], ['fixation','Carbon fixed / s'], ['respiration','Carbon respired / s'],
  ['carbon','Organic carbon reserve'], ['oxygen','Internal oxygen'], ['inorganic','Internal inorganic carbon'],
  ['exudate','Gut exudate energy'], ['tissue','Gut tissue energy'], ['waste','Gut waste energy'],
] as const;

export function organismOverview(): string {
  return `<div class="organism-overview">
    <div class="organism-reserves">${reserve('energy','Energy')}${reserve('minerals','Minerals')}${reserve('integrity','Integrity')}${reserve('gut','Gut contents')}</div>
    <dl class="organism-facts inspector-metrics">
      ${[['feeding','Feeding'],['age','Age'],['maturity','Growth to division size'],['attachment','Colony']].map(([key,label]) => `<div><dt>${label}</dt><dd data-value="${key}"></dd></div>`).join('')}
      <div class="organism-signal" hidden><dt>Outgoing signal</dt><dd data-value="signal"></dd></div>
    </dl>
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

export function updateOrganismOverview(root: HTMLElement, cell: Cell, visible?: RenderCell): void {
  const values: Record<string, string> = {
    feeding: feeding(cell), age: clock(cell.age), maturity: cell.divisionReady ? 'Ready to divide' : percent(cell.maturity),
    attachment: visible?.attachedNeighbors ? `${visible.attachedNeighbors} attached neighbor${visible.attachedNeighbors === 1 ? '' : 's'}` : 'Free-living',
    signal: visible?.contactSignal === undefined ? '' : precise(visible.contactSignal),
    area: precise(cell.bodyArea), mass: precise(cell.bodyMass), growth: precise(cell.growthRate), upkeep: precise(cell.basalMetabolism+cell.memoryCost),
    digestion: precise(cell.digestionRate), fixation: precise(cell.carbonFixationRate), respiration: precise(cell.respirationRate),
    carbon: precise(cell.organicCarbon), oxygen: precise(cell.internalOxygen), inorganic: precise(cell.internalInorganicCarbon),
    exudate: amount(cell.gutExudateEnergy), tissue: amount(cell.gutTissueEnergy), waste: amount(cell.gutWasteEnergy),
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
    values[key] = key === 'integrity' ? percent(value) : amount(value) + ' / ' + amount(capacity);
    meter.setAttribute('aria-valuetext',values[key]);
  }
  root.querySelector<HTMLElement>('.organism-signal')!.hidden = visible?.contactSignal === undefined;
  for (const field of root.querySelectorAll<HTMLElement>('[data-value]')) {
    const text = values[field.dataset.value!];
    if (field.textContent !== text) field.textContent = text;
  }
}
