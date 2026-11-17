export const escapeHtml = (value: string): string => value.replace(/[&<>"']/g, character => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character]!);
const paths: Record<string, string> = {
  cell: '<circle cx="12" cy="12" r="9"/><circle cx="11" cy="12" r="3"/><path d="m17 7 1 1"/>',
  play: '<path d="m8 5 11 7-11 7z"/>', pause: '<path d="M8 5v14M16 5v14"/>',
  plus: '<path d="M12 5v14M5 12h14"/>', close: '<path d="m6 6 12 12M6 18 18 6"/>',
  back: '<path d="M19 12H5m6-6-6 6 6 6"/>', arrow: '<path d="M5 12h14m-6-6 6 6-6 6"/>',
  graph: '<rect x="2" y="3" width="6" height="5" rx="1"/><rect x="2" y="16" width="6" height="5" rx="1"/><rect x="16" y="9" width="6" height="6" rx="1"/><path d="M8 5.5h4V12h4M8 18.5h4V12"/>',
  dna: '<path d="M6 3c0 8 12 10 12 18M18 3c0 8-12 10-12 18M7 6h10M8 10h8M8 14h8M7 18h10"/>',
  light: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5 19 19M5 19l1.5-1.5M17.5 6.5 19 5"/>',
  heat: '<path d="M9 14.5V5a3 3 0 0 1 6 0v9.5a5 5 0 1 1-6 0Z"/><path d="M12 9v10"/>',
  flow: '<path d="M3 8h13a3 3 0 1 0-3-3M3 12h16a3 3 0 1 1-3 3M3 16h5a3 3 0 1 1-3 3"/>',
  download: '<path d="M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5"/>',
  upload: '<path d="M12 16V3m-5 5 5-5 5 5M4 16v5h16v-5"/>',
  help: '<circle cx="12" cy="12" r="9"/><path d="M9.5 8a2.5 2.5 0 0 1 5 0c0 2-2.5 2-2.5 4M12 16h.01"/>',
  more: '<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>',
  copy: '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V4H4v12h4"/>',
  search: '<circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/>',
  expand: '<path d="M8 3H3v5M16 3h5v5M3 16v5h5M21 16v5h-5"/>',
  refresh: '<path d="M20 7v5h-5M4 17v-5h5M6 7a7 7 0 0 1 12-2l2 3M4 16l2 3a7 7 0 0 0 12-2"/>',
  undo: '<path d="M9 5 4 10l5 5M4 10h10a6 6 0 0 1 0 12" transform="translate(0 -2)"/>',
  redo: '<path d="m15 5 5 5-5 5M20 10H10a6 6 0 0 0 0 12" transform="translate(0 -2)"/>',
  save: '<path d="M5 3h12l4 4v14H3V3h2Z"/><path d="M7 3v6h10V3M7 21v-8h10v8"/>',
  chevron: '<path d="m9 5 7 7-7 7"/>',
  check: '<path d="m5 12 4 4L19 6"/>', trash: '<path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14M10 11v6M14 11v6"/>',
};
export const icon = (name: string): string => `<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths[name] || paths.cell}</svg>`;
let dialogSequence = 0;
export function createDialog(title: string, bodyHtml: string, className = ''): HTMLDialogElement {
  const dialog = document.createElement('dialog');
  const headingId = `dialog-heading-${++dialogSequence}`;
  dialog.className = `app-dialog ${className}`;
  dialog.setAttribute('aria-labelledby', headingId);
  dialog.innerHTML = `<header class="dialog-header"><h2 id="${headingId}">${escapeHtml(title)}</h2><button type="button" class="icon-button" aria-label="Close ${escapeHtml(title)}" data-close>${icon('close')}</button></header><div class="dialog-body">${bodyHtml}</div>`;
  dialog.querySelector('[data-close]')!.addEventListener('click', () => dialog.close());
  dialog.addEventListener('click', event => {
    if (dialog.dataset.saving) return;
    if (event.target !== dialog) return;
    const rect = dialog.getBoundingClientRect();
    if (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom) dialog.close();
  });
  dialog.addEventListener('close', () => dialog.remove(), { once: true });
  document.body.append(dialog);
  dialog.showModal();
  return dialog;
}
export function setDialogError(dialog: HTMLDialogElement, error: unknown): void {
  let target = dialog.querySelector<HTMLElement>('.form-error');
  if (!target) {
    target = document.createElement('p'); target.className = 'form-error'; target.setAttribute('role', 'alert');
    dialog.querySelector('.dialog-body')!.append(target);
  }
  target.textContent = error instanceof Error ? error.message : String(error);
  target.hidden = false;
}
export function downloadFile(name: string, content: string | Blob, type = 'application/json'): void {
  const url = URL.createObjectURL(content instanceof Blob ? content : new Blob([content], { type }));
  const link = document.createElement('a'); link.href = url; link.download = name; document.body.append(link); link.click(); link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}
export function inputNumber(input: HTMLInputElement, label: string, min: number, max: number): number {
  const value = input.valueAsNumber;
  if (!Number.isFinite(value) || value < min || value > max) throw new Error(`${label} must be between ${min} and ${max}.`);
  return value;
}
export function clock(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const parts = [Math.floor(total / 60) % 60, total % 60].map(value => String(value).padStart(2, '0'));
  if (total >= 3600) parts.unshift(String(Math.floor(total / 3600)).padStart(2, '0'));
  return parts.join(':');
}
export function readableError(error: unknown): string { return error instanceof Error ? error.message : String(error); }
