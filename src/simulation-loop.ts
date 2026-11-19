export interface SimulationWork<T> { target: T; speed: number; busy: boolean }

/** Simulation pacing is independent of display refresh and hidden-window rAF. */
export class SimulationLoop<T> {
  private running = false;
  private owner: Window | null = null;
  private timer = 0;
  private epoch = 0;
  private inFlight = false;
  private target: T | null = null;
  private speed = 0;
  private last = 0;
  private credit = 0;
  private millisecondsPerTick = -1;

  constructor(private readonly host: () => Window,
    private readonly work: () => SimulationWork<T> | null,
    private readonly advance: (target: T, steps: number, now: number) => Promise<void>,
    private readonly failed: (target: T, error: unknown) => void,
    private readonly now: () => number = () => performance.now()) {}

  start(): void { if (!this.running) { this.running = true; this.reset(); } }
  stop(): void { this.running = false; this.cancel(); this.target = null; this.credit = 0; }
  reset(): void {
    this.cancel(); this.target = null; this.credit = 0; this.last = this.now();
    this.refresh();
  }
  refresh(): void {
    if (!this.running || this.inFlight) return;
    if (this.timer && this.owner === this.host()) return;
    this.cancel(); this.schedule(0);
  }
  private cancel(): void {
    this.epoch++;
    if (this.timer) this.owner?.clearTimeout(this.timer);
    this.timer = 0; this.owner = null;
  }
  private schedule(delay: number): void {
    if (!this.running || this.inFlight) return;
    const epoch = this.epoch, owner = this.host();
    this.owner = owner;
    this.timer = owner.setTimeout(() => {
      if (epoch !== this.epoch || !this.running) return;
      this.timer = 0; this.pump();
    }, Math.max(0, delay));
  }
  private pump(): void {
    const work = this.work(), now = this.now();
    if (!work) { this.target = null; this.credit = 0; this.last = now; return; }
    if (work.target !== this.target || work.speed !== this.speed) {
      if (work.target !== this.target) this.millisecondsPerTick = -1;
      this.target = work.target; this.speed = work.speed; this.credit = 0; this.last = now;
    }
    const elapsed = Math.max(0, now - this.last); this.last = now;
    const budget = this.millisecondsPerTick >= 0 ? Math.max(1, Math.floor(12 / Math.max(.001, this.millisecondsPerTick))) : 1;
    const limit = Math.min(8, Math.ceil(work.speed), budget);
    // A suspended tab never accrues an offline backlog. Even a late timer is
    // allowed at most one bounded batch before returning to normal pacing.
    this.credit = Math.min(limit, this.credit + Math.min(elapsed, 200) * 30 * work.speed / 1000);
    if (work.busy) { this.credit = 0; this.schedule(16); return; }
    const steps = Math.min(limit, Math.floor(this.credit + 1e-9));
    if (!steps) { this.schedule(Math.max(1, (limit - this.credit) * 1000 / (30 * work.speed))); return; }
    this.credit -= steps; this.inFlight = true;
    const started = now;
    let operation: Promise<void>;
    try { operation = this.advance(work.target, steps, now); }
    catch (error) { operation = Promise.reject(error); }
    void operation.catch(error => this.failed(work.target, error)).finally(() => {
      const cost = Math.max(0, this.now() - started) / steps;
      this.millisecondsPerTick = this.millisecondsPerTick >= 0 ? this.millisecondsPerTick * .7 + cost * .3 : cost;
      this.inFlight = false;
      // A timer turn between every batch lets input, inspection, checkpoints
      // and queued edits run before another bounded command is accepted.
      this.refresh();
    });
  }
}
