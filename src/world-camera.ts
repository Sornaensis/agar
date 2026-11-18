/** A world-space camera. Zoom is relative to fitting the complete habitat. */
export class WorldCamera {
  width = 1200;
  height = 760;
  viewportWidth = 1;
  viewportHeight = 1;
  x = this.width / 2;
  y = this.height / 2;
  zoom = 1;

  get scale(): number { return Math.min(this.viewportWidth / this.width, this.viewportHeight / this.height) * this.zoom; }
  get offsetX(): number { return this.viewportWidth / 2 - this.x * this.scale; }
  get offsetY(): number { return this.viewportHeight / 2 - this.y * this.scale; }

  setViewport(width: number, height: number): void {
    this.viewportWidth = Math.max(1, width);
    this.viewportHeight = Math.max(1, height);
    this.constrain();
  }

  setWorld(width: number, height: number): void {
    if (width === this.width && height === this.height) return;
    this.width = width;
    this.height = height;
    this.fit();
  }

  fit(): void {
    this.zoom = 1;
    this.x = this.width / 2;
    this.y = this.height / 2;
  }

  point(screenX: number, screenY: number): { x: number; y: number } {
    return { x: (screenX - this.offsetX) / this.scale, y: (screenY - this.offsetY) / this.scale };
  }

  zoomAt(factor: number, screenX = this.viewportWidth / 2, screenY = this.viewportHeight / 2): void {
    if (!Number.isFinite(factor) || factor <= 0) return;
    const point = this.point(screenX, screenY);
    this.zoom = Math.max(0.25, Math.min(24, this.zoom * factor));
    this.x = point.x - (screenX - this.viewportWidth / 2) / this.scale;
    this.y = point.y - (screenY - this.viewportHeight / 2) / this.scale;
    this.constrain();
  }

  pan(screenDx: number, screenDy: number): void {
    this.x -= screenDx / this.scale;
    this.y -= screenDy / this.scale;
    this.constrain();
  }

  private constrain(): void {
    // Keep an edge visible, and center axes on which the complete habitat fits.
    const halfWidth = this.viewportWidth / this.scale / 2;
    const halfHeight = this.viewportHeight / this.scale / 2;
    this.x = halfWidth >= this.width / 2 ? this.width / 2 : Math.max(halfWidth, Math.min(this.width - halfWidth, this.x));
    this.y = halfHeight >= this.height / 2 ? this.height / 2 : Math.max(halfHeight, Math.min(this.height - halfHeight, this.y));
  }
}
