export type RecordingState = "idle" | "recording" | "finishing" | "complete" | "error";

const TRANSITIONS: Record<RecordingState, readonly RecordingState[]> = {
  idle: ["recording"],
  recording: ["finishing"],
  finishing: ["complete"],
  complete: ["recording"],
  error: [],
};

export class NotImplementedError extends Error {
  constructor(what: string) {
    super(`${what} is not implemented`);
    this.name = "NotImplementedError";
  }
}

/**
 * Validates a state change. Recoverable errors, retry, and Try again arrive
 * with #77 and #78; until then, entering the error state is not implemented.
 */
export function transition(from: RecordingState, to: RecordingState): RecordingState {
  if (to === "error") throw new NotImplementedError("The error state");
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

/** Which controls are enabled. Pause, Continue, and Cancel are stubs (#80). */
export function enabledControls(state: RecordingState): Controls {
  const canStart = state === "idle" || state === "complete";
  return {
    language: canStart,
    start: canStart,
    pause: state === "recording",
    continue: false,
    stop: state === "recording",
    cancel: state === "recording" || state === "finishing",
  };
}
