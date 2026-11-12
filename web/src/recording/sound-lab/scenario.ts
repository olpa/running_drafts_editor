import { createApplicationCapture } from "../application-capture.ts";
import type { AccountId, CurrentSession, SessionGateway } from "../../session/gateway.ts";
import { AUTH_PROVIDERS } from "../../session/providers.ts";
import { createSession } from "../../session/session.ts";
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
import { createAllowanceFeedback } from "../../alerts/allowance-feedback.ts";
import { createUsage } from "../../usage/usage.ts";
import type { UsageSnapshot } from "../../usage/gateway.ts";

export type ScenarioKind =
  | "healthy"
  | "short"
  | "older"
  | "single"
  | "two"
  | "burst"
  | "final"
  | "failure"
  | "finalization"
  | "sealing"
  | "repeated"
  | "allowance-storage-warning"
  | "allowance-duration-warning"
  | "allowance-overlap"
  | "allowance-storage-rejection"
  | "allowance-duration-rejection"
  | "allowance-duration-zero";

function usageSnapshot(name: ScenarioKind, intervalMs: number): UsageSnapshot {
  const storageWarning =
    name === "allowance-storage-warning" ||
    name === "allowance-overlap" ||
    name === "allowance-storage-rejection";
  const durationWarning =
    name === "allowance-duration-warning" ||
    name === "allowance-overlap" ||
    name === "allowance-duration-rejection" ||
    name === "allowance-duration-zero";
  return {
    plan: "free",
    audioStorage: {
      usedBytes: 0,
      grantedBytes: 0,
      warningAtBytes: storageWarning ? 6 : 107_374_182,
      limitBytes: storageWarning ? 12 : 134_217_728,
    },
    audioAssetDuration: {
      grantedMs: 0,
      warningAtMs: durationWarning ? 2 * intervalMs : 2_880_000,
      limitMs: durationWarning ? 3 * intervalMs : 3_600_000,
    },
  };
}

interface Pending {
  finish(): void;
  cancel(): void;
}

/** Fail-closed environmental adapters; no HTTP client or browser recorder is constructed. */
export function createLabScenario(options: {
  name: ScenarioKind;
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
  const config = {
    ...DEFAULT_CONFIG,
    frameIntervalMs: options.intervalMs,
    secureDeadlineMs: options.deadlineMs,
  };
  const access = createMicrophoneAccess();
  const accountId = "5728cdd2-909d-4207-8303-c04b270c308f" as AccountId;
  const currentSession: CurrentSession = {
    account: { id: accountId },
    provider: AUTH_PROVIDERS[0]!,
    profile: {
      email: "reviewer@example.test",
      emailVerified: true,
      displayName: "Storybook reviewer",
      givenName: null,
      familyName: null,
      pictureUrl: null,
      locale: null,
    },
    sessionExpiresAt: "2099-01-01T00:00:00Z",
  };
  const sessionGateway: SessionGateway = {
    async current() {
      log("Session gateway: current signed-in session");
      return currentSession;
    },
    async logout() {
      return unexpected("Session logout");
    },
    loginUrl() {
      return unexpected("Session login URL");
    },
  };
  const session = createSession(sessionGateway, {
    current: () => ({ path: "/", search: "", href: "https://storybook.invalid/" }),
    navigate: () => unexpected("Session navigation"),
    replace: () => unexpected("Session address replacement"),
  });
  void session.bootstrap();
  const snapshot = usageSnapshot(name, options.intervalMs);
  const usage = createUsage({
    async current() {
      log(`Usage gateway: current ${snapshot.plan} snapshot`);
      return snapshot;
    },
  });
  const allowanceFeedback = createAllowanceFeedback(options.cues, scheduler);
  const microphones: SimulatedMicrophone[] = [];
  const assets = new Map<string, { frames: number; duration: number }>();
  const frames = new Set<Pending>();
  const seals = new Set<Pending>();
  const allowanceRejectedAssets = new Set<string>();
  let holdFrames = name === "single" || name === "two" || name === "final";
  let holdSeals = ["healthy", "single", "burst", "short", "final", "older"].includes(name);
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
      log("Recording gateway: create Project request");
      log("Recording gateway: create Project outcome simulated Project");
      return {
        id: "scenario-workbench",
        title: "Simulated Capture",
        createdAt: "2026-01-01",
        updatedAt: "2026-01-01",
      };
    },
    async getProject() {
      return unexpected("getProject");
    },
    async createSection(projectId) {
      log(`Recording gateway: create Section request for ${projectId}`);
      if (projectId !== "scenario-workbench") return unexpected(`createSection ${projectId}`);
      const position = currentPosition++;
      const id = `asset-${position + 1}`;
      assets.set(id, { frames: 0, duration: 0 });
      log(`Recording gateway: create Section outcome section-${position + 1} / ${id}`);
      return { sectionId: `section-${position + 1}`, audioAssetId: id, position };
    },
    putFrame(id, frame, signal) {
      log(`Recording gateway: put frame ${id} f${frame.seq} request`);
      const asset = assets.get(id);
      if (!asset) return Promise.reject(new Error(`Unexpected Audio Asset ${id}`));
      const rejectedDimension =
        frame.seq === 2 && name === "allowance-storage-rejection"
          ? "storage"
          : frame.seq === 2 && name === "allowance-duration-rejection"
            ? "duration"
            : null;
      if (rejectedDimension && !allowanceRejectedAssets.has(id)) {
        allowanceRejectedAssets.add(id);
        log(
          `Recording gateway: put frame ${id} f${frame.seq} outcome ${rejectedDimension} rejection`,
        );
        const storage = rejectedDimension === "storage";
        return Promise.reject(
          new ApiError("PUT", `/audio-assets/${id}/frames/${frame.seq}`, 429, {
            title: storage
              ? "Stored audio allowance exceeded"
              : "Recording duration allowance exceeded",
            detail: storage
              ? "The simulated frame would exceed this Account's stored audio allowance."
              : "The simulated frame would exceed this Audio Asset's duration allowance.",
            code: storage ? "audio_storage_limit_exceeded" : "audio_asset_duration_limit_exceeded",
            referenceId: `storybook-${id}`,
          }),
        );
      }
      asset.frames = Math.max(asset.frames, frame.seq + 1);
      asset.duration = Math.max(asset.duration, frame.endMs);
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
            log(`Recording gateway: put frame ${id} f${frame.seq} outcome secured`);
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
    log(`Recording gateway: ${reason} ${id} request`);
    const asset = assets.get(id);
    if (!asset) return unexpected(`seal ${id}`);
    if (sealingFailures > 0) {
      sealingFailures--;
      log(`Recording gateway: ${reason} ${id} outcome failure`);
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
    const summary = {
      id,
      mediaType: "audio/webm;codecs=opus",
      isSealed: true,
      sealReason: reason,
      sealedAt: "2026-01-01",
      frameCount: asset.frames,
      observedDurationMs: asset.duration,
      storedBytes: asset.frames * 3,
    };
    log(`Recording gateway: ${reason} ${id} outcome sealed`);
    return summary;
  }
  const app = createApplicationCapture({
    config,
    cues: options.cues,
    alerts: options.alerts,
    authorizeStart: () => session.authorizeRecordingStart(),
    usage,
    allowanceFeedback,
    environment: {
      gateway,
      scheduler,
      microphoneAccess: access,
      now: scheduler.now,
      uuid: () => `lab-${++sequence}`,
      wallClock: () => new Date("2026-01-01T12:00:00Z"),
      async openCapture(captureOptions) {
        log("Microphone adapter: open Capture request");
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
        log("Microphone adapter: open Capture outcome simulated microphone");
        if (name === "failure" || name === "repeated")
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
  usage.setAccount(accountId);
  app.setSessionAccount(accountId);
  async function advance(ms: number, quiet = false): Promise<void> {
    if (!manual || disposed) return;
    if (!quiet) log(`Advance simulated clock by ${ms} ms`);
    await manual.advanceBy(ms);
    await settle();
  }
  return {
    app,
    session,
    alerts: options.alerts,
    usage,
    allowanceFeedback,
    microphoneAccess: access,
    config,
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
      usage.dispose();
      allowanceFeedback.dispose();
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
