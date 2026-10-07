import template from "./seed-app.html?raw";
import { ApiClient, type Chunk, type Language, type RecordingStatus } from "../api.js";
import { MockFrameSource, openMicrophone, type FrameSource, type TransportFrame } from "../capture.js";
import { loadConfig, type SeedConfig } from "../config.js";
import { CuePlayer } from "../cues.js";
import { RadarLog } from "../radar-log.js";
import { enabledControls, transition, type Controls, type RecordingState } from "../state.js";

const STATE_LABELS: Record<RecordingState, string> = {
  idle: "Ready",
  recording: "Recording",
  finishing: "Finishing",
  complete: "Complete",
  error: "Error",
};

/** Everything that belongs to one Start-to-complete run. */
class ActiveRecording {
  recordingId = "";
  startedAt = performance.now();
  stream: MediaStream | null = null;
  frameSource: FrameSource | null = null;
  /** Uploads run one after another so frames arrive in sequence order. */
  uploads: Promise<void> = Promise.resolve();
  pollTimer: number | null = null;
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

  connectedCallback(): void {
    if (this.controls) return;
    this.innerHTML = template;
    this.controls = {
      language: this.element("language"),
      start: this.element("start"),
      pause: this.element("pause"),
      continue: this.element("continue"),
      stop: this.element("stop"),
      cancel: this.element("cancel"),
    };
    this.stateLabel = this.element("state-label");
    this.transcript = this.element("transcript");
    this.logView = this.element("log");
    this.controls.start.addEventListener("click", () => void this.start());
    this.controls.stop.addEventListener("click", () => void this.stop());
    for (const name of ["pause", "continue", "cancel"] as const) {
      this.controls[name].addEventListener("click", () => alert("not implemented"));
    }
    this.resetLog();
    this.render();
  }

  private element<T extends HTMLElement>(testId: string): T {
    const element = this.querySelector<T>(`[data-testid="${testId}"]`);
    if (!element) throw new Error(`Missing seed-app element: ${testId}`);
    return element;
  }

  private async start(): Promise<void> {
    this.cues.prepare();
    const recording = new ActiveRecording();
    this.active = recording;
    this.transcript.replaceChildren();
    this.resetLog();
    this.setState("recording");
    try {
      recording.stream = await openMicrophone();
      this.write(recording, "mic ok");
      const language = this.controls.language.value as Language;
      ({ recordingId: recording.recordingId } = await this.api.createRecording(language));
      this.write(recording, `rec ${recording.recordingId.slice(0, 8)} ${language}`);
      this.cues.play("start");
      this.write(recording, "cue start");
      recording.frameSource = new MockFrameSource(this.config.frameIntervalMs);
      recording.frameSource.start((frame) => this.enqueueUpload(recording, frame));
      this.schedulePoll(recording);
    } catch (error) {
      this.fail(recording, error);
    }
  }

  private async stop(): Promise<void> {
    const recording = this.active;
    if (!recording?.frameSource) return;
    this.setState("finishing");
    recording.frameSource.stop();
    try {
      await recording.uploads;
      await this.api.finish(recording.recordingId);
      this.write(recording, "fin sent");
    } catch (error) {
      this.fail(recording, error);
    }
  }

  private enqueueUpload(recording: ActiveRecording, frame: TransportFrame): void {
    recording.uploads = recording.uploads.then(async () => {
      this.write(recording, `f${frame.seq} sent ${formatBytes(frame.data.size)}`);
      const sentAt = performance.now();
      const ack = await this.api.putFrame(recording.recordingId, frame);
      if (ack.seq !== frame.seq || !ack.acknowledged) {
        throw new Error(`Unexpected acknowledgement for frame ${frame.seq}: ${JSON.stringify(ack)}`);
      }
      this.write(recording, `f${frame.seq} ack ${Math.round(performance.now() - sentAt)}ms`);
    });
    recording.uploads.catch((error: unknown) => this.fail(recording, error));
  }

  private schedulePoll(recording: ActiveRecording): void {
    recording.pollTimer = window.setTimeout(() => void this.poll(recording), this.config.pollIntervalMs);
  }

  private async poll(recording: ActiveRecording): Promise<void> {
    try {
      const status = await this.api.getRecording(recording.recordingId);
      if (this.active !== recording) return;
      this.showChunks(recording, status.chunks);
      if (status.status !== recording.serverStatus) {
        recording.serverStatus = status.status;
        this.write(recording, `srv ${status.status}`);
      }
      if (status.status === "complete") {
        this.complete(recording);
      } else {
        this.schedulePoll(recording);
      }
    } catch (error) {
      this.fail(recording, error);
    }
  }

  private showChunks(recording: ActiveRecording, chunks: Chunk[]): void {
    chunks.forEach((chunk, index) => {
      const shown = recording.shownChunks[index];
      if (shown && shown.id !== chunk.id) {
        throw new Error(`Chunk order changed at position ${index + 1}: ${shown.id} -> ${chunk.id}`);
      }
      if (!shown) this.write(recording, `+c${index + 1}`);
    });
    if (JSON.stringify(chunks) === JSON.stringify(recording.shownChunks)) return;
    recording.shownChunks = chunks;
    this.transcript.replaceChildren(
      ...chunks.map((chunk) => {
        const item = document.createElement("li");
        item.dataset.testid = "chunk";
        item.dataset.chunkId = chunk.id;
        item.dataset.startMs = String(chunk.startMs);
        item.dataset.endMs = String(chunk.endMs);
        item.textContent = chunk.text;
        return item;
      }),
    );
  }

  private complete(recording: ActiveRecording): void {
    recording.stream?.getTracks().forEach((track) => track.stop());
    this.setState("complete");
    this.cues.play("stop");
    this.write(recording, "cue stop");
  }

  private fail(recording: ActiveRecording, error: unknown): void {
    const message = error instanceof Error ? error.message : String(error);
    this.write(recording, `ERR ${message}`);
    this.setState("error");
  }

  private setState(next: RecordingState): void {
    this.state = transition(this.state, next);
    if (this.active) this.write(this.active, next);
    this.render();
  }

  private render(): void {
    this.dataset.state = this.state;
    this.stateLabel.textContent = STATE_LABELS[this.state];
    const enabled = enabledControls(this.state);
    for (const name of Object.keys(this.controls) as (keyof Controls)[]) {
      this.controls[name].disabled = !enabled[name];
    }
    this.controls.start.textContent = this.state === "complete" ? "Start new recording" : "Start";
  }

  private resetLog(): void {
    this.log = new RadarLog(this.config.logColumns, this.config.logRows, this.config.logGapChars);
    this.logView.style.setProperty("--log-columns", String(this.config.logColumns));
    this.renderLog();
  }

  private write(recording: ActiveRecording, entry: string): void {
    if (this.active !== recording) return;
    const seconds = ((performance.now() - recording.startedAt) / 1000).toFixed(1);
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

function formatBytes(bytes: number): string {
  return bytes < 1024 ? `${bytes}B` : `${Math.round(bytes / 1024)}KB`;
}

customElements.define("seed-app", SeedApp);
