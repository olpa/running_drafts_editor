import template from "./seed-app.html?raw";
import "./recording-panel.js";
import { loadConfig } from "../config.js";
import { RadarLog } from "../radar-log.js";
import type { Chunk } from "../api.js";
import type { RecordingEvent } from "../recording.js";

/** Composes recording controls, transcript display, and the diagnostic log. */
export class SeedApp extends HTMLElement {
  private readonly config = loadConfig();
  private log!: RadarLog;
  private transcript!: HTMLOListElement;
  private logView!: HTMLPreElement;

  connectedCallback(): void {
    if (this.transcript) return;
    this.innerHTML = template;
    this.transcript = this.element("transcript");
    this.logView = this.element("log");
    this.resetLog();
    this.querySelector("recording-panel")!.addEventListener("recording-event", (event) => {
      const detail = (event as CustomEvent<RecordingEvent>).detail;
      if (detail.type === "reset") {
        this.transcript.replaceChildren();
        this.resetLog();
      } else if (detail.type === "chunks") {
        this.showChunks(detail.chunks);
      } else if (detail.type === "log") {
        this.log.write(detail.message);
        this.renderLog();
      }
    });
  }

  private showChunks(chunks: readonly Chunk[]): void {
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

  private element<T extends HTMLElement>(testId: string): T {
    const element = this.querySelector<T>(`[data-testid="${testId}"]`);
    if (!element) throw new Error(`Missing seed-app element: ${testId}`);
    return element;
  }

  private resetLog(): void {
    this.log = new RadarLog(this.config.logColumns, this.config.logRows, this.config.logGapChars);
    this.logView.style.setProperty("--log-columns", String(this.config.logColumns));
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
customElements.define("seed-app", SeedApp);
