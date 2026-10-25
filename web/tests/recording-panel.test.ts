import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { Window, type HTMLButtonElement as HappyButtonElement,
  type HTMLElement as HappyHTMLElement } from "happy-dom";
import { ApiError, type AudioAssetSummary, type FrameAck, type ProjectSummary } from "../src/api.ts";
import type { CaptureOptions, MicrophoneCapture, TransportFrame } from "../src/capture.ts";
import type { RecordingBackend, RecordingDependencies } from "../src/recording.ts";
import type { AccountId } from "../src/session.ts";

const browser = new Window({ url: "https://editor.test/projects/existing" });
Object.assign(globalThis, {
  window: browser, document: browser.document, location: browser.location, history: browser.history,
  HTMLElement: browser.HTMLElement, HTMLButtonElement: browser.HTMLButtonElement,
  customElements: browser.customElements, CustomEvent: browser.CustomEvent, Event: browser.Event,
});

await import("../src/components/recording-panel.ts");

const ACCOUNT_ID = "5728cdd2-909d-4207-8303-c04b270c308f" as AccountId;

interface ConfigurablePanel extends HappyHTMLElement {
  setDependencies(dependencies: RecordingDependencies): void;
  setProjectId(projectId: string): void;
  setSessionAccount(accountId: AccountId): void;
  finishModule(): void;
}

class FakeCapture implements MicrophoneCapture {
  private onFrame: ((frame: TransportFrame) => void) | null = null;
  stops = 0;
  disposed = false;

  constructor(readonly options: CaptureOptions) {}

  async start(onFrame: (frame: TransportFrame) => void): Promise<void> { this.onFrame = onFrame; }
  async stop(): Promise<void> { this.stops++; }
  dispose(): void { this.disposed = true; }
  emit(frame: TransportFrame): void { this.onFrame?.(frame); }
}

class FakeBackend implements RecordingBackend {
  readonly requests: { recordingId: string; frame: TransportFrame;
    result: ReturnType<typeof deferred<FrameAck>> }[] = [];
  readonly sectionProjects: string[] = [];
  interrupts = 0;
  completes = 0;
  completion: ReturnType<typeof deferred<AudioAssetSummary>> | null = null;
  private createdSections = 0;

  async createProject(): Promise<ProjectSummary> {
    throw new Error("An existing Project must not be replaced.");
  }

  async getProject(projectId: string): Promise<ProjectSummary> {
    return { id: projectId, title: "Canonical field notes", createdAt: "now", updatedAt: "now" };
  }

  async createSection(projectId: string): Promise<{
    sectionId: string; audioAssetId: string; position: number;
  }> {
    this.sectionProjects.push(projectId);
    const suffix = ["one", "two"][this.createdSections] ?? String(this.createdSections + 1);
    return { sectionId: `section-${suffix}`, audioAssetId: `asset-${suffix}`,
      position: this.createdSections++ };
  }

  putFrame(recordingId: string, frame: TransportFrame): Promise<FrameAck> {
    const result = deferred<FrameAck>();
    this.requests.push({ recordingId, frame, result });
    return result.promise;
  }

  async complete(): Promise<AudioAssetSummary> {
    this.completes++;
    return this.completion ? this.completion.promise : asset("completed");
  }
  async interrupt(): Promise<AudioAssetSummary> { this.interrupts++; return asset("interrupted"); }
}

function asset(sealReason: "completed" | "interrupted"): AudioAssetSummary {
  return { id: "asset-one", mediaType: "audio/webm;codecs=opus", isSealed: true, sealReason,
    sealedAt: "now", frameCount: 1, observedDurationMs: 10_000, storedBytes: 5 };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

async function settle(): Promise<void> {
  for (let index = 0; index < 20; index++) await Promise.resolve();
}

function button(root: HappyHTMLElement, label: string): HappyButtonElement | null {
  return [...root.querySelectorAll<HappyButtonElement>("button")]
    .find((candidate) => candidate.textContent === label && !candidate.hidden) ?? null;
}

test("view remount preserves its canonical Project and exact pending frame for recovery", async (t) => {
  browser.document.body.replaceChildren();
  const backend = new FakeBackend();
  const captures: FakeCapture[] = [];
  const panel = browser.document.createElement("recording-panel") as ConfigurablePanel;
  panel.setProjectId("existing-project");
  panel.setDependencies({ backend, now: () => performance.now(), openCapture: async (options) => {
    const capture = new FakeCapture(options);
    captures.push(capture);
    return capture;
  }});
  browser.document.body.append(panel);
  panel.setSessionAccount(ACCOUNT_ID);
  t.after(() => { panel.finishModule(); panel.remove(); });
  await settle();

  assert.equal(panel.querySelector('[data-testid="project-title"]')?.textContent,
    "Canonical field notes");
  button(panel, "Start")!.click();
  await settle();
  assert.deepEqual(backend.sectionProjects, ["existing-project"]);
  const primary = button(panel, "Stop current recording to start new")!;
  assert.equal(primary.dataset.mode, "stop");
  assert.equal(primary.disabled, false);

  const bytes = new Blob([new Uint8Array([0, 17, 42, 128, 255])],
    { type: "audio/webm;codecs=opus" });
  const frame: TransportFrame = {
    seq: 0, startMs: 0, endMs: 10_000, mediaType: bytes.type, data: bytes,
  };
  captures[0]!.emit(frame);
  assert.equal(backend.requests.length, 1);
  backend.requests[0]!.result.reject(new Error("offline"));
  await settle();

  panel.remove();
  await settle();
  assert.equal(captures[0]!.stops, 1, "disconnect releases the active Capture");
  browser.document.body.append(panel);
  await settle();

  assert.equal(panel.querySelector('[data-testid="project-title"]')?.textContent,
    "Canonical field notes");
  const startNew = button(panel, "Start new recording")!;
  assert.equal(startNew.dataset.mode, "start-new");
  assert.equal(startNew.disabled, false);
  button(panel, "Upload pending audio")!.click();
  assert.equal(backend.requests.length, 2);
  assert.equal(backend.requests[1]!.recordingId, "asset-one");
  assert.equal(backend.requests[1]!.frame, frame);
  assert.equal(backend.requests[1]!.frame.data, bytes);
  backend.requests[1]!.result.resolve({
    sequence: 0, acknowledged: true, nextExpectedSequence: 1, sha256: "frame-digest",
  });
  await settle();
  assert.equal(backend.interrupts, 1);
});

test("primary control defines animated and reduced-motion versions of the same state change", () => {
  const css = readFileSync(new URL("../src/style.css", import.meta.url), "utf8");
  assert.match(css, /\.primary-control\s*\{[^}]*transition:/su);
  assert.match(css, /@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{[\s\S]*?\.primary-control\s*\{\s*transition:\s*none;/u);
});

test("Section components expose completion and retain their DOM identity", async (t) => {
  browser.document.body.replaceChildren();
  const backend = new FakeBackend();
  backend.completion = deferred<AudioAssetSummary>();
  const captures: FakeCapture[] = [];
  const panel = browser.document.createElement("recording-panel") as ConfigurablePanel;
  panel.setProjectId("existing-project");
  panel.setDependencies({ backend, now: () => performance.now(), openCapture: async (options) => {
    const capture = new FakeCapture(options);
    captures.push(capture);
    return capture;
  }});
  browser.document.body.append(panel);
  panel.setSessionAccount(ACCOUNT_ID);
  t.after(() => { panel.finishModule(); panel.remove(); });
  await settle();

  button(panel, "Start")!.click();
  await settle();
  button(panel, "Stop current recording to start new")!.click();
  await settle();

  const section = panel.querySelector<HappyHTMLElement>('[data-testid="section"]')!;
  const completionStatus = section.querySelector('[role="status"]');
  assert.equal(section.localName, "section");
  assert.equal(section.querySelector("h3")?.textContent, "Section 1");
  assert.equal(section.dataset.completion, "pending");
  assert.match(section.textContent!, /Completing recording…/u);
  assert.equal(section.hasAttribute("data-section-id"), false);

  button(panel, "Start new recording")!.click();
  await settle();
  assert.equal(panel.querySelector('[data-testid="section"]'), section,
    "another Section does not replace an existing component");

  backend.completion.resolve(asset("completed"));
  await settle();
  assert.equal(panel.querySelector('[data-testid="section"]'), section);
  assert.equal(section.querySelector('[role="status"]'), completionStatus);
  assert.equal(section.dataset.completion, "succeeded");
  assert.match(section.textContent!, /Recording complete/u);
});

test("frame conflict shows details without recovery and leaves a new recording startable", async (t) => {
  browser.document.body.replaceChildren();
  const backend = new FakeBackend();
  const captures: FakeCapture[] = [];
  const logs: string[] = [];
  const panel = browser.document.createElement("recording-panel") as ConfigurablePanel;
  panel.setProjectId("existing-project");
  panel.setDependencies({ backend, now: () => performance.now(), openCapture: async (options) => {
    const capture = new FakeCapture(options);
    captures.push(capture);
    return capture;
  }});
  panel.addEventListener("recording-event", (event) => {
    const detail = (event as unknown as CustomEvent<{ type: string; message?: string }>).detail;
    if (detail.type === "log" && detail.message) logs.push(detail.message);
  });
  browser.document.body.append(panel);
  panel.setSessionAccount(ACCOUNT_ID);
  t.after(() => { panel.finishModule(); panel.remove(); });
  await settle();

  button(panel, "Start")!.click();
  await settle();
  captures[0]!.emit({ seq: 0, startMs: 0, endMs: 1_000, mediaType: "audio/webm;codecs=opus",
    data: new Blob([new Uint8Array([9, 8, 7])]) });
  backend.requests[0]!.result.reject(new ApiError("PUT", "/frames/0", 409, {
    title: "Stored frame conflict", detail: "The stored bytes differ.", code: "unexpected_conflict",
    referenceId: "ui_conflict_7",
  }));
  await settle();

  assert.equal(captures[0]!.stops, 1);
  assert.equal(button(panel, "Upload pending audio"), null);
  assert.equal(backend.completes, 0);
  assert.equal(backend.interrupts, 0);
  assert.match(panel.querySelector('[data-testid="section"]')!.textContent!,
    /Capture stopped because audio delivery conflicted with stored data/u);
  assert.match(panel.textContent!, /Title: Stored frame conflict/u);
  assert.match(panel.textContent!, /Detail: The stored bytes differ\./u);
  assert.match(panel.textContent!, /Code: unexpected_conflict/u);
  assert.match(panel.textContent!, /Reference ID: ui_conflict_7/u);
  assert.ok(logs.some((message) => message.includes(
    "HTTP 409 code=unexpected_conflict referenceId=ui_conflict_7")));

  button(panel, "Start new recording")!.click();
  await settle();
  assert.equal(captures[1]!.disposed, false);
  assert.equal(backend.sectionProjects.length, 2);
});
