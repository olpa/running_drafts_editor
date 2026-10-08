import assert from "node:assert/strict";
import { test, type TestContext } from "node:test";
import { RecordingController, type RecordingBackend, type RecordingEvent } from "../src/recording.ts";
import { DEFAULT_CONFIG } from "../src/config.ts";
import type { CaptureOptions, RecordingCapture, TransportFrame } from "../src/capture.ts";
import type { CreatedRecording, FrameAck, Language, RecordingStatus } from "../src/api.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
async function settle(): Promise<void> {
  for (let i = 0; i < 12; i++) await Promise.resolve();
}
class FakeCapture implements RecordingCapture {
  private onFrame: ((frame: TransportFrame) => void) | null = null;
  private readonly stopped = deferred<void>();
  private seq = 0;
  private endMs = 0;
  starts = 0;
  stops = 0;
  disposed = false;
  constructor(readonly options: CaptureOptions) {}
  start(onFrame: (frame: TransportFrame) => void): void { this.starts++; this.onFrame = onFrame; }
  stop(): Promise<void> { this.stops++; return this.stopped.promise; }
  dispose(): void { this.disposed = true; }
  boundary(durationMs = 10_000): boolean {
    if (!this.options.canContinue()) return false;
    this.emit(durationMs);
    return true;
  }
  finalize(durationMs: number): void {
    this.emit(durationMs);
    this.stopped.resolve();
  }
  fail(): void { this.options.onError(new Error("Microphone ended.")); }
  private emit(durationMs: number): void {
    const startMs = this.endMs;
    this.endMs += durationMs;
    this.onFrame?.({ seq: ++this.seq, startMs, endMs: this.endMs,
      mediaType: "audio/webm;codecs=opus", data: new Blob([`audio ${this.seq}`]) });
  }
}
class Backend implements RecordingBackend {
  languages: Language[] = [];
  finishes: string[] = [];
  requests: { recordingId: string; frame: TransportFrame; signal: AbortSignal;
    result: ReturnType<typeof deferred<FrameAck>> }[] = [];
  status: RecordingStatus = { status: "recording", chunks: [] };
  async createRecording(language: Language): Promise<CreatedRecording> {
    this.languages.push(language);
    return { recordingId: `recording-${this.languages.length}` };
  }
  putFrame(recordingId: string, frame: TransportFrame, signal: AbortSignal): Promise<FrameAck> {
    const result = deferred<FrameAck>();
    this.requests.push({ recordingId, frame, signal, result });
    return result.promise;
  }
  async finish(recordingId: string): Promise<void> { this.finishes.push(recordingId); }
  async getRecording(_recordingId: string): Promise<RecordingStatus> { return this.status; }
  ack(index: number): void {
    const request = this.requests[index]!;
    request.result.resolve({ seq: request.frame.seq, acknowledged: true });
  }
}
function harness(t: TestContext, options: {
  maxRecordingMs?: number;
  pollIntervalMs?: number;
  open?: (capture: FakeCapture) => Promise<RecordingCapture>;
} = {}) {
  t.mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"], now: 0 });
  const backend = new Backend();
  const captures: FakeCapture[] = [];
  const events: RecordingEvent[] = [];
  const controller = new RecordingController({ ...DEFAULT_CONFIG,
    maxRecordingMs: options.maxRecordingMs ?? 120_000,
    pollIntervalMs: options.pollIntervalMs ?? 100_000,
  }, {
    backend, now: () => Date.now(),
    openCapture: (captureOptions) => {
      const capture = new FakeCapture(captureOptions);
      captures.push(capture);
      return options.open ? options.open(capture) : Promise.resolve(capture);
    },
  });
  controller.subscribe((event) => events.push(event));
  t.after(() => controller.dispose());
  const cues = () => events.filter((event) => event.type === "cue").map((event) => event.cue);
  return { controller, backend, captures, events, cues };
}

test("stop waits for final audio and its acknowledgement before finish; completion allows another recording", async (t) => {
  const { controller, backend, captures, events, cues } = harness(t);
  await controller.start("de");
  assert.equal(controller.snapshot.state, "recording");
  assert.equal(controller.snapshot.canStart, false);
  assert.equal(controller.snapshot.hasUnsecuredAudio, true);
  t.mock.timers.tick(10_000);
  captures[0]!.boundary();
  backend.ack(0);
  await settle();
  assert.equal(controller.snapshot.securedMs, 10_000);
  t.mock.timers.tick(2_500);
  assert.equal(controller.snapshot.pendingMs, 2_500);
  const stop = controller.stop();
  assert.equal(controller.snapshot.state, "finishing");
  assert.equal(controller.snapshot.capturing, false);
  assert.equal(controller.snapshot.pendingFrames, 0);
  assert.equal(controller.snapshot.pendingMs, 2_500);
  assert.equal(controller.snapshot.hasUnsecuredAudio, true);
  await settle();
  assert.equal(controller.snapshot.hasUnsecuredAudio, true);
  assert.deepEqual(backend.finishes, []);
  captures[0]!.finalize(2_500);
  await stop;
  assert.deepEqual(backend.finishes, []);
  assert.equal(controller.snapshot.pendingMs, 2_500);
  assert.equal(controller.snapshot.hasUnsecuredAudio, true);
  backend.ack(1);
  await settle();
  assert.deepEqual(backend.finishes, ["recording-1"]);
  assert.equal(controller.snapshot.hasUnsecuredAudio, false);
  assert.equal(controller.snapshot.canStart, false);
  backend.status = { status: "complete", chunks: [{ id: "chunk-1", startMs: 0, endMs: 12_500, text: "Text." }] };
  t.mock.timers.tick(100_000);
  await settle();
  assert.equal(controller.snapshot.state, "complete");
  assert.equal(controller.snapshot.canStart, true);
  assert.deepEqual(cues(), ["start", "health", "health", "stop"]);
  assert.equal(events.filter((event) => event.type === "chunks").length, 1);
  await controller.start("ru");
  assert.equal(captures.length, 2);
  assert.deepEqual(backend.languages, ["de", "ru"]);
  assert.equal(controller.snapshot.securedMs, 0);
  assert.equal(events.filter((event) => event.type === "reset").length, 2);
});

test("delay warns once; the third boundary stops capture, retains audio, and cancels future attempts", async (t) => {
  const { controller, backend, captures, cues } = harness(t);
  await controller.start("en");
  t.mock.timers.tick(10_000);
  assert.equal(captures[0]!.boundary(), true);
  t.mock.timers.tick(5_000);
  assert.equal(controller.snapshot.feedback, "Audio upload delayed. Recording continues.");
  assert.equal(controller.snapshot.capturing, true);
  t.mock.timers.tick(5_000);
  assert.equal(captures[0]!.boundary(), true);
  t.mock.timers.tick(10_000);
  assert.equal(captures[0]!.boundary(), false);
  assert.equal(controller.snapshot.state, "error");
  assert.equal(controller.snapshot.capturing, false);
  assert.equal(controller.snapshot.canStart, false);
  assert.equal(controller.snapshot.feedback, "Recording stopped because audio could not be uploaded.");
  captures[0]!.finalize(10_000);
  await settle();
  assert.equal(controller.snapshot.pendingFrames, 3);
  assert.equal(controller.snapshot.pendingMs, 30_000);
  assert.equal(controller.snapshot.canRetry, true);
  const attempts = backend.requests.length;
  t.mock.timers.tick(20_000);
  assert.equal(backend.requests.length, attempts);
  assert.deepEqual(cues(), ["start", "delayed", "interrupted"]);
  await controller.start("ru");
  assert.equal(captures.length, 1);

  // An earlier request can succeed after stop, without sending later frames or restarting capture.
  backend.ack(0);
  await settle();
  assert.equal(controller.snapshot.pendingFrames, 2);
  assert.equal(backend.requests.length, attempts);
  assert.equal(controller.snapshot.capturing, false);
  backend.ack(1);
  await settle();
  assert.equal(cues().filter((cue) => cue === "health").length, 1);

  controller.retry();
  assert.equal(controller.snapshot.state, "finishing");
  assert.equal(captures[0]!.starts, 1);
  assert.equal(captures[0]!.stops, 1);
  assert.equal(backend.requests[attempts]!.recordingId, "recording-1");
  assert.equal(backend.requests[attempts]!.frame.seq, 2);
  backend.ack(attempts);
  await settle();
  backend.ack(attempts + 1);
  await settle();
  assert.equal(controller.snapshot.pendingFrames, 0);
  assert.equal(controller.snapshot.feedback, "");
  assert.deepEqual(backend.finishes, ["recording-1"]);
  assert.equal(cues().filter((cue) => cue === "health").length, 3);
  assert.deepEqual(backend.languages, ["en"]);
});

test("automatic duration stop announces capture cessation before final audio or processing completes", async (t) => {
  const { controller, backend, captures, cues } = harness(t, { maxRecordingMs: 12_500, pollIntervalMs: 100 });
  const getRecording = backend.getRecording.bind(backend);
  backend.getRecording = async () => { throw new Error("processing unavailable"); };
  await controller.start("en");
  t.mock.timers.tick(10_000);
  captures[0]!.boundary();
  backend.ack(0);
  await settle();
  t.mock.timers.tick(2_500);
  assert.equal(controller.snapshot.capturing, false);
  assert.equal(controller.snapshot.elapsedMs, 12_500);
  assert.equal(controller.snapshot.feedback, "Recording stopped at the time limit.");
  assert.equal(controller.snapshot.hasUnsecuredAudio, true);
  assert.equal(controller.snapshot.pendingMs, 2_500);
  assert.deepEqual(cues(), ["start", "health", "stop"]);
  captures[0]!.finalize(2_500);
  await settle();
  backend.ack(1);
  await settle();
  t.mock.timers.tick(10_000);
  await settle();
  assert.equal(controller.snapshot.elapsedMs, 12_500);
  assert.equal(controller.snapshot.state, "finishing");
  assert.equal(controller.snapshot.hasUnsecuredAudio, false);
  assert.deepEqual(backend.finishes, ["recording-1"]);
  assert.deepEqual(cues(), ["start", "health", "stop", "health"]);
  backend.getRecording = getRecording;
  backend.status = { status: "complete", chunks: [] };
  t.mock.timers.tick(100);
  await settle();
  assert.equal(controller.snapshot.state, "complete");
  assert.equal(cues().filter((cue) => cue === "stop").length, 1);
});

test("microphone failure announces interruption and preserves the final frame for manual recovery", async (t) => {
  const { controller, backend, captures, cues } = harness(t);
  await controller.start("en");
  t.mock.timers.tick(1_000);
  captures[0]!.fail();
  assert.equal(controller.snapshot.feedback, "Recording stopped because the microphone became unavailable.");
  captures[0]!.finalize(1_000);
  await settle();
  assert.equal(backend.requests.length, 0);
  assert.equal(controller.snapshot.pendingMs, 1_000);
  assert.equal(controller.snapshot.canRetry, true);
  assert.deepEqual(cues(), ["start", "interrupted"]);
  controller.retry();
  backend.ack(0);
  await settle();
  assert.deepEqual(backend.finishes, ["recording-1"]);
  assert.equal(captures[0]!.starts, 1);
});

test("slow processing and failed polls do not block audio acceptance or trigger an upload warning", async (t) => {
  const { controller, backend, captures, cues } = harness(t, { pollIntervalMs: 100 });
  backend.getRecording = async () => { throw new Error("processing unavailable"); };
  await controller.start("en");
  t.mock.timers.tick(10_000);
  await settle();
  captures[0]!.boundary();
  backend.ack(0);
  await settle();
  assert.equal(controller.snapshot.securedMs, 10_000);
  assert.equal(controller.snapshot.capturing, true);
  assert.equal(controller.snapshot.feedback, "");
  assert.deepEqual(cues(), ["start", "health"]);
});

test("finish failure can be retried without resubmitting acknowledged bytes or restarting capture", async (t) => {
  const { controller, backend, captures } = harness(t);
  let failures = 1;
  backend.finish = async (id) => {
    if (failures > 0) { failures--; throw new Error("finish unavailable"); }
    backend.finishes.push(id);
  };
  await controller.start("en");
  const stopping = controller.stop();
  captures[0]!.finalize(500);
  await stopping;
  backend.ack(0);
  await settle();
  assert.equal(controller.snapshot.state, "error");
  assert.equal(controller.snapshot.canRetry, true);
  assert.match(controller.snapshot.feedback, /finish unavailable/);
  controller.retry();
  await settle();
  assert.deepEqual(backend.finishes, ["recording-1"]);
  assert.equal(backend.requests.length, 1);
  assert.equal(captures[0]!.starts, 1);
});

test("microphone denial remains recoverable without creating a backend recording", async (t) => {
  const { controller, backend, cues } = harness(t, { open: async () => { throw new Error("permission denied"); } });
  await controller.start("en");
  assert.equal(controller.snapshot.state, "error");
  assert.equal(controller.snapshot.canStart, true);
  assert.equal(controller.snapshot.canRetry, false);
  assert.match(controller.snapshot.feedback, /permission denied/);
  assert.deepEqual(backend.languages, []);
  assert.deepEqual(cues(), []);
});

test("backend creation failure releases the microphone and permits a fresh start", async (t) => {
  const { controller, backend, captures } = harness(t);
  const create = backend.createRecording.bind(backend);
  backend.createRecording = async () => { throw new Error("create unavailable"); };
  await controller.start("en");
  assert.equal(captures[0]!.disposed, true);
  assert.equal(captures[0]!.starts, 0);
  assert.equal(controller.snapshot.canStart, true);
  backend.createRecording = create;
  await controller.start("de");
  assert.equal(controller.snapshot.state, "recording");
  assert.equal(captures[1]!.starts, 1);
});

test("a second start during microphone acquisition is ignored; disposal releases late acquisition", async (t) => {
  const acquisition = deferred<RecordingCapture>();
  const { controller, backend, captures, events } = harness(t, { open: () => acquisition.promise });
  const starting = controller.start("en");
  await controller.start("de");
  assert.equal(captures.length, 1);
  assert.equal(controller.snapshot.canStart, false);
  controller.dispose();
  const count = events.length;
  acquisition.resolve(captures[0]!);
  await starting;
  assert.equal(captures[0]!.disposed, true);
  assert.equal(captures[0]!.starts, 0);
  assert.equal(events.length, count);
  assert.deepEqual(backend.languages, []);
});

test("disposal during backend creation suppresses stale start and subsequent events", async (t) => {
  const creation = deferred<CreatedRecording>();
  const { controller, backend, captures, events } = harness(t);
  backend.createRecording = () => creation.promise;
  const starting = controller.start("en");
  await settle();
  controller.dispose();
  const count = events.length;
  creation.resolve({ recordingId: "late-recording" });
  await starting;
  assert.equal(captures[0]!.disposed, true);
  assert.equal(captures[0]!.starts, 0);
  t.mock.timers.tick(120_000);
  assert.equal(events.length, count);
});

test("a subscriber can dispose on start without leaving timers or polling alive", async (t) => {
  const { controller, backend, captures, cues } = harness(t, { pollIntervalMs: 100 });
  let polls = 0;
  backend.getRecording = async () => {
    polls++;
    return backend.status;
  };
  controller.subscribe((event) => {
    if (event.type === "changed" && event.snapshot.state === "recording") controller.dispose();
  });
  await controller.start("en");
  assert.equal(captures[0]!.disposed, true);
  assert.equal(controller.snapshot.capturing, false);
  assert.equal(controller.snapshot.hasUnsecuredAudio, false);
  t.mock.timers.tick(120_000);
  await settle();
  assert.equal(polls, 0);
  assert.deepEqual(cues(), []);
});
