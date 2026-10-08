import assert from "node:assert/strict";
import { test, type TestContext } from "node:test";
import { FrameDelivery } from "../src/delivery.ts";
import type { TransportFrame } from "../src/capture.ts";
import type { FrameAck } from "../src/api.ts";

function frame(seq: number): TransportFrame {
  return { seq, startMs: (seq - 1) * 10_000, endMs: seq * 10_000,
    mediaType: "audio/webm;codecs=opus", data: new Blob([`audio ${seq}`]) };
}
async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve();
}
function harness(t: TestContext) {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const requests: { frame: TransportFrame; signal: AbortSignal;
    resolve: (ack: FrameAck) => void; reject: (error: Error) => void }[] = [];
  const events: string[] = [];
  const delivery = new FrameDelivery({
    retryDelaysMs: [5_000, 10_000, 17_000], requestTimeoutMs: 60_000,
    send: (frame, signal) => new Promise((resolve, reject) => requests.push({ frame, signal, resolve, reject })),
    log: (message) => events.push(message), changed: () => {},
    acknowledged: (frame) => events.push(`health ${frame.seq}`),
    delayed: () => events.push("delayed"),
  });
  t.after(() => delivery.dispose());
  return { delivery, requests, events };
}

test("retains ordered bytes and only releases a matching success", async (t) => {
  const { delivery, requests, events } = harness(t);
  delivery.enqueue(frame(1));
  delivery.enqueue(frame(2));
  assert.equal(delivery.pendingCount, 2);
  assert.equal(delivery.pendingMs, 20_000);
  assert.equal(requests.length, 1);
  requests[0]!.resolve({ seq: 9, acknowledged: true });
  await settle();
  assert.equal(delivery.pendingCount, 2);
  assert.equal(events.filter((event) => event.startsWith("health")).length, 0);
  t.mock.timers.tick(5_000);
  requests[1]!.resolve({ seq: 1, acknowledged: true });
  await settle();
  assert.equal(delivery.pendingCount, 1);
  assert.equal(requests[2]!.frame.seq, 2);
  requests[2]!.resolve({ seq: 2, acknowledged: true });
  await settle();
  assert.equal(delivery.pendingCount, 0);
  assert.equal(delivery.delayed, false);
});

test("retry starts after 5, 10, 17 seconds independently of a 60-second request timeout", (t) => {
  const { delivery, requests, events } = harness(t);
  delivery.enqueue(frame(1));
  t.mock.timers.tick(4_999);
  assert.equal(requests.length, 1);
  t.mock.timers.tick(1);
  assert.equal(requests.length, 2);
  t.mock.timers.tick(10_000);
  assert.equal(requests.length, 3);
  t.mock.timers.tick(17_000);
  assert.equal(requests.length, 4);
  assert.ok(requests.every((request) => request.frame === requests[0]!.frame));
  assert.ok(requests.every((request) => !request.signal.aborted));
  assert.deepEqual(events.filter((event) => event === "delayed"), ["delayed"]);
  t.mock.timers.tick(27_999);
  assert.equal(requests[0]!.signal.aborted, false);
  t.mock.timers.tick(1);
  assert.equal(requests[0]!.signal.aborted, true);
  assert.equal(requests[1]!.signal.aborted, false);
  assert.equal(requests.length, 4);
});

test("overlapping success acknowledges once and cannot cancel the next frame's retry", async (t) => {
  const { delivery, requests, events } = harness(t);
  delivery.enqueue(frame(1));
  delivery.enqueue(frame(2));
  t.mock.timers.tick(5_000);
  requests[1]!.resolve({ seq: 1, acknowledged: true });
  await settle();
  assert.equal(requests[2]!.frame.seq, 2);
  requests[0]!.resolve({ seq: 1, acknowledged: true });
  await settle();
  assert.deepEqual(events.filter((event) => event.startsWith("health")), ["health 1"]);
  t.mock.timers.tick(5_000);
  assert.equal(requests[3]!.frame.seq, 2);
});

test("backlog freeze cancels scheduled retries and accepts late success without sending the tail", async (t) => {
  const { delivery, requests } = harness(t);
  const original = frame(1);
  delivery.enqueue(original);
  delivery.enqueue(frame(2));
  delivery.freeze();
  delivery.enqueue(frame(3));
  assert.equal(delivery.pendingCount, 3);
  t.mock.timers.tick(32_000);
  assert.equal(requests.length, 1);
  requests[0]!.resolve({ seq: 1, acknowledged: true });
  await settle();
  assert.equal(delivery.pendingCount, 2);
  assert.equal(requests.length, 1);
  delivery.retry();
  assert.equal(requests[1]!.frame.seq, 2);
  requests[1]!.resolve({ seq: 2, acknowledged: true });
  await settle();
  assert.equal(requests[2]!.frame.seq, 3);
  requests[2]!.resolve({ seq: 3, acknowledged: true });
  await settle();
  assert.equal(delivery.pendingCount, 0);
});

test("failed requests preserve audio and new frames do not restart the retry budget", async (t) => {
  const { delivery, requests } = harness(t);
  const original = frame(1);
  delivery.enqueue(original);
  requests[0]!.reject(new Error("offline"));
  await settle();
  t.mock.timers.tick(5_000);
  delivery.enqueue(frame(2));
  assert.equal(requests.length, 2);
  assert.equal(requests[1]!.frame, original);
  delivery.freeze();
  delivery.retry();
  assert.equal(requests[2]!.frame, original);
  assert.equal(delivery.pendingMs, 20_000);
});

test("a timed-out request cannot acknowledge audio; manual retry preserves its original bytes", async (t) => {
  const { delivery, requests, events } = harness(t);
  const original = frame(1);
  delivery.enqueue(original);
  delivery.freeze();
  t.mock.timers.tick(60_000);
  assert.equal(requests[0]!.signal.aborted, true);
  requests[0]!.resolve({ seq: 1, acknowledged: true });
  await settle();
  assert.equal(delivery.pendingCount, 1);
  assert.equal(events.filter((event) => event.startsWith("health")).length, 0);
  delivery.retry();
  assert.equal(requests[1]!.frame, original);
  requests[1]!.resolve({ seq: 1, acknowledged: true });
  await settle();
  assert.equal(delivery.pendingCount, 0);
  assert.deepEqual(events.filter((event) => event.startsWith("health")), ["health 1"]);
});
