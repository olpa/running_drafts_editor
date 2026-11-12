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
import type { UsageSnapshot } from "../usage/gateway.ts";
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
    currentAttempts: 0,
    logouts: [] as Promise<void>[],
    logoutAttempts: 0,
  };
  const gateway: SessionGateway = {
    async current() {
      backend.currentAttempts++;
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
    projectsGateway: {
      listProjects: async () => [],
      getProjectDetail: async (projectId) => ({
        id: projectId,
        title: "Existing",
        createdAt: "now",
        updatedAt: "now",
        document: { sections: [] },
      }),
      audioDownloadUrl: (id) => `https://api.example.test/audio-assets/${id}/download`,
    } satisfies ProjectsGateway,
    healthGateway: { check: async () => Promise.reject(new Error("unused")) },
    usageGateway: {
      current: async (): Promise<UsageSnapshot> => ({
        plan: "free",
        audioStorage: {
          usedBytes: 0,
          grantedBytes: 0,
          warningAtBytes: 107_374_182,
          limitBytes: 134_217_728,
        },
        audioAssetDuration: {
          grantedMs: 0,
          warningAtMs: 2_880_000,
          limitMs: 3_600_000,
        },
      }),
    },
    confirmDiscardPending: () => true,
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

async function openNewCapture(root: HTMLElement): Promise<void> {
  testId(root, "new-recording")!.click();
  await flushPromises();
  expect(testId(root, "capture-modal")).not.toBeNull();
}

describe("application", () => {
  it("retains real queued audio across short-session reauthentication and resumes only for its Account", async () => {
    window.history.replaceState(null, "", "/projects/one?tab=record");
    const { application, root, captures, recordingGateway, backend } = mountApplication();
    await flushPromises();
    expect(root.querySelector("time")?.getAttribute("datetime")).toBe(
      (backend.current as CurrentSession).sessionExpiresAt,
    );

    await openNewCapture(root);
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
    expect(testId(root, "log")).toBeNull();

    // The Session now ends within six hours, so the next start is refused and logs out.
    backend.current = session(ACCOUNT_A, HOUR);
    await application.session.bootstrap();
    await flushPromises();
    expect(application.session.authorizeRecordingStart()).toBe(false);
    await flushPromises();

    expect(button(root, "Continue with Google")).not.toBeNull();
    expect(root.querySelector("time")).toBeNull();
    expect(testId(root, "capture-modal")).not.toBeNull();
    expect(capture.disposed).toBe(true);
    expect(recordingGateway.requests).toHaveLength(1);
    const leave = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(leave);
    expect(leave.defaultPrevented).toBe(true);

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
      "/projects?keep=1&keep=2&authError=cancelled&authError=provider_unavailable&referenceId=attacker&referenceId=other",
    );
    const { root, backend } = mountApplication();
    backend.current = "signedOut";
    await flushPromises();
    // The session was read before the test switched the backend; read it again.
    await mounted[0]!.session.bootstrap({ error: "authentication_failed", referenceId: null });
    await flushPromises();

    expect(root.textContent).toMatch(/Sign-in could not be verified/u);
    expect(root.textContent).not.toMatch(/cancelled|attacker|Reference:/u);
    expect(window.location.pathname + window.location.search).toBe("/projects?keep=1&keep=2");
  });

  it("shows one validated reference after a failed callback and removes its parameters", async () => {
    window.history.replaceState(
      null,
      "",
      "/projects?keep=1&authError=authentication_failed&referenceId=safe_REF-42",
    );
    const { sessionGateway } = { sessionGateway: sessionBackend() };
    sessionGateway.backend.current = "signedOut";
    const { application, root } = mountApplication({ sessionGateway: sessionGateway.gateway });
    await flushPromises();

    expect(root.textContent).toMatch(/Reference: safe_REF-42/u);
    expect(window.location.pathname + window.location.search).toBe("/projects?keep=1");
    expect(application.router.currentRoute.value.fullPath).toBe("/projects?keep=1");
    expect(button(root, "Continue with Google")).not.toBeNull();
  });

  it("renders unknown paths without bootstrapping private authentication", async () => {
    window.history.replaceState(null, "", "/record?keep=1#fragment");
    const authentication = sessionBackend();
    const { root } = mountApplication({ sessionGateway: authentication.gateway });
    await flushPromises();

    expect(testId(root, "page-not-found")).not.toBeNull();
    expect(authentication.backend.currentAttempts).toBe(0);
    expect(window.location.pathname + window.location.search + window.location.hash).toBe(
      "/record?keep=1#fragment",
    );
  });

  it("loads a direct opaque Project route and its compact navigation in parallel", async () => {
    window.history.replaceState(null, "", "/projects/Opaque-ID/");
    const reads: string[] = [];
    const summary = {
      id: "Opaque-ID",
      title: "Canonical route title",
      createdAt: "now",
      updatedAt: "now",
    };
    const { root } = mountApplication({
      projectsGateway: {
        async listProjects() {
          reads.push("list");
          return [summary];
        },
        async getProjectDetail(projectId) {
          reads.push(`detail:${projectId}`);
          return { ...summary, document: { sections: [] } };
        },
        audioDownloadUrl: (id) => `/audio-assets/${id}/download`,
      },
    });
    await flushPromises();

    expect(reads).toEqual(["list", "detail:Opaque-ID"]);
    expect(testId(root, "project-detail")?.getAttribute("data-project-id")).toBe("Opaque-ID");
    expect(root.querySelector('[aria-current="page"]')?.textContent).toContain(
      "Canonical route title",
    );
    expect(window.location.pathname).toBe("/projects/Opaque-ID/");
    expect(globalThis.document.title).toBe("Canonical route title — Running Drafts");
  });

  it("keeps a rejected Project URL and renders the privacy-preserving state", async () => {
    window.history.replaceState(null, "", "/projects/not-a-uuid");
    const { root } = mountApplication({
      projectsGateway: {
        listProjects: async () => [],
        getProjectDetail: async () => {
          throw new ApiError("GET", "/projects/not-a-uuid", 404);
        },
        audioDownloadUrl: (id) => `/audio-assets/${id}/download`,
      },
    });
    await flushPromises();

    expect(testId(root, "project-not-found")).not.toBeNull();
    expect(window.location.pathname).toBe("/projects/not-a-uuid");
    expect(globalThis.document.title).toBe("Project not found — Running Drafts");
  });

  it("matches static paths case-sensitively", async () => {
    window.history.replaceState(null, "", "/Projects");
    const authentication = sessionBackend();
    const { root } = mountApplication({ sessionGateway: authentication.gateway });
    await flushPromises();

    expect(testId(root, "page-not-found")).not.toBeNull();
    expect(authentication.backend.currentAttempts).toBe(0);
  });

  it("renders Projects at the canonical root and opens Capture without changing history", async () => {
    const { root } = mountApplication();
    await flushPromises();

    expect(window.location.pathname).toBe("/projects");
    expect(testId(root, "private-navigation")).not.toBeNull();
    expect(testId(root, "projects-workspace")).not.toBeNull();
    expect(root.textContent).toContain("No Projects yet.");

    await openNewCapture(root);
    expect(window.location.pathname).toBe("/projects");
    testId(root, "close-capture")!.click();
    await flushPromises();
    expect(testId(root, "capture-modal")).toBeNull();
    expect(window.location.pathname).toBe("/projects");
  });

  it("uses real Project links and focuses the routed heading on ordinary navigation", async () => {
    window.history.replaceState(null, "", "/projects?keep=1#notes");
    const summary = {
      id: "project-link",
      title: "Linked Project",
      createdAt: "now",
      updatedAt: "now",
    };
    const { root } = mountApplication({
      projectsGateway: {
        listProjects: async () => [summary],
        getProjectDetail: async () => ({ ...summary, document: { sections: [] } }),
        audioDownloadUrl: (id) => `/audio-assets/${id}/download`,
      },
    });
    await flushPromises();

    const link = root.querySelector<HTMLAnchorElement>('[data-testid="project-list"] a')!;
    expect(link.getAttribute("href")).toBe("/projects/project-link?keep=1#notes");
    link.click();
    await flushPromises();

    expect(window.location.pathname).toBe("/projects/project-link");
    expect(window.location.search + window.location.hash).toBe("?keep=1#notes");
    expect(document.activeElement).toBe(root.querySelector("#project-detail-title"));
  });

  it("makes routed content inert while Capture is open and restores its trigger", async () => {
    const { root } = mountApplication();
    await flushPromises();
    const trigger = testId(root, "new-recording")!;
    trigger.focus();
    trigger.click();
    await flushPromises();

    let background: HTMLElement | null = trigger;
    let isInert = false;
    while (background) {
      isInert ||= background.inert;
      background = background.parentElement;
    }
    expect(isInert).toBe(true);
    expect(document.activeElement).toBe(testId(root, "close-capture"));
    testId(root, "close-capture")!.click();
    await flushPromises();
    expect(document.activeElement).toBe(trigger);
  });

  it("keeps warning and Capture-alert actions inside the modal over one inert page copy", async () => {
    const { application, root } = mountApplication();
    await flushPromises();
    await openNewCapture(root);

    application.allowanceFeedback.play("storage");
    application.alerts.raise({ kind: "unrequested-capture-stop", sectionId: "attention" });
    await flushPromises();

    const dialog = root.querySelector<HTMLElement>('[role="dialog"]')!;
    const feedbackLayer = root.querySelector<HTMLElement>(".global-feedback-layer")!;
    expect(feedbackLayer.inert).toBe(true);
    expect(root.querySelectorAll('[data-testid="capture-alerts"]')).toHaveLength(2);
    expect(button(dialog, "Stop warning sound")).not.toBeNull();
    expect(button(dialog, "Acknowledge")).not.toBeNull();

    button(dialog, "Stop warning sound")!.click();
    button(dialog, "Acknowledge")!.click();
    await flushPromises();
    expect(application.allowanceFeedback.active.value).toBe(false);
    expect(application.alerts.alerts.value).toEqual([]);
  });

  it("keeps routed context inert behind focused Capture and closes after finalization", async () => {
    const { application, root, captures } = mountApplication();
    await flushPromises();
    await openNewCapture(root);
    testId(root, "start")!.click();
    await flushPromises();

    expect(testId(root, "private-navigation")).not.toBeNull();
    expect(testId(root, "capture-modal")).not.toBeNull();
    expect(root.textContent).toContain("Backend health");
    const blocked = application.router.push({
      name: "project-detail",
      params: { projectId: "another" },
    });
    await flushPromises();
    expect(window.location.pathname).toBe("/projects");
    await blocked;
    testId(root, "start")!.click();
    await flushPromises();
    expect(captures).toHaveLength(1);
    expect(captures[0]!.disposed).toBe(true);
    expect(testId(root, "capture-modal")).toBeNull();
    expect(window.location.pathname).toBe("/projects/project-a");
    expect(testId(root, "private-navigation")).not.toBeNull();
  });

  it("drops private Project data when the user signs out", async () => {
    const { application, root } = mountApplication();
    await flushPromises();

    await application.session.logout();
    await flushPromises();
    expect(testId(root, "private-navigation")).toBeNull();
    expect(testId(root, "projects-workspace")).toBeNull();
    expect(application.projects.list.value).toEqual({ kind: "idle" });
    expect(application.projects.detail.value).toEqual({ kind: "idle" });
  });

  it("confirms destructive logout and clears pending work before the server responds", async () => {
    let allow = false;
    let confirmations = 0;
    const pendingLogout = deferred<void>();
    const { application, root, captures, recordingGateway, backend } = mountApplication({
      confirmDiscardPending: () => {
        confirmations++;
        return allow;
      },
    });
    backend.logouts.push(pendingLogout.promise);
    await flushPromises();
    await openNewCapture(root);
    testId(root, "start")!.click();
    await flushPromises();
    captures[0]!.emit({
      seq: 0,
      startMs: 0,
      endMs: 1_000,
      mediaType: "audio/webm;codecs=opus",
      data: new Blob(["pending"]),
    });
    expect(recordingGateway.requests).toHaveLength(1);

    await application.session.logout();
    expect(confirmations).toBe(1);
    expect(application.session.state.value.status).toBe("signedIn");
    expect(application.recording.hasRetainedWork.value).toBe(true);

    allow = true;
    const logout = application.session.logout();
    expect(confirmations).toBe(2);
    expect(application.recording.hasRecorder.value).toBe(false);
    expect(application.usage.snapshot.value).toBeNull();
    expect(application.alerts.alerts.value).toEqual([]);
    pendingLogout.resolve();
    await logout;
    expect(application.session.state.value.status).toBe("signedOut");
  });

  it("reuses authentication loss for Account suspension and retains pending audio", async () => {
    const { application, root, captures, recordingGateway } = mountApplication();
    await flushPromises();
    await openNewCapture(root);
    testId(root, "start")!.click();
    await flushPromises();
    captures[0]!.emit({
      seq: 0,
      startMs: 0,
      endMs: 1_000,
      mediaType: "audio/webm;codecs=opus",
      data: new Blob(["pending"]),
    });
    recordingGateway.requests[0]!.result.reject(
      new ApiError("PUT", "/frames/0", 403, {
        title: "Account unavailable",
        code: "account_suspended",
        referenceId: "support-ref",
      }),
    );
    await flushPromises();

    expect(application.session.state.value.status).toBe("signedOut");
    expect(root.textContent).toContain("Contact support for help.");
    expect(application.recording.hasRetainedWork.value).toBe(true);
    expect(application.recording.snapshot.value.sections[0]!.canUploadPending).toBe(false);
    expect(application.usage.snapshot.value).toBeNull();
    expect(application.projects.list.value).toEqual({ kind: "idle" });
    expect(application.projects.detail.value).toEqual({ kind: "idle" });
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
  await openNewCapture(root);
  testId(root, "start")!.click();
  await flushPromises();
  const focused = document.activeElement;
  captures[0]!.options.onError(new Error("Microphone ended"));
  await flushPromises();
  expect(testId(root, "capture-alerts")).not.toBeNull();
  expect(document.activeElement).toBe(focused);
  expect(starts).toBe(1);
  expect(testId(root, "capture-alerts")).not.toBeNull();
  testId(root, "close-capture")!.click();
  await flushPromises();
  await openNewCapture(root);
  testId(root, "start")!.click();
  await flushPromises();
  expect(application.recording.snapshot.value.capturing).toBe(true);
  expect(starts).toBe(1);
  expect(stops).toBe(0);
  await application.session.logout();
  await flushPromises();
  expect(testId(root, "capture-alerts")).toBeNull();
  expect(stops).toBe(1);
});

it("offers takeover when an earlier Capture is stuck finalizing, and keeps Capture alerts independent", async () => {
  const { root, application, captures } = mountApplication();
  await flushPromises();
  await openNewCapture(root);
  testId(root, "start")!.click();
  await flushPromises();
  // A stale finalization promise must not prevent the user's explicit takeover.
  captures[0]!.stop = () => new Promise(() => {});
  testId(root, "start")!.click();
  await flushPromises();
  expect(application.recording.snapshot.value.sections[0]!.finalizingCapture).toBe(true);
  testId(root, "start")!.click();
  await flushPromises();
  expect(root.querySelectorAll('[role="dialog"]')).toHaveLength(1);
  const takeoverPrompt = root.querySelector<HTMLElement>(".microphone-prompt")!;
  expect(takeoverPrompt.closest('[role="dialog"]')).not.toBeNull();
  expect(testId(root, "start")!.closest("[inert]")).not.toBeNull();
  expect(document.activeElement).toBe(button(root, "Cancel"));
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
  expect(button(root, "Stop previous Capture and start new")).toBeNull();
  expect(testId(root, "capture-modal")).not.toBeNull();
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
  await openNewCapture(root);
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
  application.session.authenticationExpired();
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
  application.session.authenticationExpired();
  await flushPromises();
  sealing.resolve(asset(recordingGateway.assetId, "completed"));
  await flushPromises();
  expect(played).toEqual(["capture-started", "capture-completed"]);
  expect(application.recording.hasRecorder.value).toBe(false);
});
