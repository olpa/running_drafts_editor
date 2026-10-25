import assert from "node:assert/strict";
import test, { type TestContext } from "node:test";
import { MediaRecorderFrameSource, openMicrophoneCapture, requireTransportFrameMediaType,
  TRANSPORT_FRAME_MEDIA_TYPE } from "../src/capture.ts";
import type { TransportFrame } from "../src/capture.ts";

Object.defineProperty(globalThis, "MediaRecorder",
  { value: function () {}, writable: true, configurable: true });

class FakeRecorder extends EventTarget {
  static instances: FakeRecorder[] = [];
  static supported = [TRANSPORT_FRAME_MEDIA_TYPE, "audio/webm"];
  static reportedMediaType: string | null = null;
  static throwDuringConstruction = false;
  static throwDuringStart = false;
  static throwDuringStop = false;
  static isTypeSupported(type: string): boolean { return this.supported.includes(type); }
  state = "inactive";
  mimeType: string;
  starts: (number | undefined)[] = [];
  stops = 0;

  constructor(_stream: MediaStream, options: { mimeType: string }) {
    super();
    if (FakeRecorder.throwDuringConstruction) throw new Error("recorder construction failed");
    this.mimeType = FakeRecorder.reportedMediaType ?? options.mimeType;
    FakeRecorder.instances.push(this);
  }

  start(timeslice?: number): void {
    if (FakeRecorder.throwDuringStart) throw new Error("recorder startup failed");
    this.starts.push(timeslice);
    this.state = "recording";
  }

  stop(): void {
    this.stops++;
    if (FakeRecorder.throwDuringStop) throw new Error("recorder stop failed");
    this.state = "inactive";
  }

  emit(data: Blob): void {
    const event = new Event("dataavailable");
    Object.defineProperty(event, "data", { value: data });
    this.dispatchEvent(event);
  }

  fail(): void { this.dispatchEvent(new Event("error")); }

  finish(data = new Blob(["tail"])): void {
    this.emit(data);
    this.dispatchEvent(new Event("stop"));
  }
}

class FakeTrack extends EventTarget {
  readyState: MediaStreamTrackState = "live";
  stops = 0;
  constructor(readonly settings: MediaTrackSettings = {}) { super(); }
  getSettings(): MediaTrackSettings { return this.settings; }
  stop(): void { this.stops++; this.readyState = "ended"; }
  end(): void { this.readyState = "ended"; this.dispatchEvent(new Event("ended")); }
}

function installRecorder(t: TestContext): void {
  const original = globalThis.MediaRecorder;
  globalThis.MediaRecorder = FakeRecorder as unknown as typeof MediaRecorder;
  t.after(() => { globalThis.MediaRecorder = original; });
  FakeRecorder.instances = [];
  FakeRecorder.supported = [TRANSPORT_FRAME_MEDIA_TYPE, "audio/webm"];
  FakeRecorder.reportedMediaType = null;
  FakeRecorder.throwDuringConstruction = false;
  FakeRecorder.throwDuringStart = false;
  FakeRecorder.throwDuringStop = false;
}

function installMicrophone(t: TestContext, tracks: FakeTrack[],
  inspectConstraints: (constraints: MediaStreamConstraints) => void = () => {}): void {
  const originalNavigator = globalThis.navigator;
  t.after(() => Object.defineProperty(globalThis, "navigator",
    { value: originalNavigator, configurable: true }));
  Object.defineProperty(globalThis, "navigator", { configurable: true, value: { mediaDevices: {
    getSupportedConstraints: () => { throw new Error("constraint reporting must not be consulted"); },
    getUserMedia: async (constraints: MediaStreamConstraints) => {
      inspectConstraints(constraints);
      return {
        getTracks: () => tracks,
        getAudioTracks: () => tracks,
      } as unknown as MediaStream;
    },
  }}});
}

test("one recorder uses the configured timeslice and stop retains its final partial event", async (t) => {
  installRecorder(t);
  const frames: TransportFrame[] = [];
  const source = new MediaRecorderFrameSource({} as MediaStream, 10_000, () => true, assert.fail);
  await source.start((frame) => frames.push(frame));
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
  assert.ok(frames.every((frame) => frame.mediaType === TRANSPORT_FRAME_MEDIA_TYPE));
  assert.equal(await frames[2]!.data.text(), "tail");
  assert.equal(frames[1]!.startMs, frames[0]!.endMs);
  assert.equal(frames[2]!.startMs, frames[1]!.endMs);
});

test("the second pending frame stops capture and the final partial event makes the three-frame bound", async (t) => {
  installRecorder(t);
  const frames: TransportFrame[] = [];
  const source = new MediaRecorderFrameSource({} as MediaStream, 10_000,
    () => frames.length < 2, assert.fail);
  await source.start((frame) => frames.push(frame));
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
  assert.throws(requireTransportFrameMediaType, /WebM\/Opus/);
  FakeRecorder.supported = [];
  assert.throws(requireTransportFrameMediaType, /does not support/);
});

test("rejects unsupported WebM/Opus before requesting microphone permission", async (t) => {
  installRecorder(t);
  FakeRecorder.supported = [];
  let requested = false;
  installMicrophone(t, [], () => { requested = true; });
  await assert.rejects(() => openMicrophoneCapture({ intervalMs: 1, canContinue: () => true,
    onError: assert.fail }), /does not support/);
  assert.equal(requested, false);
});

test("accepts harmless MIME normalization and rejects incompatible recorder reporting", async (t) => {
  installRecorder(t);
  FakeRecorder.reportedMediaType = " Audio/WebM ; codecs = \"OPUS\" ";
  const accepted = new MediaRecorderFrameSource({} as MediaStream, 10, () => true, assert.fail);
  accepted.dispose();

  const track = new FakeTrack();
  installMicrophone(t, [track]);
  FakeRecorder.reportedMediaType = "";
  await assert.rejects(() => openMicrophoneCapture({ intervalMs: 1, canContinue: () => true,
    onError: assert.fail }), /did not create a WebM\/Opus/);
  assert.equal(track.stops, 1);
});

test("zero-byte events consume neither sequence nor an assigned timing boundary", async (t) => {
  installRecorder(t);
  const frames: TransportFrame[] = [];
  const source = new MediaRecorderFrameSource({} as MediaStream, 10_000, () => true, assert.fail);
  await source.start((frame) => frames.push(frame));
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

test("stereo settings and absent channel-constraint reporting do not block capture", async (t) => {
  installRecorder(t);
  const track = new FakeTrack({ channelCount: 2, sampleRate: 48_000 });
  installMicrophone(t, [track], (constraints) => {
    assert.deepEqual(constraints.audio, { channelCount: { ideal: 1 }, echoCancellation: true });
  });
  const capture = await openMicrophoneCapture({ intervalMs: 10, canContinue: () => true,
    onError: assert.fail });
  await capture.start(() => {});
  const stopping = capture.stop();
  FakeRecorder.instances[0]!.finish();
  await stopping;
  assert.equal(track.stops, 1);
});

test("missing and ended audio tracks are rejected and all acquired tracks are released", async (t) => {
  installRecorder(t);
  installMicrophone(t, []);
  await assert.rejects(() => openMicrophoneCapture({ intervalMs: 1, canContinue: () => true,
    onError: assert.fail }), /one usable audio track/);

  const ended = new FakeTrack();
  ended.readyState = "ended";
  installMicrophone(t, [ended]);
  await assert.rejects(() => openMicrophoneCapture({ intervalMs: 1, canContinue: () => true,
    onError: assert.fail }), /one usable audio track/);
  assert.equal(ended.stops, 1);
});

test("recorder construction and startup failures release the microphone", async (t) => {
  installRecorder(t);
  const constructionTrack = new FakeTrack();
  installMicrophone(t, [constructionTrack]);
  FakeRecorder.throwDuringConstruction = true;
  await assert.rejects(() => openMicrophoneCapture({ intervalMs: 1, canContinue: () => true,
    onError: assert.fail }), /construction failed/);
  assert.equal(constructionTrack.stops, 1);

  FakeRecorder.throwDuringConstruction = false;
  const startupTrack = new FakeTrack();
  installMicrophone(t, [startupTrack]);
  const capture = await openMicrophoneCapture({ intervalMs: 1, canContinue: () => true,
    onError: assert.fail });
  FakeRecorder.throwDuringStart = true;
  await assert.rejects(() => capture.start(() => {}), /startup failed/);
  assert.equal(startupTrack.stops, 1);
});

test("finalization timeout rejects stop and releases tracks exactly once", async (t) => {
  installRecorder(t);
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const track = new FakeTrack();
  installMicrophone(t, [track]);
  const capture = await openMicrophoneCapture({ intervalMs: 1, canContinue: () => true,
    onError: assert.fail, finalizationTimeoutMs: 25 });
  await capture.start(() => {});
  const assertion = assert.rejects(capture.stop(), /finalization deadline/);
  t.mock.timers.tick(25);
  await assertion;
  capture.dispose();
  assert.equal(track.stops, 1);
});

test("a spontaneous recorder failure is reported once and cleans up after final bytes", async (t) => {
  installRecorder(t);
  const track = new FakeTrack();
  installMicrophone(t, [track]);
  const frames: TransportFrame[] = [];
  const errors: Error[] = [];
  const capture = await openMicrophoneCapture({ intervalMs: 10, canContinue: () => true,
    onError: (error) => errors.push(error) });
  await capture.start((frame) => frames.push(frame));
  const recorder = FakeRecorder.instances[0]!;
  recorder.fail();
  recorder.fail();
  recorder.finish();
  await capture.stop();

  assert.equal(errors.length, 1);
  assert.equal(await frames[0]!.data.text(), "tail");
  assert.equal(track.stops, 1);
});

test("dispose is immediate and idempotent, and late events only warn", async (t) => {
  installRecorder(t);
  const warnings: unknown[][] = [];
  t.mock.method(console, "warn", (...args: unknown[]) => { warnings.push(args); });
  const track = new FakeTrack();
  installMicrophone(t, [track]);
  const frames: TransportFrame[] = [];
  const errors: Error[] = [];
  const capture = await openMicrophoneCapture({ intervalMs: 10, canContinue: () => true,
    onError: (error) => errors.push(error) });
  await capture.start((frame) => frames.push(frame));
  capture.dispose();
  capture.dispose();
  const recorder = FakeRecorder.instances[0]!;
  recorder.emit(new Blob(["late"]));
  recorder.fail();
  recorder.dispatchEvent(new Event("stop"));

  assert.equal(track.stops, 1);
  assert.equal(frames.length, 0);
  assert.equal(errors.length, 0);
  assert.equal(warnings.length, 3);
});
