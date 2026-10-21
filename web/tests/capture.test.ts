import assert from "node:assert/strict";
import test, { type TestContext } from "node:test";
import { openRecordingCapture, RecorderFrameSource, recordingMediaType } from "../src/capture.ts";
import type { TransportFrame } from "../src/capture.ts";

Object.defineProperty(globalThis, "MediaRecorder", { value: function () {}, writable: true, configurable: true });

class FakeRecorder extends EventTarget {
  static instances: FakeRecorder[] = [];
  static supported = ["audio/webm;codecs=opus", "audio/webm"];
  static isTypeSupported(type: string): boolean { return this.supported.includes(type); }
  state = "inactive";
  mimeType: string;
  starts: (number | undefined)[] = [];
  stops = 0;
  constructor(_stream: MediaStream, options: { mimeType: string }) {
    super(); this.mimeType = options.mimeType; FakeRecorder.instances.push(this);
  }
  start(timeslice?: number): void { this.starts.push(timeslice); this.state = "recording"; }
  stop(): void { this.stops++; this.state = "inactive"; }
  emit(data: Blob): void {
    const event = new Event("dataavailable");
    Object.defineProperty(event, "data", { value: data });
    this.dispatchEvent(event);
  }
  finish(data = new Blob(["tail"])): void {
    this.emit(data);
    this.dispatchEvent(new Event("stop"));
  }
}

function installRecorder(t: TestContext): void {
  const original = globalThis.MediaRecorder;
  globalThis.MediaRecorder = FakeRecorder as unknown as typeof MediaRecorder;
  t.after(() => { globalThis.MediaRecorder = original; });
  FakeRecorder.instances = [];
  FakeRecorder.supported = ["audio/webm;codecs=opus", "audio/webm"];
}

test("one recorder uses the configured timeslice and stop retains its final partial event", async (t) => {
  installRecorder(t);
  const frames: TransportFrame[] = [];
  const source = new RecorderFrameSource({} as MediaStream, 10_000, () => true, assert.fail);
  source.start((frame) => frames.push(frame));
  const recorder = FakeRecorder.instances[0]!;
  recorder.emit(new Blob(["first"]));
  recorder.emit(new Blob(["second"]));
  let stopped = false;
  const stopping = source.stop().then(() => { stopped = true; });
  await Promise.resolve();
  assert.equal(stopped, false);
  recorder.finish();
  await stopping;

  assert.deepEqual(recorder.starts, [10_000]);
  assert.equal(recorder.stops, 1);
  assert.deepEqual(frames.map((frame) => frame.seq), [0, 1, 2]);
  assert.equal(await frames[2]!.data.text(), "tail");
  assert.equal(frames[1]!.startMs, frames[0]!.endMs);
  assert.equal(frames[2]!.startMs, frames[1]!.endMs);
});

test("the second pending frame stops capture and the final partial event makes the three-frame bound", async (t) => {
  installRecorder(t);
  const frames: TransportFrame[] = [];
  const source = new RecorderFrameSource({} as MediaStream, 10_000,
    () => frames.length < 2, assert.fail);
  source.start((frame) => frames.push(frame));
  const recorder = FakeRecorder.instances[0]!;
  recorder.emit(new Blob(["first"]));
  assert.equal(recorder.state, "recording");
  recorder.emit(new Blob(["second"]));
  assert.equal(recorder.state, "inactive");
  recorder.finish();
  await source.stop();
  assert.equal(frames.length, 3);
  assert.deepEqual(recorder.starts, [10_000]);
});

test("requires the canonical WebM/Opus capability without a bare WebM fallback", (t) => {
  installRecorder(t);
  FakeRecorder.supported = ["audio/webm"];
  assert.throws(recordingMediaType, /WebM\/Opus/);
  FakeRecorder.supported = [];
  assert.throws(recordingMediaType, /does not support/);
});

test("zero-byte events consume neither sequence nor an assigned timing boundary", async (t) => {
  installRecorder(t);
  const frames: TransportFrame[] = [];
  const source = new RecorderFrameSource({} as MediaStream, 10_000, () => true, assert.fail);
  source.start((frame) => frames.push(frame));
  const recorder = FakeRecorder.instances[0]!;
  recorder.emit(new Blob([]));
  recorder.emit(new Blob(["first"]));
  recorder.emit(new Blob([]));
  recorder.emit(new Blob(["second"]));
  const stopping = source.stop();
  recorder.finish();
  await stopping;
  assert.deepEqual(frames.map((frame) => frame.seq), [0, 1, 2]);
  assert.equal(frames[0]!.startMs, 0);
  assert.equal(frames[1]!.startMs, frames[0]!.endMs);
});

test("rejects missing mono constraints and a non-mono acquired track before recording", async (t) => {
  installRecorder(t);
  const originalNavigator = globalThis.navigator;
  t.after(() => Object.defineProperty(globalThis, "navigator",
    { value: originalNavigator, configurable: true }));
  let requested = false;
  Object.defineProperty(globalThis, "navigator", { configurable: true, value: { mediaDevices: {
    getSupportedConstraints: () => ({ channelCount: false }),
    getUserMedia: async () => { requested = true; return {} as MediaStream; },
  }}});
  await assert.rejects(() => openRecordingCapture({ intervalMs: 1, canContinue: () => true,
    onError: assert.fail }), /request mono/);
  assert.equal(requested, false);

  let released = false;
  const track = { getSettings: () => ({ channelCount: 2 }), addEventListener() {},
    removeEventListener() {}, stop: () => { released = true; } };
  Object.defineProperty(globalThis, "navigator", { configurable: true, value: { mediaDevices: {
    getSupportedConstraints: () => ({ channelCount: true }),
    getUserMedia: async (constraints: MediaStreamConstraints) => {
      assert.deepEqual(constraints.audio, { channelCount: { exact: 1 }, echoCancellation: true });
      return { getTracks: () => [track], getAudioTracks: () => [track] } as unknown as MediaStream;
    },
  }}});
  await assert.rejects(() => openRecordingCapture({ intervalMs: 1, canContinue: () => true,
    onError: assert.fail }), /one-channel/);
  assert.equal(released, true);
});

test("unexpected recorder end reports a Capture interruption after preserving final bytes", async (t) => {
  installRecorder(t);
  const frames: TransportFrame[] = [];
  const errors: Error[] = [];
  const source = new RecorderFrameSource({} as MediaStream, 10_000, () => true,
    (error) => errors.push(error));
  source.start((frame) => frames.push(frame));
  const recorder = FakeRecorder.instances[0]!;
  recorder.state = "inactive";
  recorder.finish();
  await source.stop();
  assert.equal(await frames[0]!.data.text(), "tail");
  assert.equal(errors.length, 1);
});
