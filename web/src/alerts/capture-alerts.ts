import { inject, readonly, shallowRef, type InjectionKey, type Ref } from "vue";

export type CaptureAlert =
  | {
      readonly kind: "transport-frame-overdue";
      readonly sectionId: string;
      readonly frameId: number;
      readonly elapsedSeconds: number;
    }
  | {
      readonly kind:
        "capture-finalization-failed" | "audio-asset-sealing-failed" | "unrequested-capture-stop";
      readonly sectionId: string;
    };

declare const captureAlertIdBrand: unique symbol;
export type CaptureAlertId = string & { readonly [captureAlertIdBrand]: true };

type CaptureAlertIdentity =
  | Pick<
      Extract<CaptureAlert, { kind: "transport-frame-overdue" }>,
      "kind" | "sectionId" | "frameId"
    >
  | Extract<CaptureAlert, { kind: Exclude<CaptureAlert["kind"], "transport-frame-overdue"> }>;

export function captureAlertId(alert: CaptureAlertIdentity): CaptureAlertId {
  return JSON.stringify([
    alert.sectionId,
    alert.kind,
    alert.kind === "transport-frame-overdue" ? alert.frameId : null,
  ]) as CaptureAlertId;
}

export interface ProblemSound {
  loopProblem(onUnavailable: () => void): () => void;
}

export interface CaptureAlerts {
  readonly alerts: Readonly<Ref<readonly CaptureAlert[]>>;
  readonly soundUnavailable: Readonly<Ref<boolean>>;
  raise(alert: CaptureAlert): void;
  resolve(id: CaptureAlertId): void;
  acknowledgeAll(): void;
  dispose(): void;
}

export function createCaptureAlerts(sound: ProblemSound): CaptureAlerts {
  const alerts = shallowRef<readonly CaptureAlert[]>([]);
  const soundUnavailable = shallowRef(false);
  let stopSound: (() => void) | null = null;
  let generation = 0;
  let disposed = false;

  function stop(): void {
    generation++;
    const cleanup = stopSound;
    stopSound = null;
    try {
      cleanup?.();
    } catch {
      /* Sound cleanup cannot prevent alert resolution or acknowledgement. */
    }
  }

  function start(): void {
    const current = ++generation;
    soundUnavailable.value = false;
    try {
      stopSound = sound.loopProblem(() => {
        if (!disposed && alerts.value.length > 0 && current === generation) {
          soundUnavailable.value = true;
        }
      });
    } catch {
      soundUnavailable.value = true;
    }
  }

  function clear(): void {
    alerts.value = [];
    soundUnavailable.value = false;
    stop();
  }

  return {
    alerts: readonly(alerts),
    soundUnavailable: readonly(soundUnavailable),
    raise(alert) {
      if (disposed) return;
      const id = captureAlertId(alert);
      if (alerts.value.some((active) => captureAlertId(active) === id)) return;
      const startSound = alerts.value.length === 0;
      alerts.value = [...alerts.value, alert];
      if (startSound) start();
    },
    resolve(id) {
      if (disposed) return;
      const remaining = alerts.value.filter((alert) => captureAlertId(alert) !== id);
      if (remaining.length === alerts.value.length) return;
      alerts.value = remaining;
      if (remaining.length === 0) {
        soundUnavailable.value = false;
        stop();
      }
    },
    acknowledgeAll: clear,
    dispose() {
      disposed = true;
      stop();
    },
  };
}

export const captureAlertsKey: InjectionKey<CaptureAlerts> = Symbol("capture-alerts");

export function useCaptureAlerts(): CaptureAlerts {
  const alerts = inject(captureAlertsKey);
  if (!alerts) throw new Error("The Capture alerts module was not provided.");
  return alerts;
}
