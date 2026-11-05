import { flushPromises } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import type { ProjectsGateway } from "../projects/gateway.ts";
import type { CaptureOptions, MicrophoneCapture, TransportFrame } from "../recording/capture.ts";
import type { FrameAck, RecordingGateway } from "../recording/gateway.ts";
import type { AccountId, CurrentSession, SessionGateway } from "../session/gateway.ts";
import { resolveProvider } from "../session/providers.ts";
import { DEFAULT_CONFIG } from "../shared/config.ts";
import { ApiError } from "../shared/http.ts";
import type { AudioAssetSummary, ProjectSummary } from "../shared/project-types.ts";
import { createApplication, type Application, type ApplicationAdapters } from "./application.ts";

const ACCOUNT_A = "5728cdd2-909d-4207-8303-c04b270c308f" as AccountId;
const ACCOUNT_B = "37a919e8-4091-4c7f-809c-e1c856f3de2c" as AccountId;
const HOUR = 60 * 60 * 1000;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function asset(id: string, sealReason: "completed" | "interrupted"): AudioAssetSummary {
  return {
    id,
    mediaType: "audio/webm;codecs=opus",
    isSealed: true,
    sealReason,
    sealedAt: "now",
    frameCount: 0,
    observedDurationMs: 0,
    storedBytes: 0,
  };
}

function session(accountId: AccountId, remainingMs: number): CurrentSession {
  return {
    account: { id: accountId },
    provider: resolveProvider("google"),
    profile: {
      email: null,
      emailVerified: null,
      displayName: null,
      givenName: null,
      familyName: null,
      pictureUrl: null,
      locale: null,
    },
    sessionExpiresAt: new Date(Date.now() + remainingMs).toISOString(),
  };
}

class FakeCapture implements MicrophoneCapture {
  private onFrame: ((frame: TransportFrame) => void) | null = null;
  disposed = false;
  constructor(readonly options: CaptureOptions) {}
  async start(onFrame: (frame: TransportFrame) => void): Promise<void> {
    this.onFrame = onFrame;
  }
  requestFrame(): void {}
  async stop(): Promise<void> {}
  dispose(): void {
    this.disposed = true;
  }
  beforeEmit: () => void = () => {};
  emit(frame: TransportFrame): void {
    this.beforeEmit();
    this.onFrame?.(frame);
  }
}

class FakeRecordingGateway implements RecordingGateway {
  readonly assetId = "recording-owned-by-account-a";
  readonly requests: {
    assetId: string;
    frame: TransportFrame;
    result: ReturnType<typeof deferred<FrameAck>>;
  }[] = [];
  async createProject(): Promise<ProjectSummary> {
    return { id: "project-a", title: "Canonical", createdAt: "now", updatedAt: "now" };
  }
  async getProject(projectId: string): Promise<ProjectSummary> {
    return { id: projectId, title: "Existing", createdAt: "now", updatedAt: "now" };
  }
  async createSection() {
    return { sectionId: "section-a", audioAssetId: this.assetId, position: 0 };
  }
  putFrame(assetId: string, frame: TransportFrame): Promise<FrameAck> {
    const result = deferred<FrameAck>();
    this.requests.push({ assetId, frame, result });
    return result.promise;
  }
  async complete(): Promise<AudioAssetSummary> {
    return asset(this.assetId, "completed");
  }
  async interrupt(): Promise<AudioAssetSummary> {
    return asset(this.assetId, "interrupted");
  }
}

/** A session backend whose answers the test changes while the application runs. */
function sessionBackend() {
  const backend = {
    current: session(ACCOUNT_A, 8 * HOUR) as CurrentSession | "signedOut" | "failure",
    logouts: [] as Promise<void>[],
    logoutAttempts: 0,
  };
  const gateway: SessionGateway = {
    async current() {
      if (backend.current === "signedOut") {
        throw new ApiError("GET", "/api/v1/me", 401, { code: "authentication_required" });
      }
      if (backend.current === "failure") throw new ApiError("GET", "/api/v1/me", 503);
      return backend.current;
    },
    logout: () => backend.logouts[backend.logoutAttempts++] ?? Promise.resolve(),
    loginUrl: (key) => `https://api.example.test/auth/login/${key}`,
  };
  return { backend, gateway };
}

const mounted: Application[] = [];
const roots: HTMLElement[] = [];

function mountApplication(adapters: ApplicationAdapters = {}) {
  const captures: FakeCapture[] = [];
  let frameTick: (() => void) | null = null;
  const recordingGateway = new FakeRecordingGateway();
  const { backend, gateway } = sessionBackend();
  const application = createApplication({
    sessionGateway: gateway,
    recordingGateway,
    projectsGateway: { listProjects: async () => [] } as unknown as ProjectsGateway,
    healthGateway: { check: async () => Promise.reject(new Error("unused")) },
    cues: { prepare() {}, play() {}, loopProblem: () => () => {}, dispose() {} },
    environment: {
      scheduler: {
        now: () => performance.now(),
        schedule(delay, callback) {
          if (delay === DEFAULT_CONFIG.frameIntervalMs) {
            frameTick = callback;
            return () => {
              if (frameTick === callback) frameTick = null;
            };
          }
          const timer = setTimeout(callback, delay);
          return () => clearTimeout(timer);
        },
      },
      now: () => performance.now(),
      openCapture: async (options) => {
        const capture = new FakeCapture(options);
        capture.beforeEmit = () => {
          const callback = frameTick;
          frameTick = null;
          callback?.();
        };
        captures.push(capture);
        return capture;
      },
    },
    ...adapters,
  });
  const root = document.createElement("div");
  document.body.append(root);
  application.app.mount(root);
  mounted.push(application);
  roots.push(root);
  return { application, root, captures, recordingGateway, backend };
}

afterEach(() => {
  for (const application of mounted.splice(0)) application.app.unmount();
  for (const root of roots.splice(0)) root.remove();
  window.history.replaceState(null, "", "/");
});

function button(root: HTMLElement, label: string): HTMLButtonElement | null {
  return (
    [...root.querySelectorAll("button")].find(
      (candidate) => candidate.textContent?.trim() === label,
    ) ?? null
  );
}

function testId(root: HTMLElement, id: string): HTMLElement | null {
  return root.querySelector<HTMLElement>(`[data-testid="${id}"]`);
}

describe("application", () => {
  it("retains real queued audio across a failed short-session logout and resumes only for its Account", async () => {
    window.history.replaceState(null, "", "/projects/one?tab=record");
    const { application, root, captures, recordingGateway, backend } = mountApplication();
    const firstLogout = deferred<void>();
    backend.logouts.push(firstLogout.promise);
    await flushPromises();
    expect(root.querySelector("time")?.getAttribute("datetime")).toBe(
      (backend.current as CurrentSession).sessionExpiresAt,
    );

    testId(root, "start")!.click();
    await flushPromises();
    const capture = captures[0]!;
    const exactBytes = new Blob([new Uint8Array([0, 17, 42, 128, 255])], {
      type: "audio/webm;codecs=opus",
    });
    const queuedFrame: TransportFrame = {
      seq: 0,
      startMs: 0,
      endMs: 10_000,
      mediaType: exactBytes.type,
      data: exactBytes,
    };
    capture.emit(queuedFrame);
    expect(recordingGateway.requests).toHaveLength(1);
    expect(recordingGateway.requests[0]!.assetId).toBe(recordingGateway.assetId);
    expect(recordingGateway.requests[0]!.frame.data).toBe(exactBytes);
    await flushPromises();
    expect(testId(root, "log")!.textContent).toContain("f0 sent");

    // The Session now ends within six hours, so the next start is refused and logs out.
    backend.current = session(ACCOUNT_A, HOUR);
    await application.session.bootstrap();
    await flushPromises();
    expect(application.session.authorizeRecordingStart()).toBe(false);
    await flushPromises();

    expect(button(root, "Continue with Google")).toBeNull();
    expect(root.querySelector("time")).toBeNull();
    expect(testId(root, "recording-workspace")).not.toBeNull();
    expect(testId(root, "private-results")).toBeNull();
    expect(capture.disposed).toBe(false);
    expect(recordingGateway.requests).toHaveLength(1);
    const leave = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(leave);
    expect(leave.defaultPrevented).toBe(true);

    firstLogout.reject(new TypeError("offline"));
    await flushPromises();
    expect(button(root, "Retry logout")).not.toBeNull();
    expect(button(root, "Continue with Google")).toBeNull();
    button(root, "Retry logout")!.click();
    await flushPromises();
    expect(button(root, "Continue with Google")).not.toBeNull();

    backend.current = session(ACCOUNT_A, 8 * HOUR);
    await application.session.bootstrap();
    await flushPromises();
    expect(recordingGateway.requests).toHaveLength(2);
    expect(recordingGateway.requests[1]!.frame).toBe(queuedFrame);
    expect(new Uint8Array(await queuedFrame.data.arrayBuffer())).toEqual(
      new Uint8Array([0, 17, 42, 128, 255]),
    );

    application.session.authenticationExpired();
    await flushPromises();
    expect(root.querySelector("time")).toBeNull();
    backend.current = session(ACCOUNT_B, 8 * HOUR);
    await application.session.bootstrap();
    await flushPromises();
    expect(recordingGateway.requests).toHaveLength(2);
    expect(root.textContent).toMatch(/different Account/u);
  });

  it("updates the session expiry after bootstrap and drops it on expiry or failure", async () => {
    const { application, root, backend } = mountApplication();
    await flushPromises();
    const expiryOf = () => root.querySelector("time")?.getAttribute("datetime");
    const first = (backend.current as CurrentSession).sessionExpiresAt;
    expect(expiryOf()).toBe(first);

    backend.current = session(ACCOUNT_A, 10 * HOUR);
    await application.session.bootstrap();
    await flushPromises();
    expect(expiryOf()).toBe((backend.current as CurrentSession).sessionExpiresAt);
    expect(expiryOf()).not.toBe(first);

    application.session.authenticationExpired();
    await flushPromises();
    expect(expiryOf()).toBeUndefined();

    await application.session.bootstrap();
    await flushPromises();
    expect(expiryOf()).toBe((backend.current as CurrentSession).sessionExpiresAt);

    backend.current = "failure";
    await application.session.bootstrap();
    await flushPromises();
    expect(expiryOf()).toBeUndefined();
  });

  it("turns ambiguous callback fields into a generic failure and removes every reserved value", async () => {
    window.history.replaceState(
      null,
      "",
      "/login?keep=1&keep=2&authError=cancelled&authError=provider_unavailable&referenceId=attacker&referenceId=other",
    );
    const { root, backend } = mountApplication();
    backend.current = "signedOut";
    await flushPromises();
    // The session was read before the test switched the backend; read it again.
    await mounted[0]!.session.bootstrap({ error: "authentication_failed", referenceId: null });
    await flushPromises();

    expect(root.textContent).toMatch(/Sign-in could not be verified/u);
    expect(root.textContent).not.toMatch(/cancelled|attacker|Reference:/u);
    expect(window.location.pathname + window.location.search).toBe("/login?keep=1&keep=2");
  });

  it("shows one validated reference after a failed callback and removes its parameters", async () => {
    window.history.replaceState(
      null,
      "",
      "/login?keep=1&authError=authentication_failed&referenceId=safe_REF-42",
    );
    const { sessionGateway } = { sessionGateway: sessionBackend() };
    sessionGateway.backend.current = "signedOut";
    const { root } = mountApplication({ sessionGateway: sessionGateway.gateway });
    await flushPromises();

    expect(root.textContent).toMatch(/Reference: safe_REF-42/u);
    expect(window.location.pathname + window.location.search).toBe("/login?keep=1");
    expect(button(root, "Continue with Google")).not.toBeNull();
  });

  it("lets a signed-in user reach Projects through visible navigation and return to recording", async () => {
    const { root } = mountApplication();
    await flushPromises();

    expect(testId(root, "private-navigation")).not.toBeNull();
    expect(testId(root, "recording-workspace")).not.toBeNull();
    expect(testId(root, "projects-workspace")).toBeNull();

    testId(root, "projects-view")!.click();
    await flushPromises();
    expect(testId(root, "recording-workspace")).toBeNull();
    expect(testId(root, "projects-workspace")).not.toBeNull();
    expect(root.textContent).toContain("No Projects yet.");

    testId(root, "record-view")!.click();
    await flushPromises();
    expect(testId(root, "recording-workspace")).not.toBeNull();
    expect(testId(root, "projects-workspace")).toBeNull();
  });

  it("keeps a recording in progress while the user looks at Projects", async () => {
    const { root, captures } = mountApplication();
    await flushPromises();
    testId(root, "start")!.click();
    await flushPromises();

    testId(root, "projects-view")!.click();
    await flushPromises();
    testId(root, "record-view")!.click();
    await flushPromises();

    expect(captures).toHaveLength(1);
    expect(captures[0]!.disposed).toBe(false);
    expect(testId(root, "start")!.getAttribute("data-mode")).toBe("stop");
  });

  it("drops private Project data when the user signs out", async () => {
    const { application, root } = mountApplication();
    await flushPromises();
    testId(root, "projects-view")!.click();
    await flushPromises();

    await application.session.logout();
    await flushPromises();
    expect(testId(root, "private-navigation")).toBeNull();
    expect(testId(root, "projects-workspace")).toBeNull();
    expect(application.projects.view.value).toEqual({ kind: "empty" });
  });
});

it("keeps Capture alerts across views, new Capture, and sign-out until explicit acknowledgement", async () => {
  let starts = 0;
  let stops = 0;
  const { root, application, captures } = mountApplication({
    cues: {
      prepare() {},
      play() {},
      dispose() {},
      loopProblem() {
        starts++;
        return () => {
          stops++;
        };
      },
    },
  });
  await flushPromises();
  testId(root, "start")!.click();
  await flushPromises();
  const focused = document.activeElement;
  captures[0]!.options.onError(new Error("Microphone ended"));
  await flushPromises();
  expect(testId(root, "capture-alerts")).not.toBeNull();
  expect(document.activeElement).toBe(focused);
  expect(starts).toBe(1);
  testId(root, "projects-view")!.click();
  await flushPromises();
  expect(testId(root, "capture-alerts")).not.toBeNull();
  testId(root, "record-view")!.click();
  await flushPromises();
  testId(root, "start")!.click();
  await flushPromises();
  expect(application.recording.snapshot.value.capturing).toBe(true);
  expect(starts).toBe(1);
  expect(stops).toBe(0);
  await application.session.logout();
  await flushPromises();
  expect(testId(root, "capture-alerts")).not.toBeNull();
  expect(stops).toBe(0);
  button(root, "Acknowledge")!.click();
  await flushPromises();
  expect(testId(root, "capture-alerts")).toBeNull();
  expect(stops).toBe(1);
});

it("offers takeover when an earlier Capture is stuck finalizing, and keeps Capture alerts independent", async () => {
  const { root, application, captures } = mountApplication();
  await flushPromises();
  testId(root, "start")!.click();
  await flushPromises();
  // A stale finalization promise must not prevent the user's explicit takeover.
  captures[0]!.stop = () => new Promise(() => {});
  testId(root, "start")!.click();
  await flushPromises();
  expect(application.recording.snapshot.value.sections[0]!.finalizingCapture).toBe(true);
  testId(root, "start")!.click();
  await flushPromises();
  expect(root.querySelector('[role="dialog"]')).not.toBeNull();
  button(root, "Cancel")!.click();
  await flushPromises();
  expect(captures).toHaveLength(1);
  expect(captures[0]!.disposed).toBe(false);
  testId(root, "start")!.click();
  await flushPromises();
  application.alerts.raise({ kind: "unrequested-capture-stop", sectionId: "manual" });
  button(root, "Stop previous Capture and start new")!.click();
  await flushPromises();
  expect(captures).toHaveLength(2);
  expect(captures[0]!.disposed).toBe(true);
  expect(application.recording.snapshot.value.capturing).toBe(true);
  expect(application.recording.snapshot.value.sections[0]!.finalizingCapture).toBe(false);
  expect(application.alerts.alerts.value).toHaveLength(1);
  expect(root.querySelector('[role="dialog"]')).toBeNull();
});

it("plays completion when a signed-out Account's last retained Section seals", async () => {
  const played: string[] = [];
  const { application, root, captures, recordingGateway, backend } = mountApplication({
    cues: {
      prepare() {},
      play: (cue) => {
        played.push(cue);
      },
      loopProblem: () => () => {},
      dispose() {},
    },
  });
  await flushPromises();
  testId(root, "start")!.click();
  await flushPromises();
  captures[0]!.emit({
    seq: 0,
    startMs: 0,
    endMs: 10_000,
    mediaType: "audio/webm;codecs=opus",
    data: new Blob(["retained"]),
  });
  testId(root, "start")!.click();
  await flushPromises();
  await application.session.logout();
  await flushPromises();
  // Complete the retained work after restoring its Account, then sign out while
  // sealing is in flight. Completion must precede synchronous ownership cleanup.
  backend.current = session(ACCOUNT_A, 8 * HOUR);
  await application.session.bootstrap();
  await flushPromises();
  const sealing = deferred<AudioAssetSummary>();
  recordingGateway.complete = () => sealing.promise;
  const request = recordingGateway.requests.at(-1)!;
  request.result.resolve({
    sequence: 0,
    acknowledged: true,
    sha256: "a".repeat(64),
    nextExpectedSequence: 1,
  });
  await flushPromises();
  await application.session.logout();
  await flushPromises();
  sealing.resolve(asset(recordingGateway.assetId, "completed"));
  await flushPromises();
  expect(played).toEqual(["capture-started", "capture-completed"]);
  expect(application.recording.hasRecorder.value).toBe(false);
});
