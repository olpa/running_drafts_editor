import assert from "node:assert/strict";
import { test, type TestContext } from "node:test";
import { RecordingController, type RecordingBackend, type RecordingEvent } from "../src/recording.ts";
import { DEFAULT_CONFIG } from "../src/config.ts";
import type { CaptureOptions, RecordingCapture, TransportFrame } from "../src/capture.ts";
import { ApiError, type AudioAssetSummary, type FrameAck, type ProjectSummary } from "../src/api.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
async function settle(): Promise<void> {
  for (let i = 0; i < 16; i++) await Promise.resolve();
}
function ack(sequence: number): FrameAck {
  return { sequence, acknowledged: true, sha256: "a".repeat(64), nextExpectedSequence: sequence + 1 };
}
function sealed(id: string, reason: "completed" | "interrupted"): AudioAssetSummary {
  return { id, mediaType: "audio/webm;codecs=opus", isSealed: true, sealReason: reason,
    sealedAt: "2026-10-21T14:32:00Z", frameCount: 0, observedDurationMs: 0, storedBytes: 0 };
}

class FakeCapture implements RecordingCapture {
  private onFrame: ((frame: TransportFrame) => void) | null = null;
  private readonly stopped = deferred<void>();
  private seq = -1;
  private endMs = 0;
  starts = 0;
  stops = 0;
  disposed = false;
  constructor(readonly options: CaptureOptions) {}
  start(onFrame: (frame: TransportFrame) => void): void { this.starts++; this.onFrame = onFrame; }
  stop(): Promise<void> { this.stops++; return this.stopped.promise; }
  dispose(): void { this.disposed = true; }
  boundary(durationMs = 10_000, bytes = 10): boolean {
    this.emit(durationMs, bytes);
    return this.options.canContinue();
  }
  finalize(durationMs = 1_000, bytes = 10): void {
    this.emit(durationMs, bytes);
    this.stopped.resolve();
  }
  finalizeWithoutFrame(): void { this.stopped.resolve(); }
  fail(): void { this.options.onError(new Error("Microphone ended.")); }
  private emit(durationMs: number, bytes: number): void {
    const startMs = this.endMs;
    this.endMs += durationMs;
    this.onFrame?.({ seq: ++this.seq, startMs, endMs: this.endMs,
      mediaType: "audio/webm;codecs=opus", data: new Blob([new Uint8Array(bytes)]) });
  }
}

class Backend implements RecordingBackend {
  projectCalls: { key: string; title: string }[] = [];
  projectLoads: string[] = [];
  sectionCalls: { projectId: string; key: string }[] = [];
  requests: { assetId: string; frame: TransportFrame; signal: AbortSignal;
    result: ReturnType<typeof deferred<FrameAck>> }[] = [];
  completes: { assetId: string; frameCount: number }[] = [];
  interrupts: string[] = [];
  projectFailures = 0;
  sectionFailures = 0;
  completeFailures = 0;
  completeResult: AudioAssetSummary | null = null;
  interruptFailures = 0;
  createdSections = 0;

  async createProject(key: string, title: string): Promise<ProjectSummary> {
    this.projectCalls.push({ key, title });
    if (this.projectFailures-- > 0) throw new Error("project unavailable");
    return { id: "project-1", title: "Canonical backend title", createdAt: "now", updatedAt: "now" };
  }
  async getProject(projectId: string): Promise<ProjectSummary> {
    this.projectLoads.push(projectId);
    return { id: projectId, title: "Existing canonical title", createdAt: "before", updatedAt: "now" };
  }
  async createSection(projectId: string, key: string) {
    this.sectionCalls.push({ projectId, key });
    if (this.sectionFailures-- > 0) throw new Error("section unavailable");
    const number = ++this.createdSections;
    return { sectionId: `section-${number}`, audioAssetId: `asset-${number}`, position: number - 1 };
  }
  putFrame(assetId: string, frame: TransportFrame, signal: AbortSignal): Promise<FrameAck> {
    const result = deferred<FrameAck>();
    this.requests.push({ assetId, frame, signal, result });
    return result.promise;
  }
  async complete(assetId: string, frameCount: number): Promise<AudioAssetSummary> {
    this.completes.push({ assetId, frameCount });
    if (this.completeFailures-- > 0) throw new Error("complete unavailable");
    return this.completeResult ?? sealed(assetId, "completed");
  }
  async interrupt(assetId: string): Promise<AudioAssetSummary> {
    this.interrupts.push(assetId);
    if (this.interruptFailures-- > 0) throw new Error("interrupt unavailable");
    return sealed(assetId, "interrupted");
  }
  resolveRequest(index: number): void {
    const request = this.requests[index]!;
    request.result.resolve(ack(request.frame.seq));
  }
}

function harness(t: TestContext, options: { projectId?: string; maxFrameBytes?: number; maxRecordingMs?: number;
  open?: (capture: FakeCapture) => Promise<RecordingCapture> } = {}) {
  t.mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"], now: 0 });
  const backend = new Backend();
  const captures: FakeCapture[] = [];
  const events: RecordingEvent[] = [];
  const ids = ["00000000-0000-4000-8000-000000000001", "00000000-0000-4000-8000-000000000002",
    "00000000-0000-4000-8000-000000000003", "00000000-0000-4000-8000-000000000004"];
  const dependencies = {
    backend, now: () => Date.now(), wallClock: () => new Date("2026-10-21T14:32:00Z"),
    locale: "en-GB", uuid: () => ids.shift()!,
    openCapture: (captureOptions: CaptureOptions) => {
      const capture = new FakeCapture(captureOptions);
      captures.push(capture);
      return options.open ? options.open(capture) : Promise.resolve(capture);
    },
  };
  const context = options.projectId ? { accountId: "account-a", projectId: options.projectId }
    : { accountId: "account-a" };
  const controller = new RecordingController({ ...DEFAULT_CONFIG, maxRecordingMs: options.maxRecordingMs ?? 120_000,
    maxFrameBytes: options.maxFrameBytes ?? DEFAULT_CONFIG.maxFrameBytes }, dependencies, context);
  controller.subscribe((event) => events.push(event));
  t.after(() => controller.dispose());
  const cues = () => events.filter((event) => event.type === "cue").map((event) => event.cue);
  return { controller, backend, captures, events, cues };
}

test("adopts the canonical Project title and reuses creation keys across retries", async (t) => {
  const { controller, backend, captures } = harness(t);
  backend.projectFailures = 1;
  await controller.start();
  assert.equal(captures[0]!.disposed, true);
  assert.match(controller.snapshot.startProblem, /project unavailable/);
  backend.sectionFailures = 1;
  await controller.start();
  assert.equal(captures[1]!.disposed, true);
  await controller.start();

  assert.equal(controller.snapshot.project?.title, "Canonical backend title");
  assert.deepEqual(backend.projectCalls, [
    { key: "00000000-0000-4000-8000-000000000001", title: "Draft — 21 Oct 2026, 14:32" },
    { key: "00000000-0000-4000-8000-000000000001", title: "Draft — 21 Oct 2026, 14:32" },
  ]);
  assert.equal(backend.sectionCalls.length, 2);
  assert.equal(backend.sectionCalls[0]!.key, backend.sectionCalls[1]!.key);
  assert.equal(captures[2]!.starts, 1);
});

test("an existing Project ID loads and displays its canonical summary before appending", async (t) => {
  const { controller, backend } = harness(t, { projectId: "existing-project" });
  await settle();
  assert.equal(controller.snapshot.project?.title, "Existing canonical title");
  await controller.start();
  assert.deepEqual(backend.projectCalls, []);
  assert.deepEqual(backend.projectLoads, ["existing-project"]);
  assert.equal(backend.sectionCalls[0]!.projectId, "existing-project");
});

test("normal stop delivers the final partial frame before completing with assigned frame count", async (t) => {
  const { controller, backend, captures } = harness(t);
  await controller.start();
  captures[0]!.boundary(10_000);
  const stopping = controller.stop();
  assert.equal(controller.snapshot.sections[0]!.completion, "pending");
  captures[0]!.finalize(2_500);
  await stopping;
  assert.equal(controller.snapshot.sections[0]!.completion, "pending");
  assert.equal(controller.snapshot.canStart, true, "a new recorder need not wait for delivery or sealing");
  assert.deepEqual(backend.requests.map((request) => request.frame.seq), [0]);
  backend.resolveRequest(0);
  await settle();
  assert.deepEqual(backend.requests.map((request) => request.frame.seq), [0, 1]);
  assert.deepEqual(backend.completes, []);
  backend.resolveRequest(1);
  await settle();
  assert.deepEqual(backend.completes, [{ assetId: "asset-1", frameCount: 2 }]);
  assert.equal(controller.snapshot.sections[0]!.hasUnsecuredAudio, false);
  assert.equal(controller.snapshot.sections[0]!.completion, "succeeded");
});

test("duration stop finalizes capture and emits its stop cue only once", async (t) => {
  const { controller, backend, captures, cues } = harness(t, { maxRecordingMs: 500 });
  await controller.start();
  t.mock.timers.tick(500);
  assert.equal(captures[0]!.stops, 1);
  captures[0]!.finalize(500);
  await settle();
  backend.resolveRequest(0);
  await settle();
  assert.deepEqual(backend.completes, [{ assetId: "asset-1", frameCount: 1 }]);
  assert.equal(cues().filter((cue) => cue === "stop").length, 1);
  assert.equal(controller.snapshot.sections[0]!.problem, "");
});

test("a normally completed zero-frame Section succeeds from the backend result", async (t) => {
  const { controller, backend, captures } = harness(t);
  await controller.start();
  const stopping = controller.stop();
  assert.equal(controller.snapshot.sections[0]!.completion, "pending");
  captures[0]!.finalizeWithoutFrame();
  await stopping;

  assert.deepEqual(backend.completes, [{ assetId: "asset-1", frameCount: 0 }]);
  assert.equal(controller.snapshot.sections[0]!.completion, "succeeded");
});

test("completion failure keeps its original command retryable without blocking another Section", async (t) => {
  const { controller, backend, captures } = harness(t);
  backend.completeFailures = 1;
  await controller.start();
  const stopping = controller.stop();
  captures[0]!.finalize(500);
  await stopping;
  backend.resolveRequest(0);
  await settle();
  const first = controller.snapshot.sections[0]!;
  assert.equal(first.completion, "failed");
  assert.equal(first.completionProblem, "Could not complete recording.");
  assert.equal(first.canRetryCompletion, true);
  await controller.start();
  assert.equal(captures[1]!.starts, 1);
  controller.retryClose(first.sectionId);
  assert.equal(controller.snapshot.sections[0]!.completion, "pending");
  await settle();
  assert.deepEqual(backend.completes, [
    { assetId: "asset-1", frameCount: 1 },
    { assetId: "asset-1", frameCount: 1 },
  ]);
  assert.equal(controller.snapshot.sections[0]!.completion, "succeeded");
});

test("a sealed non-completed response is a terminal completion failure", async (t) => {
  const { controller, backend, captures } = harness(t);
  backend.completeResult = sealed("asset-1", "interrupted");
  await controller.start();
  const stopping = controller.stop();
  captures[0]!.finalize(0, 0);
  await stopping;
  backend.resolveRequest(0);
  await settle();

  const section = controller.snapshot.sections[0]!;
  assert.equal(section.completion, "failed");
  assert.equal(section.completionProblem,
    "Recording was sealed as interrupted and cannot be completed.");
  assert.equal(section.canRetryCompletion, false);
  assert.equal(controller.snapshot.hasPendingWork, false);
});

test("an unsealed completion response remains retryable", async (t) => {
  const { controller, backend, captures } = harness(t);
  backend.completeResult = { ...sealed("asset-1", "completed"), isSealed: false,
    sealReason: null, sealedAt: null };
  await controller.start();
  const stopping = controller.stop();
  captures[0]!.finalizeWithoutFrame();
  await stopping;

  const section = controller.snapshot.sections[0]!;
  assert.equal(section.completion, "failed");
  assert.equal(section.completionProblem, "Could not complete recording.");
  assert.equal(section.canRetryCompletion, true);
  assert.equal(controller.snapshot.hasPendingWork, true);
});

test("a sealed completion without a reason is a specific terminal failure", async (t) => {
  const { controller, backend, captures } = harness(t);
  backend.completeResult = { ...sealed("asset-1", "completed"), sealReason: null };
  await controller.start();
  const stopping = controller.stop();
  captures[0]!.finalizeWithoutFrame();
  await stopping;

  const section = controller.snapshot.sections[0]!;
  assert.equal(section.completion, "failed");
  assert.equal(section.completionProblem,
    "Recording was sealed without normal completion and cannot be completed.");
  assert.equal(section.canRetryCompletion, false);
  assert.equal(controller.snapshot.hasPendingWork, false);
});

test("backlog retains three frames while a new Section uploads concurrently and old cues stay silent", async (t) => {
  const { controller, backend, captures, cues } = harness(t);
  await controller.start();
  assert.equal(captures[0]!.boundary(), true);
  assert.equal(captures[0]!.boundary(), false);
  captures[0]!.finalize();
  await settle();
  assert.equal(controller.snapshot.sections[0]!.pendingFrames, 3);
  assert.equal(controller.snapshot.sections[0]!.pendingMs, 21_000);
  assert.equal(controller.snapshot.sections[0]!.canUploadPending, true);
  const frozenAttempts = backend.requests.length;
  t.mock.timers.tick(40_000);
  assert.equal(backend.requests.length, frozenAttempts);

  await controller.start();
  captures[1]!.boundary();
  const newAssetRequest = backend.requests.findIndex((request) => request.assetId !== "asset-1");
  assert.notEqual(newAssetRequest, -1);
  backend.resolveRequest(0);
  await settle();
  assert.equal(cues().filter((cue) => cue === "health").length, 0, "old acknowledgement is silent");
  backend.resolveRequest(newAssetRequest);
  await settle();
  assert.equal(cues().filter((cue) => cue === "health").length, 1);

  const oldSection = controller.snapshot.sections[0]!;
  controller.uploadPendingAudio(oldSection.sectionId);
  const recoveryStart = backend.requests.length - 1;
  assert.equal(backend.requests[recoveryStart]!.assetId, "asset-1");
  backend.resolveRequest(recoveryStart);
  await settle();
  backend.resolveRequest(backend.requests.length - 1);
  await settle();
  assert.deepEqual(backend.interrupts, ["asset-1"]);
  assert.equal(controller.snapshot.sections[0]!.completion, null);
  assert.equal(cues().filter((cue) => cue === "health").length, 1);
});

test("interruption close failure is retryable and does not block a later recorder", async (t) => {
  const { controller, backend, captures } = harness(t);
  backend.interruptFailures = 1;
  await controller.start();
  captures[0]!.fail();
  captures[0]!.finalize(500);
  await settle();
  const sectionId = controller.snapshot.sections[0]!.sectionId;
  controller.uploadPendingAudio(sectionId);
  backend.resolveRequest(0);
  await settle();
  assert.match(controller.snapshot.sections[0]!.closeProblem, /Could not close recording/);
  await controller.start();
  assert.equal(captures[1]!.starts, 1);
  controller.retryClose(sectionId);
  await settle();
  assert.deepEqual(backend.interrupts, ["asset-1", "asset-1"]);
});

test("authentication suspension freezes every Section and same-Account resume restarts each queue", async (t) => {
  const { controller, backend, captures } = harness(t);
  await controller.start();
  captures[0]!.boundary();
  const firstStop = controller.stop();
  captures[0]!.finalize();
  await firstStop;
  await controller.start();
  captures[1]!.boundary();
  controller.suspendForAuthentication();
  const attempts = backend.requests.length;
  t.mock.timers.tick(40_000);
  assert.equal(backend.requests.length, attempts);
  assert.equal(controller.snapshot.hasUnsecuredAudio, true);
  controller.resumeAfterAuthentication("account-b");
  assert.equal(backend.requests.length, attempts, "a different Account cannot resume delivery");
  controller.resumeAfterAuthentication("account-a");
  assert.equal(backend.requests.length, attempts + 2);
  assert.equal(new Set(backend.requests.slice(-2).map((request) => request.assetId)).size, 2);
});

test("oversize audio remains retained and stops automatic submission", async (t) => {
  const { controller, backend, captures } = harness(t, { maxFrameBytes: 8 });
  await controller.start();
  captures[0]!.boundary(1_000, 9);
  captures[0]!.finalize(100, 1);
  await settle();
  assert.equal(backend.requests.length, 0);
  assert.equal(controller.snapshot.sections[0]!.pendingFrames, 2);
  assert.equal(controller.snapshot.sections[0]!.canUploadPending, false);
  assert.match(controller.snapshot.sections[0]!.problem, /too large to upload/);
  controller.uploadPendingAudio(controller.snapshot.sections[0]!.sectionId);
  assert.equal(backend.requests.length, 0, "manual recovery must enforce the same size guard");
});

test("frame conflict terminally retains the Section and permits an independent new recording", async (t) => {
  const { controller, backend, captures, events } = harness(t);
  await controller.start();
  captures[0]!.boundary(1_250, 13);
  const originalFrame = backend.requests[0]!.frame;

  t.mock.timers.tick(DEFAULT_CONFIG.retryFirstMs);
  assert.equal(backend.requests.length, 2, "the conflict races an identical retry");
  assert.equal(backend.requests[1]!.frame, originalFrame);
  assert.equal(backend.requests[1]!.frame.data, originalFrame.data);

  backend.requests[0]!.result.reject(new ApiError("PUT", "/frames/0", 409, {
    title: "Frame conflict",
    detail: "Sequence 0 differs from stored audio.",
    code: "frame_conflict",
    referenceId: "conflict_REF-42",
  }));
  await settle();
  assert.equal(captures[0]!.stops, 1, "a conflict stops active capture");
  assert.equal(backend.requests[1]!.signal.aborted, true, "overlapping delivery is cancelled");

  captures[0]!.finalize(750, 7);
  await settle();
  const conflicted = controller.snapshot.sections[0]!;
  assert.equal(conflicted.pendingFrames, 2, "the conflicting and final frames remain retained");
  assert.equal(conflicted.pendingMs, 2_000, "retained frame timing remains intact");
  assert.equal(conflicted.canUploadPending, false, "unsafe manual recovery stays hidden");
  assert.match(conflicted.problem, /Capture stopped because audio delivery conflicted with stored data/u);
  assert.match(conflicted.problem, /Title: Frame conflict/u);
  assert.match(conflicted.problem, /Detail: Sequence 0 differs from stored audio\./u);
  assert.match(conflicted.problem, /Code: frame_conflict/u);
  assert.match(conflicted.problem, /Reference ID: conflict_REF-42/u);

  backend.resolveRequest(1);
  await settle();
  assert.equal(controller.snapshot.sections[0]!.pendingFrames, 2,
    "a late acknowledgement cannot release terminally retained bytes");
  const frozenAttempts = backend.requests.length;
  t.mock.timers.tick(60_000);
  controller.uploadPendingAudio(conflicted.sectionId);
  assert.equal(backend.requests.length, frozenAttempts, "timers and recovery cannot send again");
  assert.deepEqual(backend.completes, []);
  assert.deepEqual(backend.interrupts, []);
  assert.ok(events.some((event) => event.type === "log"
    && event.message.includes("HTTP 409 code=frame_conflict referenceId=conflict_REF-42")));

  await controller.start();
  assert.equal(captures[1]!.starts, 1);
  captures[1]!.boundary(500, 5);
  assert.equal(backend.requests.at(-1)!.assetId, "asset-2");
  assert.equal(backend.requests.at(-1)!.frame.seq, 0, "the new Section has an independent sequence");
  assert.equal(controller.snapshot.sections[0]!.pendingFrames, 2);
});

test("same-Account restoration keeps backlog recovery user initiated", async (t) => {
  const { controller, backend, captures } = harness(t);
  await controller.start();
  assert.equal(captures[0]!.boundary(), true);
  assert.equal(captures[0]!.boundary(), false);
  captures[0]!.finalize();
  await settle();
  const attempts = backend.requests.length;
  const sectionId = controller.snapshot.sections[0]!.sectionId;

  controller.suspendForAuthentication();
  controller.resumeAfterAuthentication("account-a");
  assert.equal(backend.requests.length, attempts);
  controller.uploadPendingAudio(sectionId);
  assert.equal(backend.requests.length, attempts + 1);

  controller.suspendForAuthentication();
  controller.resumeAfterAuthentication("account-a");
  assert.equal(backend.requests.length, attempts + 2,
    "an already requested recovery resumes for the original Account");
});

test("capability failure creates no backend resources and permits retry", async (t) => {
  const { controller, backend } = harness(t, { open: async () => { throw new Error("mono unavailable"); } });
  await controller.start();
  assert.deepEqual(backend.projectCalls, []);
  assert.deepEqual(backend.sectionCalls, []);
  assert.equal(controller.snapshot.canStart, true);
  assert.match(controller.snapshot.startProblem, /mono unavailable/);
});
