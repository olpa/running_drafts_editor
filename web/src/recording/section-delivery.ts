import type { Scheduler } from "./frame-timing.ts";
import { computed, reactive, type ComputedRef } from "vue";
import { ApiError, type ProblemDetails } from "../shared/http.ts";
import type { AudioAssetSummary } from "../shared/project-types.ts";
import type { TransportFrame } from "./capture.ts";
import { FrameDelivery } from "./delivery.ts";
import type { RecordingGateway } from "./gateway.ts";

export type StopReason =
  "user" | "backlog" | "capture" | "oversize" | "conflict" | "hard" | "authentication";
export type CompletionStatus = "pending" | "failed" | "succeeded";

/** Pending frames at which capture stops so that audio cannot pile up behind a failing upload. */
export const BACKLOG_LIMIT = 2;

/**
 * What lets this Section send its audio.
 * - `automatic`: the queue retries on its own.
 * - `waiting`: capture was interrupted; the user must ask for delivery.
 * - `requested`: delivery was asked for, or there is nothing left to send.
 * - `blocked-*`: terminal; the bytes stay in this page.
 */
type RecoveryMode =
  | "automatic"
  | "waiting"
  | "requested"
  | "blocked-oversize"
  | "blocked-conflict"
  | "blocked-target";
type CapturePhase = "running" | "stopping" | "ended";
type CloseCommand =
  { readonly kind: "complete"; readonly frameCount: number } | { readonly kind: "interrupt" };
type CloseLifecycle =
  | { kind: "open" }
  | { kind: "closing"; command: CloseCommand }
  | { kind: "failed"; command: CloseCommand; problem: string }
  | { kind: "closed"; command: CloseCommand; result: AudioAssetSummary };

export interface DeliveryView {
  readonly securedMs: number;
  readonly pendingFrames: number;
  readonly pendingMs: number;
  /** Time of the last frame handed to delivery, in capture time. */
  readonly lastFrameEndMs: number;
  /** An acknowledgement is overdue; capture may still be running. */
  readonly delayed: boolean;
  readonly completion: CompletionStatus | null;
  readonly completionProblem: string;
  readonly canRetryCompletion: boolean;
  readonly problem: string;
  readonly canUploadPending: boolean;
  readonly closeProblem: string;
  readonly canRetryClose: boolean;
  /** Capture has ended but the Section is not sealed yet. */
  readonly awaitingSeal: boolean;
}

export interface SectionDeliveryOptions {
  audioAssetId: string;
  retryDelaysMs: readonly number[];
  requestTimeoutMs: number;
  maxFrameBytes: number;
  gateway: Pick<RecordingGateway, "putFrame" | "complete" | "interrupt">;
  now: () => number;
  scheduler?: Scheduler;
  log: (message: string) => void;
  onAcknowledged: (frame: TransportFrame) => void;
  onSealed?: () => void;
  onSealFailure?: (error: unknown) => void;
  canSeal?: () => boolean;
  onDelayed: () => void;
  /** Delivery cannot continue safely, so capture should stop. */
  requestCaptureStop: (reason: "oversize" | "conflict" | "hard") => void;
  onAllowanceRejected?: (dimension: "storage" | "duration") => void;
  onAccountSuspended?: () => void;
}

/**
 * Everything durable about one Section's audio: the frame queue, whether it may
 * send, and sealing the Audio Asset. It outlives capture and survives sign-out.
 */
export interface SectionDelivery {
  readonly view: ComputedRef<DeliveryView>;
  /** The backlog limit is reached; capture must not continue. */
  readonly backlogged: boolean;
  accept(frame: TransportFrame): void;
  /** Capture began finalizing. An interruption freezes delivery until the user asks. */
  captureStopping(reason: StopReason): void;
  captureEnded(reason: StopReason): void;
  /** Finalization failed; the Section is treated as interrupted. */
  captureFailedToFinalize(problem: string): void;
  /** Capture never began; the Section will be sealed as interrupted. */
  captureNeverStarted(problem: string): void;
  /** Send the close command when capture has ended and nothing is pending. */
  closeIfReady(): Promise<void>;
  requestUpload(): void;
  retryClose(): void;
  /** The Application Session ended: stop sending and closing, keep every byte. */
  suspend(): void;
  resume(): void;
  dispose(): void;
}

export function createSectionDelivery(options: SectionDeliveryOptions): SectionDelivery {
  const state = reactive({
    recovery: "automatic" as RecoveryMode,
    suspended: false,
    close: { kind: "open" } as CloseLifecycle,
    phase: "running" as CapturePhase,
    reason: null as StopReason | null,
    problem: "",
    securedMs: 0,
    lastFrameEndMs: 0,
    assignedFrames: 0,
    pendingFrames: 0,
    pendingMs: 0,
    delayed: false,
  });
  let disposed = false;

  const frames = new FrameDelivery({
    retryDelaysMs: options.retryDelaysMs,
    requestTimeoutMs: options.requestTimeoutMs,
    send: (frame, signal) => options.gateway.putFrame(options.audioAssetId, frame, signal),
    log: options.log,
    now: options.now,
    ...(options.scheduler ? { scheduler: options.scheduler } : {}),
    changed: () => {
      if (disposed) return;
      state.pendingFrames = frames.pendingCount;
      state.pendingMs = frames.pendingMs;
      state.delayed = frames.delayed;
      void closeIfReady();
    },
    acknowledged: (frame) => {
      state.securedMs = Math.max(state.securedMs, frame.endMs);
      options.onAcknowledged(frame);
    },
    delayed: () => options.onDelayed(),
    failed: (_frame, error) => handleFrameFailure(error),
  });

  function blocked(): boolean {
    return (
      state.recovery === "blocked-oversize" ||
      state.recovery === "blocked-conflict" ||
      state.recovery === "blocked-target"
    );
  }

  function interrupted(): boolean {
    return state.phase === "ended" && state.reason !== null && isInterruptionReason(state.reason);
  }

  function handleFrameFailure(error: unknown): void {
    if (!(error instanceof ApiError)) return;
    const code = error.problem?.code;
    if (code === "audio_storage_limit_exceeded" || code === "audio_asset_duration_limit_exceeded") {
      frames.freeze();
      state.recovery = "waiting";
      const dimension = code === "audio_storage_limit_exceeded" ? "storage" : "duration";
      state.problem = problemDescription(
        `Capture stopped because the ${dimension} allowance rejected this audio. The exact pending audio remains on this page.`,
        error.problem,
      );
      options.log(
        `allowance stop HTTP ${error.status} code=${code}` +
          ` referenceId=${error.problem?.referenceId ?? "none"}`,
      );
      options.onAllowanceRejected?.(dimension);
      options.requestCaptureStop("hard");
      return;
    }
    if (code === "account_suspended") {
      frames.terminate();
      state.recovery = "blocked-target";
      state.problem = [
        "Audio delivery stopped because this Account is unavailable. Contact support for help. Pending audio remains on this page for the original Account.",
        error.problem?.referenceId ? `Reference ID: ${error.problem.referenceId}` : "",
      ]
        .filter(Boolean)
        .join(" ");
      options.onAccountSuspended?.();
      options.requestCaptureStop("hard");
      return;
    }
    if (error.status === 404) {
      frames.terminate();
      state.recovery = "blocked-target";
      state.problem = problemDescription(
        "Audio delivery stopped because its original recording is no longer available. Pending audio remains on this page but cannot be uploaded or sealed.",
        error.problem,
      );
      options.requestCaptureStop("hard");
      return;
    }
    if (error.status === 413 || code === "frame_too_large") {
      frames.terminate();
      state.recovery = "blocked-oversize";
      state.problem = problemDescription(
        "Capture stopped because the backend rejected an audio frame as too large. Pending audio remains on this page but cannot be retried safely.",
        error.problem,
      );
      options.requestCaptureStop("hard");
      return;
    }
    if (error.status !== 409 || state.recovery === "blocked-conflict") return;
    frames.terminate();
    state.recovery = "blocked-conflict";
    state.problem = problemDescription(
      "Capture stopped because audio delivery conflicted with stored data. Audio remains in this page.",
      error.problem,
    );
    options.log(
      `frame conflict HTTP 409 code=${error.problem?.code ?? "none"}` +
        ` referenceId=${error.problem?.referenceId ?? "none"}`,
    );
    options.requestCaptureStop("conflict");
  }

  async function closeIfReady(): Promise<void> {
    if (
      disposed ||
      state.suspended ||
      state.phase !== "ended" ||
      frames.pendingCount ||
      options.canSeal?.() === false ||
      state.close.kind === "closed" ||
      state.close.kind === "closing" ||
      blocked()
    ) {
      return;
    }
    if (interrupted() && state.recovery !== "requested") return;
    const command: CloseCommand =
      state.close.kind === "failed"
        ? state.close.command
        : interrupted()
          ? { kind: "interrupt" }
          : { kind: "complete", frameCount: state.assignedFrames };
    await close(command);
  }

  async function close(command: CloseCommand): Promise<void> {
    if (state.close.kind === "closing" || state.close.kind === "closed" || disposed) return;
    state.close = { kind: "closing", command };
    try {
      const result =
        command.kind === "complete"
          ? await options.gateway.complete(options.audioAssetId, command.frameCount)
          : await options.gateway.interrupt(options.audioAssetId);
      if (disposed) return;
      // Report confirmed sealing before publishing the terminal state. That
      // state can synchronously release a former Account's completed work.
      if (result.isSealed) options.onSealed?.();
      state.close = result.isSealed
        ? { kind: "closed", command, result }
        : {
            kind: "failed",
            command,
            problem:
              command.kind === "complete"
                ? "Could not complete recording"
                : "Could not close recording",
          };
      if (result.isSealed) {
        state.problem = "";
      } else
        options.onSealFailure?.(
          new Error(
            state.close.kind === "failed" ? state.close.problem : "Audio Asset was not sealed.",
          ),
        );
      options.log(command.kind === "complete" ? "complete sent" : "interrupt sent");
    } catch (error) {
      if (disposed) return;
      const problem =
        command.kind === "complete"
          ? `Could not complete recording: ${errorMessage(error)}`
          : `Could not close recording: ${errorMessage(error)}`;
      state.close = { kind: "failed", command, problem };
      options.log(`ERR ${problem}`);
      options.onSealFailure?.(error);
    }
  }

  function completionStatus(): CompletionStatus | null {
    if (state.phase === "running" || (state.reason && isInterruptionReason(state.reason))) {
      return null;
    }
    const closing = state.close;
    if (closing.kind === "open" || closing.kind === "closing") return "pending";
    if (closing.command.kind !== "complete") return null;
    if (closing.kind === "failed") return "failed";
    return closing.result.isSealed && closing.result.sealReason === "completed"
      ? "succeeded"
      : "failed";
  }

  function completionProblem(): string {
    const closing = state.close;
    if (closing.kind === "failed" && closing.command.kind === "complete") {
      return "Could not complete recording.";
    }
    if (closing.kind === "closed" && closing.command.kind === "complete") {
      return closing.result.sealReason === "interrupted"
        ? "Recording was sealed as interrupted and cannot be completed."
        : "Recording was sealed without normal completion and cannot be completed.";
    }
    return "";
  }

  const view = computed<DeliveryView>(() => {
    const completion = completionStatus();
    const closing = state.close;
    const closeProblem =
      closing.kind === "failed" && closing.command.kind === "interrupt" ? closing.problem : "";
    return {
      securedMs: state.securedMs,
      pendingFrames: state.pendingFrames,
      pendingMs: state.pendingMs,
      lastFrameEndMs: state.lastFrameEndMs,
      delayed: state.delayed,
      completion,
      completionProblem: completion === "failed" ? completionProblem() : "",
      canRetryCompletion: closing.kind === "failed" && closing.command.kind === "complete",
      problem: state.problem,
      canUploadPending: state.phase === "ended" && state.pendingFrames > 0 && !blocked(),
      closeProblem,
      canRetryClose: !!closeProblem,
      awaitingSeal: state.phase === "ended" && closing.kind !== "closed",
    };
  });

  return {
    view,
    get backlogged() {
      return frames.pendingCount >= BACKLOG_LIMIT;
    },
    accept(frame) {
      state.lastFrameEndMs = frame.endMs;
      state.assignedFrames++;
      if (frame.data.size > options.maxFrameBytes) {
        frames.freeze();
        state.recovery = "blocked-oversize";
        state.problem = OVERSIZE_PROBLEM;
        frames.enqueue(frame);
        options.requestCaptureStop("oversize");
        return;
      }
      frames.enqueue(frame);
    },
    captureStopping(reason) {
      state.phase = "stopping";
      state.reason = reason;
      if (!isInterruptionReason(reason)) return;
      if (!blocked()) state.recovery = reason === "authentication" ? "requested" : "waiting";
      state.problem ||=
        reason === "backlog"
          ? "Capture was interrupted because audio could not be uploaded."
          : reason === "oversize"
            ? OVERSIZE_PROBLEM
            : reason === "authentication"
              ? "Capture stopped because the Application Session ended. Pending audio remains on this page for the original Account."
              : "Capture was interrupted because the microphone became unavailable.";
      frames.freeze();
    },
    captureEnded(reason) {
      state.phase = "ended";
      state.reason = reason;
      if (isInterruptionReason(reason) && frames.pendingCount === 0 && !blocked()) {
        state.recovery = "requested";
      }
    },
    captureFailedToFinalize(problem) {
      state.phase = "ended";
      state.reason = "capture";
      // A blocked mode stays, with its more specific problem.
      if (!blocked()) {
        state.recovery = frames.pendingCount ? "waiting" : "requested";
        state.problem = problem;
      }
      frames.freeze();
    },
    captureNeverStarted(problem) {
      state.phase = "ended";
      state.reason = "capture";
      state.recovery = "requested";
      state.problem = problem;
    },
    closeIfReady,
    requestUpload() {
      if (state.phase !== "ended" || !frames.pendingCount || blocked()) return;
      if (interrupted()) state.recovery = "requested";
      state.problem = "";
      frames.retry();
    },
    retryClose() {
      if (state.close.kind === "failed") void close(state.close.command);
    },
    suspend() {
      state.suspended = true;
      frames.freeze();
    },
    resume() {
      state.suspended = false;
      if (
        frames.pendingCount &&
        (state.recovery === "automatic" || state.recovery === "requested")
      ) {
        frames.retry();
      }
      void closeIfReady();
    },
    dispose() {
      disposed = true;
      frames.dispose();
    },
  };
}

const OVERSIZE_PROBLEM =
  "Capture was interrupted because an audio frame is too large to upload. Audio remains in this page.";

export function isInterruptionReason(reason: StopReason): boolean {
  return (
    reason === "backlog" ||
    reason === "capture" ||
    reason === "oversize" ||
    reason === "conflict" ||
    reason === "hard" ||
    reason === "authentication"
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function problemDescription(prefix: string, problem: Readonly<ProblemDetails> | null): string {
  const details = [
    problem?.type ? `Type: ${problem.type}` : "",
    problem?.title ? `Title: ${problem.title}` : "",
    problem?.status !== undefined ? `Status: ${problem.status}` : "",
    problem?.detail ? `Detail: ${problem.detail}` : "",
    problem?.instance ? `Instance: ${problem.instance}` : "",
    problem?.code ? `Code: ${problem.code}` : "",
    problem?.referenceId ? `Reference ID: ${problem.referenceId}` : "",
  ].filter(Boolean);
  return [prefix, ...details].join(" ");
}
