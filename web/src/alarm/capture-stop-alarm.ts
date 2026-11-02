import { inject, shallowRef, type InjectionKey, type Ref } from "vue";
import type { Cues } from "../recording/cues.ts";

export interface CaptureStopAlarm {
  readonly active: Readonly<Ref<boolean>>;
  readonly soundUnavailable: Readonly<Ref<boolean>>;
  activate(): void;
  acknowledge(): void;
  dispose(): void;
}

export function createCaptureStopAlarm(
  cues: Cues,
  log: (message: string) => void = console.warn,
): CaptureStopAlarm {
  const active = shallowRef(false);
  const soundUnavailable = shallowRef(false);
  let stopSound: (() => void) | null = null;
  let disposed = false;
  let generation = 0;
  function stop(): void {
    const cleanup = stopSound;
    stopSound = null;
    try {
      cleanup?.();
    } catch {
      /* Audio failure must not block acknowledgement. */
    }
  }
  return {
    active,
    soundUnavailable,
    activate() {
      if (disposed) return;
      if (active.value) {
        log("Additional unrequested Capture stop while alarm is active.");
        return;
      }
      const current = ++generation;
      active.value = true;
      soundUnavailable.value = false;
      try {
        stopSound = cues.loopInterrupted(() => {
          if (active.value && !disposed && current === generation) soundUnavailable.value = true;
        });
      } catch {
        soundUnavailable.value = true;
      }
    },
    acknowledge() {
      generation++;
      active.value = false;
      soundUnavailable.value = false;
      stop();
    },
    dispose() {
      generation++;
      disposed = true;
      stop();
    },
  };
}
export const captureStopAlarmKey: InjectionKey<CaptureStopAlarm> = Symbol("capture-stop-alarm");
export function useCaptureStopAlarm(): CaptureStopAlarm {
  const alarm = inject(captureStopAlarmKey);
  if (!alarm) throw new Error("The Capture stop alarm was not provided.");
  return alarm;
}
