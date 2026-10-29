import assert from "node:assert/strict";
import { afterEach, describe, it, vi } from "vitest";
import { ApiError } from "../shared/http.ts";
import type { AudioAssetSummary } from "../shared/project-types.ts";
import type { TransportFrame } from "./capture.ts";
import type { FrameAck } from "./gateway.ts";
import { createSectionDelivery, type SectionDelivery } from "./section-delivery.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
const settle = async () => {
  for (let i = 0; i < 16; i++) await Promise.resolve();
};
const frame = (seq: number, bytes = 4): TransportFrame => ({
  seq,
  startMs: seq * 1_000,
  endMs: (seq + 1) * 1_000,
  mediaType: "audio/webm;codecs=opus",
  data: new Blob([new Uint8Array(bytes)]),
});
const ack = (sequence: number): FrameAck => ({
  sequence,
  acknowledged: true,
  sha256: "a".repeat(64),
  nextExpectedSequence: sequence + 1,
});
const sealed = (reason: "completed" | "interrupted"): AudioAssetSummary => ({
  id: "asset",
  mediaType: "audio/webm;codecs=opus",
  isSealed: true,
  sealReason: reason,
  sealedAt: "now",
  frameCount: 0,
  observedDurationMs: 0,
  storedBytes: 0,
});

const disposables: SectionDelivery[] = [];
afterEach(() => {
  while (disposables.length) disposables.pop()!.dispose();
  vi.useRealTimers();
});

function harness(options: { maxFrameBytes?: number } = {}) {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval"] });
  const sent: {
    frame: TransportFrame;
    signal: AbortSignal;
    result: ReturnType<typeof deferred<FrameAck>>;
  }[] = [];
  const calls = {
    complete: [] as number[],
    interrupt: 0,
    stops: [] as string[],
    cues: [] as string[],
  };
  const logs: string[] = [];
  const state = { completeFails: 0, completeResult: sealed("completed") };
  const delivery = createSectionDelivery({
    audioAssetId: "asset",
    retryDelaysMs: [5_000, 10_000, 17_000],
    requestTimeoutMs: 60_000,
    maxFrameBytes: options.maxFrameBytes ?? 1_000,
    gateway: {
      putFrame: (_id, item, signal) => {
        const result = deferred<FrameAck>();
        sent.push({ frame: item, signal, result });
        return result.promise;
      },
      complete: async (_id, frameCount) => {
        calls.complete.push(frameCount);
        if (state.completeFails-- > 0) throw new Error("complete unavailable");
        return state.completeResult;
      },
      interrupt: async () => {
        calls.interrupt++;
        return sealed("interrupted");
      },
    },
    now: () => 0,
    log: (message) => logs.push(message),
    onAcknowledged: () => calls.cues.push("health"),
    onDelayed: () => calls.cues.push("delayed"),
    requestCaptureStop: (reason) => calls.stops.push(reason),
  });
  disposables.push(delivery);
  return { delivery, sent, calls, logs, state };
}

describe("SectionDelivery", () => {
  it("sends in order, tracks secured audio, and completes with the assigned frame count", async () => {
    const { delivery, sent, calls } = harness();
    delivery.accept(frame(0));
    delivery.accept(frame(1));
    assert.equal(delivery.view.value.pendingFrames, 2);
    assert.equal(sent.length, 1, "one frame in flight at a time");

    delivery.captureStopping("user");
    assert.equal(delivery.view.value.completion, "pending");
    delivery.captureEnded("user");
    sent[0]!.result.resolve(ack(0));
    await settle();
    assert.equal(sent.length, 2);
    assert.equal(delivery.view.value.securedMs, 1_000);
    assert.deepEqual(calls.complete, [], "a frame is still pending");

    sent[1]!.result.resolve(ack(1));
    await settle();
    assert.deepEqual(calls.complete, [2]);
    assert.equal(delivery.view.value.completion, "succeeded");
    assert.equal(delivery.view.value.awaitingSeal, false);
    assert.deepEqual(calls.cues, ["health", "health"]);
  });

  it("reports the backlog limit", () => {
    const { delivery } = harness();
    delivery.accept(frame(0));
    assert.equal(delivery.backlogged, false);
    delivery.accept(frame(1));
    assert.equal(delivery.backlogged, true);
  });

  it("keeps an oversize frame, blocks sending, and asks capture to stop", () => {
    const { delivery, sent, calls } = harness({ maxFrameBytes: 8 });
    delivery.accept(frame(0, 9));
    assert.equal(sent.length, 0);
    assert.deepEqual(calls.stops, ["oversize"]);
    assert.match(delivery.view.value.problem, /too large to upload/);
    delivery.captureStopping("oversize");
    delivery.captureEnded("oversize");
    assert.equal(delivery.view.value.pendingFrames, 1);
    assert.equal(delivery.view.value.canUploadPending, false);
    delivery.requestUpload();
    assert.equal(sent.length, 0, "recovery must enforce the same guard");
  });

  it("treats a frame conflict as terminal and keeps every byte", async () => {
    const { delivery, sent, calls, logs } = harness();
    delivery.accept(frame(0));
    sent[0]!.result.reject(
      new ApiError("PUT", "/frames/0", 409, {
        title: "Frame conflict",
        code: "frame_conflict",
        referenceId: "ref-1",
      }),
    );
    await settle();
    assert.deepEqual(calls.stops, ["conflict"]);
    assert.match(delivery.view.value.problem, /Title: Frame conflict/);
    assert.match(delivery.view.value.problem, /Reference ID: ref-1/);
    assert.ok(logs.some((line) => line.includes("HTTP 409 code=frame_conflict referenceId=ref-1")));

    delivery.captureStopping("conflict");
    delivery.captureEnded("conflict");
    delivery.requestUpload();
    await settle();
    assert.equal(sent.length, 1);
    assert.equal(delivery.view.value.pendingFrames, 1);
    assert.equal(delivery.view.value.canUploadPending, false);
    assert.equal(calls.interrupt, 0);
  });

  it("freezes an interrupted Section until the user asks, then interrupts it", async () => {
    const { delivery, sent, calls } = harness();
    delivery.accept(frame(0));
    delivery.accept(frame(1));
    delivery.captureStopping("backlog");
    delivery.captureEnded("backlog");
    assert.match(delivery.view.value.problem, /could not be uploaded/);
    assert.equal(delivery.view.value.completion, null);
    assert.equal(delivery.view.value.canUploadPending, true);
    vi.advanceTimersByTime(60_000);
    assert.equal(sent.length, 1, "no automatic retry while waiting for the user");

    delivery.requestUpload();
    assert.equal(delivery.view.value.problem, "");
    sent.at(-1)!.result.resolve(ack(0));
    await settle();
    sent.at(-1)!.result.resolve(ack(1));
    await settle();
    assert.equal(calls.interrupt, 1);
    assert.deepEqual(calls.complete, []);
  });

  it("an interruption with nothing pending is interrupted at once", async () => {
    const { delivery, calls } = harness();
    delivery.captureStopping("capture");
    delivery.captureEnded("capture");
    await delivery.closeIfReady();
    assert.equal(calls.interrupt, 1);
  });

  it("suspension stops sending and closing; resume restarts both", async () => {
    const { delivery, sent, calls } = harness();
    delivery.accept(frame(0));
    delivery.suspend();
    vi.advanceTimersByTime(30_000);
    assert.equal(sent.length, 1);
    delivery.captureStopping("user");
    delivery.captureEnded("user");
    sent[0]!.result.resolve(ack(0));
    await settle();
    assert.deepEqual(
      calls.complete,
      [],
      "a late acknowledgement frees the queue but not the close",
    );

    delivery.resume();
    await settle();
    assert.deepEqual(calls.complete, [1]);
  });

  it("a Section whose capture never began is interrupted only after resume", async () => {
    const { delivery, calls } = harness();
    delivery.suspend();
    delivery.captureNeverStarted("Capture did not start.");
    await delivery.closeIfReady();
    assert.equal(calls.interrupt, 0);
    delivery.resume();
    await settle();
    assert.equal(calls.interrupt, 1);
    assert.equal(delivery.view.value.problem, "");
  });

  it("keeps a failed completion retryable", async () => {
    const { delivery, calls, state } = harness();
    state.completeFails = 1;
    delivery.captureStopping("user");
    delivery.captureEnded("user");
    await delivery.closeIfReady();
    assert.equal(delivery.view.value.completion, "failed");
    assert.equal(delivery.view.value.completionProblem, "Could not complete recording.");
    assert.equal(delivery.view.value.canRetryCompletion, true);

    delivery.retryClose();
    await settle();
    assert.equal(delivery.view.value.completion, "succeeded");
    assert.deepEqual(calls.complete, [0, 0]);
  });

  it("a sealed non-completed answer is a terminal completion failure", async () => {
    const { delivery, state } = harness();
    state.completeResult = sealed("interrupted");
    delivery.captureStopping("user");
    delivery.captureEnded("user");
    await delivery.closeIfReady();
    assert.equal(delivery.view.value.completion, "failed");
    assert.equal(delivery.view.value.canRetryCompletion, false);
    assert.equal(delivery.view.value.awaitingSeal, false);
  });

  it("does nothing after dispose", async () => {
    const { delivery, sent, calls } = harness();
    delivery.accept(frame(0));
    delivery.dispose();
    delivery.captureStopping("user");
    delivery.captureEnded("user");
    sent[0]!.result.resolve(ack(0));
    await settle();
    await delivery.closeIfReady();
    assert.deepEqual(calls.complete, []);
  });
});
