export interface Scheduler {
  now(): number;
  schedule(delayMs: number, callback: () => void): () => void;
}

export const browserScheduler: Scheduler = {
  now: () => performance.now(),
  schedule(delayMs, callback) {
    const timer = setTimeout(callback, Math.max(0, delayMs));
    return () => clearTimeout(timer);
  },
};

export interface TransportFrameTiming {
  intervalMs: number;
  secureDeadlineMs: number;
}

export function transportFrameTiming(value: TransportFrameTiming): Readonly<TransportFrameTiming> {
  if (!Number.isFinite(value.intervalMs) || value.intervalMs <= 0)
    throw new Error("Transport frame interval must be positive and finite.");
  if (
    !Number.isFinite(value.secureDeadlineMs) ||
    value.secureDeadlineMs < 500 ||
    value.secureDeadlineMs > 1500
  )
    throw new Error("Transport frame securing deadline must be between 500 and 1500 ms.");
  return Object.freeze({ ...value });
}

export interface FrameTiming {
  markFrameRequested(frameId: number, atMs: number): void;
  stopFrameTicks(): void;
  dispose(): void;
}

export function createFrameTiming(options: {
  timing: TransportFrameTiming;
  scheduler: Scheduler;
  onFrameTick: () => void;
  onFrameDeadline: (frameId: number) => void;
}): FrameTiming {
  const timing = transportFrameTiming(options.timing);
  const { scheduler } = options;
  const pending = new Set<() => void>();
  let tick: (() => void) | null = null;
  let stopped = false;
  let disposed = false;
  function schedule(at: number, callback: () => void): () => void {
    const cancel = scheduler.schedule(at - scheduler.now(), () => {
      pending.delete(cancel);
      if (!disposed) callback();
    });
    pending.add(cancel);
    return cancel;
  }
  return {
    markFrameRequested(frameId, atMs) {
      if (disposed) return;
      schedule(atMs + timing.secureDeadlineMs, () => options.onFrameDeadline(frameId));
      if (stopped) return;
      tick?.();
      if (tick) pending.delete(tick);
      tick = schedule(atMs + timing.intervalMs, () => {
        tick = null;
        options.onFrameTick();
      });
    },
    stopFrameTicks() {
      stopped = true;
      tick?.();
      if (tick) pending.delete(tick);
      tick = null;
    },
    dispose() {
      disposed = true;
      for (const cancel of pending) cancel();
      pending.clear();
    },
  };
}
