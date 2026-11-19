import type { Cell } from './engine';
import type { SimulationSession } from './session';
import { COUNT_SCALE, createDialog, setDialogError } from './ui';

export function cellEditActions(bonded:boolean): string {
  return `<div class="cell-edit-actions" aria-label="Organism tools"><button type="button" class="button small subtle" data-action="move-cell" title="Drag the selected cell or choose its destination">Move cell</button><button type="button" class="button small subtle" data-action="move-colony"${bonded?'':' disabled'} title="Move every cell connected by a bond. You can also Alt-drag a linked cell.">Move linked group</button><button type="button" class="button small subtle" data-action="edit-reserves">Edit reserves</button></div>`;
}

export async function editReserves(session: SimulationSession, id: number, host: Document, isCurrent: () => boolean): Promise<boolean> {
  const cell = await session.inspect(engine => engine.cellSnapshot(id));
  if (!isCurrent()) return false;
  if (!cell) throw new Error('This organism is no longer alive.');
  const scale = session.counted ? COUNT_SCALE : 1;
  const display=(n:number) => session.counted ? Math.round(n*scale) : Number(n.toFixed(4));
  const step=session.counted ? '1' : 'any';
  const dialog = createDialog('Edit reserves · organism ' + id, `<form>
    <p class="muted">Set a reserve amount or adjust energy by 10. Empty fields keep their current levels.</p>
    <div class="reserve-fields"><label for="reserve-energy">Available energy <span data-reserve-level="energy"></span></label>
    <input id="reserve-energy" type="number" inputmode="${session.counted ? 'numeric' : 'decimal'}" step="${step}" min="0" placeholder="Keep current">
    <div class="reserve-adjust"><button type="button" class="button small subtle" data-energy-delta="-10" aria-label="Subtract 10 energy">−10 ATP</button>
    <button type="button" class="button small subtle" data-energy-delta="10" aria-label="Add 10 energy">+10 ATP</button>
    <span data-delta-description aria-live="polite"></span></div>
    <label for="reserve-minerals">Stored minerals <span data-reserve-level="minerals"></span></label>
    <input id="reserve-minerals" type="number" inputmode="${session.counted ? 'numeric' : 'decimal'}" step="${step}" min="0" placeholder="Keep current"></div>
    <p class="reserve-zero" role="status" hidden>Setting available energy to zero ends this organism’s life.</p>
    <p class="form-error" role="alert" hidden></p>
    <footer class="dialog-actions"><button type="button" class="button subtle" data-reserve-cancel>Cancel</button>
    <button type="submit" class="button primary" disabled>Apply reserves</button></footer></form>`, 'reserve-editor', host);
  const form = dialog.querySelector('form')!;
  const energy = dialog.querySelector<HTMLInputElement>('#reserve-energy')!;
  const minerals = dialog.querySelector<HTMLInputElement>('#reserve-minerals')!;
  const submit = form.querySelector<HTMLButtonElement>('button[type="submit"]')!;
  let shownEnergy = display(cell.energy), energyDelta = 0, changed = false;
  const showLevels = (current: Cell): void => {
    shownEnergy = display(current.energy);
    const show = (input: HTMLInputElement, key: string, amount: number, capacity: number): void => {
      input.max = String(session.counted ? Math.floor(capacity*scale+1e-8) : capacity);
      form.querySelector(`[data-reserve-level="${key}"]`)!.textContent = `${display(amount)} of ${input.max}`;
    };
    show(energy, 'energy', current.energy, current.energyCapacity);
    show(minerals, 'minerals', current.nutrientStored, current.nutrientCapacity);
  };
  showLevels(cell);
  const wouldEndLife = (): boolean => energy.valueAsNumber === 0 || (energyDelta < 0 && shownEnergy + energyDelta <= 0);
  const showChange = (): void => {
    const zero = wouldEndLife();
    form.querySelector<HTMLElement>('.reserve-zero')!.hidden = !zero;
    submit.textContent = zero ? 'Set energy to zero' : 'Apply reserves';
    submit.disabled = !energy.value && !minerals.value && energyDelta === 0;
    form.querySelector('[data-delta-description]')!.textContent = energyDelta > 0 ? `Add ${energyDelta} to live ATP, up to capacity.`
      : energyDelta < 0 ? `Remove ${-energyDelta} from live ATP.` : '';
  };
  for (const button of form.querySelectorAll<HTMLButtonElement>('[data-energy-delta]')) {
    button.addEventListener('click', () => {
      energy.value = ''; energyDelta += Number(button.dataset.energyDelta); showChange();
    });
  }
  form.addEventListener('input', event => { if (event.target === energy) energyDelta = 0; showChange(); });
  form.querySelector('[data-reserve-cancel]')!.addEventListener('click', () => dialog.close());
  form.addEventListener('submit', event => {
    event.preventDefault();
    if (submit.disabled || !isCurrent() || !form.reportValidity()) return;
    const energyValue = energy.value === '' ? null : energy.valueAsNumber/scale;
    const mineralValue = minerals.value === '' ? null : minerals.valueAsNumber/scale;
    const allowZero = wouldEndLife(), delta = energyDelta;
    form.inert = true; submit.disabled = true;
    void session.mutate(async engine => {
      let target = energyValue;
      if (delta !== 0) {
        const live = await engine.cellSnapshot(id);
        if (!live) throw new Error('This organism is no longer alive.');
        target = Math.max(0,Math.min(session.counted ? Math.floor(live.energyCapacity*scale+1e-8)/scale : live.energyCapacity,live.energy + delta/scale));
        if (target === 0 && !allowZero) throw new Error('The live energy level fell. Review the new level before draining this organism.');
      }
      await engine.setCellStores(id, target, mineralValue);
    }).then(() => {
      changed = true;
      void session.checkpoint().catch(() => {});
      if (dialog.open) dialog.close();
    }).catch(async error => {
      if (!dialog.open || !isCurrent()) return;
      setDialogError(dialog,error);
      const current = await session.inspect(engine => engine.cellSnapshot(id)).catch(() => null);
      form.inert = false;
      if (current && dialog.open) { showLevels(current); showChange(); }
    });
  });
  return new Promise(resolve => {
    const closeOnPageHide = (): void => { if (dialog.open) dialog.close(); };
    host.defaultView?.addEventListener('pagehide', closeOnPageHide);
    dialog.addEventListener('close', () => {
      host.defaultView?.removeEventListener('pagehide', closeOnPageHide);
      resolve(changed);
    }, {once:true});
  });
}
