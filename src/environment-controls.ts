import type { SourceKind } from './engine';
import type { EnvironmentConfig } from './storage';
import type { FieldOverlay } from './renderer';
import { escapeHtml, icon, inputNumber } from './ui';

export type SourceConfig = EnvironmentConfig['sources'][number];
export type BarrierConfig = EnvironmentConfig['barriers'][number];
export type EnvironmentTool = 'select' | `source:${SourceKind}` | 'barrier:fixed' | 'barrier:floating';
export type ObjectSelection = { kind: 'source' | 'barrier'; id: number };
export const SOURCE_NAMES = ['Light', 'Heat / cooling', 'Vortex', 'Mineral emitter', 'Mineral absorber', 'Directional jet', 'Current band', 'Radial flow', 'Oxygen emitter', 'Oxygen absorber', 'Inorganic carbon emitter', 'Inorganic carbon absorber', 'Aerator'] as const;
export const SOURCE_ICONS = ['light', 'heat', 'flow', 'plus', 'download', 'arrow', 'flow', 'flow', 'plus', 'download', 'plus', 'download', 'flow'] as const;
export const SOURCE_HINTS = [
  'Illuminates the surrounding water. Strength 0 switches it off.',
  'Positive strength heats; negative strength cools.',
  'Positive and negative strength rotate the water in opposite directions.',
  'Adds dissolved mineral material to the surrounding water.',
  'Removes available dissolved minerals. Removed material is counted separately.',
  'A local stream along its direction. Negative strength reverses the flow.',
  'A long, narrow current. Combine parallel bands with opposite strengths.',
  'Positive strength pushes water outward; negative strength draws it inward.',
  'Adds dissolved oxygen. This external input is recorded in the oxygen budget.',
  'Removes available dissolved oxygen. It cannot remove more than the water holds.',
  'Adds dissolved inorganic carbon for carbon fixation. This pool includes effective CO₂ and bicarbonate availability.',
  'Removes available dissolved inorganic carbon. Removed carbon is recorded separately.',
  'Connects nearby water to the gas reservoir. Oxygen and inorganic carbon move toward their equilibrium concentrations, in either direction.',
] as const;
export const sourceMinimum = (kind: SourceKind): number => [0, 3, 4].includes(kind) || kind >= 8 ? 0 : -1;
export const defaultSource = (kind: SourceKind, x: number, y: number): SourceConfig => ({
  kind, x, y, strength: 0.7, radius: kind === 6 ? 90 : 200, angle: 0, length: kind === 6 ? 1000 : 600,
});
export const defaultBarrier = (floating: boolean, x: number, y: number): BarrierConfig => ({
  x, y, length: floating ? 160 : 300, width: 30, angle: 0, floating,
});
export function toolOptions(selected: EnvironmentTool = 'select'): string {
  const option = (value: string, label: string): string => `<option value="${value}"${selected === value ? ' selected' : ''}>${label}</option>`;
  const group = (label:string, kinds:readonly number[]):string => `<optgroup label="${label}">${kinds.map(kind => option(`source:${kind}`,SOURCE_NAMES[kind])).join('')}</optgroup>`;
  return option('select','Select object') + group('Light & heat',[0,1]) + group('Currents',[2,5,6,7])
    + group('Minerals',[3,4]) + group('Oxygen',[8,9]) + group('Inorganic carbon',[10,11]) + group('Gas exchange',[12])
    + '<optgroup label="Barriers">' + option('barrier:fixed','Fixed barrier') + option('barrier:floating','Floating barrier') + '</optgroup>';
}
export function toolName(tool: EnvironmentTool): string {
  if (tool.startsWith('source:')) return SOURCE_NAMES[Number(tool.split(':')[1])];
  return tool === 'barrier:fixed' ? 'Fixed barrier' : tool === 'barrier:floating' ? 'Floating barrier' : 'Select object';
}
const degrees = (radians: number): number => radians * 180 / Math.PI;
export function formatEnvironmentNumber(value: number, key: string, min = -Infinity, max = Infinity): string {
  const places = key === 'x' || key === 'y' ? 2 : 4, factor = 10 ** places;
  let rounded = Number(value.toFixed(places));
  if (rounded > max) rounded = Math.floor(max * factor) / factor;
  if (rounded < min) rounded = Math.ceil(min * factor) / factor;
  return String(rounded);
}
const field = (key: string, label: string, value: number, min: number, max: number, step: string, editable: boolean): string => {
  const displayed = formatEnvironmentNumber(value, key, min, max);
  return `<label>${label}<input type="number" data-object-field="${key}" data-presented-value="${displayed}" aria-label="${escapeHtml(label)}" value="${displayed}" min="${min}" max="${max}" step="${step}"${editable ? '' : ' readonly'}></label>`;
};
export function sourceFields(source: SourceConfig, width: number, height: number, editable = true, position = true): string {
  const spatial = position ? field('x', 'X', source.x, 0, width - Number.EPSILON * width, 'any', editable) + field('y', 'Y', source.y, 0, height - Number.EPSILON * height, 'any', editable) : '';
  const chemical = [3,4,8,9,10,11].includes(source.kind);
  const rateHint = chemical ? ' Full strength transfers up to 1.5 units per simulated second within the radius.' : '';
  return `<div class="source-fields">${spatial}${field('strength', 'Strength', source.strength, sourceMinimum(source.kind), 1, '0.05', editable)}${field('radius', source.kind === 5 || source.kind === 6 ? 'Half-width' : 'Radius', source.radius, 20, 1000, '10', editable)}${source.kind === 5 || source.kind === 6 ? field('angle', 'Direction (°)', degrees(source.angle), -360, 360, '5', editable) + field('length', 'Length', source.length, 40, 4800, '20', editable) : ''}</div><p class="environment-hint">${SOURCE_HINTS[source.kind]}${rateHint}</p>`;
}
export function barrierFields(barrier: BarrierConfig, width: number, height: number, editable = true, position = true): string {
  const spatial = position ? field('x', 'X', barrier.x, 0, width - Number.EPSILON * width, 'any', editable) + field('y', 'Y', barrier.y, 0, height - Number.EPSILON * height, 'any', editable) : '';
  return `<div class="source-fields">${spatial}${field('length', 'Length', barrier.length, 20, 2400, '20', editable)}${field('width', 'Thickness', barrier.width, 10, 400, '10', editable)}${field('angle', 'Angle (°)', degrees(barrier.angle), -360, 360, '5', editable)}<label>Motion<select data-object-field="floating" aria-label="Barrier motion"${editable ? '' : ' disabled'}><option value="false"${barrier.floating ? '' : ' selected'}>Fixed</option><option value="true"${barrier.floating ? ' selected' : ''}>Floating</option></select></label></div><p class="environment-hint">${barrier.floating ? 'Drifts with the local current and obstructs organisms, particles and field transport.' : 'Stays in place and obstructs organisms, particles and field transport.'}</p>`;
}
export function readObjectFields<T extends SourceConfig | BarrierConfig>(container: Element, original: T, width: number, height: number): T {
  const result = { ...original };
  const values = result as unknown as Record<string, number | boolean>;
  container.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-object-field]').forEach(input => {
    const key = input.dataset.objectField!;
    // Display rounding is presentation-only until the user edits this field.
    if (input.value === input.dataset.presentedValue) return;
    if (key === 'floating') { values[key] = input.value === 'true'; return; }
    const numberInput = input as HTMLInputElement;
    const min = key === 'x' || key === 'y' ? 0 : Number(numberInput.min);
    const max = key === 'x' ? width - Number.EPSILON * width : key === 'y' ? height - Number.EPSILON * height : Number(numberInput.max);
    const value = inputNumber(numberInput, input.getAttribute('aria-label') || key, min, max);
    values[key] = key === 'angle' ? value * Math.PI / 180 : value;
  });
  return result;
}
export function objectList(sources: ReadonlyArray<SourceConfig & { id?: number }>, barriers: ReadonlyArray<BarrierConfig & { id?: number }>, selected: ObjectSelection | null, useIds = false): string {
  const row = (kind: 'source' | 'barrier', id: number, label: string, symbol: string, detail: string): string =>
    `<button type="button" class="source-item${selected?.kind === kind && selected.id === id ? ' active' : ''}" data-object-kind="${kind}" data-object-id="${id}" aria-pressed="${selected?.kind === kind && selected.id === id}"><span>${icon(symbol)}${label}</span><small>${detail}</small></button>`;
  return sources.map((source, i) => row('source', useIds ? source.id! : i, SOURCE_NAMES[source.kind], SOURCE_ICONS[source.kind], useIds ? `#${source.id}` : `${Math.round(source.x)}, ${Math.round(source.y)}`)).join('')
    + barriers.map((barrier, i) => row('barrier', useIds ? barrier.id! : i, barrier.floating ? 'Floating barrier' : 'Fixed barrier', 'cell', useIds ? `#${barrier.id}` : `${Math.round(barrier.x)}, ${Math.round(barrier.y)}`)).join('')
    || '<p class="source-empty">No environment objects. Choose a tool and click the world to place one.</p>';
}
export const cameraControls = (prefix: string): string => `<div class="camera-controls" role="group" aria-label="View controls"><button type="button" data-camera="${prefix}:out" aria-label="Zoom out">−</button><button type="button" data-camera="${prefix}:fit" aria-label="Fit world">Fit</button><button type="button" data-camera="${prefix}:in" aria-label="Zoom in">+</button></div>`;

export function fieldOptions(selected: FieldOverlay): string {
  return [['none','No overlay'],['light','Light'],['heat','Temperature'],['flow','Water flow'],['nutrients','Minerals'],['oxygen','Oxygen'],['carbon','Inorganic carbon']].map(([value,label]) =>
    `<option value="${value}"${selected===value?' selected':''}>${label}</option>`).join('');
}

/** One world unit per press, or ten with Shift, independent of camera zoom. */
export function arrowNudge(event: KeyboardEvent): {x:number;y:number} | null {
  const step = event.shiftKey ? 10 : 1;
  switch (event.key) {
    case 'ArrowRight': return {x:step,y:0};
    case 'ArrowLeft': return {x:-step,y:0};
    case 'ArrowUp': return {x:0,y:-step};
    case 'ArrowDown': return {x:0,y:step};
    default: return null;
  }
}
