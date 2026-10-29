import { shallowRef, type Ref } from "vue";
import type { MicrophoneCapture, TransportFrame } from "./capture.ts";
import type { StopReason } from "./section-delivery.ts";

export type CaptureState =
  | { kind: "preparing" }
  | { kind: "capturing"; startedAt: number; elapsedMs: number }
  | { kind: "finalizing"; reason: StopReason; startedAt: number; elapsedMs: number }
  | { kind: "ended"; reason: StopReason; elapsedMs: number };

export type StopOutcome =
  { outcome: "skipped" } | { outcome: "ended" } | { outcome: "failed"; error: unknown };

export interface SectionCaptureOptions {
  capture: MicrophoneCapture;
  /** Monotonic elapsed-time clock. */
  now: () => number;
  maxRecordingMs: number;
  /** The duration limit was reached. */
  onLimit: () => void;
}

/**
 * The transient part of one Section: the microphone, the progress and duration
 * timers, and the capture lifecycle. It is the only owner that releases the
 * microphone. It knows nothing about delivery.
 */
export interface SectionCapture {
  readonly state: Readonly<Ref<CaptureState>>;
  /** Start capturing. On failure the microphone is released and the state is `ended`. */
  start(onFrame: (frame: TransportFrame) => void): Promise<void>;
  /**
   * Stop and wait for the final frame. `onFinalizing` runs synchronously after the
   * state becomes `finalizing`, before the wait. Does nothing unless capturing.
   */
  stop(reason: StopReason, onFinalizing: () => void): Promise<StopOutcome>;
  /** Give up before capturing: release the microphone and mark the run ended. */
  abandon(): void;
  /** Release everything now. Safe to call repeatedly. */
  dispose(): void;
}

export function createSectionCapture(options: SectionCaptureOptions): SectionCapture {
  const { capture, now } = options;
  const state = shallowRef<CaptureState>({ kind: "preparing" });
  let progressTimer: ReturnType<typeof setInterval> | null = null;
  let limitTimer: ReturnType<typeof setTimeout> | null = null;

  function clearTimers(): void {
    if (progressTimer !== null) clearInterval(progressTimer);
    if (limitTimer !== null) clearTimeout(limitTimer);
    progressTimer = null;
    limitTimer = null;
  }

  return {
    state,
    async start(onFrame) {
      const startedAt = now();
      progressTimer = setInterval(() => {
        const current = state.value;
        if (current.kind !== "capturing") return;
        state.value = { ...current, elapsedMs: now() - current.startedAt };
      }, 100);
      limitTimer = setTimeout(() => options.onLimit(), options.maxRecordingMs);
      state.value = { kind: "capturing", startedAt, elapsedMs: 0 };
      try {
        await capture.start(onFrame);
      } catch (error) {
        clearTimers();
        state.value = { kind: "ended", reason: "capture", elapsedMs: 0 };
        capture.dispose();
        throw error;
      }
    },
    async stop(reason, onFinalizing) {
      const current = state.value;
      if (current.kind !== "capturing") return { outcome: "skipped" };
      clearTimers();
      const elapsedMs = now() - current.startedAt;
      state.value = { kind: "finalizing", reason, startedAt: current.startedAt, elapsedMs };
      onFinalizing();
      try {
        await capture.stop();
      } catch (error) {
        state.value = { kind: "ended", reason: "capture", elapsedMs };
        return { outcome: "failed", error };
      }
      state.value = { kind: "ended", reason, elapsedMs };
      return { outcome: "ended" };
    },
    abandon() {
      clearTimers();
      state.value = { kind: "ended", reason: "capture", elapsedMs: 0 };
      capture.dispose();
    },
    dispose() {
      clearTimers();
      capture.dispose();
    },
  };
}
