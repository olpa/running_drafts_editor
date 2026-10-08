import type { ApiClient, Chunk, Language, RecordingStatus } from "./api.js";
import type { RecordingCapture, CaptureOptions } from "./capture.js";
import type { SeedConfig } from "./config.js";
import type { Cue } from "./cues.js";
import { FrameDelivery } from "./delivery.ts";
import { enabledControls, transition, type RecordingState } from "./state.ts";

export type RecordingBackend = Pick<ApiClient, "createRecording" | "putFrame" | "finish" | "getRecording">;
type RecordingConfig = Pick<SeedConfig, "frameIntervalMs" | "maxRecordingMs" | "pollIntervalMs"
  | "retryFirstMs" | "retrySecondMs" | "retryThirdMs" | "requestTimeoutMs">;

export interface RecordingDependencies {
  backend: RecordingBackend;
  openCapture: (options: CaptureOptions) => Promise<RecordingCapture>;
  now: () => number;
}

export interface RecordingSnapshot {
  readonly state: RecordingState;
  readonly elapsedMs: number;
  readonly securedMs: number;
  readonly pendingMs: number;
  readonly pendingFrames: number;
  readonly capturing: boolean;
  readonly canStart: boolean;
  readonly canRetry: boolean;
  readonly hasUnsecuredAudio: boolean;
  readonly feedback: string;
}

export type RecordingEvent =
  | { type: "changed"; snapshot: RecordingSnapshot }
  | { type: "cue"; cue: Cue }
  | { type: "chunks"; chunks: readonly Chunk[] }
  | { type: "log"; message: string }
  | { type: "reset" };

type StopReason = "user" | "duration" | "backlog" | "capture";
class ActiveRecording {
  recordingId = "";
  startedAt: number | null = null;
  elapsedMs = 0;
  lastFrameEndMs = 0;
  securedMs = 0;
  capture: RecordingCapture | null = null;
  delivery!: FrameDelivery;
  capturing = false;
  finalized = false;
  dead = false;
  stopReason: StopReason | null = null;
  problem = "";
  finishing = false;
  finishAccepted = false;
  pollTimer: ReturnType<typeof setTimeout> | null = null;
  progressTimer: ReturnType<typeof setInterval> | null = null;
  limitTimer: ReturnType<typeof setTimeout> | null = null;
  serverStatus: RecordingStatus["status"] | null = null;
  shownChunks: Chunk[] = [];
}

/** The complete recording workflow, independent of DOM rendering and sound output. */
export class RecordingController {
  private state: RecordingState = "idle";
  private active: ActiveRecording | null = null;
  private disposed = false;
  private readonly listeners = new Set<(event: RecordingEvent) => void>();

  constructor(private readonly config: RecordingConfig, private readonly dependencies: RecordingDependencies) {}

  get snapshot(): RecordingSnapshot {
    const recording = this.active;
    const pendingFrames = recording?.delivery.pendingCount ?? 0;
    const currentMs = recording?.capturing
      ? Math.max(0, recording.elapsedMs - recording.lastFrameEndMs) : 0;
    const canRestart = !recording || this.state === "complete"
      || (!recording.stopReason && !recording.capturing && !pendingFrames);
    return {
      state: this.state,
      elapsedMs: recording?.elapsedMs ?? 0,
      securedMs: recording?.securedMs ?? 0,
      pendingMs: (recording?.delivery.pendingMs ?? 0) + currentMs,
      pendingFrames,
      capturing: recording?.capturing ?? false,
      canStart: !this.disposed && enabledControls(this.state).start && canRestart,
      canRetry: !this.disposed && !!recording && !!(
        (pendingFrames && (recording.delivery.delayed || recording.stopReason))
        || (recording.finalized && this.state === "error")
      ),
      hasUnsecuredAudio: !!recording?.capturing || pendingFrames > 0,
      feedback: recording?.problem || (recording?.delivery.delayed
        ? recording.capturing ? "Audio upload delayed. Recording continues." : "Audio upload delayed. Recording has stopped."
        : recording?.stopReason === "duration" ? "Recording stopped at the time limit." : ""),
    };
  }

  /** Subscribe to updates and semantic events; receives the current snapshot immediately. */
  subscribe(listener: (event: RecordingEvent) => void): () => void {
    this.listeners.add(listener);
    listener({ type: "changed", snapshot: this.snapshot });
    return () => { this.listeners.delete(listener); };
  }

  async start(language: Language): Promise<void> {
    if (!this.snapshot.canStart) return;
    if (this.active) {
      this.active.dead = true;
      this.active.delivery.dispose();
    }
    const recording = new ActiveRecording();
    recording.delivery = new FrameDelivery({
      retryDelaysMs: [this.config.retryFirstMs, this.config.retrySecondMs, this.config.retryThirdMs],
      requestTimeoutMs: this.config.requestTimeoutMs,
      send: (frame, signal) => this.dependencies.backend.putFrame(recording.recordingId, frame, signal),
      log: (message) => this.write(recording, message),
      changed: () => {
        if (!this.isCurrent(recording)) return;
        this.changed();
        void this.maybeFinish(recording);
      },
      acknowledged: (frame) => {
        if (!this.isCurrent(recording)) return;
        recording.securedMs = frame.endMs;
        this.emit({ type: "cue", cue: "health" });
      },
      delayed: () => { if (this.isCurrent(recording)) this.emit({ type: "cue", cue: "delayed" }); },
    });
    this.active = recording;
    this.emit({ type: "reset" });
    this.setState("starting");
    if (!this.isCurrent(recording)) return;
    try {
      const capture = await this.dependencies.openCapture({
        intervalMs: this.config.frameIntervalMs,
        canContinue: () => {
          if (recording.delivery.pendingCount >= 2) {
            void this.stopCapture("backlog");
            return false;
          }
          return recording.capturing && this.isCurrent(recording);
        },
        onError: (error) => {
          this.write(recording, `ERR ${error.message}`);
          if (this.isCurrent(recording)) void this.stopCapture("capture");
        },
      });
      if (!this.isCurrent(recording)) {
        capture.dispose();
        return;
      }
      recording.capture = capture;
      this.write(recording, "mic ok");
      ({ recordingId: recording.recordingId } = await this.dependencies.backend.createRecording(language));
      if (!this.isCurrent(recording)) return;
      this.write(recording, `rec ${recording.recordingId.slice(0, 8)} ${language}`);
      recording.startedAt = this.dependencies.now();
      recording.capturing = true;
      capture.start((frame) => {
        if (!this.isCurrent(recording)) return;
        recording.lastFrameEndMs = frame.endMs;
        recording.delivery.enqueue(frame);
      });
      this.setState("recording");
      if (!this.isCurrent(recording)) return;
      this.emit({ type: "cue", cue: "start" });
      if (!this.isCurrent(recording)) return;
      this.write(recording, "cue start");
      recording.progressTimer = setInterval(() => {
        recording.elapsedMs = this.dependencies.now() - recording.startedAt!;
        this.changed();
      }, 100);
      recording.limitTimer = setTimeout(() => void this.stopCapture("duration"), this.config.maxRecordingMs);
      this.schedulePoll(recording);
    } catch (error) {
      if (!this.isCurrent(recording)) return;
      recording.capturing = false;
      recording.capture?.dispose();
      recording.problem = `Could not start recording: ${errorMessage(error)}`;
      this.write(recording, `ERR ${recording.problem}`);
      this.setState("error");
    }
  }

  stop(): Promise<void> {
    return this.stopCapture("user");
  }

  retry(): void {
    const recording = this.active;
    if (!recording || !this.snapshot.canRetry) return;
    if (recording.stopReason && this.state === "error") {
      recording.problem = "";
      this.setState("finishing");
    }
    recording.delivery.retry();
    void this.maybeFinish(recording);
  }

  dispose(): void {
    this.disposed = true;
    if (this.active) {
      this.active.dead = true;
      this.active.capturing = false;
      this.cleanupTimers(this.active);
      this.active.delivery.dispose();
      this.active.capture?.dispose();
    }
    this.listeners.clear();
  }

  private async stopCapture(reason: StopReason): Promise<void> {
    const recording = this.active;
    if (!this.isCurrent(recording) || !recording.capturing || !recording.capture) return;
    recording.capturing = false;
    recording.stopReason = reason;
    recording.elapsedMs = this.dependencies.now() - recording.startedAt!;
    if (recording.progressTimer !== null) clearInterval(recording.progressTimer);
    if (recording.limitTimer !== null) clearTimeout(recording.limitTimer);
    recording.progressTimer = recording.limitTimer = null;
    this.write(recording, `capture stop ${reason}`);
    if (reason === "backlog" || reason === "capture") {
      recording.problem = reason === "backlog"
        ? "Recording stopped because audio could not be uploaded."
        : "Recording stopped because the microphone became unavailable.";
      recording.delivery.freeze();
      this.setState("error");
      this.emit({ type: "cue", cue: "interrupted" });
    } else {
      this.setState("finishing");
    }
    await recording.capture.stop();
    if (!this.isCurrent(recording)) return;
    recording.finalized = true;
    this.changed();
    await this.maybeFinish(recording);
  }

  private async maybeFinish(recording: ActiveRecording): Promise<void> {
    if (!this.isCurrent(recording) || !recording.finalized || recording.delivery.pendingCount
      || recording.finishing || recording.finishAccepted) return;
    recording.finishing = true;
    if (this.state === "error") {
      recording.problem = "";
      this.setState("finishing");
    }
    try {
      await this.dependencies.backend.finish(recording.recordingId);
      if (!this.isCurrent(recording)) return;
      recording.finishAccepted = true;
      this.write(recording, "fin sent");
    } catch (error) {
      if (!this.isCurrent(recording)) return;
      recording.problem = `Could not finish recording: ${errorMessage(error)}`;
      this.write(recording, `ERR ${recording.problem}`);
      this.setState("error");
    } finally {
      recording.finishing = false;
      if (this.isCurrent(recording)) this.changed();
    }
  }

  private schedulePoll(recording: ActiveRecording): void {
    recording.pollTimer = setTimeout(() => void this.poll(recording), this.config.pollIntervalMs);
  }

  private async poll(recording: ActiveRecording): Promise<void> {
    try {
      const status = await this.dependencies.backend.getRecording(recording.recordingId);
      if (!this.isCurrent(recording)) return;
      this.showChunks(recording, status.chunks);
      if (status.status !== recording.serverStatus) {
        recording.serverStatus = status.status;
        this.write(recording, `srv ${status.status}`);
      }
      if (status.status === "complete" && recording.finishAccepted) {
        this.cleanupTimers(recording);
        this.setState("complete");
        this.emit({ type: "cue", cue: "stop" });
        this.write(recording, "cue stop");
        return;
      }
    } catch (error) {
      if (!this.isCurrent(recording)) return;
      this.write(recording, `poll ERR ${errorMessage(error)}`);
    }
    if (this.isCurrent(recording)) this.schedulePoll(recording);
  }

  private showChunks(recording: ActiveRecording, chunks: Chunk[]): void {
    chunks.forEach((chunk, index) => {
      const shown = recording.shownChunks[index];
      if (shown && shown.id !== chunk.id) throw new Error(`Chunk order changed at position ${index + 1}.`);
      if (!shown) this.write(recording, `+c${index + 1}`);
    });
    if (JSON.stringify(chunks) === JSON.stringify(recording.shownChunks)) return;
    recording.shownChunks = chunks;
    this.emit({ type: "chunks", chunks });
  }

  private isCurrent(recording: ActiveRecording | null): recording is ActiveRecording {
    return !!recording && this.active === recording && !recording.dead && !this.disposed;
  }

  private cleanupTimers(recording: ActiveRecording): void {
    if (recording.pollTimer !== null) clearTimeout(recording.pollTimer);
    if (recording.progressTimer !== null) clearInterval(recording.progressTimer);
    if (recording.limitTimer !== null) clearTimeout(recording.limitTimer);
  }

  private setState(next: RecordingState): void {
    if (this.state === next) return;
    this.state = transition(this.state, next);
    if (this.active) this.write(this.active, next);
    this.changed();
  }

  private changed(): void {
    this.emit({ type: "changed", snapshot: this.snapshot });
  }

  private emit(event: RecordingEvent): void {
    for (const listener of this.listeners) listener(event);
  }

  private write(recording: ActiveRecording, entry: string): void {
    if (!this.isCurrent(recording)) return;
    const seconds = recording.startedAt === null ? "0.0" : ((this.dependencies.now() - recording.startedAt) / 1000).toFixed(1);
    this.emit({ type: "log", message: `${seconds} ${entry}` });
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
