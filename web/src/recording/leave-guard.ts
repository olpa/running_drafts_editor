import { onBeforeUnmount, onMounted } from "vue";
import type { ApplicationCapture } from "./application-capture.ts";

/** Ask the browser to confirm leaving the page while audio is not yet durably delivered. */
export function useLeaveGuard(recording: ApplicationCapture): void {
  const beforeLeave = (event: BeforeUnloadEvent): void => {
    if (recording.hasUnsecuredAudio.value) {
      event.preventDefault();
      event.returnValue = "";
    }
  };
  onMounted(() => window.addEventListener("beforeunload", beforeLeave));
  onBeforeUnmount(() => window.removeEventListener("beforeunload", beforeLeave));
}
