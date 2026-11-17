import type { Cell, EnvironmentSnapshot, EnvironmentSource, WorldSnapshot } from './engine';

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
export type FieldOverlay = 'none' | 'light' | 'heat' | 'flow' | 'nutrients';
interface RenderOptions { sensors?: boolean; trails?: boolean; overlay?: FieldOverlay }

/** Draws snapshots only. Rust owns every position, velocity, and biological state. */
export class Renderer {
  private readonly context: CanvasRenderingContext2D;
  private readonly observer: ResizeObserver;
  private readonly sprites = new Map<string, HTMLCanvasElement>();
  private readonly trails = new Map<number, TrailPoint[]>();
  private background: HTMLCanvasElement | null = null;
  private fieldCanvas: HTMLCanvasElement | null = null;
  private fieldEnvironment: EnvironmentSnapshot | null = null;
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

  constructor(private readonly canvas: HTMLCanvasElement) {
    const context = canvas.getContext('2d', { alpha: false });
    if (!context) throw new Error('This browser does not support the 2D simulation view.');
    this.context = context;
    this.observer = new ResizeObserver(() => this.resize());
    this.observer.observe(canvas);
    this.resize();
  }

  resize(): void {
    const rect = this.canvas.getBoundingClientRect();
    this.cssWidth = Math.max(1, rect.width);
    this.cssHeight = Math.max(1, rect.height);
    this.pixelRatio = Math.min(window.devicePixelRatio || 1, 2);
    const width = Math.max(1, Math.round(this.cssWidth * this.pixelRatio));
    const height = Math.max(1, Math.round(this.cssHeight * this.pixelRatio));
    if (this.canvas.width !== width) this.canvas.width = width;
    if (this.canvas.height !== height) this.canvas.height = height;
    this.updateTransform();
  }

  private updateTransform(): void {
    this.scale = Math.min(this.cssWidth / this.worldWidth, this.cssHeight / this.worldHeight);
    this.offsetX = (this.cssWidth - this.worldWidth * this.scale) / 2;
    this.offsetY = (this.cssHeight - this.worldHeight * this.scale) / 2;
  }

  toWorld(clientX: number, clientY: number): { x: number; y: number } | null {
    const rect = this.canvas.getBoundingClientRect();
    const x = (clientX - rect.left - this.offsetX) / this.scale;
    const y = (clientY - rect.top - this.offsetY) / this.scale;
    if (x < 0 || y < 0 || x >= this.worldWidth || y >= this.worldHeight) return null;
    return { x, y };
  }

  pickCell(world: WorldSnapshot, clientX: number, clientY: number): Cell | null {
    const position = this.toWorld(clientX, clientY);
    if (!position) return null;
    let nearest: Cell | null = null;
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

  pickSource(world: WorldSnapshot, clientX: number, clientY: number): EnvironmentSource | null {
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

  render(world: WorldSnapshot, selectedId: number | null, options: RenderOptions = {}): void {
    if (world.width !== this.worldWidth || world.height !== this.worldHeight) {
      this.worldWidth = world.width;
      this.worldHeight = world.height;
      this.background = null;
      this.updateTransform();
    }
    if (this.pixelRatio !== Math.min(window.devicePixelRatio || 1, 2)) this.resize();
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
    context.drawImage(this.getBackground(), 0, 0, this.worldWidth, this.worldHeight);
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

    const selected = world.cells.find((cell) => cell.id === selectedId);
    if (selected && options.sensors !== false) {
      for (const point of this.wrappedPositions(selected.x, selected.y, selected.senseRange)) {
        this.drawSensors(selected, point.x, point.y);
      }
    }

    // A single path for each food layer avoids hundreds of shadows or gradients.
    context.fillStyle = 'rgba(241,173,89,0.07)';
    context.beginPath();
    for (const food of world.food) {
      for (const point of this.wrappedPositions(food.x, food.y, 5)) {
        context.moveTo(point.x + 5, point.y);
        context.arc(point.x, point.y, 5, 0, Math.PI * 2);
      }
    }
    context.fill();
    context.fillStyle = 'rgba(237,179,108,0.82)';
    context.beginPath();
    for (const food of world.food) {
      const radius = 1.4 + clamp(Math.max(food.energy / 40, food.nutrients), 0, 1) * 0.7;
      for (const point of this.wrappedPositions(food.x, food.y, radius)) {
        context.moveTo(point.x + radius, point.y);
        context.arc(point.x, point.y, radius, 0, Math.PI * 2);
      }
    }
    context.fill();

    for (const cell of world.cells) {
      const margin = Math.max(cell.radius * 2.4 + 5, cell.id === selectedId ? cell.radius + 8 / this.scale : 0);
      for (const point of this.wrappedPositions(cell.x, cell.y, margin)) {
        this.drawCell(cell, point.x, point.y, cell.id === selectedId);
      }
    }
    this.drawSources(world.environment.sources, overlay);
    context.restore();
  }

  private drawEnvironment(environment: EnvironmentSnapshot, mode: FieldOverlay): void {
    const { columns, rows, samples } = environment;
    if (!this.fieldCanvas) this.fieldCanvas = document.createElement('canvas');
    if (this.fieldEnvironment !== environment || this.fieldMode !== mode) {
      this.fieldCanvas.width = columns;
      this.fieldCanvas.height = rows;
      const field = this.fieldCanvas.getContext('2d')!;
      const pixels = field.createImageData(columns, rows);
      for (let index = 0; index < columns * rows; index += 1) {
        const offset = index * 4;
        const sample = index * 5;
        if (mode === 'light') {
          pixels.data[offset] = 245; pixels.data[offset + 1] = 203; pixels.data[offset + 2] = 102;
          pixels.data[offset + 3] = Math.round(clamp(samples[sample], 0, 1) * 78);
        } else if (mode === 'heat') {
          const difference = samples[sample + 1] - 0.5;
          pixels.data[offset] = difference >= 0 ? 242 : 73;
          pixels.data[offset + 1] = difference >= 0 ? 120 : 155;
          pixels.data[offset + 2] = difference >= 0 ? 84 : 228;
          pixels.data[offset + 3] = Math.round(clamp(Math.abs(difference) * 2, 0, 1) * 108);
        } else if (mode === 'nutrients') {
          const level = Math.max(0, samples[sample + 4]);
          pixels.data[offset] = 104; pixels.data[offset + 1] = 215; pixels.data[offset + 2] = 157;
          // Fixed scale: a fading patch always means depletion, independent of other patches.
          pixels.data[offset + 3] = Math.round(level / (level + 0.5) * 145);
        } else {
          pixels.data[offset] = 102; pixels.data[offset + 1] = 182; pixels.data[offset + 2] = 224;
          pixels.data[offset + 3] = Math.round(clamp(Math.hypot(samples[sample + 2], samples[sample + 3]) / 30, 0, 1) * 60);
        }
      }
      field.putImageData(pixels, 0, 0);
      this.fieldEnvironment = environment;
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
      for (let row = 1; row < rows; row += 3) {
        for (let column = 1; column < columns; column += 3) {
          const offset = (row * columns + column) * 5;
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
    for (const source of sources) {
      const color = source.kind === 0 ? '#efd089' : source.kind === 1 ? (source.strength >= 0 ? '#ed9c82' : '#83c5eb') : source.kind === 2 ? '#b6acf5' : '#81d99d';
      const label = source.kind === 0 ? 'L' : source.kind === 1 ? (source.strength >= 0 ? 'H' : 'C') : source.kind === 2 ? 'V' : 'N';
      const showRadius = (source.kind === 0 && overlay === 'light') || (source.kind === 1 && overlay === 'heat') || (source.kind === 2 && overlay === 'flow') || (source.kind === 3 && overlay === 'nutrients');
      for (const point of this.wrappedPositions(source.x, source.y, showRadius ? source.radius : 12 / this.scale)) {
        if (showRadius) {
          context.lineWidth = 0.7 / this.scale;
          context.strokeStyle = rgba(color, 0.22);
          context.setLineDash([3 / this.scale, 6 / this.scale]);
          context.beginPath();
          context.arc(point.x, point.y, source.radius, 0, Math.PI * 2);
          context.stroke();
        }
        context.setLineDash([]);
        context.beginPath();
        context.arc(point.x, point.y, 8 / this.scale, 0, Math.PI * 2);
        context.fillStyle = '#142329';
        context.fill();
        context.lineWidth = 1 / this.scale;
        context.strokeStyle = rgba(color, 0.8);
        context.stroke();
        context.fillStyle = color;
        context.font = `600 ${8 / this.scale}px ui-monospace, monospace`;
        context.textAlign = 'center';
        context.textBaseline = 'middle';
        context.fillText(label, point.x, point.y + 0.3 / this.scale);
      }
    }
    context.restore();
  }

  private getBackground(): HTMLCanvasElement {
    if (this.background) return this.background;
    const background = document.createElement('canvas');
    background.width = Math.ceil(this.worldWidth);
    background.height = Math.ceil(this.worldHeight);
    const context = background.getContext('2d')!;
    const gradient = context.createRadialGradient(this.worldWidth * 0.4, this.worldHeight * 0.4, 0,
      this.worldWidth * 0.5, this.worldHeight * 0.5, this.worldWidth * 0.7);
    gradient.addColorStop(0, '#10231f');
    gradient.addColorStop(0.6, '#0b1a1b');
    gradient.addColorStop(1, '#091317');
    context.fillStyle = gradient;
    context.fillRect(0, 0, background.width, background.height);
    context.fillStyle = 'rgba(128,170,161,0.18)';
    context.beginPath();
    for (let x = 20; x < this.worldWidth; x += 40) {
      for (let y = 20; y < this.worldHeight; y += 40) {
        context.moveTo(x + 0.65, y);
        context.arc(x, y, 0.65, 0, Math.PI * 2);
      }
    }
    context.fill();
    context.strokeStyle = 'rgba(112,164,154,0.08)';
    context.lineWidth = 1;
    context.strokeRect(0.5, 0.5, this.worldWidth - 1, this.worldHeight - 1);
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

  private drawCell(cell: Cell, x: number, y: number, selected: boolean): void {
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
    const size = radius * (96 / 21.5);
    context.drawImage(this.sprite(color), -size / 2, -size / 2, size, size);
    if (cell.bodyPhotosynthesis >= 0.05) {
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
    if (cell.bodyAttack >= 0.05) {
      context.strokeStyle = 'rgba(235,143,129,0.85)';
      context.lineWidth = 1.1;
      context.beginPath();
      context.moveTo(radius * 0.62, -radius * 0.35);
      context.lineTo(radius * 1.05, 0);
      context.lineTo(radius * 0.62, radius * 0.35);
      context.stroke();
    }
    context.restore();
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

  private drawSensors(cell: Cell, x: number, y: number): void {
    const context = this.context;
    const color = genomeColor(cell.genomeId);
    context.save();
    context.fillStyle = rgba(color, 0.025);
    context.strokeStyle = rgba(color, 0.23);
    context.lineWidth = 1 / this.scale;
    context.setLineDash([4 / this.scale, 7 / this.scale]);
    context.beginPath();
    context.arc(x, y, cell.senseRange, 0, Math.PI * 2);
    context.fill();
    context.stroke();
    context.setLineDash([]);
    context.beginPath();
    context.moveTo(x + Math.cos(cell.angle) * (cell.radius + 10), y + Math.sin(cell.angle) * (cell.radius + 10));
    context.lineTo(x + Math.cos(cell.angle) * (cell.radius + 24), y + Math.sin(cell.angle) * (cell.radius + 24));
    context.strokeStyle = rgba(color, 0.5);
    context.stroke();
    const sensor = (angle: number, distance: number, stroke: string, dashed: boolean): void => {
      const tx = x + Math.cos(cell.angle + angle) * distance;
      const ty = y + Math.sin(cell.angle + angle) * distance;
      context.strokeStyle = stroke;
      context.lineWidth = 1 / this.scale;
      context.setLineDash(dashed ? [3 / this.scale, 5 / this.scale] : []);
      context.beginPath();
      context.moveTo(x, y);
      context.lineTo(tx, ty);
      context.stroke();
      context.setLineDash([]);
      context.beginPath();
      context.arc(tx, ty, 5 / this.scale, 0, Math.PI * 2);
      context.stroke();
    };
    if (cell.foodVisible) sensor(cell.foodAngle, cell.foodDistance, 'rgba(241,180,103,0.68)', false);
    if (cell.neighborVisible) sensor(cell.neighborAngle, cell.neighborDistance, 'rgba(172,161,238,0.4)', true);
    if (cell.preyVisible) sensor(cell.preyAngle, cell.preyDistance, 'rgba(238,165,114,0.5)', true);
    if (cell.threatVisible) sensor(cell.threatAngle, cell.threatDistance, 'rgba(242,126,114,0.65)', true);
    context.restore();
  }

  private wrappedPositions(x: number, y: number, margin: number): { x: number; y: number }[] {
    const xs = [x];
    const ys = [y];
    if (x < margin) xs.push(x + this.worldWidth);
    if (x > this.worldWidth - margin) xs.push(x - this.worldWidth);
    if (y < margin) ys.push(y + this.worldHeight);
    if (y > this.worldHeight - margin) ys.push(y - this.worldHeight);
    return xs.flatMap((px) => ys.map((py) => ({ x: px, y: py })));
  }

  private recordTrails(world: WorldSnapshot): void {
    const living = new Set<number>();
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

  private drawTrails(world: WorldSnapshot): void {
    const context = this.context;
    context.save();
    context.lineWidth = 1.1 / this.scale;
    context.lineCap = 'round';
    for (const cell of world.cells) {
      const points = this.trails.get(cell.id);
      if (!points || points.length < 2) continue;
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
    this.trails.clear();
    this.sprites.clear();
    this.background = null;
    this.fieldCanvas = null;
    this.fieldEnvironment = null;
  }
}
