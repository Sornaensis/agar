/** One existing player can move between documents; its worker and state stay owned by the app. */
export type PlayerWindowMode = 'picture-in-picture' | 'window';
export type PlayerCloseReason = 'return' | 'closed' | 'session' | 'unload';
interface PictureInPictureApi {
  requestWindow(options?: { width?: number; height?: number; disallowReturnToOpener?: boolean }): Promise<Window>;
}
interface PlayerViewCallbacks {
  bindDocument(document: Document): () => void;
  changed(reason?: PlayerCloseReason): void;
  visibilityChanged(): void;
}

export class PlayerView {
  private target: Window | null = null;
  private element: HTMLElement | null = null;
  private marker: Comment | null = null;
  private unbind: (() => void) | null = null;
  private removeControls: (() => void) | null = null;
  private opening: Promise<PlayerWindowMode> | null = null;
  private currentMode: PlayerWindowMode | null = null;
  private title = '';
  private readonly hidden = (): void => this.callbacks.visibilityChanged();
  private readonly closed = (): void => this.finish('closed');

  constructor(private readonly opener: Window, private readonly callbacks: PlayerViewCallbacks) {}

  get supportsAlwaysOnTop(): boolean {
    return Boolean((this.opener as Window & { documentPictureInPicture?: PictureInPictureApi }).documentPictureInPicture?.requestWindow);
  }
  get isOpen(): boolean { return this.target !== null; }
  get mode(): PlayerWindowMode | null { return this.currentMode; }
  get window(): Window | null { return this.target && !this.target.closed ? this.target : null; }
  get document(): Document | null { return this.window?.document ?? null; }
  get visible(): boolean { return Boolean(this.window && !this.window.document.hidden); }

  /** Call directly from a user gesture; requesting a PiP window needs activation. */
  open(element: HTMLElement, title: string, isCurrent: () => boolean = () => true): Promise<PlayerWindowMode> {
    if (this.target) { this.target.focus(); return Promise.resolve(this.currentMode!); }
    if (this.opening) return this.opening;
    const mode: PlayerWindowMode = this.supportsAlwaysOnTop ? 'picture-in-picture' : 'window';
    const rect = element.getBoundingClientRect();
    const width = Math.max(480, Math.min(1000, Math.round(rect.width)));
    const height = Math.max(340, Math.min(720, Math.round(rect.height)));
    let request: Promise<Window>;
    try {
      if (mode === 'picture-in-picture') {
        request = (this.opener as Window & { documentPictureInPicture: PictureInPictureApi }).documentPictureInPicture.requestWindow({ width, height, disallowReturnToOpener: true });
      } else {
        // Ordinary popups have no web API for enforcing always-on-top behavior.
        const target = this.opener.open('', '_blank', `popup,width=${width},height=${height}`);
        if (!target) throw new Error('The player window was blocked. Allow popups for this site and try again.');
        request = Promise.resolve(target);
      }
    } catch (error) { return Promise.reject(error); }
    const operation = request.then(target => {
      if (!isCurrent()) { target.close(); throw new Error('The player changed before the window opened. Open it again.'); }
      try {
        const doc = target.document;
        doc.title = title + ' · agar player';
        const base = doc.createElement('base'); base.href = this.opener.document.baseURI; doc.head.append(base);
        this.copyStyles(doc);
        const shell = doc.createElement('main'); shell.className = 'player-window-shell';
        doc.body.className = 'player-window'; doc.body.dataset.playerWindowMode = mode; doc.body.replaceChildren(shell);
        const toast = doc.createElement('div'); toast.className = 'toast'; toast.id = 'player-window-toast'; toast.setAttribute('role', 'status'); toast.setAttribute('aria-live', 'polite'); doc.body.append(toast);
        this.target = target; this.currentMode = mode; this.element = element;
        this.marker = this.opener.document.createComment('agar-player-location');
        element.replaceWith(this.marker); shell.append(element);
        target.addEventListener('pagehide', this.closed, { once: true });
        doc.addEventListener('visibilitychange', this.hidden);
        this.removeControls = bindFloatingControls(target);
        this.unbind = this.callbacks.bindDocument(doc);
        this.title = title;
        this.callbacks.changed();
        return mode;
      } catch (error) {
        if (this.target === target) this.finish('return');
        target.close();
        throw error;
      }
    });
    const pending = operation.finally(() => { if (this.opening === pending) this.opening = null; });
    this.opening = pending;
    return pending;
  }

  close(reason: PlayerCloseReason = 'return'): void {
    const target = this.target;
    if (!target) return;
    this.finish(reason);
    if (!target.closed) target.close();
  }

  focus(): void { this.window?.focus(); }
  setTitle(title: string): void {
    if (!this.target || title === this.title) return;
    this.title = title; this.target.document.title = title + ' · agar player';
  }

  private finish(reason: PlayerCloseReason): void {
    const target = this.target;
    if (!target) return;
    target.removeEventListener('pagehide', this.closed);
    target.document.removeEventListener('visibilitychange', this.hidden);
    this.removeControls?.(); this.removeControls = null;
    this.unbind?.(); this.unbind = null;
    const element = this.element, marker = this.marker;
    this.target = null; this.currentMode = null; this.element = null; this.marker = null;
    if (element) {
      // The main app may have navigated away and removed the original slot.
      // Preserve the same node for the app to remount into its current layout.
      if (marker?.isConnected) marker.replaceWith(element);
      else this.opener.document.createDocumentFragment().append(element);
    }
    marker?.remove();
    this.callbacks.changed(reason);
  }

  private copyStyles(destination: Document): void {
    for (const sheet of Array.from(this.opener.document.styleSheets)) {
      try {
        const style = destination.createElement('style');
        style.textContent = Array.from(sheet.cssRules, rule => rule.cssText).join('\n');
        destination.head.append(style);
      } catch {
        if (!sheet.href) continue;
        const link = destination.createElement('link'); link.rel = 'stylesheet'; link.href = sheet.href;
        destination.head.append(link);
      }
    }
  }
}

/** Revealing the floating HUD never replaces controls, moves focus, or alters the canvas. */
function bindFloatingControls(host:Window):()=>void {
  const doc=host.document, body=doc.body;
  const lifetime=new AbortController(), options={capture:true,signal:lifetime.signal};
  const setMode=(mode:'hidden'|'pointer'|'keyboard'|'touch'):void=>{
    if (body.dataset.playerControls!==mode) body.dataset.playerControls=mode;
  };
  setMode('hidden');

  // The first touch wakes the controls. Its synthetic click must not also
  // place an organism, edit a barrier, or submit an open modal dialog.
  const wakingTouches=new Set<number>();
  let suppressClick=false;
  const pointer=(event:PointerEvent):void=>{
    if (event.pointerType==='touch') {
      if (event.type==='pointerdown') {
        const waking=body.dataset.playerControls==='hidden' || wakingTouches.size>0;
        suppressClick=waking; setMode('touch');
        if (waking) wakingTouches.add(event.pointerId);
      }
      if (wakingTouches.has(event.pointerId)) {
        if (event.type==='pointerup' || event.type==='pointercancel') wakingTouches.delete(event.pointerId);
        event.preventDefault(); event.stopImmediatePropagation();
      }
      return;
    }
    if (event.type==='pointerover' || event.type==='pointermove' || event.type==='pointerdown') {
      if (event.type==='pointerdown') suppressClick=false;
      // Captured drags can cross the window without a pointerleave event.
      const outside=event.clientX<0 || event.clientY<0 || event.clientX>=host.innerWidth || event.clientY>=host.innerHeight;
      // Chromium can recompute mouse boundaries for a stationary pointer
      // outside this window after a touch has just revealed the controls.
      if (!outside) setMode('pointer');
      else if (body.dataset.playerControls!=='touch') setMode('hidden');
    } else if (event.type==='pointerout' && !event.relatedTarget && body.dataset.playerControls!=='touch') setMode('hidden');
  };
  for (const type of ['pointerover','pointermove','pointerout','pointerdown','pointerup','pointercancel'] as const) {
    doc.addEventListener(type,pointer,options);
  }
  doc.addEventListener('keydown',()=>{suppressClick=false;setMode('keyboard');},options);
  doc.addEventListener('focusin',event=>{
    if (body.dataset.playerControls==='hidden' && (event.target as Element).matches?.(':focus-visible')) setMode('keyboard');
  },options);
  doc.addEventListener('click',event=>{
    if (suppressClick && event.detail!==0) {
      suppressClick=false;event.preventDefault();event.stopImmediatePropagation();return;
    }
    if ((event.target as Element).closest?.('[data-player-hide]')) {
      // Explicit dismissal returns keyboard navigation to the world.
      if ((event.target as Element).closest('[data-player-hide]')===doc.activeElement) doc.getElementById('world')?.focus({preventScroll:true});
      setMode('hidden');
    } else if (event.detail===0 && body.dataset.playerControls==='hidden') setMode('keyboard');
  },options);
  const hide=():void=>{wakingTouches.clear();suppressClick=false;setMode('hidden');};
  host.addEventListener('blur',hide,{signal:lifetime.signal});
  doc.addEventListener('visibilitychange',()=>{if(doc.hidden)hide();},{signal:lifetime.signal});
  return ()=>lifetime.abort();
}

/** One clock and pending callback; hidden players use a bounded timer fallback. */
export class PlayerFrameLoop {
  private owner: Window | null = null;
  private request = 0;
  private timer = false;
  private epoch = 0;
  private running = false;
  constructor(private readonly host: () => Window, private readonly frame: (now: number) => void,
    private readonly now: () => number = () => performance.now(),
    private readonly continueHidden: () => boolean = () => true) {}
  start(): void { if (!this.running) { this.running = true; this.refresh(); } }
  stop(): void {
    this.running = false; this.epoch++;
    this.cancelRequest(); this.owner = null;
  }
  refresh(): void {
    if (!this.running) return;
    const owner = this.host();
    const timer = owner.document.hidden && this.continueHidden();
    if (this.owner === owner && this.timer === timer && this.request) return;
    this.cancelRequest();
    this.owner = owner; this.timer = timer;
    const epoch = ++this.epoch;
    const tick = (): void => {
      if (!this.running || epoch !== this.epoch) return;
      this.request = 0;
      try { this.frame(this.now()); }
      finally { if (this.running && epoch === this.epoch) this.refresh(); }
    };
    // Hidden documents stop rAF. Keep lightweight checkpoint/UI housekeeping
    // alive here; the independent simulation loop paces worker batches. Both
    // remain subject to browser/OS suspension. Visibility invalidates old work.
    this.request = timer ? owner.setTimeout(tick, 100) : owner.requestAnimationFrame(tick);
  }
  private cancelRequest(): void {
    if (this.request) {
      if (this.timer) this.owner?.clearTimeout(this.request);
      else this.owner?.cancelAnimationFrame(this.request);
    }
    this.request = 0;
  }
}
