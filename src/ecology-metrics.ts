import type { MetricsSnapshot } from './engine';
import type { EcologyHistoryPoint, SimulationMetadata } from './storage';
import { clock, COUNT_SCALE, escapeHtml } from './ui';
import { genomeColor } from './renderer';

const number = new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 });
const count = (value: number | bigint): string => number.format(value);
const label = (metadata: SimulationMetadata, id: number): string => metadata.genomeNames[String(id)] || 'Genome ' + id;

export function ecologyMarkup(): string {
  return `<section class="ecology-panel" id="ecology-panel" aria-label="Lineage metrics">
    <header class="ecology-heading"><h2>Food web</h2><span data-ecology-period></span></header>
    <p class="ecology-empty" data-ecology-empty hidden></p>
    <div class="ecology-census" data-ecology-census><table aria-label="Founder lineages">
      <thead><tr><th scope="col">Founder lineage</th><th scope="col">Alive</th><th scope="col">Births</th><th scope="col">Deaths</th><th scope="col" title="Prey killed by this lineage">Kills</th><th scope="col">Last birth</th></tr></thead>
      <tbody data-ecology-rows></tbody></table></div>
    <section class="ecology-chart" aria-label="Population history">
      <header class="ecology-heading"><h3>Population history</h3><select data-ecology-selection aria-label="Lineage to chart"><option value="0">All organisms</option></select></header>
      <div class="ecology-plot"><span data-ecology-peak></span><svg data-ecology-plot viewBox="0 0 960 112" preserveAspectRatio="none" role="img" aria-label="Population history"></svg><span>0</span></div>
      <div class="ecology-times"><span data-ecology-from></span><span data-ecology-to></span></div>
    </section>
    <div class="ecology-lineage-detail" data-ecology-detail hidden></div>
  </section>`;
}

export function budgetStatusMarkup(): string {
  return `<section class="ecology-balance" id="ecology-balance" aria-label="Ecological balance sheet">
    <header class="ecology-heading"><h2>Resource balance</h2></header>
    <p data-budget-status></p><details data-energy-detail hidden>
      <summary>Where energy went</summary><dl data-energy-loss></dl>
      <p class="muted">The difference is supplied energy minus energy held, withdrawn and dissipated.</p>
    </details>
  </section>`;
}

type TrendPoint = { time: number; population: number };
type Row = NonNullable<MetricsSnapshot['ecology']>['lineages'][number];

/** Use a separate population scale for each selected lineage so small groups stay visible. */
function drawHistory(target: HTMLElement, points: TrendPoint[], color: string, name: string): void {
  const first = points[0], end = points[points.length - 1];
  const peak = points.reduce((max, point) => Math.max(max, point.population), 0);
  const top = Math.max(1, peak);
  const span = end.time - first.time;
  const xy = points.map(point => [
    span ? ((point.time - first.time) / span * 960).toFixed(1) : 960,
    (110 - point.population / top * 106).toFixed(1),
  ].join(','));
  const svg = target.querySelector<SVGElement>('[data-ecology-plot]')!;
  svg.setAttribute('aria-label', `${name}, ${count(first.population)} at ${clock(first.time)}, ${count(end.population)} at ${clock(end.time)}, peak ${count(peak)}`);
  svg.innerHTML = `<path d="M0 4H960M0 57H960M0 110H960" stroke="#374042" stroke-dasharray="3 5" vector-effect="non-scaling-stroke"/>
    ${span ? `<path d="M${xy.join(' L')}" fill="none" stroke="${color}" stroke-width="2" vector-effect="non-scaling-stroke"/>`
      : `<circle cx="957" cy="${xy[0].split(',')[1]}" r="3" fill="${color}"/>`}`;
  target.querySelector('[data-ecology-peak]')!.textContent = count(top);
  target.querySelector('[data-ecology-from]')!.textContent = clock(first.time);
  target.querySelector('[data-ecology-to]')!.textContent = clock(end.time);
}

function recentEvents(row: Row, time: number, since: number, history: EcologyHistoryPoint[]): string {
  const target = time - 300;
  let start: EcologyHistoryPoint | undefined;
  for (const point of history) {
    if (!point.events || point.time < since || point.time >= time) continue;
    if (!start || Math.abs(point.time - target) < Math.abs(start.time - target)) start = point;
  }
  if (!start) return '<p class="ecology-recent">Recent activity will appear after the next census.</p>';
  const previous = start.events!.find(([id]) => id === row.rootGenomeId);
  const births = row.births - (previous?.[1] ?? 0);
  const deaths = row.deaths - (previous?.[2] ?? 0);
  const kills = row.kills - (previous?.[3] ?? 0);
  const duration = time - start.time;
  return `<div class="ecology-recent" data-ecology-recent><h4>Last ${duration < 1 ? '&lt;1 s' : clock(duration)}</h4>
    <span>${count(births)} ${births === 1 ? 'birth' : 'births'}</span>
    <span>${count(deaths)} ${deaths === 1 ? 'death' : 'deaths'}</span>
    <span>${count(kills)} ${kills === 1 ? 'kill' : 'kills'}</span></div>`;
}

function lineageDetail(row: Row, counted: boolean, since: number, time: number, history: EcologyHistoryPoint[]): string {
  const quantity = (value: number): string => {
    const units = counted ? value * COUNT_SCALE : value;
    return units > 0 && units < .01 ? units.toPrecision(2) : count(units);
  };
  const pair = (title: string, value: string): string => `<div><dt>${title}</dt><dd>${value}</dd></div>`;
  const unit = counted ? 'counts' : 'units';
  const reserve = (title: string, value: number | undefined): string => {
    if (value === undefined) return '';
    const stock = counted ? Math.round(value * COUNT_SCALE) : value;
    const amount = counted ? count(stock) : quantity(stock);
    return pair(title, amount + ' ' + (stock === 1 ? unit.slice(0, -1) : unit));
  };
  return `${recentEvents(row, time, since, history)}<dl class="ecology-readouts">
    ${pair(since ? 'At tracking start or introduced' : 'Introduced', count(row.introduced))}
    ${pair('Highest living generation', row.alive ? count(row.maxLivingGeneration) : '—')}
    ${pair('Killed by predators', count(row.predationDeaths))}
    ${pair('Died with depleted ATP', count(row.depletedDeaths))}
    ${pair('Other deaths', count(row.otherDeaths))}
    ${pair('Mean progress to doubled area', row.alive ? count(row.meanMaturity * 100) + '%' : '—')}
    ${pair('Digestion', quantity(row.digestionRate) + ' energy ' + unit + '/s')}
    ${pair('Carbon fixed', quantity(row.carbonFixationRate) + ' C ' + unit + '/s')}
  </dl><h4 class="ecology-reserve-heading">Reserves in living cells</h4><dl class="ecology-readouts ecology-reserves">
    ${pair('At ≥95% ATP capacity', count(row.nearFullAtp) + ' / ' + count(row.alive))}
    ${row.lowAtpWithReserveFuel === undefined ? '' : pair('<span title="Cells with ATP at or below 10% of their ATP capacity and stored organic carbon alone worth at least 25% of that same capacity in gross energy.">ATP ≤10% · reserve fuel ≥25%</span>', count(row.lowAtpWithReserveFuel) + ' / ' + count(row.alive))}
    ${row.zeroFreeMinerals === undefined ? '' : pair('Cells with no free minerals', count(row.zeroFreeMinerals) + ' / ' + count(row.alive))}
    ${reserve('Free minerals', row.freeMinerals)}
    ${reserve('Stored organic carbon', row.reserveOrganicCarbon)}
    ${reserve('Food energy in guts', row.gutEnergy)}
  </dl>`;
}

const lastSnapshot = new WeakMap<HTMLElement, MetricsSnapshot>();
export function renderEcology(target: HTMLElement, world: MetricsSnapshot, metadata: SimulationMetadata, force = false): void {
  if (lastSnapshot.get(target) === world && !force) return;
  lastSnapshot.set(target, world);
  const { ecology } = world;
  const rows = ecology?.lineages ?? [];
  const select = target.querySelector<HTMLSelectElement>('[data-ecology-selection]')!;
  const empty = target.querySelector<HTMLElement>('[data-ecology-empty]')!;
  const period = target.querySelector<HTMLElement>('[data-ecology-period]')!;
  period.textContent = ecology ? `Events since ${clock(ecology.since)}` : '';
  empty.hidden = rows.length > 0;
  empty.textContent = ecology ? 'No lineages have been observed.' : 'Lineage observations are unavailable.';
  target.querySelector<HTMLElement>('[data-ecology-census]')!.hidden = !rows.length;
  const options = ['<option value="0">All organisms</option>', ...rows.map(row =>
    `<option value="${row.rootGenomeId}">${escapeHtml(label(metadata, row.rootGenomeId))}</option>`)].join('');
  if (select.dataset.options !== options) {
    const choice = select.value;
    select.innerHTML = options; select.dataset.options = options;
    select.value = rows.some(row => String(row.rootGenomeId) === choice) ? choice : '0';
  }
  select.hidden = !rows.length;
  const selected = rows.find(row => row.rootGenomeId === Number(select.value));
  const choose = (): void => renderEcology(target, world, metadata, true);
  select.onchange = choose;

  const tbody = target.querySelector<HTMLTableSectionElement>('[data-ecology-rows]')!;
  for (let index = 0; index < rows.length; index++) {
    const row = rows[index];
    const tr = tbody.rows[index] ?? tbody.insertRow();
    if (!tr.cells.length) {
      tr.innerHTML = '<th scope="row"><button type="button" class="ecology-lineage"><i aria-hidden="true"></i><span></span></button></th>'
        + ['Alive','Births','Deaths','Kills','Last birth'].map(name => `<td data-label="${name}"></td>`).join('');
    }
    const name = label(metadata, row.rootGenomeId);
    const button = tr.querySelector('button')!;
    button.style.setProperty('--lineage-color', genomeColor(row.rootGenomeId));
    button.querySelector('span')!.textContent = name;
    button.setAttribute('aria-label', 'Show population history and physiology for ' + name);
    button.setAttribute('aria-pressed', String(row === selected));
    button.onclick = () => { select.value = String(row.rootGenomeId); choose(); };
    const age = row.lastBirth === null ? 'None recorded' : clock(world.time - row.lastBirth) + ' ago';
    const values = [count(row.alive), count(row.births), count(row.deaths), count(row.kills), age];
    values.forEach((value, i) => { tr.cells[i + 1].textContent = value; });
    tr.cells[5].title = row.lastBirth === null ? '' : 'Last birth at ' + clock(row.lastBirth);
  }
  while (tbody.rows.length > rows.length) tbody.deleteRow(-1);

  const fullHistory = metadata.ecologyHistory;
  let points: TrendPoint[];
  if (selected) {
    const id = selected.rootGenomeId;
    points = (fullHistory ?? []).filter(p => p.time < world.time && p.time >= ecology!.since).map(p => ({
      time: p.time, population: p.populations.find(pair => pair[0] === id)?.[1] ?? 0,
    }));
    points.push({ time: world.time, population: selected.alive });
  } else {
    // Retain the earlier total-population history of imported runs.
    points = metadata.history.filter(p => p.time < world.time);
    if (fullHistory?.length) {
      const start = points[0]?.time ?? world.time;
      const older = fullHistory.filter(p => p.time < start)
        .map(p => ({ time: p.time, population: p.populations.reduce((total, pair) => total + pair[1], 0) }));
      points = [...older, ...points];
    }
    points.push({ time: world.time, population: world.population });
  }
  drawHistory(target, points, selected ? genomeColor(selected.rootGenomeId) : '#8be2c5',
    selected ? label(metadata, selected.rootGenomeId) : 'All organisms');
  const detail = target.querySelector<HTMLElement>('[data-ecology-detail]')!;
  detail.hidden = !selected;
  if (selected) detail.innerHTML = lineageDetail(selected, Boolean(world.countAudit), ecology!.since, world.time, fullHistory ?? []);
}

const energyLabels = [
  ['maintenance', 'Maintenance & motion'], ['respiration', 'Respiration'],
  ['decomposition', 'Decomposition'], ['fabrication', 'Fabrication'],
  ['feedingAndDivision', 'Feeding, uptake & division'], ['overflow', 'Storage overflow'],
  ['opening', 'Before tracking began'],
] as const;
export function recordedEnergyLoss(loss: NonNullable<MetricsSnapshot['energyLoss']>): bigint {
  return energyLabels.reduce((sum, [key]) => sum + BigInt(loss[key]), 0n);
}
export function renderBudgetStatus(target: HTMLElement, world: MetricsSnapshot, mineralRemoved: number): void {
  const { countAudit: audit, energyLoss } = world;
  const messages: string[] = [];
  const details = target.querySelector<HTMLDetailsElement>('[data-energy-detail]')!;
  details.hidden = !audit || !energyLoss;
  if (audit) {
    const difference = (pool: keyof typeof audit): bigint => {
      const { added, held, removed } = audit[pool];
      return BigInt(added) - BigInt(held) - BigInt(removed);
    };
    for (const [key, name] of [['mineral','Minerals'], ['carbon','Carbon'], ['oxygen','Oxygen']] as const) {
      const gap = difference(key);
      if (gap !== 0n) messages.push(`${name} difference ${count(gap)}`);
    }
    if (energyLoss) {
      const gap = difference('energy') - recordedEnergyLoss(energyLoss);
      if (gap !== 0n) messages.push('Energy difference ' + count(gap));
      const entries = energyLabels.filter(([key]) => key !== 'opening' || BigInt(energyLoss.opening) !== 0n);
      target.querySelector('[data-energy-loss]')!.innerHTML = [...entries.map(([key, name]) =>
        [name, count(BigInt(energyLoss[key]))]), ['Difference', count(gap)]].map(([name,value]) =>
        `<div><dt>${name}</dt><dd>${value}</dd></div>`).join('');
    } else if (difference('energy') < 0n) messages.push('Energy exceeds supply');
  } else {
    const checks = [
      ['Minerals', world.nutrientInputTotal - world.nutrientTotal - mineralRemoved - world.mineralWithdrawnTotal, world.nutrientInputTotal],
      ['Carbon', world.carbonInputTotal - world.carbonTotal - world.carbonRemovedTotal, world.carbonInputTotal],
      ['Oxygen', world.oxygenInputTotal + world.carbonFixedTotal - world.oxygenDissolvedTotal - world.oxygenCellTotal - world.oxygenRemovedTotal - world.carbonRespiredTotal - world.decayRespiredTotal, world.oxygenInputTotal + world.carbonFixedTotal],
    ] as const;
    for (const [name, error, supplied] of checks) if (Math.abs(error) > 1e-6 + Math.abs(supplied) * 1e-9)
      messages.push(name + ' difference ' + error.toPrecision(3));
    const input = world.energyInputTotal + world.photosynthesisTotal;
    if (world.energyTotal + world.energyWithdrawnTotal - input > 1e-6 + Math.abs(input) * 1e-9) messages.push('Energy exceeds supply');
  }
  const status = target.querySelector<HTMLElement>('[data-budget-status]')!;
  status.classList.toggle('budget-mismatch', messages.length > 0);
  status.textContent = messages.length ? messages.join(' · ') : audit && energyLoss
    ? 'Minerals, carbon, oxygen and energy balance.' : 'Minerals, carbon and oxygen balance. Stored and withdrawn energy are within supply.';
}
