export type RecordingState = "idle" | "starting" | "recording" | "finishing" | "complete" | "error";

const TRANSITIONS: Record<RecordingState, readonly RecordingState[]> = {
  idle: ["starting"],
  starting: ["recording", "error"],
  recording: ["finishing", "error"],
  finishing: ["complete", "error"],
  complete: ["starting"],
  error: ["starting", "finishing"],
};

export function transition(from: RecordingState, to: RecordingState): RecordingState {
  if (!TRANSITIONS[from].includes(to)) {
    throw new Error(`Invalid recording state transition: ${from} -> ${to}`);
  }
  return to;
}

export interface Controls {
  language: boolean;
  start: boolean;
  pause: boolean;
  continue: boolean;
  stop: boolean;
  cancel: boolean;
}

/** Pause, Continue, and Cancel remain stubs (#80). */
export function enabledControls(state: RecordingState): Controls {
  const canStart = state === "idle" || state === "complete" || state === "error";
  return {
    language: canStart,
    start: canStart,
    pause: state === "recording",
    continue: false,
    stop: state === "recording",
    cancel: state === "recording" || state === "finishing",
  };
}
