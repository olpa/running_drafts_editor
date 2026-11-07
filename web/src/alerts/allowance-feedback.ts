import { computed, inject, readonly, shallowRef, type InjectionKey, type Ref } from "vue";
import type { ProblemSound } from "./capture-alerts.ts";
import { browserScheduler, type Scheduler } from "../recording/frame-timing.ts";

export type AllowanceDimension = "storage" | "duration";

export interface AllowanceFeedback {
  readonly active: Readonly<Ref<boolean>>;
  readonly soundUnavailable: Readonly<Ref<boolean>>;
  play(dimension: AllowanceDimension): void;
  silence(): void;
  dispose(): void;
}

/** Bounded warning playback; it is not an acknowledgement-governed Capture alert. */
export function createAllowanceFeedback(
  sound: ProblemSound & {
    loopAllowanceWarning?: (onUnavailable: () => void) => () => void;
  },
  scheduler: Scheduler = browserScheduler,
): AllowanceFeedback {
  const activeDimensions = shallowRef<ReadonlySet<AllowanceDimension>>(new Set());
  const soundUnavailable = shallowRef(false);
  const stops = new Map<AllowanceDimension, () => void>();
  const timers = new Map<AllowanceDimension, () => void>();
  let generation = 0;
  let disposed = false;

  function finish(dimension: AllowanceDimension): void {
    timers.get(dimension)?.();
    timers.delete(dimension);
    try {
      stops.get(dimension)?.();
    } catch {
      // Sound cleanup never changes warning state.
    }
    stops.delete(dimension);
    const next = new Set(activeDimensions.value);
    next.delete(dimension);
    activeDimensions.value = next;
    if (next.size === 0) soundUnavailable.value = false;
  }

  function silence(): void {
    generation++;
    for (const dimension of [...activeDimensions.value]) finish(dimension);
  }

  return {
    active: computed(() => activeDimensions.value.size > 0),
    soundUnavailable: readonly(soundUnavailable),
    play(dimension) {
      if (disposed || activeDimensions.value.has(dimension)) return;
      const current = generation;
      activeDimensions.value = new Set([...activeDimensions.value, dimension]);
      try {
        const loop = sound.loopAllowanceWarning?.bind(sound) ?? sound.loopProblem.bind(sound);
        stops.set(
          dimension,
          loop(() => {
            if (!disposed && current === generation && activeDimensions.value.has(dimension)) {
              soundUnavailable.value = true;
            }
          }),
        );
      } catch {
        soundUnavailable.value = true;
      }
      timers.set(
        dimension,
        scheduler.schedule(3_000, () => finish(dimension)),
      );
    },
    silence,
    dispose() {
      if (disposed) return;
      disposed = true;
      silence();
    },
  };
}

export const allowanceFeedbackKey: InjectionKey<AllowanceFeedback> = Symbol("allowance-feedback");

export function useAllowanceFeedback(): AllowanceFeedback {
  const feedback = inject(allowanceFeedbackKey);
  if (!feedback) throw new Error("Allowance feedback was not provided.");
  return feedback;
}
