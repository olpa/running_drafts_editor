import type { Cue, Cues } from "./cues.ts";
import type { SectionEvent } from "./section-events.ts";

export function playSectionFeedback(event: SectionEvent, cues: Cues): void {
  let cue: Cue | undefined;
  switch (event.type) {
    case "captureStarted":
      cue = "start";
      break;
    case "transportFrameSecured":
      if (event.captureActive && !event.finalFrame) cue = "health";
      break;
    case "captureFinalizationFailed":
      if (event.reason === "user") cue = "delayed";
      break;
    case "transportFrameOverdue":
    case "audioAssetSealingFailed":
      cue = "delayed";
      break;
    case "audioAssetSealed":
      cue = "stop";
      break;
    // The application alarm owns the unrequested-stop sound and its lifetime.
    case "unrequestedCaptureStopped":
      break;
  }
  if (cue) {
    try {
      cues.play(cue);
    } catch {
      /* Visible state remains available when sound fails. */
    }
  }
}
