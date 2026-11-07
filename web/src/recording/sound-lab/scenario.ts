import { createApplicationCapture } from "../application-capture.ts";
import type { AccountId } from "../../session/gateway.ts";
import { createMicrophoneAccess } from "../microphone-access.ts";
import { browserScheduler, transportFrameTiming, type Scheduler } from "../frame-timing.ts";
import type { CaptureOptions, MicrophoneCapture, TransportFrame } from "../capture.ts";
import type { FrameAck, RecordingGateway } from "../gateway.ts";
import type { AudioAssetSummary } from "../../shared/project-types.ts";
import { DEFAULT_CONFIG } from "../../shared/config.ts";
import { ManualScheduler, settle } from "../../test-support/manual-scheduler.ts";
import type { Cues } from "../cues.ts";
import type { CaptureAlerts } from "../../alerts/capture-alerts.ts";
import { ApiError } from "../../shared/http.ts";

export const scenarios = {
  palette:
    "Audition candidates directly. Choose automatic review to play a healthy Capture sequence.",
  healthy:
    "Listen to three secured frames at the production cadence, followed by user Stop and sealing. The workbench leaves at least two seconds between the last heartbeat and Completion.",
  accelerated:
    "The healthy Capture sequence plays at 5× speed by default. The simulated clock increases automatically; audio keeps its normal duration.",
  single:
    "The first frame misses its securing deadline and keeps sounding Problem. Automatic playback recovers before the next frame boundary, then leaves at least two seconds between its recovery heartbeat and Completion.",
  two: "Two frames become overdue. Production backlog protection stops Capture and raises Capture alerts. Both delay lines remain visible before arranged recovery; acknowledgement stays yours.",
  burst:
    "The first frame recovers just before the second frame request. Their healthy cues arrive 40 simulated ms apart and overlap without a sound queue; Completion follows at least two seconds later.",
  stop: "A short Capture ends through user Stop. Final audio secures, then the simulated backend confirms sealing and completion sounds.",
  final:
    "User Stop is followed by an overdue final frame, recovery, and sealing. Securing the final frame resolves its identified Capture alert without a Healthy heartbeat.",
  duration:
    "The simulated backend rejects a frame at the duration allowance. Production stops Capture, retains the frame, and repeats Problem until you acknowledge the Capture alert.",
  failure:
    "The recorder fails after one frame interval plus 500 simulated ms. Inspect its Section causes, global Capture alerts, and arranged delivery recovery.",
  coexist:
    "A backend duration rejection stops Capture. The simulated backend then confirms sealing while the unrequested-stop Capture alert stays active and suppresses Completion.",
  older:
    "Section 1 stops, Section 2 starts, and the older Section seals during newer Capture. The sequence then stops and seals the newer Section.",
  repeated:
    "Two successive Sections receive backend duration rejections. Two identified Capture alerts share one Problem loop; acknowledgement stays yours.",
  unavailable:
    "Sound failure is simulated. A backend duration rejection stops Capture; the visible Capture alert reports unavailable sound and remains usable.",
} as const;
export type ScenarioName = keyof typeof scenarios;

interface Pending {
  finish(): void;
  cancel(): void;
}

/** Fail-closed environmental adapters; no HTTP client or browser recorder is constructed. */
export function createLabScenario(options: {
  name: ScenarioName;
  manual: boolean;
  intervalMs: number;
  deadlineMs: number;
  cues: Cues;
  alerts: CaptureAlerts;
  log: (message: string) => void;
}) {
  const { log, name } = options;
  transportFrameTiming({ intervalMs: options.intervalMs, secureDeadlineMs: options.deadlineMs });
  const manual = options.manual ? new ManualScheduler() : null;
  const base: Scheduler = manual ?? browserScheduler;
  const cancellations = new Set<() => void>();
  const scheduler: Scheduler = {
    now: () => base.now(),
    schedule(delay, callback) {
      const cancel = base.schedule(delay, () => {
        cancellations.delete(cancel);
        callback();
      });
      cancellations.add(cancel);
      return () => {
        cancel();
        cancellations.delete(cancel);
      };
    },
  };
  const scenarioLimitMs = 2 * options.intervalMs;
  const config = {
    ...DEFAULT_CONFIG,
    frameIntervalMs: options.intervalMs,
    secureDeadlineMs: options.deadlineMs,
  };
  const access = createMicrophoneAccess();
  const microphones: SimulatedMicrophone[] = [];
  const assets = new Map<string, { frames: number; duration: number }>();
  const frames = new Set<Pending>();
  const seals = new Set<Pending>();
  const allowanceRejectedAssets = new Set<string>();
  let holdFrames = name === "single" || name === "two" || name === "final";
  let holdSeals = [
    "palette",
    "healthy",
    "accelerated",
    "single",
    "burst",
    "stop",
    "final",
    "coexist",
    "older",
  ].includes(name);
  let finalizationFailure = false;
  let sealingFailures = 0;
  let disposed = false;
  let sequence = 0;
  let currentPosition = 0;
  function unexpected(message: string): never {
    log(`ERROR: Unexpected adapter activity: ${message}`);
    throw new Error(`Unexpected adapter activity: ${message}`);
  }
  const gateway: RecordingGateway = {
    async createProject() {
      return {
        id: "sound-lab",
        title: "Simulated Capture",
        createdAt: "2026-01-01",
        updatedAt: "2026-01-01",
      };
    },
    async getProject() {
      return unexpected("getProject");
    },
    async createSection(projectId) {
      if (projectId !== "sound-lab") return unexpected(`createSection ${projectId}`);
      const position = currentPosition++;
      const id = `asset-${position + 1}`;
      assets.set(id, { frames: 0, duration: 0 });
      return { sectionId: `section-${position + 1}`, audioAssetId: id, position };
    },
    putFrame(id, frame, signal) {
      const asset = assets.get(id);
      if (!asset) return Promise.reject(new Error(`Unexpected Audio Asset ${id}`));
      asset.frames = Math.max(asset.frames, frame.seq + 1);
      asset.duration = Math.max(asset.duration, frame.endMs);
      if (
        ["duration", "coexist", "repeated", "unavailable"].includes(name) &&
        frame.seq === 1 &&
        !allowanceRejectedAssets.has(id)
      ) {
        allowanceRejectedAssets.add(id);
        return Promise.reject(
          new ApiError("PUT", `/audio-assets/${id}/frames/${frame.seq}`, 429, {
            title: "Recording duration allowance exceeded",
            detail: "The simulated frame would exceed this Audio Asset's duration allowance.",
            code: "audio_asset_duration_limit_exceeded",
            referenceId: `storybook-${id}`,
          }),
        );
      }
      return new Promise<FrameAck>((resolve, reject) => {
        const pending: Pending = {
          finish() {
            if (!frames.delete(pending)) return;
            signal.removeEventListener("abort", abort);
            resolve({
              sequence: frame.seq,
              acknowledged: true,
              sha256: "a".repeat(64),
              nextExpectedSequence: frame.seq + 1,
            });
          },
          cancel() {
            if (!frames.delete(pending)) return;
            signal.removeEventListener("abort", abort);
            reject(new Error("Simulated request aborted."));
          },
        };
        const abort = () => pending.cancel();
        frames.add(pending);
        signal.addEventListener("abort", abort, { once: true });
        if (signal.aborted) {
          abort();
          return;
        }
        if (holdFrames) return;
        const delay = name === "burst" ? (frame.seq === 0 ? options.intervalMs - 30 : 10) : 80;
        scheduler.schedule(delay, () => pending.finish());
      });
    },
    complete: (id) => seal(id, "completed"),
    interrupt: (id) => seal(id, "interrupted"),
  };
  async function seal(id: string, reason: "completed" | "interrupted"): Promise<AudioAssetSummary> {
    const asset = assets.get(id);
    if (!asset) return unexpected(`seal ${id}`);
    if (sealingFailures > 0) {
      sealingFailures--;
      throw new Error("Simulated Audio Asset sealing failure.");
    }
    if (holdSeals)
      await new Promise<void>((resolve) => {
        const pending: Pending = {
          finish() {
            seals.delete(pending);
            resolve();
          },
          cancel() {
            seals.delete(pending);
            resolve();
          },
        };
        seals.add(pending);
        log(`Sealing ${id} waits for simulated confirmation.`);
      });
    return {
      id,
      mediaType: "audio/webm;codecs=opus",
      isSealed: true,
      sealReason: reason,
      sealedAt: "2026-01-01",
      frameCount: asset.frames,
      observedDurationMs: asset.duration,
      storedBytes: asset.frames * 3,
    };
  }
  const app = createApplicationCapture({
    config,
    cues: options.cues,
    alerts: options.alerts,
    authorizeStart: () => true,
    environment: {
      gateway,
      scheduler,
      microphoneAccess: access,
      now: scheduler.now,
      uuid: () => `lab-${++sequence}`,
      wallClock: () => new Date("2026-01-01T12:00:00Z"),
      async openCapture(captureOptions) {
        if (disposed) return unexpected("openCapture after disposal");
        const microphone = new SimulatedMicrophone(
          scheduler,
          captureOptions,
          () => {
            const fail = finalizationFailure;
            finalizationFailure = false;
            return fail;
          },
          name !== "two",
        );
        microphones.push(microphone);
        if (name === "failure")
          microphone.onStart = () => {
            scheduler.schedule(options.intervalMs + 500, () => microphone.fail());
          };
        return microphone;
      },
    },
    onEvent(_sectionId, event) {
      log(`event: ${event.type}${"frameId" in event ? ` f${event.frameId}` : ""}`);
    },
  });
  app.setSessionAccount("5728cdd2-909d-4207-8303-c04b270c308f" as AccountId);
  async function advance(ms: number, quiet = false): Promise<void> {
    if (!manual || disposed) return;
    if (!quiet) log(`Advance simulated clock by ${ms} ms`);
    await manual.advanceBy(ms);
    await settle();
  }
  return {
    app,
    config,
    scenarioLimitMs,
    manual,
    scheduler,
    advance,
    async recover() {
      holdFrames = false;
      for (const pending of [...frames]) pending.finish();
      for (const section of app.snapshot.value.sections) app.uploadPendingAudio(section.sectionId);
      await settle();
      log(
        "Delivery recovery enabled; future frames use the normal simulated acknowledgement delay.",
      );
    },
    confirmSealing() {
      holdSeals = false;
      for (const pending of [...seals]) pending.finish();
      log("Simulated backend confirms pending and future sealing.");
    },
    failCapture() {
      microphones.at(-1)?.fail();
    },
    failFinalization() {
      finalizationFailure = true;
      log("Next user Stop will fail during Capture finalization.");
    },
    failNextSeal() {
      sealingFailures++;
      log("Next sealing attempt will fail.");
    },
    dispose() {
      disposed = true;
      app.dispose();
      access.dispose();
      for (const cancel of cancellations) cancel();
      cancellations.clear();
      for (const pending of [...frames, ...seals]) pending.cancel();
    },
  };
}
export type LabScenario = ReturnType<typeof createLabScenario>;

class SimulatedMicrophone implements MicrophoneCapture {
  private sink: ((frame: TransportFrame) => void) | null = null;
  private seq = 0;
  private last = 0;
  private startedAt = 0;
  private active = false;
  onStart = () => {};
  constructor(
    private scheduler: Scheduler,
    private options: CaptureOptions,
    private stopFails: () => boolean,
    private finalBlob: boolean,
  ) {}
  async start(sink: (frame: TransportFrame) => void): Promise<void> {
    this.sink = sink;
    this.startedAt = this.scheduler.now();
    this.active = true;
    this.onStart();
  }
  requestFrame(): void {
    if (!this.active || !this.sink)
      throw new Error("Unexpected frame request outside simulated Capture.");
    const end = Math.max(this.last + 1, this.scheduler.now() - this.startedAt);
    this.sink({
      seq: this.seq++,
      startMs: this.last,
      endMs: end,
      mediaType: "audio/webm;codecs=opus",
      data: new Blob(["lab"]),
    });
    this.last = end;
  }
  async stop(): Promise<void> {
    if (this.stopFails()) {
      this.active = false;
      throw new Error("Simulated recorder finalization failure.");
    }
    if (this.active && this.finalBlob) this.requestFrame();
    this.active = false;
  }
  fail(): void {
    if (this.active) this.options.onError(new Error("Simulated microphone became unavailable."));
  }
  dispose(): void {
    this.active = false;
    this.sink = null;
  }
}
