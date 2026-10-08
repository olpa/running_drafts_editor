import template from "./seed-app.html?raw";
import { ApiClient, type Chunk, type Language, type RecordingStatus } from "../api.js";
import { RecorderFrameSource, openMicrophone, recordingMediaType, type FrameSource } from "../capture.js";
import { loadConfig, type SeedConfig } from "../config.js";
import { CuePlayer } from "../cues.js";
import { FrameDelivery } from "../delivery.js";
import { RadarLog } from "../radar-log.js";
import { enabledControls, transition, type Controls, type RecordingState } from "../state.js";

type StopReason = "user" | "duration" | "backlog" | "capture";
const STATE_LABELS: Record<RecordingState, string> = {
  idle: "Ready", starting: "Starting", recording: "Recording", finishing: "Finishing",
  complete: "Complete", error: "Recording stopped",
};

class ActiveRecording {
  recordingId = "";
  startedAt = 0;
  elapsedMs = 0;
  lastFrameEndMs = 0;
  securedMs = 0;
  stream: MediaStream | null = null;
  frameSource: FrameSource | null = null;
  delivery!: FrameDelivery;
  capturing = false;
  finalized = false;
  dead = false;
  stopReason: StopReason | null = null;
  problem = "";
  finishing = false;
  finishAccepted = false;
  pollTimer: number | null = null;
  progressTimer: number | null = null;
  limitTimer: number | null = null;
  serverStatus: RecordingStatus["status"] | null = null;
  shownChunks: Chunk[] = [];
}

export class SeedApp extends HTMLElement {
  private readonly config: SeedConfig = loadConfig();
  private readonly api = new ApiClient();
  private readonly cues = new CuePlayer(this);
  private state: RecordingState = "idle";
  private active: ActiveRecording | null = null;
  private log!: RadarLog;
  private controls!: Record<keyof Controls, HTMLButtonElement | HTMLSelectElement>;
  private stateLabel!: HTMLElement;
  private transcript!: HTMLOListElement;
  private logView!: HTMLPreElement;
  private feedback!: HTMLElement;
  private retryButton!: HTMLButtonElement;
  private readonly beforeLeave = (event: BeforeUnloadEvent): void => {
    if (this.active?.capturing || this.active?.delivery.pendingCount) {
      event.preventDefault();
      event.returnValue = "";
    }
  };

  connectedCallback(): void {
    if (this.controls) return;
    this.innerHTML = template;
    this.controls = {
      language: this.element("language"), start: this.element("start"),
      pause: this.element("pause"), continue: this.element("continue"),
      stop: this.element("stop"), cancel: this.element("cancel"),
    };
    this.stateLabel = this.element("state-label");
    this.transcript = this.element("transcript");
    this.logView = this.element("log");
    this.feedback = this.element("feedback");
    this.retryButton = this.element("retry");
    this.controls.start.addEventListener("click", () => void this.start());
    this.controls.stop.addEventListener("click", () => void this.stop("user"));
    this.retryButton.addEventListener("click", () => this.retry());
    for (const name of ["pause", "continue", "cancel"] as const) {
      this.controls[name].addEventListener("click", () => alert("not implemented"));
    }
    window.addEventListener("beforeunload", this.beforeLeave);
    this.resetLog();
    this.render();
  }

  disconnectedCallback(): void {
    window.removeEventListener("beforeunload", this.beforeLeave);
    if (this.active) {
      this.active.dead = true;
      this.cleanupTimers(this.active);
      this.active.delivery.dispose();
      void this.active.frameSource?.stop();
      this.active.stream?.getTracks().forEach((track) => track.stop());
    }
  }

  private element<T extends HTMLElement>(testId: string): T {
    const element = this.querySelector<T>(`[data-testid="${testId}"]`);
    if (!element) throw new Error(`Missing seed-app element: ${testId}`);
    return element;
  }

  private async start(): Promise<void> {
    if (this.active && (this.active.delivery.pendingCount || this.active.capturing)) return;
    this.active?.delivery.dispose();
    this.cues.prepare();
    const recording = new ActiveRecording();
    recording.delivery = new FrameDelivery({
      retryDelaysMs: [this.config.retryFirstMs, this.config.retrySecondMs, this.config.retryThirdMs],
      requestTimeoutMs: this.config.requestTimeoutMs,
      send: (frame, signal) => this.api.putFrame(recording.recordingId, frame, signal),
      log: (message) => this.write(recording, message),
      changed: () => {
        if (!this.isCurrent(recording)) return;
        this.render();
        void this.maybeFinish(recording);
      },
      acknowledged: (frame) => {
        if (!this.isCurrent(recording)) return;
        recording.securedMs = frame.endMs;
        this.cues.play("health");
      },
      delayed: () => { if (this.isCurrent(recording)) this.cues.play("delayed"); },
    });
    this.active = recording;
    this.transcript.replaceChildren();
    this.resetLog();
    this.setState("starting");
    try {
      recordingMediaType();
      const stream = await openMicrophone();
      if (!this.isCurrent(recording)) {
        stream.getTracks().forEach((track) => track.stop());
        return;
      }
      recording.stream = stream;
      this.write(recording, "mic ok");
      const language = this.controls.language.value as Language;
      ({ recordingId: recording.recordingId } = await this.api.createRecording(language));
      if (!this.isCurrent(recording)) return;
      this.write(recording, `rec ${recording.recordingId.slice(0, 8)} ${language}`);
      recording.frameSource = new RecorderFrameSource(stream, this.config.frameIntervalMs, () => {
        if (recording.delivery.pendingCount >= 2) {
          void this.stop("backlog");
          return false;
        }
        return recording.capturing;
      }, (error) => {
        this.write(recording, `ERR ${error.message}`);
        void this.stop("capture");
      });
      recording.startedAt = performance.now();
      recording.capturing = true;
      recording.frameSource.start((frame) => {
        if (!this.isCurrent(recording)) return;
        recording.lastFrameEndMs = frame.endMs;
        recording.delivery.enqueue(frame);
      });
      for (const track of stream.getAudioTracks()) {
        track.addEventListener("ended", () => {
          if (recording.capturing && this.isCurrent(recording)) void this.stop("capture");
        });
      }
      this.setState("recording");
      this.cues.play("start");
      this.write(recording, "cue start");
      recording.progressTimer = window.setInterval(() => {
        recording.elapsedMs = performance.now() - recording.startedAt;
        this.render();
      }, 100);
      recording.limitTimer = window.setTimeout(() => void this.stop("duration"), this.config.maxRecordingMs);
      this.schedulePoll(recording);
    } catch (error) {
      if (!this.isCurrent(recording)) return;
      recording.capturing = false;
      recording.stream?.getTracks().forEach((track) => track.stop());
      recording.problem = `Could not start recording: ${errorMessage(error)}`;
      this.write(recording, `ERR ${recording.problem}`);
      this.setState("error");
    }
  }

  private async stop(reason: StopReason): Promise<void> {
    const recording = this.active;
    if (!recording?.capturing || !recording.frameSource) return;
    recording.capturing = false;
    recording.stopReason = reason;
    recording.elapsedMs = performance.now() - recording.startedAt;
    if (recording.progressTimer !== null) window.clearInterval(recording.progressTimer);
    if (recording.limitTimer !== null) window.clearTimeout(recording.limitTimer);
    recording.progressTimer = recording.limitTimer = null;
    this.write(recording, `capture stop ${reason}`);
    if (reason === "backlog" || reason === "capture") {
      recording.problem = reason === "backlog"
        ? "Recording stopped because audio could not be uploaded."
        : "Recording stopped because the microphone became unavailable.";
      recording.delivery.freeze();
      this.setState("error");
      this.cues.play("interrupted");
    } else {
      this.setState("finishing");
    }
    await recording.frameSource.stop();
    recording.stream?.getTracks().forEach((track) => track.stop());
    if (!this.isCurrent(recording)) return;
    recording.finalized = true;
    this.render();
    await this.maybeFinish(recording);
  }

  private retry(): void {
    const recording = this.active;
    if (!recording) return;
    this.cues.prepare();
    if (recording.stopReason && this.state === "error") {
      recording.problem = "";
      this.setState("finishing");
    }
    recording.delivery.retry();
    void this.maybeFinish(recording);
  }

  private async maybeFinish(recording: ActiveRecording): Promise<void> {
    if (!this.isCurrent(recording) || !recording.finalized || recording.delivery.pendingCount
      || recording.finishing || recording.finishAccepted) return;
    recording.finishing = true;
    if (this.state === "error") {
      recording.problem = "";
      this.setState("finishing");
    }
    try {
      await this.api.finish(recording.recordingId);
      if (!this.isCurrent(recording)) return;
      recording.finishAccepted = true;
      this.write(recording, "fin sent");
    } catch (error) {
      if (!this.isCurrent(recording)) return;
      recording.problem = `Could not finish recording: ${errorMessage(error)}`;
      this.write(recording, `ERR ${recording.problem}`);
      this.setState("error");
    } finally {
      recording.finishing = false;
      if (this.isCurrent(recording)) this.render();
    }
  }

  private schedulePoll(recording: ActiveRecording): void {
    recording.pollTimer = window.setTimeout(() => void this.poll(recording), this.config.pollIntervalMs);
  }

  private async poll(recording: ActiveRecording): Promise<void> {
    try {
      const status = await this.api.getRecording(recording.recordingId);
      if (!this.isCurrent(recording)) return;
      this.showChunks(recording, status.chunks);
      if (status.status !== recording.serverStatus) {
        recording.serverStatus = status.status;
        this.write(recording, `srv ${status.status}`);
      }
      if (status.status === "complete" && recording.finishAccepted) {
        this.cleanupTimers(recording);
        this.setState("complete");
        this.cues.play("stop");
        this.write(recording, "cue stop");
        return;
      }
    } catch (error) {
      if (!this.isCurrent(recording)) return;
      this.write(recording, `poll ERR ${errorMessage(error)}`);
    }
    if (this.isCurrent(recording)) this.schedulePoll(recording);
  }

  private showChunks(recording: ActiveRecording, chunks: Chunk[]): void {
    chunks.forEach((chunk, index) => {
      const shown = recording.shownChunks[index];
      if (shown && shown.id !== chunk.id) throw new Error(`Chunk order changed at position ${index + 1}.`);
      if (!shown) this.write(recording, `+c${index + 1}`);
    });
    if (JSON.stringify(chunks) === JSON.stringify(recording.shownChunks)) return;
    recording.shownChunks = chunks;
    this.transcript.replaceChildren(...chunks.map((chunk) => {
      const item = document.createElement("li");
      item.dataset.testid = "chunk";
      item.dataset.chunkId = chunk.id;
      item.dataset.startMs = String(chunk.startMs);
      item.dataset.endMs = String(chunk.endMs);
      item.textContent = chunk.text;
      return item;
    }));
  }

  private isCurrent(recording: ActiveRecording): boolean {
    return this.active === recording && !recording.dead;
  }

  private cleanupTimers(recording: ActiveRecording): void {
    if (recording.pollTimer !== null) window.clearTimeout(recording.pollTimer);
    if (recording.progressTimer !== null) window.clearInterval(recording.progressTimer);
    if (recording.limitTimer !== null) window.clearTimeout(recording.limitTimer);
  }

  private setState(next: RecordingState): void {
    if (this.state === next) return;
    this.state = transition(this.state, next);
    if (this.active) this.write(this.active, next);
    this.render();
  }

  private render(): void {
    if (this.dataset.state !== this.state) this.dataset.state = this.state;
    this.stateLabel.textContent = STATE_LABELS[this.state];
    const recording = this.active;
    const enabled = enabledControls(this.state);
    const canRestart = !recording || this.state === "complete"
      || (!recording.stopReason && !recording.capturing && !recording.delivery.pendingCount);
    enabled.start &&= canRestart;
    enabled.language &&= canRestart;
    for (const name of Object.keys(this.controls) as (keyof Controls)[]) {
      this.controls[name].disabled = !enabled[name];
    }
    this.controls.start.textContent = this.state === "complete" ? "Start new recording" : "Start";
    const currentMs = recording?.capturing
      ? Math.max(0, recording.elapsedMs - recording.lastFrameEndMs) : 0;
    const pendingMs = (recording?.delivery.pendingMs ?? 0) + currentMs;
    this.element("elapsed").textContent = formatTime(recording?.elapsedMs ?? 0);
    this.element("pending").textContent = `${formatSeconds(pendingMs)} of audio pending`;
    this.element("secured").textContent = `Audio secured: ${formatTime(recording?.securedMs ?? 0)}`;
    this.retryButton.hidden = !recording || !(
      (recording.delivery.pendingCount && (recording.delivery.delayed || recording.stopReason))
      || (recording.finalized && this.state === "error")
    );
    this.retryButton.disabled = !recording || (!recording.delivery.pendingCount && !recording.finalized);
    this.retryButton.textContent = recording?.delivery.pendingCount
      ? `Upload pending audio (${formatSeconds(pendingMs)})` : "Try again";
    this.feedback.textContent = recording?.problem || (recording?.delivery.delayed
      ? recording.capturing ? "Audio upload delayed. Recording continues." : "Audio upload delayed. Recording has stopped."
      : recording?.stopReason === "duration" ? "Recording stopped at the time limit."
      : "");
    this.feedback.dataset.kind = this.state === "error" ? "error" : "warning";
  }

  private resetLog(): void {
    this.log = new RadarLog(this.config.logColumns, this.config.logRows, this.config.logGapChars);
    this.logView.style.setProperty("--log-columns", String(this.config.logColumns));
    this.renderLog();
  }

  private write(recording: ActiveRecording, entry: string): void {
    if (!this.isCurrent(recording)) return;
    const seconds = recording.startedAt ? ((performance.now() - recording.startedAt) / 1000).toFixed(1) : "0.0";
    this.log.write(`${seconds} ${entry}`);
    this.renderLog();
  }

  private renderLog(): void {
    const { lines, cursor } = this.log.view();
    const text = lines.join("\n");
    const mark = document.createElement("span");
    mark.className = "cursor";
    mark.textContent = text.charAt(cursor);
    this.logView.replaceChildren(text.slice(0, cursor), mark, text.slice(cursor + 1));
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
function formatSeconds(ms: number): string {
  const seconds = Math.ceil(ms / 1000);
  return `${seconds} ${seconds === 1 ? "second" : "seconds"}`;
}
function formatTime(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}
customElements.define("seed-app", SeedApp);
