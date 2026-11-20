import type { Food, FrameParticles } from './engine';
import { createProgram } from './gpu-program';
import type { PeriodicVisibility } from './periodic-visibility';

const VERTEX = `#version 300 es
layout(location = 0) in vec4 particle; // Center, core radius, halo radius
uniform vec4 camera;
uniform vec2 viewport;
uniform float pixelsPerUnit;
uniform int shape;
uniform bool halo;
uniform bool quads;
flat out vec2 center;
flat out float radius;
void main() {
  center = ((particle.xy * camera.xy + camera.zw) * 0.5 + 0.5) * viewport;
  radius = (halo ? particle.w : particle.z) * pixelsPerUnit;
  float halfSize = radius + (shape == 3 ? 0.3 * pixelsPerUnit : 0.0) + 1.0;
  vec2 position;
  if (quads) {
    vec2 corner = vec2(float(gl_VertexID % 2), float(gl_VertexID / 2)) * 2.0 - 1.0;
    position = center + halfSize * corner;
    gl_PointSize = 1.0;
  } else {
    // A point whose vertex falls outside clip space can disappear even when
    // part of its disk is visible. Keep its square over the true shape.
    position = clamp(center, vec2(0.5), viewport - 0.5);
    vec2 shift = abs(position - center);
    gl_PointSize = ceil(2.0 * (halfSize + max(shift.x, shift.y)));
  }
  gl_Position = vec4(position * 2.0 / viewport - 1.0, 0.0, 1.0);
}`;

const FRAGMENT = `#version 300 es
precision highp float;
precision highp int;
uniform vec4 pigment;
uniform int shape;
uniform float pixelsPerUnit;
flat in vec2 center;
flat in float radius;
out vec4 color;
void main() {
  vec2 delta = abs(gl_FragCoord.xy - center);
  float coverage;
  if (shape == 1) {
    coverage = 0.5 + (radius - delta.x - delta.y) * 0.70710678;
  } else if (shape == 2) {
    vec2 edge = clamp(vec2(radius * 0.7 + 0.5) - delta, 0.0, 1.0);
    coverage = edge.x * edge.y;
  } else {
    float distance = length(delta);
    if (shape == 3) {
      float halfStroke = 0.3 * pixelsPerUnit;
      float outer = clamp(radius + halfStroke + 0.5 - distance, 0.0, 1.0);
      float inner = clamp(radius - halfStroke + 0.5 - distance, 0.0, 1.0);
      coverage = outer - inner;
    } else {
      coverage = radius + 0.5 - distance;
    }
  }
  if (coverage <= 0.0) discard;
  color = vec4(pigment.rgb, pigment.a * min(coverage, 1.0));
}`;

const MATERIALS = [
  { rgb: [237, 198, 108], core: 0.86, halo: 0.07 },
  { rgb: [225, 137, 122], core: 0.90, halo: 0.08 },
  { rgb: [158, 151, 130], core: 0.72, halo: 0.035 },
];
const LIGAND = [191, 155, 234];
const STRIDE = 4;

class InstanceBatch {
  private data = new Float32Array(STRIDE * 256);
  private gpuBytes = 0;
  count = 0;

  constructor(readonly buffer: WebGLBuffer) {}

  append(x: number, y: number, core: number, halo: number): void {
    const offset = STRIDE * this.count++;
    if (offset === this.data.length) {
      const larger = new Float32Array(this.data.length * 2);
      larger.set(this.data);
      this.data = larger;
    }
    this.data[offset] = x;
    this.data[offset + 1] = y;
    this.data[offset + 2] = core;
    this.data[offset + 3] = halo;
  }

  upload(gl: WebGL2RenderingContext): void {
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    if (this.gpuBytes !== this.data.byteLength) {
      this.gpuBytes = this.data.byteLength;
      gl.bufferData(gl.ARRAY_BUFFER, this.gpuBytes, gl.STREAM_DRAW);
    }
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, this.data, 0, STRIDE * this.count);
    gl.vertexAttribPointer(0, STRIDE, gl.FLOAT, false, 0, 0);
  }
}

/** Draw into the caller's visible framebuffer and habitat scissor. */
export class GpuParticles {
  private readonly program: WebGLProgram;
  private readonly vao: WebGLVertexArrayObject;
  private readonly maxPointSize: number;
  private readonly batches: InstanceBatch[] = [];
  private readonly viewport: WebGLUniformLocation | null;
  private readonly pixelsPerUnit: WebGLUniformLocation | null;
  private readonly shape: WebGLUniformLocation | null;
  private readonly quads: WebGLUniformLocation | null;
  private readonly camera: WebGLUniformLocation | null;
  private readonly halo: WebGLUniformLocation | null;
  private readonly pigment: WebGLUniformLocation | null;
  private readonly row: Food = { x: 0, y: 0, energy: 0, nutrients: 0, kind: 0, ligandFraction: 0 };

  constructor(private readonly gl: WebGL2RenderingContext) {
    if (gl.getParameter(gl.STENCIL_BITS) < 3) throw new Error('Particle rendering requires a stencil buffer.');
    this.program = createProgram(gl, VERTEX, FRAGMENT);
    this.maxPointSize = gl.getParameter(gl.ALIASED_POINT_SIZE_RANGE)[1];
    const vao = gl.createVertexArray();
    try {
      if (!vao) throw new Error('Could not allocate particle geometry.');
      this.vao = vao;
      this.camera = gl.getUniformLocation(this.program, 'camera');
      this.halo = gl.getUniformLocation(this.program, 'halo');
      this.viewport = gl.getUniformLocation(this.program, 'viewport');
      this.pixelsPerUnit = gl.getUniformLocation(this.program, 'pixelsPerUnit');
      this.shape = gl.getUniformLocation(this.program, 'shape');
      this.quads = gl.getUniformLocation(this.program, 'quads');
      this.pigment = gl.getUniformLocation(this.program, 'pigment');
      gl.bindVertexArray(vao);
      gl.enableVertexAttribArray(0);
      // One stream per material, plus a smaller stream for ligand-bearing particles.
      for (let index = 0; index <= MATERIALS.length; index++) {
        const buffer = gl.createBuffer();
        if (!buffer) throw new Error('Could not allocate particle instances.');
        this.batches.push(new InstanceBatch(buffer));
      }
    } catch (error) {
      this.batches.forEach(batch => gl.deleteBuffer(batch.buffer));
      gl.deleteVertexArray(vao);
      gl.deleteProgram(this.program);
      throw error;
    } finally {
      gl.bindVertexArray(null);
      gl.bindBuffer(gl.ARRAY_BUFFER, null);
    }
  }

  draw(particles: FrameParticles, visibility: PeriodicVisibility, transform: readonly [number, number, number, number] | Float32Array): void {
    if (!particles.length) return;
    for (const batch of this.batches) batch.count = 0;
    const rings = this.batches[MATERIALS.length];
    for (let i = 0; i < particles.length; i++) {
      const food = Array.isArray(particles) ? particles[i] : particles.read(i, this.row);
      const count = visibility.collect(food.x, food.y, 6);
      if (!count) continue;
      const fullness = Math.sqrt(Math.max(0, Math.min(1, food.energy / 24)));
      const core = 0.25 + (food.kind === 1 ? 2.25 : 1.65) * fullness;
      const halo = 0.5 + 4.5 * fullness;
      const ring = food.ligandFraction > 0 ? core + 0.65 + 1.5 * food.ligandFraction : 0;
      const batch = this.batches[food.kind];
      for (let j = 0; j < count; j += 2) {
        const x = visibility.points[j], y = visibility.points[j + 1];
        batch.append(x, y, core, halo);
        if (ring > 0) rings.append(x, y, ring, 0);
      }
    }

    const gl = this.gl;
    gl.bindVertexArray(this.vao);
    gl.useProgram(this.program);
    gl.uniform4f(this.camera, transform[0], transform[1], transform[2], transform[3]);
    const scale = Math.abs(transform[0]) * gl.drawingBufferWidth / 2;
    // The 6-unit visibility margin bounds how far a visible center can lie
    // offscreen. Oversize marks use the same shader on four instanced corners.
    const quads = Math.ceil(2 * (11 * scale + 2)) > this.maxPointSize;
    gl.uniform2f(this.viewport, gl.drawingBufferWidth, gl.drawingBufferHeight);
    gl.uniform1f(this.pixelsPerUnit, scale);
    gl.uniform1i(this.quads, quads ? 1 : 0);
    gl.vertexAttribDivisor(0, quads ? 1 : 0);
    gl.enable(gl.BLEND);
    gl.blendEquation(gl.FUNC_ADD);
    gl.blendFuncSeparate(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    // Each Canvas material is a compound path. Stencil prevents dense stacks
    // from repeatedly adding the same material opacity.
    gl.enable(gl.STENCIL_TEST);
    gl.stencilMask(0xff);
    gl.clearStencil(0);
    gl.clear(gl.STENCIL_BUFFER_BIT);
    gl.stencilOp(gl.KEEP, gl.KEEP, gl.REPLACE);

    for (let kind = 0; kind < MATERIALS.length; kind++) {
      const batch = this.batches[kind];
      if (!batch.count) continue;
      batch.upload(gl);
      const material = MATERIALS[kind];
      this.drawLayer(0, batch.count, material.rgb, material.halo, true, 2 * kind + 1, quads);
      this.drawLayer(kind, batch.count, material.rgb, material.core, false, 2 * kind + 2, quads);
    }
    if (rings.count) {
      rings.upload(gl);
      this.drawLayer(3, rings.count, LIGAND, 0.72, false, 7, quads);
    }
    gl.disable(gl.STENCIL_TEST);
    gl.bindVertexArray(null);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);
  }

  private drawLayer(shape: number, count: number, rgb: number[], opacity: number, halo: boolean, layer: number, quads: boolean): void {
    const gl = this.gl;
    gl.uniform1i(this.shape, shape);
    gl.uniform1i(this.halo, halo ? 1 : 0);
    gl.uniform4f(this.pigment, rgb[0] / 255, rgb[1] / 255, rgb[2] / 255, opacity);
    gl.stencilFunc(gl.NOTEQUAL, layer, 0xff);
    if (quads) gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, count);
    else gl.drawArrays(gl.POINTS, 0, count);
  }

  dispose(): void {
    const gl = this.gl;
    for (const batch of this.batches) gl.deleteBuffer(batch.buffer);
    gl.deleteVertexArray(this.vao);
    gl.deleteProgram(this.program);
  }
}
