import { captureAlertId, type CaptureAlerts, type CaptureAlert } from "../alerts/capture-alerts.ts";
import type { Cue } from "./cues.ts";
import type { SectionEvent } from "./section-events.ts";

interface CueSink {
  play(cue: Cue): void;
}

export function createCaptureFeedback(
  cues: CueSink,
  alerts: CaptureAlerts,
): (sectionId: string, event: SectionEvent) => void {
  const completionBlocked = new Set<string>();
  function raise(alert: CaptureAlert): void {
    alerts.raise(alert);
  }
  function resolve(alert: Parameters<typeof captureAlertId>[0]): void {
    alerts.resolve(captureAlertId(alert));
  }
  function play(cue: Cue): void {
    try {
      cues.play(cue);
    } catch {
      /* Visible state remains available when a one-shot cue cannot play. */
    }
  }

  return (sectionId, event) => {
    switch (event.type) {
      case "captureStarted":
        play("capture-started");
        break;
      case "transportFrameSecured":
        resolve({ kind: "transport-frame-overdue", sectionId, frameId: event.frameId });
        if (event.captureActive && !event.finalFrame) play("healthy-heartbeat");
        break;
      case "transportFrameOverdue":
        raise({
          kind: "transport-frame-overdue",
          sectionId,
          frameId: event.frameId,
          elapsedSeconds: event.elapsedSeconds,
        });
        break;
      case "captureFinalizationFailed":
        completionBlocked.add(sectionId);
        raise({ kind: "capture-finalization-failed", sectionId });
        break;
      case "audioAssetSealingFailed":
        raise({ kind: "audio-asset-sealing-failed", sectionId });
        break;
      case "audioAssetSealed":
        resolve({ kind: "audio-asset-sealing-failed", sectionId });
        if (
          !completionBlocked.delete(sectionId) &&
          !alerts.alerts.value.some((alert) => alert.sectionId === sectionId)
        ) {
          play("capture-completed");
        }
        break;
      case "unrequestedCaptureStopped":
        completionBlocked.add(sectionId);
        raise({ kind: "unrequested-capture-stop", sectionId });
        break;
    }
  };
}
