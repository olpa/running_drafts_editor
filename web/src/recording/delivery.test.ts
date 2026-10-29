import assert from "node:assert/strict";
import { afterEach, it, vi } from "vitest";
import { FrameDelivery } from "./delivery.ts";
import type { TransportFrame } from "./capture.ts";
import type { FrameAck } from "./gateway.ts";

const cleanups: (() => void)[] = [];
afterEach(() => {
  while (cleanups.length) cleanups.pop()!();
  vi.useRealTimers();
});

function frame(seq: number): TransportFrame {
  return {
    seq,
    startMs: seq * 10_000,
    endMs: (seq + 1) * 10_000,
    mediaType: "audio/webm;codecs=opus",
    data: new Blob([`audio ${seq}`]),
  };
}
function ack(sequence: number): FrameAck {
  return {
    sequence,
    acknowledged: true,
    sha256: "a".repeat(64),
    nextExpectedSequence: sequence + 1,
  };
}
async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve();
}
function harness(now?: () => number) {
  vi.useFakeTimers({ toFake: ["setTimeout"] });
  const requests: {
    frame: TransportFrame;
    signal: AbortSignal;
    resolve: (acknowledgement: FrameAck) => void;
    reject: (error: Error) => void;
  }[] = [];
  const events: string[] = [];
  const delivery = new FrameDelivery({
    retryDelaysMs: [5_000, 10_000, 17_000],
    requestTimeoutMs: 60_000,
    send: (item, signal) =>
      new Promise((resolve, reject) => requests.push({ frame: item, signal, resolve, reject })),
    log: (message) => events.push(message),
    changed: () => {},
    acknowledged: (item) => events.push(`health ${item.seq}`),
    delayed: () => events.push("delayed"),
    ...(now ? { now } : {}),
  });
  cleanups.push(() => delivery.dispose());
  return { delivery, requests, events };
}

it("rejects a first-frame or later sequence gap before sending", () => {
  const { delivery, requests } = harness();
  assert.throws(() => delivery.enqueue(frame(1)), /not consecutive/);
  assert.equal(requests.length, 0);
  delivery.enqueue(frame(0));
  assert.throws(() => delivery.enqueue(frame(2)), /not consecutive/);
});

it("serializes first uploads and retains exact bytes until a matching durable acknowledgement", async () => {
  const { delivery, requests, events } = harness();
  const first = frame(0);
  delivery.enqueue(first);
  delivery.enqueue(frame(1));
  assert.equal(requests.length, 1);
  assert.equal(requests[0]!.frame, first);
  requests[0]!.resolve(ack(9));
  await settle();
  assert.equal(delivery.pendingCount, 2);
  vi.advanceTimersByTime(5_000);
  assert.equal(requests[1]!.frame, first);
  requests[1]!.resolve(ack(0));
  await settle();
  assert.equal(requests[2]!.frame.seq, 1);
  requests[2]!.resolve(ack(1));
  await settle();
  assert.equal(delivery.pendingCount, 0);
  assert.deepEqual(
    events.filter((event) => event.startsWith("health")),
    ["health 0", "health 1"],
  );
});

it("overlapping identical retries acknowledge once and cannot disturb the next frame", async () => {
  const { delivery, requests, events } = harness();
  const original = frame(0);
  delivery.enqueue(original);
  delivery.enqueue(frame(1));
  vi.advanceTimersByTime(5_000);
  assert.equal(requests[0]!.frame, requests[1]!.frame);
  requests[1]!.resolve(ack(0));
  await settle();
  assert.equal(requests[2]!.frame.seq, 1);
  requests[0]!.resolve(ack(0));
  await settle();
  assert.deepEqual(
    events.filter((event) => event.startsWith("health")),
    ["health 0"],
  );
  vi.advanceTimersByTime(5_000);
  assert.equal(requests[3]!.frame.seq, 1);
});

it("does not release a frame when server state has not advanced past it", async () => {
  const { delivery, requests } = harness();
  delivery.enqueue(frame(0));
  requests[0]!.resolve({
    sequence: 0,
    acknowledged: true,
    sha256: "a".repeat(64),
    nextExpectedSequence: 0,
  });
  await settle();
  assert.equal(delivery.pendingCount, 1);
  vi.advanceTimersByTime(5_000);
  assert.equal(requests[1]!.frame.seq, 0);
});

it("freeze stops new attempts while accepting a late acknowledgement; recovery preserves identity", async () => {
  const { delivery, requests } = harness();
  const first = frame(0);
  const second = frame(1);
  delivery.enqueue(first);
  delivery.enqueue(second);
  delivery.freeze();
  delivery.enqueue(frame(2));
  vi.advanceTimersByTime(40_000);
  assert.equal(requests.length, 1);
  requests[0]!.resolve(ack(0));
  await settle();
  assert.equal(delivery.pendingCount, 2);
  assert.equal(requests.length, 1);
  delivery.retry();
  assert.equal(requests[1]!.frame, second);
  requests[1]!.resolve(ack(1));
  await settle();
  requests[2]!.resolve(ack(2));
  await settle();
  assert.equal(delivery.pendingCount, 0);
});

it("a timed-out request cannot release bytes", async () => {
  const { delivery, requests, events } = harness();
  const original = frame(0);
  delivery.enqueue(original);
  delivery.freeze();
  vi.advanceTimersByTime(60_000);
  assert.equal(requests[0]!.signal.aborted, true);
  requests[0]!.resolve(ack(0));
  await settle();
  assert.equal(delivery.pendingCount, 1);
  delivery.retry();
  assert.equal(requests[1]!.frame, original);
  requests[1]!.resolve(ack(0));
  await settle();
  assert.deepEqual(
    events.filter((event) => event.startsWith("health")),
    ["health 0"],
  );
});

it("reports acknowledgement latency from the injected clock", async () => {
  let now = 100;
  const { delivery, requests, events } = harness(() => now);
  delivery.enqueue(frame(0));
  now = 350;
  requests[0]!.resolve(ack(0));
  await settle();
  assert.ok(events.includes("f0 ack 250ms"), events.join(" | "));
});
