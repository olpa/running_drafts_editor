import template from "./recording-panel-template.ts";
import { ApiClient } from "../api.ts";
import { openMicrophoneCapture } from "../capture.ts";
import { loadConfig } from "../config.ts";
import { CuePlayer } from "../cues.ts";
import { RecordingController, type RecordingDependencies, type RecordingEvent,
  type RecordingSnapshot } from "../recording.ts";
import type { AccountId } from "../session.js";
import { SectionComponent } from "./section-component.ts";

/** One-Project recording module. It retains every Section until this page is left. */
export class RecordingPanel extends HTMLElement {
  private readonly config = loadConfig();
  private readonly cues = new CuePlayer(this);
  private controller: RecordingController | null = null;
  private readonly apiClient = new ApiClient({
    onAuthenticationExpired: () => this.dispatchEvent(new CustomEvent("authentication-expired", { bubbles: true })),
  });
  private recordingDependencies: RecordingDependencies | null = null;
  private sessionAccountId: AccountId | null = null;
  private moduleAccountId: AccountId | null = null;
  private existingProjectId: string | undefined;
  private unsubscribe: (() => void) | null = null;
  private primary!: HTMLButtonElement;
  private projectTitle!: HTMLElement;
  private elapsed!: HTMLElement;
  private captureIndicator!: HTMLElement;
  private captureMessage!: HTMLElement;
  private captureProgress!: HTMLElement;
  private captureTime!: HTMLElement;
  private captureDuration!: HTMLProgressElement;
  private startProblem!: HTMLElement;
  private ownershipWarning!: HTMLElement;
  private sectionList!: HTMLElement;
  private readonly sectionComponents = new Map<string, SectionComponent>();
  private latestSnapshot: RecordingSnapshot | null = null;
  private readonly beforeLeave = (event: BeforeUnloadEvent): void => {
    if (this.controller?.snapshot.hasUnsecuredAudio) {
      event.preventDefault();
      event.returnValue = "";
    }
  };

  connectedCallback(): void {
    if (!this.primary) this.setupView();
    if (this.controller) {
      this.controller.attachView();
      this.subscribeToController();
    } else if (this.sessionAccountId) {
      this.startController(this.sessionAccountId);
    }
    window.addEventListener("beforeunload", this.beforeLeave);
  }

  /** Configure browser seams before connection; production uses real adapters. */
  setDependencies(dependencies: RecordingDependencies): void {
    if (this.isConnected || this.controller) throw new Error("Recording dependencies must be set before connection.");
    this.recordingDependencies = dependencies;
  }

  /** Supply a Project chosen by a future navigation shell before recording starts. */
  setProjectId(projectId: string | undefined): void {
    if (this.controller) throw new Error("Project ID must be set before the recording module starts.");
    this.existingProjectId = projectId;
  }

  disconnectedCallback(): void {
    this.unsubscribe?.();
    this.unsubscribe = null;
    this.controller?.detachView();
    this.cues.dispose();
  }

  /** Permanently finish this page-lifetime module. A temporary view removal must not call this. */
  finishModule(): void {
    window.removeEventListener("beforeunload", this.beforeLeave);
    this.unsubscribe?.();
    this.controller?.dispose();
    this.controller = null;
    this.unsubscribe = null;
    this.moduleAccountId = null;
    this.latestSnapshot = null;
    if (this.sectionList) this.clearSections();
  }

  setSessionAccount(accountId: AccountId | null): void {
    this.sessionAccountId = accountId;
    if (accountId) this.apiClient.markAuthenticationUsable();
    if (!this.controller) {
      if (accountId) this.startController(accountId);
      return;
    }
    if (accountId && accountId === this.moduleAccountId) {
      this.ownershipWarning.textContent = "";
      this.controller.resumeAfterAuthentication(accountId);
      return;
    }
    if (this.hasRetainedWork) {
      this.ownershipWarning.textContent = accountId
        ? "Pending recording work belongs to a different Account. Sign in with the original Account to continue."
        : "Pending recording work is retained. Sign in with the original Account to continue.";
      this.controller.suspendForAuthentication();
      return;
    }
    this.resetController();
    if (accountId) this.startController(accountId);
  }

  get hasUnsecuredAudio(): boolean { return this.controller?.snapshot.hasUnsecuredAudio ?? false; }
  get hasRetainedWork(): boolean {
    const snapshot = this.controller?.snapshot;
    return snapshot?.hasPendingWork ?? false;
  }
  get ownsCurrentSession(): boolean {
    return !!this.moduleAccountId && this.moduleAccountId === this.sessionAccountId;
  }

  private startController(accountId: AccountId): void {
    this.moduleAccountId = accountId;
    const context = this.existingProjectId
      ? { accountId, projectId: this.existingProjectId } : { accountId };
    this.controller = new RecordingController(this.config, this.recordingDependencies ?? {
      backend: this.apiClient, openCapture: openMicrophoneCapture, now: () => performance.now(),
    }, context);
    this.subscribeToController();
  }

  private subscribeToController(): void {
    if (!this.controller || this.unsubscribe) return;
    this.unsubscribe = this.controller.subscribe((event) => {
      if (event.type === "changed") {
        this.latestSnapshot = event.snapshot;
        this.render(event.snapshot);
      }
      if (event.type === "cue") this.cues.play(event.cue);
      this.dispatchEvent(new CustomEvent<RecordingEvent>("recording-event", { detail: event, bubbles: true }));
      if (event.type === "changed" && !this.hasRetainedWork
        && this.moduleAccountId && this.sessionAccountId !== this.moduleAccountId) {
        const nextAccount = this.sessionAccountId;
        this.resetController();
        if (nextAccount) this.startController(nextAccount);
      }
    });
  }

  private resetController(): void {
    this.unsubscribe?.();
    this.controller?.dispose();
    this.controller = null;
    this.unsubscribe = null;
    this.moduleAccountId = null;
    this.latestSnapshot = null;
    this.ownershipWarning.textContent = "";
    this.renderEmpty();
  }

  private setupView(): void {
    this.innerHTML = template;
    this.primary = this.element("start");
    this.projectTitle = this.element("project-title");
    this.elapsed = this.element("elapsed");
    this.captureIndicator = this.element("capture-indicator");
    this.captureMessage = this.element("capture-message");
    this.captureProgress = this.element("capture-progress");
    this.captureTime = this.element("capture-time");
    this.captureDuration = this.element("capture-duration");
    this.startProblem = this.element("start-problem");
    this.ownershipWarning = this.element("ownership-warning");
    this.sectionList = this.element("sections");
    this.primary.addEventListener("click", () => {
      const snapshot = this.latestSnapshot;
      if (snapshot?.capturing) {
        void this.controller?.stop();
        return;
      }
      const request = new CustomEvent("recording-start-request", { bubbles: true, cancelable: true });
      if (!this.dispatchEvent(request) || !this.ownsCurrentSession) return;
      this.cues.prepare();
      void this.controller?.start();
    });
  }

  private render(snapshot: RecordingSnapshot): void {
    this.projectTitle.textContent = snapshot.project?.title ?? "New Project";
    this.elapsed.textContent = formatTime(snapshot.elapsedMs);
    this.captureIndicator.dataset.capturing = String(snapshot.capturing);
    this.captureProgress.hidden = !snapshot.capturing;
    this.captureTime.textContent = `${formatTime(snapshot.elapsedMs)} / ${formatTime(this.config.maxRecordingMs)}`;
    this.captureDuration.max = this.config.maxRecordingMs;
    this.captureDuration.value = Math.min(snapshot.elapsedMs, this.config.maxRecordingMs);
    this.startProblem.textContent = snapshot.startProblem;

    if (snapshot.capturing) {
      this.primary.disabled = false;
      this.primary.dataset.mode = "stop";
      this.primary.textContent = "Stop current recording to start new";
      this.captureMessage.textContent = "Capture is active. Speak now.";
    } else if (snapshot.starting) {
      this.primary.disabled = true;
      this.primary.dataset.mode = "busy";
      this.primary.textContent = "Starting…";
      this.captureMessage.textContent = "Preparing microphone capture. Please wait.";
    } else if (snapshot.sections.some((section) => section.finalizingCapture)) {
      this.primary.disabled = true;
      this.primary.dataset.mode = "busy";
      this.primary.textContent = "Finishing capture…";
      this.captureMessage.textContent = "Finalizing captured audio.";
    } else {
      this.primary.disabled = !snapshot.canStart || !this.ownsCurrentSession;
      this.primary.dataset.mode = snapshot.sections.length ? "start-new" : "start";
      this.primary.textContent = snapshot.sections.length ? "Start new recording" : "Start";
      this.captureMessage.textContent = snapshot.sections.length
        ? "Ready to capture another recording." : "Press Start when you are ready to capture audio.";
    }
    for (const section of snapshot.sections) {
      let component = this.sectionComponents.get(section.sectionId);
      if (!component) {
        component = new SectionComponent(section, {
          uploadPendingAudio: (sectionId) => {
            if (this.ownsCurrentSession) this.controller?.uploadPendingAudio(sectionId);
          },
          retryClose: (sectionId) => {
            if (this.ownsCurrentSession) this.controller?.retryClose(sectionId);
          },
        });
        this.sectionComponents.set(section.sectionId, component);
        this.sectionList.append(component.element);
      } else {
        component.update(section);
      }
    }
  }

  private renderEmpty(): void {
    if (!this.primary) return;
    this.projectTitle.textContent = "New Project";
    this.primary.textContent = "Start";
    this.primary.dataset.mode = "start";
    this.primary.disabled = true;
    this.elapsed.textContent = "0:00";
    this.captureIndicator.dataset.capturing = "false";
    this.captureMessage.textContent = "Sign in to start capture.";
    this.captureProgress.hidden = true;
    this.startProblem.textContent = "";
    this.clearSections();
  }

  private element<T extends HTMLElement>(testId: string): T {
    const element = this.querySelector<T>(`[data-testid="${testId}"]`);
    if (!element) throw new Error(`Missing recording-panel element: ${testId}`);
    return element;
  }

  private clearSections(): void {
    for (const component of this.sectionComponents.values()) component.dispose();
    this.sectionComponents.clear();
    this.sectionList.replaceChildren();
  }
}

function formatTime(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

customElements.define("recording-panel", RecordingPanel);
