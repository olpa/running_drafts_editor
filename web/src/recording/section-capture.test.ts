import assert from "node:assert/strict";
import { afterEach, describe, it, vi } from "vitest";
import type { MicrophoneCapture, TransportFrame } from "./capture.ts";
import { createSectionCapture } from "./section-capture.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

class FakeMicrophone implements MicrophoneCapture {
  frames: ((frame: TransportFrame) => void) | null = null;
  startError: Error | null = null;
  stopped = deferred<void>();
  stops = 0;
  disposals = 0;
  async start(onFrame: (frame: TransportFrame) => void): Promise<void> {
    if (this.startError) throw this.startError;
    this.frames = onFrame;
  }
  stop(): Promise<void> {
    this.stops++;
    return this.stopped.promise;
  }
  dispose(): void {
    this.disposals++;
  }
}

afterEach(() => vi.useRealTimers());

function harness(maxRecordingMs = 5_000) {
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
    now: 0,
  });
  const microphone = new FakeMicrophone();
  let limits = 0;
  const capture = createSectionCapture({
    capture: microphone,
    now: () => Date.now(),
    maxRecordingMs,
    onLimit: () => limits++,
  });
  return { capture, microphone, limits: () => limits };
}

describe("SectionCapture", () => {
  it("is preparing, then capturing with elapsed time from the injected clock", async () => {
    const { capture } = harness();
    assert.equal(capture.state.value.kind, "preparing");
    await capture.start(() => undefined);
    const capturing = capture.state.value;
    assert.equal(capturing.kind, "capturing");
    vi.advanceTimersByTime(1_000);
    const state = capture.state.value;
    assert.deepEqual(state, { kind: "capturing", startedAt: 0, elapsedMs: 1_000 });
  });

  it("reports the duration limit once", async () => {
    const { capture, limits } = harness(2_000);
    await capture.start(() => undefined);
    vi.advanceTimersByTime(1_999);
    assert.equal(limits(), 0);
    vi.advanceTimersByTime(1);
    assert.equal(limits(), 1);
    vi.advanceTimersByTime(10_000);
    assert.equal(limits(), 1);
  });

  it("finalizes in order: state, callback, wait, ended", async () => {
    const { capture, microphone, limits } = harness(2_000);
    await capture.start(() => undefined);
    vi.advanceTimersByTime(500);
    const seen: string[] = [];
    const stopping = capture.stop("user", () => seen.push(capture.state.value.kind));
    assert.deepEqual(seen, ["finalizing"], "the callback runs once the state has changed");
    assert.equal(microphone.stops, 1);
    microphone.stopped.resolve();
    assert.deepEqual(await stopping, { outcome: "ended" });
    assert.deepEqual(capture.state.value, { kind: "ended", reason: "user", elapsedMs: 500 });
    vi.advanceTimersByTime(10_000);
    assert.equal(limits(), 0, "the duration timer is cleared");
  });

  it("does nothing when asked to stop twice or before capturing", async () => {
    const { capture, microphone } = harness();
    let calls = 0;
    assert.deepEqual(await capture.stop("user", () => calls++), { outcome: "skipped" });
    await capture.start(() => undefined);
    const first = capture.stop("user", () => calls++);
    assert.deepEqual(await capture.stop("backlog", () => calls++), { outcome: "skipped" });
    microphone.stopped.resolve();
    await first;
    assert.equal(calls, 1);
    assert.equal(microphone.stops, 1);
  });

  it("turns a failed finalization into an ended run with reason capture", async () => {
    const { capture, microphone } = harness();
    await capture.start(() => undefined);
    vi.advanceTimersByTime(300);
    const stopping = capture.stop("user", () => undefined);
    const error = new Error("finalization deadline");
    microphone.stopped.reject(error);
    assert.deepEqual(await stopping, { outcome: "failed", error });
    assert.deepEqual(capture.state.value, { kind: "ended", reason: "capture", elapsedMs: 300 });
  });

  it("releases the microphone and rethrows when starting fails", async () => {
    const { capture, microphone, limits } = harness(1_000);
    microphone.startError = new Error("recorder refused");
    await assert.rejects(() => capture.start(() => undefined), /recorder refused/);
    assert.equal(microphone.disposals, 1);
    assert.deepEqual(capture.state.value, { kind: "ended", reason: "capture", elapsedMs: 0 });
    vi.advanceTimersByTime(5_000);
    assert.equal(limits(), 0);
  });

  it("abandon and dispose release the microphone and stop the timers", async () => {
    const abandoned = harness(1_000);
    await abandoned.capture.start(() => undefined);
    abandoned.capture.abandon();
    assert.equal(abandoned.microphone.disposals, 1);
    assert.deepEqual(abandoned.capture.state.value, {
      kind: "ended",
      reason: "capture",
      elapsedMs: 0,
    });
    vi.advanceTimersByTime(5_000);
    assert.equal(abandoned.limits(), 0);

    const disposed = harness(1_000);
    await disposed.capture.start(() => undefined);
    disposed.capture.dispose();
    disposed.capture.dispose();
    assert.ok(disposed.microphone.disposals >= 1);
    vi.advanceTimersByTime(5_000);
    assert.equal(disposed.limits(), 0);
  });
});
