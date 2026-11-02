import type { Scheduler } from "../recording/frame-timing.ts";

export async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await Promise.resolve();
}

/** Monotonic test clock. jumpBy models an event loop that could not run timers. */
export class ManualScheduler implements Scheduler {
  private time = 0;
  private sequence = 0;
  private tasks = new Map<number, { at: number; callback: () => void }>();
  now(): number {
    return this.time;
  }
  schedule(delayMs: number, callback: () => void): () => void {
    const id = this.sequence++;
    this.tasks.set(id, { at: this.time + Math.max(0, delayMs), callback });
    return () => {
      this.tasks.delete(id);
    };
  }
  get pendingCount(): number {
    return this.tasks.size;
  }
  jumpBy(ms: number): void {
    this.time += ms;
  }
  async advanceBy(ms: number): Promise<void> {
    const target = this.time + ms;
    await settle();
    for (;;) {
      const next = [...this.tasks]
        .filter(([, task]) => task.at <= target)
        .sort((a, b) => a[1].at - b[1].at || a[0] - b[0])[0];
      if (!next) break;
      this.time = Math.max(this.time, next[1].at);
      this.tasks.delete(next[0]);
      next[1].callback();
      await settle();
    }
    this.time = target;
    await settle();
  }
}
