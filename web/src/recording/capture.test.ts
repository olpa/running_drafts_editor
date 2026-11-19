import assert from "node:assert/strict";
import { afterEach, it, vi } from "vitest";
import {
  MediaRecorderFrameSource,
  measureMicrophoneLevel,
  openMicrophoneCapture,
  requireTransportFrameMediaType,
  TRANSPORT_FRAME_MEDIA_TYPE,
} from "./capture.ts";
import type { TransportFrame } from "./capture.ts";

Object.defineProperty(globalThis, "MediaRecorder", {
  value: function () {},
  writable: true,
  configurable: true,
});

const cleanups: (() => void)[] = [];
afterEach(() => {
  while (cleanups.length) cleanups.pop()!();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("maps silence, ordinary input, and full scale onto the neutral percentage", () => {
  assert.deepEqual(measureMicrophoneLevel(new Float32Array([0, 0])), {
    averagePercent: 0,
    peakPercent: 0,
  });
  const speech = measureMicrophoneLevel(new Float32Array([0.1, -0.1]));
  assert.ok(speech.averagePercent > 60);
  assert.equal(speech.averagePercent, speech.peakPercent);
  assert.deepEqual(measureMicrophoneLevel(new Float32Array([1, -1])), {
    averagePercent: 100,
    peakPercent: 100,
  });
  const transient = new Float32Array(2048);
  transient[0] = 1;
  const measured = measureMicrophoneLevel(transient);
  assert.ok(measured.averagePercent < 50);
  assert.equal(measured.peakPercent, 100);
});

class FakeRecorder extends EventTarget {
  static instances: FakeRecorder[] = [];
  static supported = [TRANSPORT_FRAME_MEDIA_TYPE, "audio/webm"];
  static reportedMediaType: string | null = null;
  static throwDuringConstruction = false;
  static throwDuringStart = false;
  static throwDuringStop = false;
  static isTypeSupported(type: string): boolean {
    return this.supported.includes(type);
  }
  state = "inactive";
  mimeType: string;
  starts: (number | undefined)[] = [];
  stops = 0;
  requests = 0;
  requestData(): void {
    this.requests++;
  }

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

  fail(): void {
    this.dispatchEvent(new Event("error"));
  }

  finish(data = new Blob(["tail"])): void {
    this.emit(data);
    this.dispatchEvent(new Event("stop"));
  }
}

class FakeTrack extends EventTarget {
  readyState: MediaStreamTrackState = "live";
  stops = 0;
  constructor(readonly settings: MediaTrackSettings = {}) {
    super();
  }
  getSettings(): MediaTrackSettings {
    return this.settings;
  }
  stop(): void {
    this.stops++;
    this.readyState = "ended";
  }
  end(): void {
    this.readyState = "ended";
    this.dispatchEvent(new Event("ended"));
  }
}

function installRecorder(): void {
  const original = globalThis.MediaRecorder;
  globalThis.MediaRecorder = FakeRecorder as unknown as typeof MediaRecorder;
  cleanups.push(() => {
    globalThis.MediaRecorder = original;
  });
  FakeRecorder.instances = [];
  FakeRecorder.supported = [TRANSPORT_FRAME_MEDIA_TYPE, "audio/webm"];
  FakeRecorder.reportedMediaType = null;
  FakeRecorder.throwDuringConstruction = false;
  FakeRecorder.throwDuringStart = false;
  FakeRecorder.throwDuringStop = false;
}

function installMicrophone(
  tracks: FakeTrack[],
  inspectConstraints: (constraints: MediaStreamConstraints) => void = () => {},
): void {
  const originalNavigator = globalThis.navigator;
  cleanups.push(() =>
    Object.defineProperty(globalThis, "navigator", {
      value: originalNavigator,
      configurable: true,
    }),
  );
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: {
      mediaDevices: {
        getSupportedConstraints: () => {
          throw new Error("constraint reporting must not be consulted");
        },
        getUserMedia: async (constraints: MediaStreamConstraints) => {
          inspectConstraints(constraints);
          return {
            getTracks: () => tracks,
            getAudioTracks: () => tracks,
          } as unknown as MediaStream;
        },
      },
    },
  });
}

it("one recorder uses explicit frame requests and stop retains its final partial event", async () => {
  installRecorder();
  const frames: TransportFrame[] = [];
  const source = new MediaRecorderFrameSource({} as MediaStream, assert.fail);
  await source.start((frame) => frames.push(frame));
  const recorder = FakeRecorder.instances[0]!;
  recorder.emit(new Blob(["first"]));
  recorder.emit(new Blob(["second"]));
  let stopped = false;
  const stopping = source.stop().then(() => {
    stopped = true;
  });
  await Promise.resolve();
  assert.equal(stopped, false);
  recorder.finish();
  await stopping;

  assert.deepEqual(recorder.starts, [undefined]);
  assert.equal(recorder.stops, 1);
  assert.deepEqual(
    frames.map((frame) => frame.seq),
    [0, 1, 2],
  );
  assert.ok(frames.every((frame) => frame.mediaType === TRANSPORT_FRAME_MEDIA_TYPE));
  assert.equal(await frames[2]!.data.text(), "tail");
  assert.equal(frames[1]!.startMs, frames[0]!.endMs);
  assert.equal(frames[2]!.startMs, frames[1]!.endMs);
});

it("times frames with the injected clock", async () => {
  installRecorder();
  let now = 1_000;
  const frames: TransportFrame[] = [];
  const source = new MediaRecorderFrameSource(
    {} as MediaStream,
    assert.fail,
    undefined,
    undefined,
    () => now,
  );
  await source.start((frame) => frames.push(frame));
  const recorder = FakeRecorder.instances[0]!;
  now = 11_000;
  recorder.emit(new Blob(["first"]));
  now = 21_500;
  recorder.emit(new Blob(["second"]));

  assert.deepEqual(
    frames.map((frame) => [frame.startMs, frame.endMs]),
    [
      [0, 10_000],
      [10_000, 20_500],
    ],
  );
  source.dispose();
});

it("requests frames only when the owner asks", async () => {
  installRecorder();
  const source = new MediaRecorderFrameSource({} as MediaStream, assert.fail);
  await source.start(() => {});
  const recorder = FakeRecorder.instances[0]!;
  assert.deepEqual(recorder.starts, [undefined]);
  assert.equal(recorder.requests, 0);
  source.requestFrame();
  source.requestFrame();
  assert.equal(recorder.requests, 2);
  source.dispose();
});

it("requires the canonical WebM/Opus capability without a bare WebM fallback", () => {
  installRecorder();
  FakeRecorder.supported = ["audio/webm"];
  assert.throws(requireTransportFrameMediaType, /WebM\/Opus/);
  FakeRecorder.supported = [];
  assert.throws(requireTransportFrameMediaType, /does not support/);
});

it("rejects unsupported WebM/Opus before requesting microphone permission", async () => {
  installRecorder();
  FakeRecorder.supported = [];
  let requested = false;
  installMicrophone([], () => {
    requested = true;
  });
  await assert.rejects(() => openMicrophoneCapture({ onError: assert.fail }), /does not support/);
  assert.equal(requested, false);
});

it("accepts harmless MIME normalization and rejects incompatible recorder reporting", async () => {
  installRecorder();
  FakeRecorder.reportedMediaType = ' Audio/WebM ; codecs = "OPUS" ';
  const accepted = new MediaRecorderFrameSource({} as MediaStream, assert.fail);
  accepted.dispose();

  const track = new FakeTrack();
  installMicrophone([track]);
  FakeRecorder.reportedMediaType = "";
  await assert.rejects(
    () => openMicrophoneCapture({ onError: assert.fail }),
    /did not create a WebM\/Opus/,
  );
  assert.equal(track.stops, 1);
});

it("zero-byte events consume neither sequence nor an assigned timing boundary", async () => {
  installRecorder();
  const frames: TransportFrame[] = [];
  const source = new MediaRecorderFrameSource({} as MediaStream, assert.fail);
  await source.start((frame) => frames.push(frame));
  const recorder = FakeRecorder.instances[0]!;
  recorder.emit(new Blob([]));
  recorder.emit(new Blob(["first"]));
  recorder.emit(new Blob([]));
  recorder.emit(new Blob(["second"]));
  const stopping = source.stop();
  recorder.finish();
  await stopping;
  assert.deepEqual(
    frames.map((frame) => frame.seq),
    [0, 1, 2],
  );
  assert.equal(frames[0]!.startMs, 0);
  assert.equal(frames[1]!.startMs, frames[0]!.endMs);
});

it("stereo settings and absent channel-constraint reporting do not block capture", async () => {
  installRecorder();
  const track = new FakeTrack({ channelCount: 2, sampleRate: 48_000 });
  installMicrophone([track], (constraints) => {
    assert.deepEqual(constraints.audio, { channelCount: { ideal: 1 }, echoCancellation: true });
  });
  const capture = await openMicrophoneCapture({
    onError: assert.fail,
  });
  await capture.start(() => {});
  const stopping = capture.stop();
  FakeRecorder.instances[0]!.finish();
  await stopping;
  assert.equal(track.stops, 1);
});

it("analyses the acquired stream without routing microphone input to speakers", async () => {
  installRecorder();
  const track = new FakeTrack();
  installMicrophone([track]);
  const analyser = {
    fftSize: 0,
    getFloatTimeDomainData(samples: Float32Array) {
      samples.fill(0);
      samples[0] = 1;
    },
  };
  const mediaSource = { connect: vi.fn(), disconnect: vi.fn() };
  const destination = {};
  const context = {
    state: "running",
    destination,
    createMediaStreamSource: vi.fn(() => mediaSource),
    createAnalyser: vi.fn(() => analyser),
    resume: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
  };
  vi.stubGlobal("AudioContext", function () {
    return context;
  });

  const capture = await openMicrophoneCapture({ onError: assert.fail });
  await capture.start(() => {});
  await Promise.resolve();

  const level = capture.readLevel?.();
  assert.equal(Math.round(level!.averagePercent), 45);
  assert.equal(level!.peakPercent, 100);
  assert.deepEqual(mediaSource.connect.mock.calls, [[analyser]]);
  assert.equal(
    mediaSource.connect.mock.calls.some(([target]) => target === destination),
    false,
  );
  capture.dispose();
  assert.equal(track.stops, 1);
  assert.equal(context.close.mock.calls.length, 1);
});

it("missing and ended audio tracks are rejected and all acquired tracks are released", async () => {
  installRecorder();
  installMicrophone([]);
  await assert.rejects(
    () => openMicrophoneCapture({ onError: assert.fail }),
    /one usable audio track/,
  );

  const ended = new FakeTrack();
  ended.readyState = "ended";
  installMicrophone([ended]);
  await assert.rejects(
    () => openMicrophoneCapture({ onError: assert.fail }),
    /one usable audio track/,
  );
  assert.equal(ended.stops, 1);
});

it("recorder construction and startup failures release the microphone", async () => {
  installRecorder();
  const constructionTrack = new FakeTrack();
  installMicrophone([constructionTrack]);
  FakeRecorder.throwDuringConstruction = true;
  await assert.rejects(
    () => openMicrophoneCapture({ onError: assert.fail }),
    /construction failed/,
  );
  assert.equal(constructionTrack.stops, 1);

  FakeRecorder.throwDuringConstruction = false;
  const startupTrack = new FakeTrack();
  installMicrophone([startupTrack]);
  const capture = await openMicrophoneCapture({
    onError: assert.fail,
  });
  FakeRecorder.throwDuringStart = true;
  await assert.rejects(() => capture.start(() => {}), /startup failed/);
  assert.equal(startupTrack.stops, 1);
});

it("finalization timeout rejects stop and releases tracks exactly once", async () => {
  installRecorder();
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  const track = new FakeTrack();
  installMicrophone([track]);
  const capture = await openMicrophoneCapture({
    onError: assert.fail,
    finalizationTimeoutMs: 25,
  });
  await capture.start(() => {});
  const assertion = assert.rejects(capture.stop(), /finalization deadline/);
  vi.advanceTimersByTime(25);
  await assertion;
  capture.dispose();
  assert.equal(track.stops, 1);
});

it("a spontaneous recorder failure is reported once and cleans up after final bytes", async () => {
  installRecorder();
  const track = new FakeTrack();
  installMicrophone([track]);
  const frames: TransportFrame[] = [];
  const errors: Error[] = [];
  const capture = await openMicrophoneCapture({
    onError: (error) => errors.push(error),
  });
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

it("dispose is immediate and idempotent, and late events only warn", async () => {
  installRecorder();
  const warnings: unknown[][] = [];
  vi.spyOn(console, "warn").mockImplementation((...args: unknown[]) => {
    warnings.push(args);
  });
  const track = new FakeTrack();
  installMicrophone([track]);
  const frames: TransportFrame[] = [];
  const errors: Error[] = [];
  const capture = await openMicrophoneCapture({
    onError: (error) => errors.push(error),
  });
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
