import { shallowRef, type Ref, type InjectionKey } from "vue";

export interface MicrophoneReservation {
  readonly active: boolean;
  release(): void;
}

export interface MicrophoneAccess {
  readonly takeoverRequested: Readonly<Ref<boolean>>;
  acquire(stopAndRelease: () => void): MicrophoneReservation | null;
  requestTakeover(): Promise<boolean>;
  cancelTakeover(): void;
  confirmTakeover(): void;
  dispose(): void;
}

/** Page-local exclusive access. Revoked holders cannot release their successors. */
export function createMicrophoneAccess(): MicrophoneAccess {
  const takeoverRequested = shallowRef(false);
  let holder: { stopAndRelease: () => void } | null = null;
  let disposed = false;
  let decision: ((confirmed: boolean) => void) | null = null;
  return {
    takeoverRequested,
    acquire(stopAndRelease) {
      if (disposed || holder) return null;
      const reservation = { stopAndRelease };
      holder = reservation;
      return {
        get active() {
          return holder === reservation;
        },
        release() {
          if (holder === reservation) holder = null;
        },
      };
    },
    requestTakeover() {
      if (disposed) return Promise.resolve(false);
      if (!holder) return Promise.resolve(true);
      if (decision) return Promise.resolve(false);
      takeoverRequested.value = true;
      return new Promise<boolean>((resolve) => {
        decision = resolve;
      });
    },
    cancelTakeover() {
      takeoverRequested.value = false;
      const resolve = decision;
      decision = null;
      resolve?.(false);
    },
    confirmTakeover() {
      const resolve = decision;
      if (!resolve) return;
      // Detach before cleanup: a late release cannot affect the next reservation.
      const previous = holder;
      holder = null;
      takeoverRequested.value = false;
      decision = null;
      try {
        previous?.stopAndRelease();
        resolve(true);
      } catch (error) {
        // Failed cleanup must not allow a second owner to use the microphone.
        holder = previous;
        resolve(false);
        console.error("Could not release previous microphone Capture.", error);
      }
    },
    dispose() {
      disposed = true;
      this.cancelTakeover();
      const previous = holder;
      holder = null;
      try {
        previous?.stopAndRelease();
      } catch (error) {
        console.error("Could not release microphone Capture during disposal.", error);
      }
    },
  };
}

export const microphoneAccessKey: InjectionKey<MicrophoneAccess> = Symbol("microphone-access");
