import { ORGANELLE_CATALOG } from './organelles';
import { WorldCamera } from './world-camera';
import { PeriodicVisibility } from './periodic-visibility';
import type { BodyInspection, Cell, Food, RenderCell, EnvironmentSnapshot, EnvironmentSource, EnvironmentBarrier } from './engine';
import type { RenderWorldSnapshot } from './session';

const PALETTE = ['#5ce1ba', '#aaa2ff', '#efa66b', '#6dcbea', '#ed9db7', '#d7d889'];
const clamp = (value: number, low: number, high: number): number => Math.max(low, Math.min(high, value));

/** A genome keeps its visual identity across all of its descendants. */
export function genomeColor(id: number): string {
  return PALETTE[((Math.floor(id) - 1) % PALETTE.length + PALETTE.length) % PALETTE.length];
}

function rgba(hex: string, alpha: number): string {
  return `rgba(${parseInt(hex.slice(1, 3), 16)},${parseInt(hex.slice(3, 5), 16)},${parseInt(hex.slice(5, 7), 16)},${alpha})`;
}

interface TrailPoint { x: number; y: number; time: number }
export type FieldOverlay = 'none' | 'light' | 'heat' | 'flow' | 'nutrients' | 'oxygen' | 'carbon';
interface RenderOptions { colonyActivity?:boolean; sensors?: boolean; trails?: boolean; overlay?: FieldOverlay }
export type EnvironmentSelection = { kind: 'source' | 'barrier'; id: number } | null;
export type CanvasTarget = {kind:'cell'; object:RenderCell} | {kind:'source'; object:EnvironmentSource} | {kind:'barrier'; object:EnvironmentBarrier};
interface DragHandlers {
  preferEnvironment():boolean;
  canDrag(target:CanvasTarget):boolean;
  drop(target:CanvasTarget,x:number,y:number,colony:boolean):void;
}
interface Drag {
  pointerId:number; item:CanvasTarget; offsetX:number; offsetY:number; colony:boolean;
  active:boolean; target:{x:number; y:number} | null;
}
interface CameraPointer { x: number; y: number; startX: number; startY: number; touch: boolean }

/** Draws snapshots only. Rust owns every position, velocity, and biological state. */
export class Renderer {
  private readonly context: CanvasRenderingContext2D;
  private readonly camera = new WorldCamera();
  private readonly visibility = new PeriodicVisibility();
  private readonly input = new AbortController();
  private readonly pointers = new Map<number, CameraPointer>();
  private readonly originalTouchAction: string;
  private readonly originalCursor: string;
  private suppressClickUntil = 0;
  private drag: Drag | null = null;
  private selection: EnvironmentSelection = null;
  private cameraFrame = 0;
  private renderOptions: RenderOptions = {};
  private observer: ResizeObserver;
  private renderWindow: Window & typeof globalThis;
  private readonly sprites = new Map<string, HTMLCanvasElement>();
  private readonly trails = new Map<number, TrailPoint[]>();
  private readonly livingTrails = new Set<number>();
  private background: HTMLCanvasElement | null = null;
  private fieldCanvas: HTMLCanvasElement | null = null;
  private fieldSamples: Float32Array | null = null;
  private fieldPixels: ImageData | null = null;
  private fieldMode: FieldOverlay = 'none';
  private worldWidth = 1200;
  private worldHeight = 760;
  private cssWidth = 0;
  private cssHeight = 0;
  private pixelRatio = 1;
  private scale = 1;
  private offsetX = 0;
  private offsetY = 0;
  private lastTick = -1;
  private lastWorld: RenderWorldSnapshot | null = null;
  private lastSelected: number | null = null;
  private lastDetails: Cell | null = null;
  private lastBody: BodyInspection | null = null;
  private lastOptions = '';
  private resizeVersion = 0;
  private renderedResize = -1;

  constructor(private readonly canvas: HTMLCanvasElement, private readonly drags?: DragHandlers) {
    const context = canvas.getContext('2d', { alpha: false });
    if (!context) throw new Error('This browser does not support the 2D simulation view.');
    this.context = context;
    this.renderWindow = (canvas.ownerDocument.defaultView ?? window) as Window & typeof globalThis;
    this.originalTouchAction = canvas.style.touchAction;
    this.originalCursor = canvas.style.cursor;
    canvas.style.touchAction = 'none';
    this.bindCameraInput();
    this.observer = new this.renderWindow.ResizeObserver(() => this.resize());
    this.observer.observe(canvas);
    this.resize();
  }

  /** Rebind rendering/resize delivery after the same canvas moves into or out of PiP. */
  reparent(): void {
    const target = (this.canvas.ownerDocument.defaultView ?? window) as Window & typeof globalThis;
    if (target !== this.renderWindow) {
      this.renderWindow.cancelAnimationFrame(this.cameraFrame); this.cameraFrame = 0;
      this.observer.disconnect(); this.renderWindow = target;
      this.observer = new target.ResizeObserver(() => this.resize()); this.observer.observe(this.canvas);
    }
    this.resize();
  }

  resize(): void {
    this.resizeVersion++;
    const rect = this.canvas.getBoundingClientRect();
    this.cssWidth = Math.max(1, rect.width);
    this.cssHeight = Math.max(1, rect.height);
    this.pixelRatio = Math.min(this.renderWindow.devicePixelRatio || 1, 2);
    const width = Math.max(1, Math.round(this.cssWidth * this.pixelRatio));
    const height = Math.max(1, Math.round(this.cssHeight * this.pixelRatio));
    if (this.canvas.width !== width) this.canvas.width = width;
    if (this.canvas.height !== height) this.canvas.height = height;
    this.camera.setViewport(this.cssWidth, this.cssHeight);
    this.updateTransform();
    this.requestRedraw();
  }

  private updateTransform(): void {
    this.scale = this.camera.scale;
    this.offsetX = this.camera.offsetX;
    this.offsetY = this.camera.offsetY;
    this.visibility.width = this.worldWidth; this.visibility.height = this.worldHeight;
    this.visibility.left = -this.offsetX / this.scale;
    this.visibility.right = (this.cssWidth - this.offsetX) / this.scale;
    this.visibility.top = -this.offsetY / this.scale;
    this.visibility.bottom = (this.cssHeight - this.offsetY) / this.scale;
  }

  /** Reset to a complete view without changing the simulated world. */
  fitWorld(): void { this.camera.fit(); this.cameraChanged(); }

  /** Zoom around a client coordinate, or the canvas center for toolbar controls. */
  zoomBy(factor: number, clientX?: number, clientY?: number): void {
    const rect = this.canvas.getBoundingClientRect();
    this.camera.zoomAt(factor, clientX === undefined ? this.cssWidth / 2 : clientX - rect.left,
      clientY === undefined ? this.cssHeight / 2 : clientY - rect.top);
    this.cameraChanged();
  }

  getZoom(): number { return this.camera.zoom; }
  getCamera(): { x: number; y: number; zoom: number } { return { x: this.camera.x, y: this.camera.y, zoom: this.camera.zoom }; }

  setEnvironmentSelection(selection: EnvironmentSelection): void {
    if (this.selection?.kind === selection?.kind && this.selection?.id === selection?.id) return;
    this.selection = selection;
    this.resizeVersion++;
    this.requestRedraw();
  }

  /** Consume the synthetic click following a pan or pinch, before picking/placing. */
  wasDragging(): boolean {
    const dragged = performance.now() < this.suppressClickUntil;
    this.suppressClickUntil = 0;
    return dragged;
  }

  private requestRedraw(): void {
    if (this.cameraFrame || !this.lastWorld) return;
    this.cameraFrame = this.renderWindow.requestAnimationFrame(() => {
      this.cameraFrame = 0;
      if (this.lastWorld) this.render(this.lastWorld, this.lastSelected, this.renderOptions, this.lastDetails, this.lastBody);
    });
  }

  private cameraChanged(): void {
    this.updateTransform();
    this.resizeVersion++;
    this.requestRedraw();
    this.canvas.dispatchEvent(new CustomEvent('camerachange', { detail: this.getCamera() }));
  }

  private bindCameraInput(): void {
    const signal = this.input.signal;
    this.canvas.addEventListener('wheel', event => {
      event.preventDefault();
      const amount = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? this.cssHeight : 1);
      this.zoomBy(Math.exp(-Math.max(-250, Math.min(250, amount)) * 0.0025), event.clientX, event.clientY);
    }, { passive: false, signal });
    this.canvas.addEventListener('pointerdown', event => {
      if (!this.pointers.size) this.suppressClickUntil = 0;
      const touch = event.pointerType === 'touch';
      let grab = false;
      if (this.pointers.size === 0 && event.button === 0 && !event.shiftKey && this.drags && this.lastWorld) {
        const hit = this.pickTarget(this.lastWorld,event.clientX,event.clientY,this.drags.preferEnvironment());
        const point = this.toWorld(event.clientX,event.clientY);
        if (hit && point && this.drags.canDrag(hit)) {
          // Preserve the grab offset even when the object straddles a world edge.
          const dx = hit.object.x - point.x, dy = hit.object.y - point.y;
          this.drag = {pointerId:event.pointerId,item:hit,
            offsetX:dx - Math.round(dx / this.worldWidth) * this.worldWidth,
            offsetY:dy - Math.round(dy / this.worldHeight) * this.worldHeight,
            active:false,target:null,colony:event.altKey && hit.kind==='cell' && Boolean(hit.object.attachedNeighbors)};
          grab = true;
        }
      } else if (touch && this.drag) this.clearDrag();
      if (!grab && !touch && event.button !== 1 && event.button !== 2 && !(event.button === 0 && event.shiftKey)) return;
      if (!grab && !touch) { event.preventDefault(); this.suppressClickUntil = performance.now() + 700; }
      this.canvas.focus({ preventScroll: true });
      this.pointers.set(event.pointerId, { x: event.clientX, y: event.clientY, startX: event.clientX, startY: event.clientY, touch });
      this.canvas.setPointerCapture(event.pointerId);
      this.canvas.style.cursor = 'grabbing';
      if (this.pointers.size > 1) this.suppressClickUntil = performance.now() + 700;
    }, { signal });
    this.canvas.addEventListener('pointermove', event => {
      const pointer = this.pointers.get(event.pointerId);
      if (!pointer) return;
      const oldX = pointer.x, oldY = pointer.y;
      const other = pointer.touch ? [...this.pointers.entries()].find(([id, value]) => id !== event.pointerId && value.touch)?.[1] : undefined;
      pointer.x = event.clientX; pointer.y = event.clientY;
      if (this.drag?.pointerId === event.pointerId) {
        if (!this.drag.active && Math.hypot(pointer.x-pointer.startX,pointer.y-pointer.startY) < 6) return;
        this.drag.active = true;
        this.canvas.setAttribute('data-dragging-target',this.drag.item.kind);
        this.drag.target = this.dragPoint(pointer.x,pointer.y);
        this.suppressClickUntil = performance.now()+700;
        this.resizeVersion++; this.requestRedraw(); event.preventDefault(); return;
      }
      if (other) {
        const oldDistance = Math.hypot(oldX - other.x, oldY - other.y);
        const distance = Math.hypot(pointer.x - other.x, pointer.y - other.y);
        const rect = this.canvas.getBoundingClientRect();
        if (oldDistance > 1 && distance > 1) this.camera.zoomAt(distance / oldDistance, (oldX + other.x) / 2 - rect.left, (oldY + other.y) / 2 - rect.top);
        this.camera.pan((pointer.x - oldX) / 2, (pointer.y - oldY) / 2);
        this.suppressClickUntil = performance.now() + 700;
      } else {
        if (Math.hypot(pointer.x - pointer.startX, pointer.y - pointer.startY) < 4 && !this.suppressClickUntil) return;
        this.camera.pan(pointer.x - oldX, pointer.y - oldY);
        this.suppressClickUntil = performance.now() + 700;
      }
      event.preventDefault();
      this.cameraChanged();
    }, { signal });
    const release = (event: PointerEvent): void => {
      if (!this.pointers.delete(event.pointerId)) return;
      const grab = this.drag;
      if (grab?.pointerId === event.pointerId) {
        const target = this.dragPoint(event.clientX,event.clientY);
        this.clearDrag();
        if (event.type === 'pointerup' && grab.active && target && this.drags?.canDrag(grab.item)) {
          this.drags.drop(grab.item,target.x,target.y,grab.colony);
        }
      }
      if (this.suppressClickUntil) this.suppressClickUntil = performance.now() + 700;
      if (this.canvas.hasPointerCapture(event.pointerId)) this.canvas.releasePointerCapture(event.pointerId);
      if (!this.pointers.size) this.canvas.style.cursor = this.originalCursor;
    };
    this.canvas.addEventListener('pointerup', release, { signal });
    this.canvas.addEventListener('pointercancel', release, { signal });
    this.canvas.addEventListener('lostpointercapture', release, { signal });
    this.canvas.addEventListener('contextmenu', event => event.preventDefault(), { signal });
    this.canvas.addEventListener('keydown', event => {
      if (event.key === '+' || event.key === '=') { event.preventDefault(); this.zoomBy(1.25); }
      else if (event.key === '-') { event.preventDefault(); this.zoomBy(0.8); }
      else if (event.key === 'Home' || event.key === '0') { event.preventDefault(); this.fitWorld(); }
    }, { signal });
  }

  private dragPoint(clientX:number,clientY:number): {x:number; y:number} | null {
    const rect = this.canvas.getBoundingClientRect();
    if (clientX < rect.left || clientX >= rect.right || clientY < rect.top || clientY >= rect.bottom) return null;
    const point = this.toWorld(clientX,clientY);
    if (!point || !this.drag) return null;
    return {x:(point.x + this.drag.offsetX + this.worldWidth) % this.worldWidth,
      y:(point.y + this.drag.offsetY + this.worldHeight) % this.worldHeight};
  }

  private clearDrag(): void {
    this.drag = null;
    this.canvas.removeAttribute('data-dragging-target');
    this.resizeVersion++; this.requestRedraw();
  }

  toWorld(clientX: number, clientY: number): { x: number; y: number } | null {
    const rect = this.canvas.getBoundingClientRect();
    const x = (clientX - rect.left - this.offsetX) / this.scale;
    const y = (clientY - rect.top - this.offsetY) / this.scale;
    if (x < 0 || y < 0 || x >= this.worldWidth || y >= this.worldHeight) return null;
    return { x, y };
  }

  pickCell(world: RenderWorldSnapshot, clientX: number, clientY: number): RenderCell | null {
    const position = this.toWorld(clientX, clientY);
    if (!position) return null;
    let nearest: RenderCell | null = null;
    let nearestDistance = Infinity;
    for (const cell of world.cells) {
      const rawX = Math.abs(cell.x - position.x);
      const rawY = Math.abs(cell.y - position.y);
      const dx = Math.min(rawX, world.width - rawX);
      const dy = Math.min(rawY, world.height - rawY);
      const distance = dx * dx + dy * dy;
      const hitRadius = cell.radius + 8 / this.scale;
      if (distance <= hitRadius * hitRadius && distance < nearestDistance) {
        nearest = cell;
        nearestDistance = distance;
      }
    }
    return nearest;
  }

  /** Resolve a single canvas tap without scanning particles during hover or animation. */
  pickFood(world: RenderWorldSnapshot, clientX: number, clientY: number): Food | null {
    const point = this.toWorld(clientX,clientY);
    if (!point) return null;
    let nearest: Food | null = null, best = (2.5 + 6 / this.scale) ** 2;
    for (const food of world.food) {
      const rawX = Math.abs(food.x-point.x), rawY = Math.abs(food.y-point.y);
      const dx = Math.min(rawX,world.width-rawX), dy = Math.min(rawY,world.height-rawY);
      const distance = dx*dx + dy*dy;
      if (distance < best) { nearest = food; best = distance; }
    }
    return nearest;
  }

  pickTarget(world: RenderWorldSnapshot, x:number, y:number, preferEnvironment:boolean): CanvasTarget | null {
    const cell = preferEnvironment ? null : this.pickCell(world,x,y);
    if (cell) return {kind:'cell',object:cell};
    const source = this.pickSource(world,x,y);
    if (source) return {kind:'source',object:source};
    const barrier = this.pickBarrier(world,x,y);
    if (barrier) return {kind:'barrier',object:barrier};
    const remainingCell = preferEnvironment ? this.pickCell(world,x,y) : null;
    return remainingCell ? {kind:'cell',object:remainingCell} : null;
  }

  pickSource(world: RenderWorldSnapshot, clientX: number, clientY: number): EnvironmentSource | null {
    const position = this.toWorld(clientX, clientY);
    if (!position) return null;
    let nearest: EnvironmentSource | null = null;
    let nearestDistance = (18 / this.scale) ** 2;
    for (const source of world.environment.sources) {
      const rawX = Math.abs(source.x - position.x);
      const rawY = Math.abs(source.y - position.y);
      const dx = Math.min(rawX, world.width - rawX);
      const dy = Math.min(rawY, world.height - rawY);
      const distance = dx * dx + dy * dy;
      if (distance <= nearestDistance) { nearest = source; nearestDistance = distance; }
    }
    return nearest;
  }

  pickBarrier(world: RenderWorldSnapshot, clientX: number, clientY: number): EnvironmentBarrier | null {
    const position = this.toWorld(clientX, clientY);
    if (!position) return null;
    const tolerance = 5 / this.scale;
    // Last drawn wins where barriers overlap.
    const barriers = world.environment.barriers;
    for (let index = barriers.length - 1; index >= 0; index--) {
      const barrier = barriers[index];
      const cos = Math.cos(barrier.angle), sin = Math.sin(barrier.angle);
      const count = this.visibility.collect(barrier.x, barrier.y, Math.hypot(barrier.length, barrier.width) / 2 + tolerance);
      for (let image = 0; image < count; image += 2) {
        const dx = position.x - this.visibility.points[image], dy = position.y - this.visibility.points[image + 1];
        if (Math.abs(dx * cos + dy * sin) <= barrier.length / 2 + tolerance
          && Math.abs(-dx * sin + dy * cos) <= barrier.width / 2 + tolerance) return barrier;
      }
    }
    return null;
  }

  render(world: RenderWorldSnapshot, selectedId: number | null, options: RenderOptions = {}, details: Cell | null = null, body: BodyInspection | null = null): void {
    if (world.width !== this.worldWidth || world.height !== this.worldHeight) {
      this.worldWidth = world.width;
      this.worldHeight = world.height;
      this.background = null;
      this.camera.setWorld(world.width, world.height);
      this.updateTransform();
    }
    if (this.pixelRatio !== Math.min(this.renderWindow.devicePixelRatio || 1, 2)) this.resize();
    const optionsKey = `${options.overlay ?? 'none'}:${options.trails ?? false}:${options.sensors ?? true}:${options.colonyActivity ?? true}`;
    if (this.lastWorld === world && this.lastSelected === selectedId && this.lastDetails === details && this.lastBody === body
      && this.lastOptions === optionsKey && this.renderedResize === this.resizeVersion) return;
    this.lastWorld = world; this.lastSelected = selectedId; this.lastDetails = details; this.lastBody = body;
    this.lastOptions = optionsKey; this.renderOptions = { ...options }; this.renderedResize = this.resizeVersion;
    const context = this.context;
    context.setTransform(this.pixelRatio, 0, 0, this.pixelRatio, 0, 0);
    context.globalAlpha = 1;
    context.fillStyle = '#080f12';
    context.fillRect(0, 0, this.cssWidth, this.cssHeight);
    context.save();
    context.translate(this.offsetX, this.offsetY);
    context.scale(this.scale, this.scale);
    context.beginPath();
    context.rect(0, 0, this.worldWidth, this.worldHeight);
    context.clip();
    this.drawBackground();
    const overlay = options.overlay ?? 'none';
    if (overlay !== 'none') this.drawEnvironment(world.environment, overlay);

    if (world.tick < this.lastTick) this.trails.clear();
    if (options.trails) {
      if (world.tick !== this.lastTick) this.recordTrails(world);
      this.drawTrails(world);
    } else {
      this.trails.clear();
    }
    this.lastTick = world.tick;

    const selected = selectedId === null ? undefined : world.cells.find((cell) => cell.id === selectedId);
    if (selected && details?.id === selected.id && options.sensors !== false) {
      const count = this.visibility.collect(selected.x, selected.y, selected.radius + 16);
      for (let image = 0; image < count; image += 2) {
        this.drawBodyDetails(details, this.visibility.points[image], this.visibility.points[image + 1]);
      }
    }

    // Three batched material paths keep large particle populations cheap to draw.
    const organicPaths = [new Path2D(),new Path2D(),new Path2D()];
    const organicHalos = [new Path2D(),new Path2D(),new Path2D()];
    for (const food of world.food) {
      const count = this.visibility.collect(food.x, food.y, 5);
      if (!count) continue;
      // Area tracks remaining chemical energy up to one freshly supplied packet.
      const fullness = Math.sqrt(clamp(food.energy / 24, 0, 1));
      const radius = 0.25 + (food.kind === 1 ? 2.25 : 1.65) * fullness;
      const haloRadius = 0.5 + 4.5 * fullness;
      const path = organicPaths[food.kind], halo = organicHalos[food.kind];
      for (let image = 0; image < count; image += 2) {
        const x = this.visibility.points[image], y = this.visibility.points[image + 1];
        halo.moveTo(x + haloRadius,y); halo.arc(x,y,haloRadius,0,Math.PI * 2);
        if (food.kind === 1) {
          path.moveTo(x,y-radius); path.lineTo(x+radius,y);
          path.lineTo(x,y+radius); path.lineTo(x-radius,y); path.closePath();
        } else if (food.kind === 2) {
          path.rect(x-radius*0.7,y-radius*0.7,radius*1.4,radius*1.4);
        } else {
          path.moveTo(x+radius,y); path.arc(x,y,radius,0,Math.PI * 2);
        }
      }
    }
    const organicColors = ['rgba(237,198,108,0.86)','rgba(225,137,122,0.9)','rgba(158,151,130,0.72)'];
    const haloColors = ['rgba(237,198,108,0.07)','rgba(225,137,122,0.08)','rgba(158,151,130,0.035)'];
    for (let kind=0;kind<3;kind++) {
      context.fillStyle=haloColors[kind]; context.fill(organicHalos[kind]);
      context.fillStyle=organicColors[kind]; context.fill(organicPaths[kind]);
    }

    for (const cell of world.cells) {
      const margin = Math.max(cell.radius * 2.4 + 5, cell.id === selectedId ? cell.radius + 8 / this.scale : 0);
      const count = this.visibility.collect(cell.x, cell.y, margin);
      for (let image = 0; image < count; image += 2) {
        this.drawCell(cell, this.visibility.points[image], this.visibility.points[image + 1], cell.id === selectedId);
      }
    }
    if (options.colonyActivity !== false && world.bonds.length) this.drawColonyBonds(world);
    if (selected && details?.id===selected.id && body?.cellId===selected.id && body.cellular && options.sensors!==false) {
      const count = this.visibility.collect(selected.x,selected.y,selected.radius+32/this.scale);
      for (let image = 0; image < count; image += 2) this.drawCellularDetails(details,body,this.visibility.points[image],this.visibility.points[image + 1]);
    }
    this.drawBarriers(world.environment.barriers ?? []);
    this.drawSources(world.environment.sources, overlay);
    const drag = this.drag;
    if (drag?.active && drag.target && drag.item.kind === 'cell') {
      const cell = drag.item.object;
      const count = this.visibility.collect(drag.target.x,drag.target.y,cell.radius+5/this.scale);
      context.save(); context.strokeStyle = '#e3fff5'; context.fillStyle = '#6ed5ad55';
      context.lineWidth = 2 / this.scale; context.setLineDash([4/this.scale,3/this.scale]);
      for (let image=0;image<count;image+=2) {
        context.beginPath(); context.arc(this.visibility.points[image],this.visibility.points[image+1],cell.radius,0,2*Math.PI); context.fill(); context.stroke();
      }
      context.restore();
    }
    context.restore();
  }

  private drawEnvironment(environment: EnvironmentSnapshot, mode: FieldOverlay): void {
    const { columns, rows, samples } = environment;
    if (samples.length !== columns * rows * 7) return;
    if (!this.fieldCanvas) this.fieldCanvas = document.createElement('canvas');
    if (this.fieldSamples !== samples || this.fieldMode !== mode) {
      if (this.fieldCanvas.width !== columns) this.fieldCanvas.width = columns;
      if (this.fieldCanvas.height !== rows) this.fieldCanvas.height = rows;
      const field = this.fieldCanvas.getContext('2d')!;
      if (!this.fieldPixels || this.fieldPixels.width !== columns || this.fieldPixels.height !== rows) this.fieldPixels = field.createImageData(columns, rows);
      const pixels = this.fieldPixels;
      const red = mode === 'oxygen' ? 101 : mode === 'carbon' ? 214 : 104;
      const green = mode === 'oxygen' ? 190 : mode === 'carbon' ? 165 : 215;
      const blue = mode === 'oxygen' ? 247 : mode === 'carbon' ? 242 : 157;
      for (let index = 0; index < columns * rows; index += 1) {
        const offset = index * 4;
        const sample = index * 7;
        if (mode === 'light') {
          pixels.data[offset] = 245; pixels.data[offset + 1] = 203; pixels.data[offset + 2] = 102;
          pixels.data[offset + 3] = Math.round(clamp(samples[sample], 0, 1) * 78);
        } else if (mode === 'heat') {
          const difference = samples[sample + 1] - 0.5;
          pixels.data[offset] = difference >= 0 ? 242 : 73;
          pixels.data[offset + 1] = difference >= 0 ? 120 : 155;
          pixels.data[offset + 2] = difference >= 0 ? 84 : 228;
          pixels.data[offset + 3] = Math.round(clamp(Math.abs(difference) * 2, 0, 1) * 108);
        } else if (mode === 'nutrients' || mode === 'oxygen' || mode === 'carbon') {
          const level = Math.max(0, samples[sample + (mode === 'nutrients' ? 4 : mode === 'oxygen' ? 5 : 6)]);
          pixels.data[offset] = red; pixels.data[offset + 1] = green; pixels.data[offset + 2] = blue;
          // Fixed scale: a fading patch always means depletion, independent of other patches.
          pixels.data[offset + 3] = Math.round(level / (level + 0.5) * 145);
        } else {
          pixels.data[offset] = 102; pixels.data[offset + 1] = 182; pixels.data[offset + 2] = 224;
          pixels.data[offset + 3] = Math.round(clamp(Math.hypot(samples[sample + 2], samples[sample + 3]) / 30, 0, 1) * 60);
        }
      }
      field.putImageData(pixels, 0, 0);
      this.fieldSamples = samples;
      this.fieldMode = mode;
    }
    const context = this.context;
    context.save();
    context.imageSmoothingEnabled = true;
    context.drawImage(this.fieldCanvas, 0, 0, this.worldWidth, this.worldHeight);
    if (mode === 'flow') {
      context.strokeStyle = 'rgba(145,209,233,0.46)';
      context.lineWidth = 0.8 / this.scale;
      context.beginPath();
      const stride = Math.max(1, Math.ceil(38 / (this.worldWidth / columns * this.scale)));
      const firstColumn = Math.max(0, Math.floor(-this.offsetX / this.scale / this.worldWidth * columns));
      const lastColumn = Math.min(columns, Math.ceil((this.cssWidth - this.offsetX) / this.scale / this.worldWidth * columns));
      const firstRow = Math.max(0, Math.floor(-this.offsetY / this.scale / this.worldHeight * rows));
      const lastRow = Math.min(rows, Math.ceil((this.cssHeight - this.offsetY) / this.scale / this.worldHeight * rows));
      for (let row = Math.ceil(firstRow / stride) * stride; row < lastRow; row += stride) {
        for (let column = Math.ceil(firstColumn / stride) * stride; column < lastColumn; column += stride) {
          const offset = (row * columns + column) * 7;
          const vx = samples[offset + 2], vy = samples[offset + 3];
          const speed = Math.hypot(vx, vy);
          if (speed < 0.1) continue;
          const length = clamp(speed * 2.2, 3, 35);
          const dx = vx / speed, dy = vy / speed;
          const x = (column + 0.5) / columns * this.worldWidth;
          const y = (row + 0.5) / rows * this.worldHeight;
          const tipX = x + dx * length / 2, tipY = y + dy * length / 2;
          const head = Math.min(5, length * 0.4);
          context.moveTo(x - dx * length / 2, y - dy * length / 2);
          context.lineTo(tipX, tipY);
          context.moveTo(tipX - dx * head - dy * head * 0.6, tipY - dy * head + dx * head * 0.6);
          context.lineTo(tipX, tipY);
          context.lineTo(tipX - dx * head + dy * head * 0.6, tipY - dy * head - dx * head * 0.6);
        }
      }
      context.stroke();
    }
    context.restore();
  }

  private drawSources(sources: EnvironmentSource[], overlay: FieldOverlay): void {
    const context = this.context;
    context.save();
    for (const actual of sources) {
      const moving = this.drag?.active && this.drag.item.kind === 'source' && this.drag.item.object.id === actual.id && this.drag.target;
      const source = moving ? {...actual,...moving} : actual;
      const color = source.kind === 0 ? '#efd089' : source.kind === 1 ? (source.strength >= 0 ? '#ed9c82' : '#83c5eb')
        : source.kind === 3 ? '#81d99d' : source.kind === 4 ? '#e69bcb'
        : source.kind === 8 || source.kind === 9 ? '#65bef7' : source.kind === 10 || source.kind === 11 ? '#d6a5f2' : '#a5c4f7';
      const label = ['L', source.strength >= 0 ? 'H' : 'C', 'V', 'M+', 'M−', 'J', 'B', 'R', 'O+', 'O−', 'C+', 'C−', 'Air'][source.kind];
      const selected = moving || (this.selection?.kind === 'source' && this.selection.id === source.id);
      const showRadius = selected || (source.kind === 0 && overlay === 'light') || (source.kind === 1 && overlay === 'heat')
        || ([2,5,6,7].includes(source.kind) && overlay === 'flow') || ([3,4].includes(source.kind) && overlay === 'nutrients')
        || ([8,9,12].includes(source.kind) && overlay === 'oxygen') || ([10,11,12].includes(source.kind) && overlay === 'carbon');
      const directional = source.kind === 5 || source.kind === 6;
      const margin = showRadius ? directional ? Math.hypot(source.length / 2, source.radius) : source.radius : 14 / this.scale;
      const count = this.visibility.collect(source.x, source.y, margin);
      for (let image = 0; image < count; image += 2) {
        const x = this.visibility.points[image], y = this.visibility.points[image + 1];
        if (showRadius) {
          context.save(); context.translate(x, y); context.rotate(source.angle);
          context.lineWidth = (selected ? 1.15 : 0.7) / this.scale;
          context.strokeStyle = rgba(color, selected ? 0.7 : 0.32);
          context.fillStyle = rgba(color, selected ? 0.045 : 0.018);
          context.setLineDash([4 / this.scale, 6 / this.scale]);
          context.beginPath();
          if (directional) context.roundRect(-source.length / 2, -source.radius, source.length, source.radius * 2, Math.min(source.radius, source.length * 0.15));
          else context.arc(0, 0, source.radius, 0, Math.PI * 2);
          context.fill(); context.stroke(); context.setLineDash([]);
          if (directional) {
            const sign = source.strength < 0 ? -1 : 1;
            this.drawArrow(-Math.min(source.length * 0.3, 35 / this.scale) * sign, 0,
              Math.min(source.length * 0.3, 35 / this.scale) * sign, 0, color, selected ? 0.85 : 0.6);
          } else if (source.kind === 7) {
            const inward = source.strength < 0;
            for (let index = 0; index < 4; index++) {
              const angle = index * Math.PI / 2, start = source.radius * (inward ? 0.72 : 0.36), end = source.radius * (inward ? 0.36 : 0.72);
              this.drawArrow(Math.cos(angle) * start, Math.sin(angle) * start, Math.cos(angle) * end, Math.sin(angle) * end, color, 0.6);
            }
          } else if (source.kind === 2 && source.radius * this.scale > 24) {
            const radius = source.radius * 0.5, sign = source.strength < 0 ? -1 : 1;
            context.beginPath(); context.arc(0, 0, radius, 0, sign * Math.PI * 1.4, sign < 0); context.stroke();
            const end = sign * Math.PI * 1.4;
            this.drawArrow(Math.cos(end - sign * 0.16) * radius, Math.sin(end - sign * 0.16) * radius,
              Math.cos(end) * radius, Math.sin(end) * radius, color, 0.65);
          }
          context.restore();
        }
        if (selected) {
          context.beginPath(); context.arc(x, y, 13 / this.scale, 0, Math.PI * 2);
          context.strokeStyle = rgba(color, 0.42); context.lineWidth = 1 / this.scale; context.stroke();
        }
        context.setLineDash([]);
        context.beginPath();
        context.arc(x, y, 9 / this.scale, 0, Math.PI * 2);
        context.fillStyle = selected ? '#233b42' : '#142329';
        context.fill();
        context.lineWidth = (selected ? 1.5 : 1) / this.scale;
        context.strokeStyle = rgba(color, 0.9);
        context.stroke();
        context.fillStyle = color;
        context.font = `600 ${9 / this.scale}px ui-monospace, monospace`;
        context.textAlign = 'center';
        context.textBaseline = 'middle';
        context.fillText(label, x, y + 0.3 / this.scale);
      }
    }
    context.restore();
  }

  private drawArrow(fromX: number, fromY: number, toX: number, toY: number, color: string, alpha: number): void {
    const context = this.context, distance = Math.hypot(toX - fromX, toY - fromY);
    if (distance < 1e-6) return;
    const dx = (toX - fromX) / distance, dy = (toY - fromY) / distance;
    const head = Math.min(5 / this.scale, distance * 0.4);
    context.strokeStyle = rgba(color, alpha); context.lineWidth = 1 / this.scale;
    context.beginPath(); context.moveTo(fromX, fromY); context.lineTo(toX, toY);
    context.moveTo(toX - dx * head - dy * head * 0.6, toY - dy * head + dx * head * 0.6);
    context.lineTo(toX, toY); context.lineTo(toX - dx * head + dy * head * 0.6, toY - dy * head - dx * head * 0.6);
    context.stroke();
  }

  private drawBarriers(barriers: EnvironmentBarrier[]): void {
    const context = this.context;
    context.save();
    for (const actual of barriers) {
      const moving = this.drag?.active && this.drag.item.kind === 'barrier' && this.drag.item.object.id === actual.id && this.drag.target;
      const barrier = moving ? {...actual,...moving} : actual;
      const selected = moving || (this.selection?.kind === 'barrier' && this.selection.id === barrier.id);
      const color = barrier.floating ? '#79c6df' : '#a8b6bd';
      const margin = Math.hypot(barrier.length, barrier.width) / 2 + 6 / this.scale;
      const count = this.visibility.collect(barrier.x, barrier.y, margin);
      for (let image = 0; image < count; image += 2) {
        context.save(); context.translate(this.visibility.points[image], this.visibility.points[image + 1]); context.rotate(barrier.angle);
        context.fillStyle = barrier.floating ? '#234650' : '#354148';
        context.strokeStyle = rgba(color, selected ? 1 : 0.85);
        context.lineWidth = (selected ? 1.8 : 1.1) / this.scale;
        context.beginPath(); context.rect(-barrier.length / 2, -barrier.width / 2, barrier.length, barrier.width);
        context.fill(); context.stroke();
        context.save(); context.clip();
        context.strokeStyle = rgba(color, 0.16); context.lineWidth = 1 / this.scale;
        const spacing = Math.max(12, 10 / this.scale);
        context.beginPath();
        for (let x = -barrier.length / 2 - barrier.width; x < barrier.length / 2; x += spacing) {
          context.moveTo(x, barrier.width / 2); context.lineTo(x + barrier.width, -barrier.width / 2);
        }
        context.stroke(); context.restore();
        if (selected) {
          context.strokeStyle = rgba(color, 0.65); context.lineWidth = 1 / this.scale; context.setLineDash([4 / this.scale, 3 / this.scale]);
          context.strokeRect(-barrier.length / 2 - 4 / this.scale, -barrier.width / 2 - 4 / this.scale,
            barrier.length + 8 / this.scale, barrier.width + 8 / this.scale); context.setLineDash([]);
        }
        if (barrier.length * this.scale > 65 && barrier.width * this.scale > 12) {
          context.fillStyle = rgba(color, 0.9); context.font = `500 ${Math.min(10 / this.scale, barrier.width * 0.6)}px ui-monospace, monospace`;
          context.textAlign = 'center'; context.textBaseline = 'middle';
          context.fillText(barrier.floating ? 'FLOAT' : 'FIXED', 0, 0, barrier.length - 8 / this.scale);
        }
        context.restore();
      }
    }
    context.restore();
  }

  private drawBackground(): void {
    const context = this.context;
    context.drawImage(this.getBackground(), 0, 0, this.worldWidth, this.worldHeight);
    const left = Math.max(0, -this.offsetX / this.scale), right = Math.min(this.worldWidth, (this.cssWidth - this.offsetX) / this.scale);
    const top = Math.max(0, -this.offsetY / this.scale), bottom = Math.min(this.worldHeight, (this.cssHeight - this.offsetY) / this.scale);
    const spacing = 40 * Math.max(1, Math.ceil(12 / (40 * this.scale)));
    const radius = 0.6 / this.scale;
    context.fillStyle = 'rgba(128,170,161,0.2)'; context.beginPath();
    for (let x = Math.ceil((left - 20) / spacing) * spacing + 20; x < right; x += spacing) {
      for (let y = Math.ceil((top - 20) / spacing) * spacing + 20; y < bottom; y += spacing) {
        context.moveTo(x + radius, y); context.arc(x, y, radius, 0, Math.PI * 2);
      }
    }
    context.fill(); context.strokeStyle = 'rgba(112,164,154,0.3)'; context.lineWidth = 1 / this.scale;
    context.strokeRect(0, 0, this.worldWidth, this.worldHeight);
  }

  private getBackground(): HTMLCanvasElement {
    if (this.background) return this.background;
    const background = document.createElement('canvas');
    // A low-resolution gradient avoids allocating a world-sized bitmap on large habitats.
    const resolution = Math.min(1, 1200 / Math.max(this.worldWidth, this.worldHeight));
    background.width = Math.max(1, Math.ceil(this.worldWidth * resolution));
    background.height = Math.max(1, Math.ceil(this.worldHeight * resolution));
    const context = background.getContext('2d')!;
    const gradient = context.createRadialGradient(background.width * 0.4, background.height * 0.4, 0,
      background.width * 0.5, background.height * 0.5, background.width * 0.7);
    gradient.addColorStop(0, '#10231f'); gradient.addColorStop(0.6, '#0b1a1b'); gradient.addColorStop(1, '#091317');
    context.fillStyle = gradient; context.fillRect(0, 0, background.width, background.height);
    this.background = background;
    return background;
  }

  private sprite(color: string): HTMLCanvasElement {
    const cached = this.sprites.get(color);
    if (cached) return cached;
    const sprite = document.createElement('canvas');
    sprite.width = 96;
    sprite.height = 96;
    const context = sprite.getContext('2d')!;
    context.translate(48, 48);
    const glow = context.createRadialGradient(0, 0, 15, 0, 0, 46);
    glow.addColorStop(0, rgba(color, 0.12));
    glow.addColorStop(0.5, rgba(color, 0.035));
    glow.addColorStop(1, rgba(color, 0));
    context.fillStyle = glow;
    context.fillRect(-48, -48, 96, 96);

    const membrane = context.createRadialGradient(-8, -10, 1, 0, 0, 24);
    membrane.addColorStop(0, rgba(color, 0.2));
    membrane.addColorStop(0.72, rgba(color, 0.08));
    membrane.addColorStop(1, rgba(color, 0.25));
    context.beginPath();
    context.ellipse(0, 0, 21.5, 20.5, 0, 0, Math.PI * 2);
    context.fillStyle = membrane;
    context.fill();
    context.strokeStyle = rgba(color, 0.86);
    context.lineWidth = 1.5;
    context.stroke();
    context.beginPath();
    context.ellipse(0, 0, 17.6, 16.8, 0, -2.6, -0.2);
    context.strokeStyle = rgba(color, 0.2);
    context.lineWidth = 0.9;
    context.stroke();

    const nucleus = context.createRadialGradient(-3, 0, 0, -3, 0, 9);
    nucleus.addColorStop(0, rgba(color, 0.62));
    nucleus.addColorStop(0.62, rgba(color, 0.35));
    nucleus.addColorStop(1, rgba(color, 0.09));
    context.fillStyle = nucleus;
    context.beginPath();
    context.arc(-3, 0, 8, 0, Math.PI * 2);
    context.fill();
    context.strokeStyle = rgba(color, 0.45);
    context.lineWidth = 0.8;
    context.stroke();
    context.fillStyle = rgba(color, 0.95);
    context.beginPath();
    context.arc(14, 0, 2.2, 0, Math.PI * 2);
    context.fill();
    context.fillStyle = rgba(color, 0.32);
    context.beginPath();
    context.arc(5, -9, 2.1, 0, Math.PI * 2);
    context.arc(6, 9, 2.8, 0, Math.PI * 2);
    context.fill();
    this.sprites.set(color, sprite);
    return sprite;
  }

  private drawCell(cell: RenderCell, x: number, y: number, selected: boolean): void {
    const context = this.context;
    const color = genomeColor(cell.genomeId);
    const radius = cell.radius;
    const velocity = Math.hypot(cell.vx, cell.vy);
    context.save();
    context.translate(x, y);
    context.globalAlpha = 0.45 + 0.55 * clamp(cell.energy / (cell.energyCapacity * 0.45), 0, 1);
    if (velocity > 4) {
      const heading = Math.atan2(cell.vy, cell.vx);
      const tail = radius + Math.min(velocity * 0.24, 12);
      context.beginPath();
      context.moveTo(-Math.cos(heading) * radius * 0.85, -Math.sin(heading) * radius * 0.85);
      context.lineTo(-Math.cos(heading) * tail, -Math.sin(heading) * tail);
      context.strokeStyle = rgba(color, 0.25);
      context.lineWidth = 1.2;
      context.stroke();
    }
    context.rotate(cell.angle);
    if (cell.flagellumCapacity > 0) {
      const filaments = Math.min(3,cell.flagellumCount), reach = radius * (1.1 + Math.min(0.8,cell.flagellumCapacity));
      context.strokeStyle = 'rgba(141,203,217,0.65)'; context.lineWidth = 0.8;
      for (let index=0; index<filaments; index++) {
        const spread = (index-(filaments-1)/2)*radius*0.4;
        context.beginPath(); context.moveTo(-radius*0.7,spread);
        context.bezierCurveTo(-radius-reach*0.3,spread+radius*0.25,-radius-reach*0.7,spread-radius*0.25,-radius-reach,spread);
        context.stroke();
      }
    }
    if (cell.cortexCapacity > 0) {
      context.beginPath(); context.arc(0,0,radius*1.02,0,Math.PI*2);
      context.strokeStyle = 'rgba(164,175,216,0.45)'; context.lineWidth = 0.5+Math.min(2,cell.cortexMass*10); context.stroke();
    }
    const size = radius * (96 / 21.5);
    context.drawImage(this.sprite(color), -size / 2, -size / 2, size, size);
    if (cell.photosyntheticCapacity > 0) {
      context.fillStyle = 'rgba(136,211,137,0.85)';
      context.beginPath();
      context.ellipse(-radius * 0.32, -radius * 0.36, radius * 0.22, radius * 0.13, -0.5, 0, Math.PI * 2);
      context.ellipse(-radius * 0.38, radius * 0.32, radius * 0.21, radius * 0.12, 0.5, 0, Math.PI * 2);
      context.fill();
    }
    if (cell.gutEnergy > 0 && cell.gutCapacity > 0) {
      const fullness = clamp(cell.gutEnergy / cell.gutCapacity, 0, 1);
      context.fillStyle = `rgba(222,170,102,${0.35 + fullness * 0.5})`;
      context.beginPath();
      context.arc(radius * 0.15, radius * 0.08, radius * (0.12 + 0.22 * Math.sqrt(fullness)), 0, Math.PI * 2);
      context.fill();
    }
    if (cell.piercerCapacity > 0) {
      context.strokeStyle = 'rgba(235,143,129,0.85)';
      context.lineWidth = 1.1;
      context.beginPath();
      context.moveTo(radius * 0.62, -radius * 0.35);
      context.lineTo(radius * 1.05, 0);
      context.lineTo(radius * 0.62, radius * 0.35);
      context.stroke();
    }
    context.restore();
    if (this.renderOptions.colonyActivity !== false && cell.attachedNeighbors) {
      const signal = cell.contactSignal;
      if (signal !== undefined) {
        context.beginPath();
        context.arc(x, y, radius * (signal === 0 ? 0.13 : 0.32), 0, Math.PI * 2);
        context.fillStyle = signal > 0 ? '#d4a8ff' : signal < 0 ? '#f3b75d' : '#9eaebc';
        context.fill();
      }
    }
    if (cell.photosynthesisRate > 0.05) {
      context.save();
      context.strokeStyle = `rgba(239,211,127,${clamp(cell.photosynthesisRate / 12, 0.08, 0.45)})`;
      context.lineWidth = 1.5;
      context.beginPath();
      context.arc(x, y, radius + 2, 0, Math.PI * 2);
      context.stroke();
      context.restore();
    }
    if (cell.integrity < 0.98) {
      context.save();
      context.strokeStyle = 'rgba(246,131,115,0.8)';
      context.lineWidth = 1.25 / this.scale;
      context.beginPath();
      context.arc(x, y, radius + 3 / this.scale, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * (1 - clamp(cell.integrity, 0, 1)));
      context.stroke();
      context.restore();
    }
    if (selected) {
      context.save();
      context.strokeStyle = rgba(color, 0.85);
      context.lineWidth = 1 / this.scale;
      context.setLineDash([3 / this.scale, 4 / this.scale]);
      context.beginPath();
      context.arc(x, y, radius + 7 / this.scale, 0, Math.PI * 2);
      context.stroke();
      context.restore();
    }
  }

  /** Paint the junction across the two surfaces, including both images at a world seam. */
  private drawColonyBonds(world:RenderWorldSnapshot):void {
    const ctx=this.context;
    ctx.save();
    ctx.strokeStyle='#60c7ba';
    ctx.lineWidth=2.2;
    ctx.lineCap='round';
    ctx.beginPath();
    for (const [a,b] of world.bonds) {
      const x=b.x-a.x, y=b.y-a.y;
      const dx=x-Math.round(x/world.width)*world.width, dy=y-Math.round(y/world.height)*world.height;
      const contact=a.radius/(a.radius+b.radius);
      const cx=(a.x+contact*dx+world.width)%world.width, cy=(a.y+contact*dy+world.height)%world.height;
      const count=this.visibility.collect(cx,cy,Math.hypot(dx,dy)*0.3+ctx.lineWidth);
      for (let i=0;i<count;i+=2) {
        const px=this.visibility.points[i], py=this.visibility.points[i+1];
        ctx.moveTo(px-dx*contact*0.3,py-dy*contact*0.3);
        ctx.lineTo(px+dx*(1-contact)*0.3,py+dy*(1-contact)*0.3);
      }
    }
    ctx.stroke();
    ctx.restore();
  }

  /** A selected cell shows which built families it owns; these marks are not sensing ranges. */
  private drawBodyDetails(cell: Cell, x: number, y: number): void {
    const context = this.context, present = cell.organelles.filter(organ => organ.count > 0);
    if (!present.length) return;
    const radius = cell.radius + 12 / this.scale;
    context.save();
    for (let index=0; index<present.length; index++) {
      const organ = present[index], definition = ORGANELLE_CATALOG.find(item => item.kind === organ.kind)!;
      const angle = -Math.PI/2 + index/present.length*Math.PI*2;
      context.globalAlpha = organ.capacity > 0 ? 0.9 : 0.35;
      context.fillStyle = definition.color;
      context.beginPath(); context.arc(x+Math.cos(angle)*radius,y+Math.sin(angle)*radius,2/this.scale,0,Math.PI*2); context.fill();
    }
    context.restore();
  }

  /** Physical attachment and force direction from the inspected body, with arrow length showing activity. */
  private drawCellularDetails(cell:Cell,body:BodyInspection,x:number,y:number):void {
    const motors = body.cellular?.assemblies.filter(assembly=>(assembly.kind==='flagellum' || assembly.kind==='propel()') && assembly.mass>0) ?? [];
    if (!motors.length) return;
    const context = this.context;
    context.save(); context.translate(x,y); context.rotate(cell.angle);
    for (const motor of motors) {
      const px = Math.cos(motor.at)*cell.radius, py = Math.sin(motor.at)*cell.radius;
      context.globalAlpha = motor.functionalMass>0 ? .95 : .4;
      context.fillStyle = '#a3e3ef'; context.strokeStyle = '#a3e3ef'; context.lineWidth = 1/this.scale;
      context.beginPath(); context.arc(px,py,2.2/this.scale,0,Math.PI*2); context.fill();
      if (!motor.complete || motor.activity<=0 || motor.functionalMass<=0) continue;
      const length = (9+motor.activity*16)/this.scale, dx=Math.cos(motor.direction),dy=Math.sin(motor.direction);
      const tipX=px+dx*length,tipY=py+dy*length,head=4/this.scale;
      context.beginPath();context.moveTo(px,py);context.lineTo(tipX,tipY);
      context.moveTo(tipX-dx*head-dy*head*.55,tipY-dy*head+dx*head*.55);context.lineTo(tipX,tipY);
      context.lineTo(tipX-dx*head+dy*head*.55,tipY-dy*head-dx*head*.55);context.stroke();
    }
    context.restore();
  }

  private recordTrails(world: RenderWorldSnapshot): void {
    const living = this.livingTrails; living.clear();
    for (const cell of world.cells) {
      living.add(cell.id);
      const points = this.trails.get(cell.id) ?? [];
      const last = points.at(-1);
      if (!last || world.time - last.time >= 0.1) points.push({ x: cell.x, y: cell.y, time: world.time });
      while (points.length > 24 || (points.length > 0 && world.time - points[0].time > 2.4)) points.shift();
      this.trails.set(cell.id, points);
    }
    for (const id of this.trails.keys()) if (!living.has(id)) this.trails.delete(id);
  }

  private drawTrails(world: RenderWorldSnapshot): void {
    const context = this.context;
    context.save();
    context.lineWidth = 1.1 / this.scale;
    context.lineCap = 'round';
    for (const cell of world.cells) {
      const points = this.trails.get(cell.id);
      if (!points || points.length < 2) continue;
      // Keep the history while panning, but skip paths wholly outside the view.
      const margin = 1.1 / this.scale;
      const left = this.visibility.left - margin, right = this.visibility.right + margin;
      const top = this.visibility.top - margin, bottom = this.visibility.bottom + margin;
      if (!(points[0].x >= left && points[0].x <= right && points[0].y >= top && points[0].y <= bottom)) {
        let crosses = false;
        for (let i = 1; i < points.length && !crosses; i++) {
          const a = points[i - 1], b = points[i];
          crosses = Math.abs(a.x - b.x) <= world.width / 2 && Math.abs(a.y - b.y) <= world.height / 2
            && Math.max(a.x, b.x) >= left && Math.min(a.x, b.x) <= right
            && Math.max(a.y, b.y) >= top && Math.min(a.y, b.y) <= bottom;
        }
        if (!crosses) continue;
      }
      context.strokeStyle = rgba(genomeColor(cell.genomeId), 0.17);
      context.beginPath();
      context.moveTo(points[0].x, points[0].y);
      for (let i = 1; i < points.length; i += 1) {
        const point = points[i];
        const previous = points[i - 1];
        if (Math.abs(point.x - previous.x) > world.width / 2 || Math.abs(point.y - previous.y) > world.height / 2) {
          context.moveTo(point.x, point.y);
        } else {
          context.lineTo(point.x, point.y);
        }
      }
      context.stroke();
    }
    context.restore();
  }

  dispose(): void {
    this.observer.disconnect();
    this.input.abort();
    this.renderWindow.cancelAnimationFrame(this.cameraFrame);
    this.cameraFrame = 0;
    for (const id of this.pointers.keys()) if (this.canvas.hasPointerCapture(id)) this.canvas.releasePointerCapture(id);
    this.pointers.clear(); this.drag = null; this.canvas.removeAttribute('data-dragging-target');
    this.canvas.style.touchAction = this.originalTouchAction;
    this.canvas.style.cursor = this.originalCursor;
    this.trails.clear();
    this.livingTrails.clear();
    this.sprites.clear();
    this.background = null;
    this.fieldCanvas = null;
    this.fieldSamples = null; this.fieldPixels = null;
  }
}
