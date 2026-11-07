import { computed, reactive, shallowRef, type ComputedRef } from "vue";
import type { CaptureOptions, MicrophoneCapture, TransportFrame } from "./capture.ts";
import {
  browserScheduler,
  createFrameTiming,
  transportFrameTiming,
  type Scheduler,
} from "./frame-timing.ts";
import type { CreatedSection, RecordingGateway } from "./gateway.ts";
import type { MicrophoneAccess, MicrophoneReservation } from "./microphone-access.ts";
import type { SectionEvent } from "./section-events.ts";
import {
  BACKLOG_LIMIT,
  createSectionDelivery,
  type DeliveryView,
  type SectionDelivery,
  type StopReason,
} from "./section-delivery.ts";

export interface CaptureConfig {
  frameIntervalMs: number;
  secureDeadlineMs: number;
  retryFirstMs: number;
  retrySecondMs: number;
  retryThirdMs: number;
  requestTimeoutMs: number;
  maxFrameBytes: number;
}

export interface OverdueFrame {
  readonly frameId: number;
  readonly elapsedSeconds: number | null;
}

export interface CaptureDeliveryView extends DeliveryView {
  readonly capturing: boolean;
  readonly finalizingCapture: boolean;
  readonly elapsedMs: number;
  readonly hasUnsecuredAudio: boolean;
  readonly overdueFrames: readonly OverdueFrame[];
  readonly capturedBytes: number;
}

export interface CaptureAndDelivery {
  readonly view: ComputedRef<CaptureDeliveryView>;
  start(createSection: () => Promise<CreatedSection>): Promise<void>;
  stop(): Promise<void>;
  uploadPendingAudio(): void;
  retryClose(): void;
  suspend(): void;
  resume(): void;
  dispose(): void;
}

/** One Section's entire microphone-to-sealed-asset workflow. */
export function createCaptureAndDelivery(options: {
  config: CaptureConfig;
  gateway: RecordingGateway;
  openCapture: (options: CaptureOptions) => Promise<MicrophoneCapture>;
  scheduler?: Scheduler;
  microphoneAccess: MicrophoneAccess;
  onCreated: (created: CreatedSection) => void;
  onEvent: (event: SectionEvent) => void;
  log: (message: string) => void;
}): CaptureAndDelivery {
  const { config } = options;
  const timingValue = transportFrameTiming({
    intervalMs: config.frameIntervalMs,
    secureDeadlineMs: config.secureDeadlineMs,
  });
  const scheduler = options.scheduler ?? browserScheduler;
  const phase = shallowRef<"preparing" | "capturing" | "finalizing" | "ended">("preparing");
  const elapsedMs = shallowRef(0);
  const capturedBytes = shallowRef(0);
  const overdueFrames = shallowRef<readonly OverdueFrame[]>([]);
  const requested = reactive(
    new Map<number, { at: number; arrived: boolean; overdue: boolean; final: boolean }>(),
  );
  const timers = new Set<() => void>();
  let microphone: MicrophoneCapture | null = null;
  let reservation: MicrophoneReservation | null = null;
  let delivery: SectionDelivery | null = null;
  const deliveryRef = shallowRef<SectionDelivery | null>(null);
  let disposed = false;
  let startAttempted = false;
  let suspended = false;
  let revoked = false;
  let startedAt = 0;
  let nextFrame = 0;
  let forceFinalization: (() => void) | null = null;

  function emit(event: SectionEvent): void {
    if (!disposed) options.onEvent(event);
  }
  function schedule(delay: number, callback: () => void): void {
    const cancel = scheduler.schedule(delay, () => {
      timers.delete(cancel);
      if (!disposed) callback();
    });
    timers.add(cancel);
  }
  function refreshOverdue(): void {
    overdueFrames.value = [...requested]
      .filter(([, frame]) => frame.overdue)
      .map(([frameId, frame]) => ({
        frameId,
        elapsedSeconds: Math.ceil((scheduler.now() - frame.at) / 1000),
      }));
  }
  function updateDelay(frameId: number): void {
    if (!requested.has(frameId)) return;
    refreshOverdue();
    schedule(1000, () => updateDelay(frameId));
  }
  function overdue(frameId: number): void {
    const frame = requested.get(frameId);
    if (!frame || frame.overdue) return;
    frame.overdue = true;
    refreshOverdue();
    emit({
      type: "transportFrameOverdue",
      frameId,
      elapsedSeconds: Math.ceil((scheduler.now() - frame.at) / 1000),
    });
    schedule(1000, () => updateDelay(frameId));
  }
  const timing = createFrameTiming({
    timing: timingValue,
    scheduler,
    onFrameTick: () => requestFrame(false),
    onFrameDeadline: (frameId) => schedule(0, () => overdue(frameId)),
  });
  function mark(final: boolean): number {
    const frameId = nextFrame++;
    const at = scheduler.now();
    requested.set(frameId, { at, arrived: false, overdue: false, final });
    timing.markFrameRequested(frameId, at);
    return frameId;
  }
  function checkBacklog(): void {
    if (phase.value === "capturing" && requested.size >= BACKLOG_LIMIT) void stopCapture("backlog");
  }
  function requestFrame(final: boolean): void {
    if (phase.value !== "capturing" || !microphone) return;
    mark(final);
    try {
      microphone.requestFrame();
    } catch (error) {
      options.log(`ERR ${errorMessage(error)}`);
      void stopCapture("capture");
    }
    checkBacklog();
  }
  function accept(frame: TransportFrame): void {
    if (disposed || !delivery) return;
    const pending = requested.get(frame.seq);
    if (!pending) {
      options.log(`Ignored unmatched frame ${frame.seq}`);
      return;
    }
    pending.arrived = true;
    capturedBytes.value += frame.data.size;
    emit({
      type: "transportFrameCaptured",
      bytes: frame.data.size,
      totalBytes: capturedBytes.value,
      captureActive: phase.value === "capturing",
    });
    delivery.accept(frame);
    checkBacklog();
  }
  function progress(): void {
    if (phase.value !== "capturing") return;
    elapsedMs.value = scheduler.now() - startedAt;
    schedule(100, progress);
  }
  function revoke(): void {
    revoked = true;
    // Stop is initiated as a user action, but forced resource cleanup does not
    // wait for a possibly stale workflow or finalization promise.
    const stopping = stopCapture("user");
    microphone?.dispose();
    forceFinalization?.();
    reservation?.release();
    void stopping;
  }
  async function stopCapture(reason: StopReason): Promise<void> {
    if (disposed || phase.value !== "capturing" || !microphone || !delivery) return;
    phase.value = "finalizing";
    elapsedMs.value = scheduler.now() - startedAt;
    timing.stopFrameTicks();
    delivery.captureStopping(reason);
    options.log(`capture stop ${reason}`);
    if (reason !== "user") emit({ type: "unrequestedCaptureStopped" });
    const finalId = mark(true);
    const forced = new Promise<void>((resolve) => {
      forceFinalization = resolve;
    });
    try {
      await Promise.race([microphone.stop(), forced]);
      if (disposed) return;
      delivery.captureEnded(reason);
    } catch (error) {
      if (disposed) return;
      delivery.captureFailedToFinalize(
        `Capture was interrupted while finalizing: ${errorMessage(error)}`,
      );
      options.log(`ERR Capture was interrupted while finalizing: ${errorMessage(error)}`);
      emit({ type: "captureFinalizationFailed", reason, error });
    } finally {
      forceFinalization = null;
      microphone.dispose();
      reservation?.release();
    }
    if (disposed) return;
    // A successful stop may contain no non-empty final Blob. Only that optional
    // final request disappears; missing regular Blobs remain unsecured.
    if (!requested.get(finalId)?.arrived) requested.delete(finalId);
    phase.value = "ended";
    refreshOverdue();
    await delivery.closeIfReady();
  }

  const view = computed<CaptureDeliveryView>(() => {
    const delivered = deliveryRef.value?.view.value;
    const missing = [...requested.values()].filter((frame) => !frame.arrived).length;
    const running = phase.value === "capturing" || phase.value === "finalizing";
    const unfinalizedMs = running
      ? Math.max(0, elapsedMs.value - (delivered?.lastFrameEndMs ?? 0))
      : 0;
    return {
      securedMs: delivered?.securedMs ?? 0,
      pendingFrames: (delivered?.pendingFrames ?? 0) + missing,
      pendingMs: (delivered?.pendingMs ?? 0) + unfinalizedMs,
      lastFrameEndMs: delivered?.lastFrameEndMs ?? 0,
      delayed: overdueFrames.value.length > 0,
      completion: delivered?.completion ?? null,
      completionProblem: delivered?.completionProblem ?? "",
      canRetryCompletion: delivered?.canRetryCompletion ?? false,
      problem: delivered?.problem ?? "",
      canUploadPending: delivered?.canUploadPending ?? false,
      closeProblem: delivered?.closeProblem ?? "",
      canRetryClose: delivered?.canRetryClose ?? false,
      awaitingSeal: delivered?.awaitingSeal ?? false,
      capturing: phase.value === "capturing",
      finalizingCapture: phase.value === "finalizing",
      elapsedMs: elapsedMs.value,
      hasUnsecuredAudio: running || requested.size > 0,
      overdueFrames: overdueFrames.value,
      capturedBytes: capturedBytes.value,
    };
  });
  return {
    view,
    async start(createSection) {
      if (startAttempted || disposed)
        throw new Error("A Section Capture workflow can only start once.");
      startAttempted = true;
      try {
        reservation = options.microphoneAccess.acquire(revoke);
        if (!reservation) {
          if (!(await options.microphoneAccess.requestTakeover()) || disposed || suspended) return;
          reservation = options.microphoneAccess.acquire(revoke);
          if (!reservation) throw new Error("The microphone is still in use.");
        }
        microphone = await options.openCapture({
          now: () => scheduler.now(),
          onError(error) {
            options.log(`ERR ${error.message}`);
            void stopCapture("capture");
          },
        });
        if (disposed || suspended || revoked || !reservation.active) {
          microphone.dispose();
          reservation.release();
          return;
        }
        const created = await createSection();
        if (disposed) {
          microphone.dispose();
          reservation.release();
          return;
        }
        delivery = createSectionDelivery({
          audioAssetId: created.audioAssetId,
          retryDelaysMs: [config.retryFirstMs, config.retrySecondMs, config.retryThirdMs],
          requestTimeoutMs: config.requestTimeoutMs,
          maxFrameBytes: config.maxFrameBytes,
          gateway: options.gateway,
          scheduler,
          now: () => scheduler.now(),
          log: options.log,
          onAcknowledged(frame) {
            const pending = requested.get(frame.seq);
            if (!pending) return;
            // At the exact deadline acknowledgement wins, regardless of the
            // promise/timer callback ordering within that turn.
            if (scheduler.now() > pending.at + timingValue.secureDeadlineMs) overdue(frame.seq);
            requested.delete(frame.seq);
            refreshOverdue();
            emit({
              type: "transportFrameSecured",
              frameId: frame.seq,
              captureActive: phase.value === "capturing",
              finalFrame: pending.final,
            });
          },
          onDelayed() {}, // Retry timing is diagnostic, not the securing deadline.
          onSealed: () => emit({ type: "audioAssetSealed" }),
          onSealFailure: (error) => emit({ type: "audioAssetSealingFailed", error }),
          onAllowanceRejected: (dimension) => emit({ type: "allowanceRejected", dimension }),
          onAccountSuspended: () => emit({ type: "accountSuspended" }),
          canSeal: () => requested.size === 0,
          requestCaptureStop: (reason) => {
            void stopCapture(reason);
          },
        });
        deliveryRef.value = delivery;
        options.onCreated(created);
        if (suspended || revoked || !reservation.active) {
          if (suspended) delivery.suspend();
          phase.value = "ended";
          microphone.dispose();
          reservation.release();
          delivery.captureNeverStarted(
            suspended
              ? "Capture did not start because its Application Session ended."
              : "Capture did not start because microphone access was taken over.",
          );
          await delivery.closeIfReady();
          return;
        }
        try {
          await microphone.start(accept);
        } catch (error) {
          phase.value = "ended";
          delivery.captureNeverStarted(`Capture could not begin: ${errorMessage(error)}`);
          void delivery.closeIfReady();
          throw error;
        }
        if (disposed || revoked) {
          microphone.dispose();
          reservation.release();
          if (!disposed) {
            phase.value = "ended";
            delivery.captureNeverStarted(
              "Capture did not start because microphone access was taken over.",
            );
            await delivery.closeIfReady();
          }
          return;
        }
        phase.value = "capturing";
        startedAt = scheduler.now();
        emit({ type: "captureStarted" });
        progress();
        schedule(timingValue.intervalMs, () => requestFrame(false));
      } catch (error) {
        microphone?.dispose();
        reservation?.release();
        throw error;
      }
    },
    stop: () => stopCapture("user"),
    uploadPendingAudio() {
      delivery?.requestUpload();
    },
    retryClose() {
      delivery?.retryClose();
    },
    suspend() {
      suspended = true;
      delivery?.suspend();
      if (phase.value === "capturing") void stopCapture("authentication");
    },
    resume() {
      suspended = false;
      delivery?.resume();
    },
    dispose() {
      disposed = true;
      timing.dispose();
      for (const cancel of timers) cancel();
      timers.clear();
      microphone?.dispose();
      forceFinalization?.();
      reservation?.release();
      delivery?.dispose();
    },
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
