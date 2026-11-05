import { afterEach, describe, expect, it } from "vitest";
import { createCaptureAlerts } from "../alerts/capture-alerts.ts";
import { DEFAULT_CONFIG } from "../shared/config.ts";
import type { AudioAssetSummary } from "../shared/project-types.ts";
import { ManualScheduler, settle } from "../test-support/manual-scheduler.ts";
import type { CaptureOptions, MicrophoneCapture, TransportFrame } from "./capture.ts";
import { createCaptureAndDelivery, type CaptureAndDelivery } from "./capture-and-delivery.ts";
import { createCaptureFeedback } from "./capture-feedback.ts";
import type { FrameAck, RecordingGateway } from "./gateway.ts";
import { createMicrophoneAccess } from "./microphone-access.ts";
import type { SectionEvent } from "./section-events.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
const ack = (sequence: number): FrameAck => ({
  sequence,
  acknowledged: true,
  nextExpectedSequence: sequence + 1,
  sha256: "a".repeat(64),
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
class Microphone implements MicrophoneCapture {
  private sink: ((frame: TransportFrame) => void) | null = null;
  private sequence = 0;
  private end = 0;
  requests = 0;
  stops = 0;
  released = false;
  autoBlob = true;
  finalBlob = true;
  startFailure = false;
  stopFailure = false;
  stalledStop = false;
  readonly stopping = deferred<void>();
  constructor(
    readonly clock: ManualScheduler,
    readonly options: CaptureOptions,
  ) {}
  async start(sink: (frame: TransportFrame) => void) {
    if (this.startFailure) throw new Error("start failed");
    this.sink = sink;
  }
  requestFrame(): void {
    this.requests++;
    if (this.autoBlob) this.emit();
  }
  emit(): void {
    const startMs = this.end;
    this.end = Math.max(this.end + 1, this.clock.now());
    this.sink?.({
      seq: this.sequence++,
      startMs,
      endMs: this.end,
      mediaType: "audio/webm;codecs=opus",
      data: new Blob(["audio"]),
    });
  }
  async stop(): Promise<void> {
    this.stops++;
    if (this.stalledStop) return this.stopping.promise;
    if (this.stopFailure) throw new Error("finalization failed");
    if (this.finalBlob) this.emit();
    this.released = true;
  }
  dispose(): void {
    this.released = true;
    this.stopping.resolve();
  }
}
const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
});
function harness(
  config: { frameIntervalMs?: number; secureDeadlineMs?: number; maxRecordingMs?: number } = {},
) {
  const scheduler = new ManualScheduler();
  const access = createMicrophoneAccess();
  const microphones: Microphone[] = [];
  const events: SectionEvent[] = [];
  const sounds: string[] = [];
  const requests: {
    asset: string;
    frame: TransportFrame;
    result: ReturnType<typeof deferred<FrameAck>>;
  }[] = [];
  let closeFailures = 0;
  let completeCount = 0;
  let created = 0;
  let startupFailure = false;
  let permissionFailure = false;
  const cues = {
    prepare() {},
    play: (cue: string) => sounds.push(cue),
    loopProblem: () => {
      sounds.push("problem");
      return () => {};
    },
    dispose() {},
  };
  const alerts = createCaptureAlerts(cues);
  const feedback = createCaptureFeedback(cues, alerts);
  const gateway: RecordingGateway = {
    createProject: async () => {
      throw new Error("unused");
    },
    getProject: async () => {
      throw new Error("unused");
    },
    createSection: async () => {
      throw new Error("unused");
    },
    putFrame(asset, frame) {
      const result = deferred<FrameAck>();
      requests.push({ asset, frame, result });
      return result.promise;
    },
    async complete() {
      completeCount++;
      if (closeFailures-- > 0) throw new Error("sealing failed");
      return sealed("completed");
    },
    async interrupt() {
      if (closeFailures-- > 0) throw new Error("sealing failed");
      return sealed("interrupted");
    },
  };
  function create(): CaptureAndDelivery {
    let sectionId = "uncreated";
    const workflow = createCaptureAndDelivery({
      config: { ...DEFAULT_CONFIG, ...config },
      gateway,
      scheduler,
      microphoneAccess: access,
      async openCapture(options) {
        if (permissionFailure) throw new Error("permission denied");
        const microphone = new Microphone(scheduler, options);
        microphone.startFailure = startupFailure;
        microphones.push(microphone);
        return microphone;
      },
      onCreated(created) {
        sectionId = created.sectionId;
      },
      log() {},
      onEvent(event) {
        events.push(event);
        feedback(sectionId, event);
      },
    });
    cleanups.push(() => workflow.dispose());
    return workflow;
  }
  async function start(workflow = create()) {
    await workflow.start(async () => ({
      sectionId: `section-${++created}`,
      audioAssetId: `asset-${created}`,
      position: created - 1,
    }));
    return workflow;
  }
  async function secure(index: number) {
    const request = requests[index]!;
    request.result.resolve(ack(request.frame.seq));
    await settle();
  }
  cleanups.push(() => {
    alerts.dispose();
    access.dispose();
  });
  return {
    scheduler,
    access,
    events,
    sounds,
    alerts,
    microphones,
    requests,
    create,
    start,
    secure,
    failCloses(count: number) {
      closeFailures = count;
    },
    failStartup() {
      startupFailure = true;
    },
    failPermission() {
      permissionFailure = true;
    },
    get completeCount() {
      return completeCount;
    },
  };
}

describe("CaptureAndDelivery", () => {
  it("shares one securing budget between Blob production and HTTP acknowledgement", async () => {
    const h = harness();
    const workflow = await h.start();
    const microphone = h.microphones[0]!;
    microphone.autoBlob = false;
    await h.scheduler.advanceBy(5_000);
    expect(microphone.requests).toBe(1);
    expect(workflow.view.value.pendingFrames).toBe(1);
    await h.scheduler.advanceBy(1800);
    microphone.emit();
    await h.scheduler.advanceBy(200);
    expect(h.sounds).toEqual(["capture-started", "problem"]);
    expect(workflow.view.value.overdueFrames).toEqual([{ frameId: 0, elapsedSeconds: 2 }]);
    await h.scheduler.advanceBy(1000);
    expect(workflow.view.value.overdueFrames[0]!.elapsedSeconds).toBe(3);
    await h.scheduler.advanceBy(1000);
    expect(workflow.view.value.overdueFrames[0]!.elapsedSeconds).toBe(4);
    await h.secure(0);
    expect(workflow.view.value.overdueFrames).toEqual([]);
    expect(h.sounds).toEqual(["capture-started", "problem", "healthy-heartbeat"]);
  });
  it("acknowledges a healthy frame immediately and ignores its obsolete deadline", async () => {
    const h = harness();
    const workflow = await h.start();
    await h.scheduler.advanceBy(5_000);
    await h.secure(0);
    expect(h.sounds).toEqual(["capture-started", "healthy-heartbeat"]);
    await h.scheduler.advanceBy(2000);
    expect(workflow.view.value.overdueFrames).toEqual([]);
    expect(h.sounds).toEqual(["capture-started", "healthy-heartbeat"]);
  });
  it.each(["before", "after"])(
    "lets an exact-deadline acknowledgement win when registered %s the deadline",
    async (order) => {
      const h = harness();
      const workflow = await h.start();
      if (order === "before")
        h.scheduler.schedule(7_000, () => {
          const r = h.requests[0]!;
          r.result.resolve(ack(0));
        });
      await h.scheduler.advanceBy(5_000);
      if (order === "after")
        h.scheduler.schedule(2000, () => {
          h.requests[0]!.result.resolve(ack(0));
        });
      await h.scheduler.advanceBy(2000);
      expect(h.sounds).toEqual(["capture-started", "healthy-heartbeat"]);
      expect(workflow.view.value.overdueFrames).toEqual([]);
    },
  );
  it("does not count scheduler drift before the actual frame request as a failure", async () => {
    const h = harness();
    await h.start();
    h.scheduler.jumpBy(5_400);
    await h.scheduler.advanceBy(0);
    expect(h.microphones[0]!.requests).toBe(1);
    expect(h.sounds).toEqual(["capture-started"]);
    await h.secure(0);
    await h.scheduler.advanceBy(4600);
    expect(h.microphones[0]!.requests).toBe(1);
    await h.scheduler.advanceBy(400);
    expect(h.microphones[0]!.requests).toBe(2);
  });
  it("counts missing Blobs in backlog and warns independently for every request", async () => {
    const h = harness();
    const workflow = await h.start();
    h.microphones[0]!.autoBlob = false;
    h.microphones[0]!.finalBlob = false;
    await h.scheduler.advanceBy(7_000);
    expect(h.sounds).toEqual(["capture-started", "problem"]);
    await h.scheduler.advanceBy(5_000);
    expect(h.events.filter((e) => e.type === "unrequestedCaptureStopped")).toHaveLength(1);
    expect(h.microphones[0]!.stops).toBe(1);
    expect(workflow.view.value.pendingFrames).toBe(2);
    expect(workflow.view.value.overdueFrames.map((f) => f.frameId)).toEqual([0, 1]);
    expect(h.events.filter((e) => e.type === "transportFrameOverdue")).toHaveLength(2);
    expect(h.completeCount).toBe(0);
  });
  it("retries once before allowing the second frame and interrupting stalled Capture", async () => {
    const h = harness();
    await h.start();

    await h.scheduler.advanceBy(5_000);
    expect(h.requests.map((request) => request.frame.seq)).toEqual([0]);
    await h.scheduler.advanceBy(2_000);
    expect(h.events.filter((event) => event.type === "transportFrameOverdue")).toHaveLength(1);
    await h.scheduler.advanceBy(1_000);
    expect(h.requests.map((request) => request.frame.seq)).toEqual([0, 0]);
    expect(h.events.some((event) => event.type === "unrequestedCaptureStopped")).toBe(false);

    await h.scheduler.advanceBy(2_000);
    expect(h.microphones[0]!.requests).toBe(2);
    expect(h.events.filter((event) => event.type === "unrequestedCaptureStopped")).toHaveLength(1);
  });
  it("resolves a final-frame alert without claiming Completion while the stop alert remains", async () => {
    const h = harness({ maxRecordingMs: 500 });
    const workflow = await h.start();
    await h.scheduler.advanceBy(2500);
    expect(h.events.map((e) => e.type)).toEqual([
      "captureStarted",
      "unrequestedCaptureStopped",
      "transportFrameOverdue",
    ]);
    expect(workflow.view.value.overdueFrames).toHaveLength(1);
    await h.secure(0);
    expect(h.sounds).toEqual(["capture-started", "problem"]);
    expect(workflow.view.value.overdueFrames).toEqual([]);
    expect(h.alerts.alerts.value).toEqual([
      { kind: "unrequested-capture-stop", sectionId: "section-1" },
    ]);
  });
  it("user stop is silent; its final frame has a deadline and no heartbeat", async () => {
    const h = harness({ secureDeadlineMs: 500 });
    const workflow = await h.start();
    await workflow.stop();
    expect(h.sounds).toEqual(["capture-started"]);
    await h.scheduler.advanceBy(500);
    expect(h.sounds).toEqual(["capture-started", "problem"]);
    await h.scheduler.advanceBy(500);
    expect(workflow.view.value.overdueFrames[0]!.elapsedSeconds).toBe(1);
    await h.secure(0);
    expect(h.sounds).toEqual(["capture-started", "problem", "capture-completed"]);
    await h.scheduler.advanceBy(20_000);
    expect(h.microphones[0]!.requests).toBe(0);
  });
  it("old Section recovery has no heartbeat but sealing completes during newer Capture", async () => {
    const h = harness();
    const old = await h.start();
    await h.scheduler.advanceBy(5_000);
    await old.stop();
    await h.start();
    await h.secure(0);
    await h.secure(1);
    expect(h.sounds).toEqual(["capture-started", "capture-started", "capture-completed"]);
    expect(
      h.events
        .filter((e) => e.type === "transportFrameSecured")
        .map((e) => e.captureActive && !e.finalFrame),
    ).toEqual([false, false]);
  });
  it("emits immediate overlapping acknowledgements without a sound queue", async () => {
    const h = harness({ frameIntervalMs: 50 });
    await h.start();
    await h.scheduler.advanceBy(50);
    await h.secure(0);
    await h.scheduler.advanceBy(50);
    await h.secure(1);
    // Both are emitted at the acknowledgement moment, with no sound-duration wait.
    expect(h.sounds).toEqual(["capture-started", "healthy-heartbeat", "healthy-heartbeat"]);
  });
  it("distinguishes user-stop finalization and sealing failures, including failed retries", async () => {
    const h = harness();
    const workflow = await h.start();
    h.microphones[0]!.stopFailure = true;
    h.failCloses(2);
    await workflow.stop();
    expect(h.events.map((e) => e.type)).toEqual([
      "captureStarted",
      "captureFinalizationFailed",
      "audioAssetSealingFailed",
    ]);
    expect(h.sounds).toEqual(["capture-started", "problem"]);
    workflow.retryClose();
    await settle();
    expect(h.events.filter((e) => e.type === "audioAssetSealingFailed")).toHaveLength(2);
    workflow.retryClose();
    await settle();
    expect(h.sounds.at(-1)).toBe("problem");
    expect(h.events.some((e) => e.type === "unrequestedCaptureStopped")).toBe(false);
  });
  it("does not raise a Capture alert before Capture begins and releases failed startup access", async () => {
    const h = harness();
    h.failStartup();
    await expect(h.start()).rejects.toThrow("start failed");
    expect(h.events.some((e) => e.type === "unrequestedCaptureStopped")).toBe(false);
    expect(h.microphones[0]!.released).toBe(true);
    expect(h.access.acquire(() => {})).not.toBeNull();
    const denied = harness();
    denied.failPermission();
    await expect(denied.start()).rejects.toThrow("permission denied");
    expect(denied.events).toEqual([]);
    expect(denied.access.acquire(() => {})).not.toBeNull();
  });
  it("allows explicit takeover of a stalled active holder while keeping its retained audio", async () => {
    const h = harness();
    const old = await h.start();
    await h.scheduler.advanceBy(5_000);
    const original = h.requests[0]!.frame.data;
    h.microphones[0]!.stalledStop = true;
    const next = h.create();
    const starting = h.start(next);
    await settle();
    expect(h.access.takeoverRequested.value).toBe(true);
    h.access.confirmTakeover();
    await starting;
    expect(h.microphones[0]!.released).toBe(true);
    expect(next.view.value.capturing).toBe(true);
    expect(old.view.value.pendingFrames).toBe(1);
    expect(h.requests[0]!.frame.data).toBe(original);
    expect(h.events.filter((e) => e.type === "unrequestedCaptureStopped")).toHaveLength(0);
    await h.secure(0);
    expect(h.sounds.at(-1)).toBe("capture-completed");
    // The old holder's final cleanup must not release the new reservation.
    expect(h.access.acquire(() => {})).toBeNull();
  });
  it("disposes all timing and browser resources without later events", async () => {
    const h = harness();
    const workflow = await h.start();
    await h.scheduler.advanceBy(5_000);
    workflow.dispose();
    expect(h.scheduler.pendingCount).toBe(0);
    await h.secure(0);
    await h.scheduler.advanceBy(60_000);
    expect(h.events.map((e) => e.type)).toEqual(["captureStarted"]);
    expect(h.microphones[0]!.released).toBe(true);
  });
});

it("closes a Section created after its startup reservation was taken over", async () => {
  const h = harness();
  const first = h.create();
  const creation = deferred<void>();
  const pending = first.start(async () => {
    await creation.promise;
    return { sectionId: "late-section", audioAssetId: "late-asset", position: 0 };
  });
  await settle();
  await expect(
    first.start(async () => {
      throw new Error("must not run");
    }),
  ).rejects.toThrow("only start once");
  const second = h.start();
  await settle();
  h.access.confirmTakeover();
  const next = await second;
  creation.resolve();
  await pending;
  expect(h.microphones[0]!.released).toBe(true);
  expect(next.view.value.capturing).toBe(true);
  expect(first.view.value.awaitingSeal).toBe(false);
  expect(h.events.filter((event) => event.type === "unrequestedCaptureStopped")).toHaveLength(0);
  expect(h.events.filter((event) => event.type === "audioAssetSealed")).toHaveLength(1);
  expect(h.access.acquire(() => {})).toBeNull();
});
