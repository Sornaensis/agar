import type { GasConfig, GasExchangeConfig } from './engine';
import { COUNT_SCALE, inputNumber } from './ui';

/** Shared environment-builder and live boundary controls. Starting pools are builder-only. */
export function gasFields(gases: GasConfig, editable = true, starting = false, counted = false): string {
  const field = (key: string, label: string, value: number, max: number, step: string): string => {
    const scale = counted && key !== 'exchangeRate' ? COUNT_SCALE : 1;
    const displayed = String(scale > 1 ? Math.round(value * scale) : Number(value.toFixed(4)));
    if (scale > 1) label += ' (counts/patch)';
    return `<label>${label}<input type="number" data-gas-field="${key}" data-count-scale="${scale}" data-presented-value="${displayed}" data-source-value="${value}" aria-label="${label}" value="${displayed}" min="0" max="${max * scale}" step="${scale > 1 ? 1 : step}"${editable ? '' : ' readonly'}></label>`;
  };
  return `<div class="source-fields gas-fields"><label class="gas-boundary-field">Surface boundary<select data-gas-field="mode" aria-label="Gas boundary"${editable ? '' : ' disabled'}><option value="sealed"${gases.mode === 'sealed' ? ' selected' : ''}>Sealed</option><option value="open"${gases.mode === 'open' ? ' selected' : ''}>Open to atmosphere</option></select></label>
    ${starting ? field('initialOxygen', 'Starting oxygen', gases.initialOxygen, 100, '0.1') + field('initialCarbon', 'Starting inorganic carbon', gases.initialCarbon, 100, '0.1') : ''}
    ${field('exchangeRate', 'Surface exchange / s', gases.exchangeRate, 1, '0.01')}
    ${field('oxygenEquilibrium', 'Oxygen equilibrium', gases.oxygenEquilibrium, 100, '0.1')}
    ${field('carbonEquilibrium', 'Carbon equilibrium', gases.carbonEquilibrium, 100, '0.1')}</div>
    <p class="environment-hint">Open water exchanges gases with the atmosphere. Sealed water exchanges only through placed objects. Aerators use the equilibrium values above. ${counted ? 'Each patch holds whole counts.' : 'Amounts use abstract units per water patch.'}</p>`;
}

function gasNumber(input: HTMLInputElement, label: string, max: number): number {
  const scale = Number(input.dataset?.countScale || 1);
  const entered = inputNumber(input, label, 0, max * scale);
  if (scale > 1 && !Number.isInteger(entered)) throw new Error(`${label} must be a whole number.`);
  // Viewing a rounded engine projection must not round an imported recipe.
  const original = input.dataset?.sourceValue;
  if (original !== undefined && input.value === input.dataset.presentedValue) {
    const value = Number(original);
    if (Number.isFinite(value) && value >= 0 && value <= max) return value;
  }
  return entered / scale;
}

export function readGasExchange(container: Element): GasExchangeConfig {
  const input = (key: string): HTMLInputElement => {
    const element = container.querySelector<HTMLInputElement>(`[data-gas-field="${key}"]`);
    if (!element) throw new Error('The gas controls are unavailable.');
    return element;
  };
  const mode = input('mode').value;
  if (mode !== 'sealed' && mode !== 'open') throw new Error('Choose a sealed or open gas boundary.');
  return {
    mode,
    exchangeRate: gasNumber(input('exchangeRate'), 'Surface exchange rate', 1),
    oxygenEquilibrium: gasNumber(input('oxygenEquilibrium'), 'Oxygen equilibrium', 100),
    carbonEquilibrium: gasNumber(input('carbonEquilibrium'), 'Inorganic carbon equilibrium', 100),
  };
}

export function readGasConfig(container: Element): GasConfig {
  return {
    ...readGasExchange(container),
    initialOxygen: gasNumber(container.querySelector('[data-gas-field="initialOxygen"]')!, 'Starting oxygen', 100),
    initialCarbon: gasNumber(container.querySelector('[data-gas-field="initialCarbon"]')!, 'Starting inorganic carbon', 100),
  };
}
