import type { RenderCell } from './engine';
import { createProgram } from './gpu-program';
import type { PeriodicVisibility } from './periodic-visibility';

const STRIDE = 24;
const MASK_SIZE = 96;
let cachedMask: Uint8Array<ArrayBuffer> | undefined;
const clamp01 = (value: number) => Math.max(0, Math.min(1, value));

/** The membrane, nucleus and pigment share one tint. A second texture channel holds chloroplast coverage. */
function bodyMask(): Uint8Array {
  if (cachedMask) return cachedMask;
  const pixels = new Uint8Array(MASK_SIZE * MASK_SIZE * 2);
  const ramp = (from: number, to: number, t: number) => from + (to - from) * clamp01(t);
  const over = (background: number, foreground: number) => background + foreground * (1 - background);
  function sample(x: number, y: number): number {
    const glow = (Math.hypot(x, y) - 15) / 31;
    let alpha = glow < 0.5 ? ramp(0.12, 0.035, glow * 2) : ramp(0.035, 0, glow * 2 - 1);
    const membrane = (Math.hypot(x / 21.5, y / 20.5) - 1) * 21;
    if (membrane <= 0) {
      // Intersect the point with the growing radial gradient from (-8,-10,1) to (0,0,24).
      const dx = x + 8, dy = y + 10, b = 8 * dx + 10 * dy + 23;
      const t = (Math.sqrt(Math.max(0, b * b + 365 * (dx * dx + dy * dy - 1))) - b) / 365;
      alpha = over(alpha, t < 0.72 ? ramp(0.2, 0.08, t / 0.72) : ramp(0.08, 0.25, (t - 0.72) / 0.28));
    }
    if (Math.abs(membrane) <= 0.75) alpha = over(alpha, 0.86);
    const inner = (Math.hypot(x / 17.6, y / 16.8) - 1) * 17.2;
    if (Math.abs(inner) <= 0.45) {
      const angle = Math.atan2(y / 16.8, x / 17.6);
      if (angle >= -2.6 && angle <= -0.2) alpha = over(alpha, 0.2);
    }
    const nucleus = Math.hypot(x + 3, y);
    if (nucleus <= 8) {
      const t = nucleus / 9;
      alpha = over(alpha, t < 0.62 ? ramp(0.62, 0.35, t / 0.62) : ramp(0.35, 0.09, (t - 0.62) / 0.38));
    }
    if (Math.abs(nucleus - 8) <= 0.4) alpha = over(alpha, 0.45);
    if (Math.hypot(x - 14, y) <= 2.2) alpha = over(alpha, 0.95);
    if (Math.hypot(x - 5, y + 9) <= 2.1 || Math.hypot(x - 6, y - 9) <= 2.8) alpha = over(alpha, 0.32);
    return alpha;
  }
  function chloroplast(x: number, y: number, cx: number, cy: number, rx: number, ry: number, sin: number): boolean {
    const dx = x / 21.5 - cx, dy = y / 21.5 - cy, cos = 0.87758256;
    const u = (cos * dx + sin * dy) / rx, v = (-sin * dx + cos * dy) / ry;
    return u*u + v*v <= 1;
  }
  for (let y = 0; y < MASK_SIZE; y++) {
    for (let x = 0; x < MASK_SIZE; x++) {
      let alpha = 0, green = 0;
      for (let sy = 0; sy < 4; sy++) {
        for (let sx = 0; sx < 4; sx++) {
          const px = x + (sx + 0.5) / 4 - 48, py = y + (sy + 0.5) / 4 - 48;
          alpha += sample(px, py);
          if (chloroplast(px, py, -0.32, -0.36, 0.22, 0.13, -0.47942554) ||
              chloroplast(px, py, -0.38, 0.32, 0.21, 0.12, 0.47942554)) green++;
        }
      }
      const index = 2 * (y * MASK_SIZE + x);
      pixels[index] = Math.round(alpha * 255 / 16);
      pixels[index+1] = Math.round(green * 255 / 16);
    }
  }
  cachedMask = pixels;
  return pixels;
}

const vertex = `#version 300 es
layout(location=0) in vec4 pose;       // world center, radius, quad half-extent
layout(location=1) in vec4 tint;
layout(location=2) in vec4 directions; // body heading and velocity direction
layout(location=3) in vec4 motors;     // filament count and reach, velocity tail length, cortex stroke
layout(location=4) in vec4 organs;     // gut fullness, chloroplasts, piercer, photosynthesis activity
layout(location=5) in vec4 status;     // colony signal, damage, selection
uniform vec4 transform;
uniform vec2 pixel;
uniform vec2 targetSize;
uniform bool points;
flat out vec2 origin;
flat out float radius;
flat out vec4 color, axes, motor, organ, state;
void main() {
  vec2 center = pose.xy * transform.xy + transform.zw;
  origin = (center * 0.5 + 0.5) * targetSize;
  if (points) {
    // A point is clipped by its center, even if its footprint crosses the viewport.
    vec2 safe = clamp(center, -1.0 + 1.0/targetSize, 1.0 - 1.0/targetSize);
    vec2 shiftPx = abs(safe-center) * targetSize * 0.5;
    gl_PointSize = ceil(2.0 * (pose.w / pixel.x + max(shiftPx.x,shiftPx.y)) + 2.0);
    gl_Position = vec4(safe, 0, 1);
  } else {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1)) * 2.0 - 1.0;
    gl_Position = vec4(center + corner * pose.w * transform.xy, 0, 1);
  }
  radius = pose.z;
  color = tint;
  axes = directions;
  motor = motors;
  organ = organs;
  state = status;
}`;

const fragment = `#version 300 es
precision highp float;
uniform sampler2D mask;
uniform vec2 pixel; // world units per device pixel and per CSS pixel
flat in vec2 origin;
flat in float radius;
flat in vec4 color, axes, motor, organ, state;
out vec4 outputColor;
const float TAU = 6.28318530718;
void over(inout vec4 dst, vec3 rgb, float opacity) {
  dst = dst * (1.0 - opacity) + vec4(rgb * opacity, opacity);
}
float edge(float distance) { return clamp(0.5 - distance / pixel.x, 0.0, 1.0); }
float stroke(float offset, float width) {
  return edge(offset - width * 0.5) - edge(offset + width * 0.5);
}
float segment(vec2 pos, vec2 a, vec2 b) {
  vec2 axis = b - a;
  return length(pos - a - axis * clamp(dot(pos - a, axis) / dot(axis, axis), 0.0, 1.0));
}
float filament(vec2 pos, float reach, float spread) {
  // Invert the cubic's monotone x coordinate, then evaluate the transverse bend.
  float x = -pos.x - 0.7 * radius;
  float lengthX = 0.3 * radius + reach;
  if (x < -pixel.x || x > lengthX + pixel.x) return 0.0;
  float a = 0.9 * radius + 0.9 * reach;
  float b = -0.9 * radius + 0.3 * reach;
  float c = lengthX - a - b;
  float t = clamp(x / lengthX, 0.0, 1.0);
  t = clamp(t - (((c*t+b)*t+a)*t-x) / ((3.0*c*t+2.0*b)*t+a), 0.0, 1.0);
  float y = 0.75 * radius * t * (1.0-t) * (1.0-2.0*t);
  return stroke(pos.y - spread - y, 0.8) * edge(max(-x, x-lengthX));
}
void main() {
  vec2 p = (gl_FragCoord.xy - origin) * vec2(pixel.x, -pixel.x);
  vec2 local = vec2(dot(p, axes.xy), dot(p, vec2(-axes.y, axes.x)));
  vec2 pigment = texture(mask, local * (21.5 / (96.0 * radius)) + 0.5).rg;
  const vec3 chloroplastColor = vec3(136,211,137)/255.0;
  const vec3 activityColor = vec3(239,211,127)/255.0;
  float radial = length(p);
  float membraneAlpha = pigment.r * color.a;
  float chloroplastAlpha = pigment.g * organ.y * 0.85 * color.a;
  float activityAlpha = organ.w * stroke(radial-radius-2.0, 1.5);
  if (state.w > 0.0) {
    vec4 simple = vec4(color.rgb * membraneAlpha, membraneAlpha);
    over(simple, chloroplastColor, chloroplastAlpha);
    over(simple, activityColor, activityAlpha);
    outputColor = simple;
    return;
  }
  vec4 dst = vec4(0);
  if (motor.z > 0.0) {
    float along = dot(p, -axes.zw);
    float across = dot(p, vec2(axes.w, -axes.z));
    over(dst, color.rgb, 0.25 * color.a * stroke(across, 1.2) *
      edge(max(radius * 0.85 - along, along - motor.z)));
  }
  if (motor.x > 0.0 && local.x < -0.7*radius+pixel.x && abs(local.y) < radius*0.5+pixel.x+0.4) {
    for (int i = 0; i < 3; i++) {
      if (float(i) >= motor.x) break;
      float spread = (float(i) - (motor.x-1.0)*0.5) * radius * 0.4;
      over(dst, vec3(141,203,217)/255.0, 0.65 * color.a * filament(local, motor.y, spread));
    }
  }
  if (motor.w > 0.0) over(dst, vec3(164,175,216)/255.0, 0.45 * color.a * stroke(radial-radius*1.02, motor.w));
  over(dst, color.rgb, membraneAlpha);
  over(dst, chloroplastColor, chloroplastAlpha);
  if (organ.x >= 0.0) {
    float gut = edge(length(local-radius*vec2(0.15,0.08)) - radius*(0.12+0.22*sqrt(organ.x)));
    over(dst, vec3(222,170,102)/255.0, (0.35+0.5*organ.x) * color.a * gut);
  }
  if (organ.z > 0.0) {
    float jaw = segment(vec2(local.x,abs(local.y)), radius*vec2(0.62,0.35), radius*vec2(1.05,0));
    over(dst, vec3(235,143,129)/255.0, 0.85 * color.a * edge(jaw-0.55));
  }
  if (state.x >= -1.0) {
    vec3 signalColor = state.x > 0.0 ? vec3(212,168,255) : (state.x < 0.0 ? vec3(243,183,93) : vec3(158,174,188));
    over(dst, signalColor/255.0, edge(radial-radius*(state.x==0.0 ? 0.13 : 0.32)));
  }
  over(dst, activityColor, activityAlpha);
  if (state.y > 0.0) {
    float arcRadius = radius + 3.0*pixel.y;
    float arc = stroke(radial-arcRadius, 1.25*pixel.y);
    if (arc > 0.0 && state.y < 1.0) {
      float theta = mod(atan(p.y, p.x) + 0.25*TAU + TAU, TAU);
      arc *= edge((theta-state.y*TAU)*arcRadius);
    }
    over(dst, vec3(246,131,115)/255.0, 0.8 * arc);
  }
  if (state.z > 0.0) {
    float ringRadius = radius + 7.0*pixel.y;
    float ring = stroke(radial-ringRadius, pixel.y);
    if (ring > 0.0) {
      float along = mod(atan(p.y,p.x)+TAU, TAU) * ringRadius;
      float dash = mod(along + 2.0*pixel.y, 7.0*pixel.y) - 3.5*pixel.y;
      over(dst, color.rgb, 0.85 * ring * edge(abs(dash)-1.5*pixel.y));
    }
  }
  outputColor = dst;
}`;

/** One point sprite per visible cell image, with quads for sizes beyond the device limit. The fragment shader composites the parts in their biological drawing order. */
export class GpuCells {
  private readonly program: WebGLProgram;
  private readonly vao: WebGLVertexArrayObject;
  private readonly pointVao: WebGLVertexArrayObject;
  private readonly maxPointSize: number;
  private readonly targetSize: WebGLUniformLocation | null;
  private readonly points: WebGLUniformLocation | null;
  private readonly buffer: WebGLBuffer;
  private readonly mask: WebGLTexture;
  private readonly transform: WebGLUniformLocation | null;
  private readonly pixel: WebGLUniformLocation | null;
  private readonly palette = new Map<string, number[]>();
  private instances = new Float32Array(STRIDE * 1024);
  private allocated = 0;

  constructor(private readonly gl: WebGL2RenderingContext, private readonly genomeColor: (id: number) => string) {
    this.program = createProgram(gl, vertex, fragment);
    const vao = gl.createVertexArray(), pointVao = gl.createVertexArray(), buffer = gl.createBuffer(), mask = gl.createTexture();
    if (!vao || !pointVao || !buffer || !mask) {
      gl.deleteVertexArray(vao); gl.deleteVertexArray(pointVao); gl.deleteBuffer(buffer); gl.deleteTexture(mask); gl.deleteProgram(this.program);
      throw new Error('Could not allocate the organism renderer.');
    }
    this.vao = vao; this.pointVao = pointVao;
    this.maxPointSize = gl.getParameter(gl.ALIASED_POINT_SIZE_RANGE)[1];
    this.targetSize = gl.getUniformLocation(this.program, 'targetSize');
    this.points = gl.getUniformLocation(this.program, 'points');
    this.buffer = buffer; this.mask = mask;
    this.transform = gl.getUniformLocation(this.program, 'transform');
    this.pixel = gl.getUniformLocation(this.program, 'pixel');
    for (const layout of [pointVao, vao]) {
      gl.bindVertexArray(layout);
      gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
      for (let i = 0; i < STRIDE / 4; i++) {
        gl.enableVertexAttribArray(i);
        gl.vertexAttribPointer(i, 4, gl.FLOAT, false, STRIDE * 4, i * 16);
        gl.vertexAttribDivisor(i, layout === pointVao ? 0 : 1);
      }
    }
    gl.bindVertexArray(null);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, mask);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RG8, MASK_SIZE, MASK_SIZE, 0, gl.RG, gl.UNSIGNED_BYTE, bodyMask());
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.generateMipmap(gl.TEXTURE_2D);
    gl.useProgram(this.program);
    gl.uniform1i(gl.getUniformLocation(this.program, 'mask'), 0);
  }

  draw(cells: readonly RenderCell[], selectedId: number | null, colonyActivity: boolean, transform: Float32Array,
    scale: number, visibility: PeriodicVisibility): void {
    const gl = this.gl;
    const pixel = 2 / (gl.drawingBufferWidth * transform[0]);
    let used = 0, points = true;
    for (const cell of cells) {
      const selected = cell.id === selectedId;
      const r = cell.radius;
      if (!(r > 0)) continue;
      const speed = Math.hypot(cell.vx, cell.vy);
      const tail = speed > 4 ? r + Math.min(speed * 0.24, 12) : 0;
      const filaments = cell.flagellumCapacity > 0 ? Math.min(3, cell.flagellumCount) : 0;
      const reach = filaments ? r * (1.1 + Math.min(0.8, cell.flagellumCapacity)) : 0;
      const extent = Math.max(r * (48 / 21.5), tail + 0.6, filaments ? Math.hypot(r+reach, r*0.5)+0.4 : 0,
        cell.photosynthesisRate > 0.05 ? r + 2.75 : 0, cell.integrity < 0.98 ? r + 3.625 / scale : 0,
        selected ? r + 7.5 / scale : 0) + pixel;
      const count = visibility.collect(cell.x, cell.y, extent);
      if (!count) continue;
      // A visible center can be up to one extent outside the viewport. Account for
      // that displacement and raster rounding before choosing the device's point path.
      if (4 * extent / pixel + 4 > this.maxPointSize) points = false;
      const hex = this.genomeColor(cell.genomeId);
      let rgb = this.palette.get(hex);
      if (!rgb) {
        rgb = [1, 3, 5].map(start => parseInt(hex.slice(start, start+2), 16) / 255);
        this.palette.set(hex, rgb);
      }
      const needed = used + (count / 2) * STRIDE;
      if (needed > this.instances.length) {
        const next = new Float32Array(Math.max(needed, this.instances.length * 2));
        next.set(this.instances);
        this.instances = next;
      }
      const data = this.instances;
      data[used+2] = r;
      data[used+3] = extent;
      data[used+4] = rgb[0]; data[used+5] = rgb[1]; data[used+6] = rgb[2];
      data[used+7] = 0.45 + 0.55 * clamp01(cell.energy / Math.max(Number.EPSILON, cell.energyCapacity * 0.45));
      data[used+8] = Math.cos(cell.angle); data[used+9] = Math.sin(cell.angle);
      data[used+10] = tail ? cell.vx / speed : 0; data[used+11] = tail ? cell.vy / speed : 0;
      data[used+12] = filaments; data[used+13] = reach; data[used+14] = tail;
      data[used+15] = cell.cortexCapacity > 0 ? 0.5 + Math.min(2, cell.cortexMass*10) : 0;
      data[used+16] = cell.gutEnergy > 0 && cell.gutCapacity > 0 ? clamp01(cell.gutEnergy / cell.gutCapacity) : -1;
      data[used+17] = Number(cell.photosyntheticCapacity > 0); data[used+18] = Number(cell.piercerCapacity > 0);
      data[used+19] = cell.photosynthesisRate > 0.05 ? Math.max(0.08, Math.min(0.45, cell.photosynthesisRate/12)) : 0;
      data[used+20] = colonyActivity && cell.attachedNeighbors && cell.contactSignal !== undefined ? Math.sign(cell.contactSignal) : -2;
      data[used+21] = cell.integrity < 0.98 ? 1 - clamp01(cell.integrity) : 0;
      data[used+22] = Number(selected);
      // A large producer population usually needs only the tinted body, chloroplasts and activity ring.
      data[used+23] = Number(!tail && !filaments && !cell.cortexCapacity && data[used+16] < 0 &&
        !cell.piercerCapacity && data[used+20] === -2 && !data[used+21] && !selected);
      const first = used;
      for (let image = 0; image < count; image += 2) {
        if (image) data.copyWithin(used+2, first+2, first+STRIDE);
        data[used] = visibility.points[image]; data[used+1] = visibility.points[image+1];
        used += STRIDE;
      }
    }
    if (!used) return;
    gl.useProgram(this.program);
    gl.bindVertexArray(points ? this.pointVao : this.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    if (this.allocated !== this.instances.byteLength) {
      this.allocated = this.instances.byteLength;
      gl.bufferData(gl.ARRAY_BUFFER, this.allocated, gl.DYNAMIC_DRAW);
    }
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, this.instances, 0, used);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.mask);
    gl.uniform4fv(this.transform, transform);
    gl.uniform2f(this.pixel, pixel, 1/scale);
    gl.uniform2f(this.targetSize, gl.drawingBufferWidth, gl.drawingBufferHeight);
    gl.uniform1i(this.points, Number(points));
    gl.enable(gl.BLEND);
    gl.blendEquation(gl.FUNC_ADD);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    if (points) gl.drawArrays(gl.POINTS, 0, used/STRIDE);
    else gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, used/STRIDE);
    gl.bindVertexArray(null);
  }

  dispose(): void {
    this.gl.deleteTexture(this.mask);
    this.gl.deleteBuffer(this.buffer);
    this.gl.deleteVertexArray(this.vao);
    this.gl.deleteVertexArray(this.pointVao);
    this.gl.deleteProgram(this.program);
  }
}
