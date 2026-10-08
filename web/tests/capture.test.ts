import assert from "node:assert/strict";
import { test } from "node:test";
import { RecorderFrameSource, recordingMediaType } from "../src/capture.ts";
import type { TransportFrame } from "../src/capture.ts";

Object.defineProperty(globalThis, "MediaRecorder", { value: function () {}, writable: true, configurable: true });

class FakeRecorder extends EventTarget {
  static instances: FakeRecorder[] = [];
  static supported = ["audio/webm;codecs=opus", "audio/webm"];
  static isTypeSupported(type: string): boolean { return this.supported.includes(type); }
  state = "inactive";
  mimeType: string;
  starts = 0;
  requests = 0;
  constructor(_stream: MediaStream, options: { mimeType: string }) {
    super(); this.mimeType = options.mimeType; FakeRecorder.instances.push(this);
  }
  start(): void { this.starts++; this.state = "recording"; }
  requestData(): void { this.requests++; this.emit(new Blob([`piece ${this.requests}`])); }
  stop(): void { this.state = "inactive"; }
  emit(data: Blob): void {
    const event = new Event("dataavailable");
    Object.defineProperty(event, "data", { value: data });
    this.dispatchEvent(event);
  }
  finish(): void { this.emit(new Blob(["tail"])); this.dispatchEvent(new Event("stop")); }
}

test("one recorder emits pieces and stop waits for the asynchronous final data event", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const original = globalThis.MediaRecorder;
  globalThis.MediaRecorder = FakeRecorder as unknown as typeof MediaRecorder;
  t.after(() => { globalThis.MediaRecorder = original; });
  FakeRecorder.instances = [];
  const frames: TransportFrame[] = [];
  const source = new RecorderFrameSource({} as MediaStream, 10_000, () => true, assert.fail);
  source.start((frame) => frames.push(frame));
  t.mock.timers.tick(10_000);
  t.mock.timers.tick(10_000);
  const recorder = FakeRecorder.instances[0]!;
  assert.equal(FakeRecorder.instances.length, 1);
  assert.equal(recorder.starts, 1);
  assert.equal(recorder.requests, 2);
  let stopped = false;
  const stop = source.stop().then(() => { stopped = true; });
  await Promise.resolve();
  assert.equal(stopped, false);
  recorder.finish();
  await stop;
  assert.deepEqual(frames.map((frame) => frame.seq), [1, 2, 3]);
  assert.equal(await frames[2]!.data.text(), "tail");
  assert.equal(frames[1]!.startMs, frames[0]!.endMs);
  assert.equal(frames[2]!.startMs, frames[1]!.endMs);
});

test("backlog boundary stops instead of emitting a third regular frame plus an extra tail", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const original = globalThis.MediaRecorder;
  globalThis.MediaRecorder = FakeRecorder as unknown as typeof MediaRecorder;
  t.after(() => { globalThis.MediaRecorder = original; });
  FakeRecorder.instances = [];
  const frames: TransportFrame[] = [];
  const source = new RecorderFrameSource({} as MediaStream, 10_000, () => frames.length < 2, assert.fail);
  source.start((frame) => frames.push(frame));
  for (let i = 0; i < 3; i++) t.mock.timers.tick(10_000);
  const recorder = FakeRecorder.instances[0]!;
  assert.equal(recorder.state, "inactive");
  assert.equal(recorder.requests, 2);
  recorder.finish();
  await source.stop();
  t.mock.timers.tick(60_000);
  assert.equal(frames.length, 3);
  assert.equal(recorder.requests, 2);
});

test("WebM fallback and unsupported capability are explicit", (t) => {
  const original = globalThis.MediaRecorder;
  globalThis.MediaRecorder = FakeRecorder as unknown as typeof MediaRecorder;
  t.after(() => { globalThis.MediaRecorder = original; });
  FakeRecorder.supported = ["audio/webm"];
  assert.equal(recordingMediaType(), "audio/webm");
  FakeRecorder.supported = [];
  assert.throws(recordingMediaType, /does not support/);
  FakeRecorder.supported = ["audio/webm;codecs=opus", "audio/webm"];
});

test("unexpected recorder end reports stopped capture after preserving final bytes", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const original = globalThis.MediaRecorder;
  globalThis.MediaRecorder = FakeRecorder as unknown as typeof MediaRecorder;
  t.after(() => { globalThis.MediaRecorder = original; });
  FakeRecorder.instances = [];
  const frames: TransportFrame[] = [];
  const errors: Error[] = [];
  const source = new RecorderFrameSource({} as MediaStream, 10_000, () => true, (error) => errors.push(error));
  source.start((frame) => frames.push(frame));
  const recorder = FakeRecorder.instances[0]!;
  recorder.state = "inactive";
  recorder.finish();
  await source.stop();
  assert.equal(await frames[0]!.data.text(), "tail");
  assert.equal(errors.length, 1);
  t.mock.timers.tick(60_000);
  assert.equal(recorder.requests, 0);
});
