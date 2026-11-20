import { GpuCells } from './gpu-cells';
import { GpuParticles } from './gpu-particles';
import type { RenderWorldSnapshot } from './session';
import type { PeriodicVisibility } from './periodic-visibility';

/** The browser composites the visible entity surface between the field and annotation canvases. */
export class GpuWorld {
  private particles: GpuParticles | null = null;
  private cells: GpuCells | null = null;
  private readonly transform = new Float32Array(4);

  private constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly gl: WebGL2RenderingContext,
    readonly annotations: CanvasRenderingContext2D,
    private readonly genomeColor: (id: number) => string,
    private readonly changed: () => void,
  ) {}

  static attach(base: HTMLCanvasElement, genomeColor: (id: number) => string, changed: () => void): GpuWorld | null {
    const canvas = base.ownerDocument.createElement('canvas');
    const overlay = base.ownerDocument.createElement('canvas');
    const annotations = overlay.getContext('2d');
    if (!annotations) return null;
    const gl = canvas.getContext('webgl2', {
      // Both entity passes filter their own edges in device pixels.
      alpha: true, premultipliedAlpha: true, antialias: false, depth: false, stencil: true,
      preserveDrawingBuffer: false, powerPreference: 'high-performance', failIfMajorPerformanceCaveat: true,
    });
    if (!gl) return null;
    // Some browsers still accept failIfMajorPerformanceCaveat with a CPU rasterizer.
    const info = gl.getExtension('WEBGL_debug_renderer_info');
    const driver = gl.getParameter(info?.UNMASKED_RENDERER_WEBGL ?? gl.RENDERER) as string;
    if (/SwiftShader|llvmpipe|softpipe|lavapipe|Microsoft Basic Render Driver|software rasterizer|software renderer|GDI Generic/i.test(driver)) {
      gl.getExtension('WEBGL_lose_context')?.loseContext();
      return null;
    }
    const surface = new GpuWorld(canvas, gl, annotations, genomeColor, changed);
    if (!surface.build()) {
      surface.dispose();
      return null;
    }
    canvas.className = 'gpu-world-entities';
    overlay.className = 'world-annotations';
    canvas.setAttribute('aria-hidden', 'true');
    overlay.setAttribute('aria-hidden', 'true');
    base.after(canvas, overlay);
    canvas.addEventListener('webglcontextlost', surface.lost);
    canvas.addEventListener('webglcontextrestored', surface.restored);
    return surface;
  }

  private build(): boolean {
    try {
      if (!this.gl.getContextAttributes()?.stencil) return false;
      this.particles = new GpuParticles(this.gl);
      this.cells = new GpuCells(this.gl, this.genomeColor);
      return true;
    } catch (error) {
      this.release();
      console.warn('WebGL entity renderer unavailable.', error);
      return false;
    }
  }

  get ready(): boolean { return this.cells !== null; }

  private lost = (event: Event): void => {
    event.preventDefault();
    // Objects from a lost context are invalid, including after restoration.
    this.cells = null;
    this.particles = null;
    this.canvas.hidden = this.annotations.canvas.hidden = true;
    this.changed();
  };

  private restored = (): void => {
    const available = this.build();
    this.canvas.hidden = this.annotations.canvas.hidden = !available;
    this.changed();
  };

  resize(width: number, height: number): void {
    for (const layer of [this.canvas, this.annotations.canvas]) {
      if (layer.width !== width) layer.width = width;
      if (layer.height !== height) layer.height = height;
    }
  }

  draw(world: RenderWorldSnapshot, selectedId: number | null, colonyActivity: boolean, visibility: PeriodicVisibility,
      scale: number, offsetX: number, offsetY: number, pixelRatio: number): void {
    if (!this.cells || !this.particles) return;
    const gl = this.gl;
    const width = gl.drawingBufferWidth, height = gl.drawingBufferHeight;
    if (!width || !height) return;
    // Match the 2D camera transform even when fractional CSS dimensions round at this DPR.
    const sx = 2 * pixelRatio / width, sy = -2 * pixelRatio / height;
    this.transform[0] = scale * sx;
    this.transform[1] = scale * sy;
    this.transform[2] = offsetX * sx - 1;
    this.transform[3] = offsetY * sy + 1;
    gl.viewport(0, 0, width, height);
    gl.disable(gl.SCISSOR_TEST);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    const left = Math.max(0, Math.round(offsetX * pixelRatio));
    const bottom = Math.max(0, height - Math.round((offsetY + world.height * scale) * pixelRatio));
    const right = Math.min(width, Math.round((offsetX + world.width * scale) * pixelRatio));
    const top = Math.min(height, height - Math.round(offsetY * pixelRatio));
    gl.enable(gl.SCISSOR_TEST);
    gl.scissor(left, bottom, Math.max(0, right - left), Math.max(0, top - bottom));
    this.particles.draw(world.food, visibility, this.transform);
    this.cells.draw(world.cells, selectedId, colonyActivity, this.transform, scale, visibility);
  }

  private release(): void {
    this.cells?.dispose();
    this.particles?.dispose();
    this.cells = null;
    this.particles = null;
  }

  dispose(): void {
    this.canvas.removeEventListener('webglcontextlost', this.lost);
    this.canvas.removeEventListener('webglcontextrestored', this.restored);
    this.release();
    this.gl.getExtension('WEBGL_lose_context')?.loseContext();
    this.annotations.canvas.remove();
    this.canvas.remove();
  }
}
