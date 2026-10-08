import template from "./recording-panel.html?raw";
import { ApiClient } from "../api.js";
import { openRecordingCapture } from "../capture.js";
import { loadConfig } from "../config.js";
import { CuePlayer } from "../cues.js";
import { RecordingController, type RecordingEvent, type RecordingSnapshot } from "../recording.js";
import { enabledControls, type Controls, type RecordingState } from "../state.js";
import type { Language } from "../api.js";

const STATE_LABELS: Record<RecordingState, string> = {
  idle: "Ready", starting: "Starting", recording: "Recording", finishing: "Finishing",
  complete: "Complete", error: "Recording stopped",
};

/** Recording controls and feedback; publishes recording-event for its container. */
export class RecordingPanel extends HTMLElement {
  private readonly config = loadConfig();
  private readonly cues = new CuePlayer(this);
  private controller: RecordingController | null = null;
  private unsubscribe: (() => void) | null = null;
  private controls!: Record<keyof Controls, HTMLButtonElement | HTMLSelectElement>;
  private stateLabel!: HTMLElement;
  private elapsed!: HTMLElement;
  private secured!: HTMLElement;
  private pending!: HTMLElement;
  private feedback!: HTMLElement;
  private retryButton!: HTMLButtonElement;
  private readonly beforeLeave = (event: BeforeUnloadEvent): void => {
    if (this.controller?.snapshot.hasUnsecuredAudio) {
      event.preventDefault();
      event.returnValue = "";
    }
  };

  connectedCallback(): void {
    if (this.controller) return;
    if (!this.controls) this.setupView();
    this.controller = new RecordingController(this.config, {
      backend: new ApiClient(), openCapture: openRecordingCapture, now: () => performance.now(),
    });
    this.unsubscribe = this.controller.subscribe((event) => {
      if (event.type === "changed") this.render(event.snapshot);
      if (event.type === "cue") this.cues.play(event.cue);
      this.dispatchEvent(new CustomEvent<RecordingEvent>("recording-event", { detail: event, bubbles: true }));
    });
    window.addEventListener("beforeunload", this.beforeLeave);
  }

  disconnectedCallback(): void {
    window.removeEventListener("beforeunload", this.beforeLeave);
    this.unsubscribe?.();
    this.controller?.dispose();
    this.cues.dispose();
    this.controller = null;
    this.unsubscribe = null;
  }

  private setupView(): void {
    this.innerHTML = template;
    this.controls = {
      language: this.element("language"), start: this.element("start"),
      pause: this.element("pause"), continue: this.element("continue"),
      stop: this.element("stop"), cancel: this.element("cancel"),
    };
    this.stateLabel = this.element("state-label");
    this.elapsed = this.element("elapsed");
    this.secured = this.element("secured");
    this.pending = this.element("pending");
    this.feedback = this.element("feedback");
    this.retryButton = this.element("retry");
    this.controls.start.addEventListener("click", () => {
      this.cues.prepare();
      void this.controller?.start(this.controls.language.value as Language);
    });
    this.controls.stop.addEventListener("click", () => void this.controller?.stop());
    this.retryButton.addEventListener("click", () => {
      this.cues.prepare();
      this.controller?.retry();
    });
    for (const name of ["pause", "continue", "cancel"] as const) {
      this.controls[name].addEventListener("click", () => alert("not implemented"));
    }
  }

  private render(snapshot: RecordingSnapshot): void {
    if (this.dataset.state !== snapshot.state) this.dataset.state = snapshot.state;
    this.stateLabel.textContent = STATE_LABELS[snapshot.state];
    const enabled = enabledControls(snapshot.state);
    enabled.start = enabled.language = snapshot.canStart;
    for (const name of Object.keys(this.controls) as (keyof Controls)[]) {
      this.controls[name].disabled = !enabled[name];
    }
    this.controls.start.textContent = snapshot.state === "complete" ? "Start new recording" : "Start";
    this.elapsed.textContent = formatTime(snapshot.elapsedMs);
    this.pending.textContent = `${formatSeconds(snapshot.pendingMs)} of audio pending`;
    this.secured.textContent = `Audio secured: ${formatTime(snapshot.securedMs)}`;
    this.retryButton.hidden = !snapshot.canRetry;
    this.retryButton.textContent = snapshot.pendingFrames
      ? `Upload pending audio (${formatSeconds(snapshot.pendingMs)})` : "Try again";
    this.feedback.textContent = snapshot.feedback;
    this.feedback.dataset.kind = snapshot.state === "error" ? "error" : "warning";
  }

  private element<T extends HTMLElement>(testId: string): T {
    const element = this.querySelector<T>(`[data-testid="${testId}"]`);
    if (!element) throw new Error(`Missing recording-panel element: ${testId}`);
    return element;
  }
}

function formatSeconds(ms: number): string {
  const seconds = Math.ceil(ms / 1000);
  return `${seconds} ${seconds === 1 ? "second" : "seconds"}`;
}
function formatTime(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}
customElements.define("recording-panel", RecordingPanel);
