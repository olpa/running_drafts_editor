import { describe, expect, it } from "vitest";
import { ManualScheduler } from "../test-support/manual-scheduler.ts";
import { createFrameTiming, transportFrameTiming } from "./frame-timing.ts";

describe("FrameTiming", () => {
  it("validates explicit timing and copies it", () => {
    for (const secureDeadlineMs of [500, 1000, 1500])
      expect(transportFrameTiming({ intervalMs: 10_000, secureDeadlineMs }).secureDeadlineMs).toBe(
        secureDeadlineMs,
      );
    for (const secureDeadlineMs of [499, 1501, NaN, Infinity])
      expect(() => transportFrameTiming({ intervalMs: 10_000, secureDeadlineMs })).toThrow();
    for (const intervalMs of [0, -1, NaN, Infinity])
      expect(() => transportFrameTiming({ intervalMs, secureDeadlineMs: 1000 })).toThrow();
    const input = { intervalMs: 10_000, secureDeadlineMs: 1000 };
    const validated = transportFrameTiming(input);
    input.intervalMs = 1;
    expect(validated.intervalMs).toBe(10_000);
    expect(Object.isFrozen(validated)).toBe(true);
  });

  it("uses actual request time after drift, without a catch-up frame", async () => {
    const scheduler = new ManualScheduler();
    const ticks: number[] = [];
    const deadlines: number[] = [];
    let frame = 0;
    const timing = createFrameTiming({
      timing: { intervalMs: 10_000, secureDeadlineMs: 1000 },
      scheduler,
      onFrameTick() {
        ticks.push(scheduler.now());
        timing.markFrameRequested(++frame, scheduler.now());
      },
      onFrameDeadline: (id) => deadlines.push(id),
    });
    timing.markFrameRequested(frame, scheduler.now());
    scheduler.jumpBy(10_400);
    await scheduler.advanceBy(0);
    expect(ticks).toEqual([10_400]);
    expect(deadlines).toEqual([0]);
    await scheduler.advanceBy(1000);
    expect(deadlines).toEqual([0, 1]);
    await scheduler.advanceBy(8600);
    expect(ticks).toEqual([10_400]);
    await scheduler.advanceBy(400);
    expect(ticks).toEqual([10_400, 20_400]);
    timing.dispose();
    expect(scheduler.pendingCount).toBe(0);
  });

  it("stops regular ticks but keeps old and final-frame deadlines", async () => {
    const scheduler = new ManualScheduler();
    const deadlines: number[] = [];
    const timing = createFrameTiming({
      timing: { intervalMs: 10_000, secureDeadlineMs: 1000 },
      scheduler,
      onFrameTick: () => {
        throw new Error("Ticks should have stopped");
      },
      onFrameDeadline: (id) => deadlines.push(id),
    });
    timing.markFrameRequested(0, scheduler.now());
    timing.stopFrameTicks();
    timing.markFrameRequested(1, scheduler.now());
    await scheduler.advanceBy(10_000);
    expect(deadlines).toEqual([0, 1]);
    timing.dispose();
    timing.markFrameRequested(2, scheduler.now());
    expect(scheduler.pendingCount).toBe(0);
  });
});
