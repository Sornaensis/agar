/** Reusable, ordered torus images. No arrays/objects are created per entity. */
export class PeriodicVisibility {
  readonly points = new Float64Array(18);
  width = 1200;
  height = 760;
  left = 0;
  right = 1200;
  top = 0;
  bottom = 760;

  collect(x: number, y: number, margin: number): number {
    const left = this.left - margin, right = this.right + margin;
    const top = this.top - margin, bottom = this.bottom + margin;
    // Most entities can only have their central image in the habitat.
    if (x >= margin && x <= this.width - margin && y >= margin && y <= this.height - margin) {
      if (x < left || x > right || y < top || y > bottom) return 0;
      this.points[0] = x;
      this.points[1] = y;
      return 2;
    }
    let count = 0;
    // Preserve the original painter order: center, positive seam, negative seam.
    for (let ix = 0; ix < 3; ix++) {
      if (ix === 1 && x >= margin || ix === 2 && x <= this.width - margin) continue;
      const px = ix === 0 ? x : ix === 1 ? x + this.width : x - this.width;
      if (px < left || px > right) continue;
      for (let iy = 0; iy < 3; iy++) {
        if (iy === 1 && y >= margin || iy === 2 && y <= this.height - margin) continue;
        const py = iy === 0 ? y : iy === 1 ? y + this.height : y - this.height;
        if (py < top || py > bottom) continue;
        this.points[count++] = px;
        this.points[count++] = py;
      }
    }
    return count;
  }
}
