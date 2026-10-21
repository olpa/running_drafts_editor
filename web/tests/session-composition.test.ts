import assert from "node:assert/strict";
import test from "node:test";
import { Window, type HTMLButtonElement as HappyButtonElement,
  type HTMLElement as HappyHTMLElement } from "happy-dom";
import type { CaptureOptions, RecordingCapture, TransportFrame } from "../src/capture.ts";
import type { AudioAssetSummary, FrameAck, ProjectSummary } from "../src/api.ts";
import type { RecordingBackend, RecordingDependencies } from "../src/recording.ts";
import type { AccountId, CurrentSession } from "../src/session.ts";

const browser = new Window({ url: "https://editor.test/projects/one?tab=record" });
Object.assign(globalThis, {
  window: browser, document: browser.document, location: browser.location, history: browser.history,
  HTMLElement: browser.HTMLElement, HTMLButtonElement: browser.HTMLButtonElement,
  customElements: browser.customElements, CustomEvent: browser.CustomEvent, Event: browser.Event,
});

await import("../src/components/seed-app.ts");

const ACCOUNT_A = "5728cdd2-909d-4207-8303-c04b270c308f" as AccountId;
const ACCOUNT_B = "37a919e8-4091-4c7f-809c-e1c856f3de2c" as AccountId;

interface ConfiguredSeedApp extends HappyHTMLElement {
  setRecordingDependencies(dependencies: RecordingDependencies): void;
  refreshSession(): Promise<void>;
}

class FakeCapture implements RecordingCapture {
  private onFrame: ((frame: TransportFrame) => void) | null = null;
  disposed = false;
  constructor(readonly options: CaptureOptions) {}
  start(onFrame: (frame: TransportFrame) => void): void { this.onFrame = onFrame; }
  async stop(): Promise<void> {}
  dispose(): void { this.disposed = true; }
  emit(frame: TransportFrame): void { this.onFrame?.(frame); }
}

class FakeRecordingBackend implements RecordingBackend {
  readonly recordingId = "recording-owned-by-account-a";
  readonly requests: { recordingId: string; frame: TransportFrame; signal: AbortSignal;
    result: ReturnType<typeof deferred<FrameAck>> }[] = [];
  async createProject(): Promise<ProjectSummary> {
    return { id: "project-a", title: "Canonical", createdAt: "now", updatedAt: "now" };
  }
  async getProject(projectId: string): Promise<ProjectSummary> {
    return { id: projectId, title: "Existing canonical title", createdAt: "now", updatedAt: "now" };
  }
  async createSection(): Promise<{ sectionId: string; audioAssetId: string; position: number }> {
    return { sectionId: "section-a", audioAssetId: this.recordingId, position: 0 };
  }
  putFrame(recordingId: string, frame: TransportFrame, signal: AbortSignal): Promise<FrameAck> {
    const result = deferred<FrameAck>();
    this.requests.push({ recordingId, frame, signal, result });
    return result.promise;
  }
  async complete(): Promise<AudioAssetSummary> { return asset(this.recordingId, "completed"); }
  async interrupt(): Promise<AudioAssetSummary> { return asset(this.recordingId, "interrupted"); }
}

function asset(id: string, sealReason: "completed" | "interrupted"): AudioAssetSummary {
  return { id, mediaType: "audio/webm;codecs=opus", isSealed: true, sealReason, sealedAt: "now",
    frameCount: 0, observedDurationMs: 0, storedBytes: 0 };
}

function session(accountId: AccountId, remainingMs: number): CurrentSession {
  return {
    account: { id: accountId }, provider: { id: "google", displayName: "Google" },
    profile: { email: null, emailVerified: null, displayName: null, givenName: null,
      familyName: null, pictureUrl: null, locale: null },
    sessionExpiresAt: new Date(Date.now() + remainingMs).toISOString(),
  };
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
    .find((candidate) => candidate.textContent === label) ?? null;
}

function installRequest(request: (input: string | URL | Request) => Promise<Response>): void {
  globalThis.fetch = request as typeof fetch;
  browser.fetch = request as unknown as typeof browser.fetch;
}

test("real queued audio survives failed short-session logout and resumes only for its Account", async () => {
  browser.document.body.replaceChildren();
  browser.history.replaceState(null, "", "/projects/one?tab=record");
  let currentSession = session(ACCOUNT_A, 8 * 60 * 60 * 1000);
  let logoutAttempts = 0;
  const firstLogout = deferred<Response>();
  installRequest(async (input) => {
    const path = new URL(String(input)).pathname;
    if (path === "/api/v1/auth/providers") return Response.json({ providers: [{ id: "google", displayName: "Google" }] });
    if (path === "/api/v1/me") return Response.json(currentSession);
    if (path === "/auth/logout") {
      logoutAttempts++;
      return logoutAttempts === 1 ? firstLogout.promise : new Response(null, { status: 204 });
    }
    throw new Error(`Unexpected request: ${path}`);
  });

  const backend = new FakeRecordingBackend();
  const captures: FakeCapture[] = [];
  const app = browser.document.createElement("seed-app") as ConfiguredSeedApp;
  app.setRecordingDependencies({ backend, now: () => performance.now(), openCapture: async (options) => {
    const capture = new FakeCapture(options);
    captures.push(capture);
    return capture;
  }});
  browser.document.body.append(app);
  await settle();
  const panel = app.querySelector("recording-panel")!;
  const initialExpiry = currentSession.sessionExpiresAt;
  const initialTime = app.querySelector("time");
  assert.equal(initialTime?.getAttribute("datetime"), initialExpiry);
  assert.ok(initialTime?.textContent);

  app.querySelector<HappyButtonElement>('[data-testid="start"]')!.click();
  await settle();
  const capture = captures[0];
  assert.ok(capture);
  const exactBytes = new Blob([new Uint8Array([0, 17, 42, 128, 255])], { type: "audio/webm;codecs=opus" });
  const queuedFrame: TransportFrame = { seq: 0, startMs: 0, endMs: 10_000,
    mediaType: exactBytes.type, data: exactBytes };
  capture.emit(queuedFrame);
  assert.equal(backend.requests.length, 1);
  assert.equal(backend.requests[0]!.recordingId, backend.recordingId);
  assert.equal(backend.requests[0]!.frame.data, exactBytes);

  const privateItem = browser.document.createElement("li");
  privateItem.textContent = "Account A private transcript";
  app.querySelector('[data-testid="transcript"]')!.append(privateItem);
  currentSession = session(ACCOUNT_A, 60 * 60 * 1000);
  await app.refreshSession();
  assert.equal(app.querySelector("time")?.getAttribute("datetime"), currentSession.sessionExpiresAt);
  panel.dispatchEvent(new browser.CustomEvent("recording-start-request", { bubbles: true, cancelable: true }));

  assert.equal(button(app, "Continue with Google"), null, "provider navigation must wait for logout");
  assert.equal(app.querySelector("time"), null, "logout removes private session expiry");
  assert.equal(app.querySelector("recording-panel"), panel);
  assert.equal(panel.isConnected, true);
  assert.equal(capture.disposed, false);
  assert.equal(backend.requests.length, 1, "logout freezes new delivery attempts");
  assert.equal(app.textContent?.includes("Account A private transcript"), false);
  const leave = new browser.Event("beforeunload", { cancelable: true });
  browser.dispatchEvent(leave);
  assert.equal(leave.defaultPrevented, true);

  firstLogout.reject(new TypeError("offline"));
  await settle();
  assert.ok(button(app, "Retry logout"));
  assert.equal(button(app, "Continue with Google"), null);
  assert.equal(backend.requests.length, 1);
  button(app, "Retry logout")!.click();
  await settle();
  assert.ok(button(app, "Continue with Google"));

  currentSession = session(ACCOUNT_A, 8 * 60 * 60 * 1000);
  await app.refreshSession();
  assert.equal(app.querySelector("time")?.getAttribute("datetime"), currentSession.sessionExpiresAt);
  assert.equal(backend.requests.length, 2, "the original Account resumes delivery");
  assert.equal(backend.requests[1]!.recordingId, backend.recordingId);
  assert.equal(backend.requests[1]!.frame, queuedFrame);
  assert.equal(backend.requests[1]!.frame.data, exactBytes);
  assert.deepEqual(new Uint8Array(await backend.requests[1]!.frame.data.arrayBuffer()),
    new Uint8Array([0, 17, 42, 128, 255]));

  panel.dispatchEvent(new browser.CustomEvent("authentication-expired", { bubbles: true }));
  assert.equal(app.querySelector("time"), null, "authentication expiry removes session expiry");
  currentSession = session(ACCOUNT_B, 8 * 60 * 60 * 1000);
  await app.refreshSession();
  assert.equal(backend.requests.length, 2, "a different Account sends none of the queued bytes");
  assert.match(app.textContent ?? "", /different Account/u);
});

test("session expiry updates after bootstrap and disappears on bootstrap failure", async () => {
  browser.document.body.replaceChildren();
  browser.history.replaceState(null, "", "/projects/expiry");
  let currentSession = session(ACCOUNT_A, 8 * 60 * 60 * 1000);
  let failCurrentSession = false;
  installRequest(async (input) => {
    const path = new URL(String(input)).pathname;
    if (path === "/api/v1/auth/providers") return Response.json({ providers: [{ id: "google", displayName: "Google" }] });
    if (path === "/api/v1/me") {
      return failCurrentSession
        ? Response.json({ title: "Unavailable", code: "unavailable" }, { status: 503 })
        : Response.json(currentSession);
    }
    throw new Error(`Unexpected request: ${path}`);
  });
  const app = browser.document.createElement("seed-app") as ConfiguredSeedApp;
  browser.document.body.append(app);
  await settle();
  const firstExpiry = currentSession.sessionExpiresAt;
  assert.equal(app.querySelector("time")?.getAttribute("datetime"), firstExpiry);

  currentSession = session(ACCOUNT_A, 10 * 60 * 60 * 1000);
  await app.refreshSession();
  assert.notEqual(currentSession.sessionExpiresAt, firstExpiry);
  assert.equal(app.querySelector("time")?.getAttribute("datetime"), currentSession.sessionExpiresAt);

  app.querySelector("recording-panel")!.dispatchEvent(
    new browser.CustomEvent("authentication-expired", { bubbles: true }),
  );
  assert.equal(app.querySelector("time"), null);
  await app.refreshSession();
  assert.equal(app.querySelector("time")?.getAttribute("datetime"), currentSession.sessionExpiresAt);

  failCurrentSession = true;
  await app.refreshSession();
  assert.equal(app.querySelector("time"), null, "bootstrap failure removes session expiry");
});

test("ambiguous callback fields become generic and every reserved value is removed", async () => {
  browser.document.body.replaceChildren();
  browser.history.replaceState(null, "",
    "/login?keep=1&keep=2&authError=cancelled&authError=provider_unavailable&referenceId=attacker&referenceId=other");
  installRequest(async (input) => {
    const path = new URL(String(input)).pathname;
    if (path === "/api/v1/auth/providers") return Response.json({ providers: [{ id: "google", displayName: "Google" }] });
    if (path === "/api/v1/me") return Response.json({ code: "authentication_required" }, { status: 401 });
    throw new Error(`Unexpected request: ${path}`);
  });
  const app = browser.document.createElement("seed-app") as ConfiguredSeedApp;
  browser.document.body.append(app);
  await settle();

  assert.match(app.textContent ?? "", /Sign-in could not be verified/u);
  assert.doesNotMatch(app.textContent ?? "", /cancelled|attacker|Reference:/u);
  assert.equal(browser.location.pathname + browser.location.search, "/login?keep=1&keep=2");
});

test("callback failure shows one validated reference and removes callback parameters", async () => {
  browser.document.body.replaceChildren();
  browser.history.replaceState(null, "", "/login?keep=1&authError=authentication_failed&referenceId=safe_REF-42");
  installRequest(async (input) => {
    const path = new URL(String(input)).pathname;
    if (path === "/api/v1/auth/providers") return Response.json({ providers: [{ id: "google", displayName: "Google" }] });
    if (path === "/api/v1/me") return Response.json({ code: "authentication_required" }, { status: 401 });
    throw new Error(`Unexpected request: ${path}`);
  });
  const app = browser.document.createElement("seed-app") as ConfiguredSeedApp;
  browser.document.body.append(app);
  await settle();

  assert.match(app.textContent ?? "", /Reference: safe_REF-42/u);
  assert.equal(browser.location.pathname + browser.location.search, "/login?keep=1");
  assert.ok(button(app, "Continue with Google"));
});
