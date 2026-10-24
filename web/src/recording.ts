import { ApiError, type ApiClient, type AudioAssetSummary, type CreatedSection,
  type ProblemDetails, type ProjectSummary } from "./api.ts";
import type { RecordingCapture, CaptureOptions, TransportFrame } from "./capture.js";
import type { SeedConfig } from "./config.js";
import type { Cue } from "./cues.js";
import { FrameDelivery } from "./delivery.ts";

export type RecordingBackend = Pick<ApiClient,
  "createProject" | "getProject" | "createSection" | "putFrame" | "complete" | "interrupt">;
type RecordingConfig = Pick<SeedConfig, "frameIntervalMs" | "maxRecordingMs" | "retryFirstMs"
  | "retrySecondMs" | "retryThirdMs" | "requestTimeoutMs" | "maxFrameBytes">;

export interface ProjectRecordingContext {
  accountId: string;
  projectId?: string;
}

export type CompletionStatus = "pending" | "failed" | "succeeded";

export interface RecordingDependencies {
  backend: RecordingBackend;
  openCapture: (options: CaptureOptions) => Promise<RecordingCapture>;
  /** Monotonic elapsed-time clock. */
  now: () => number;
  wallClock?: () => Date;
  uuid?: () => string;
  locale?: string;
}

export interface SectionSnapshot {
  readonly sectionId: string;
  readonly audioAssetId: string;
  readonly position: number;
  readonly capturing: boolean;
  readonly finalizingCapture: boolean;
  readonly elapsedMs: number;
  readonly securedMs: number;
  readonly pendingMs: number;
  readonly pendingFrames: number;
  readonly hasUnsecuredAudio: boolean;
  readonly completion: CompletionStatus | null;
  readonly completionProblem: string;
  readonly canRetryCompletion: boolean;
  readonly problem: string;
  readonly canUploadPending: boolean;
  readonly closeProblem: string;
  readonly canRetryClose: boolean;
}

export interface RecordingSnapshot {
  readonly project: Readonly<ProjectSummary> | null;
  readonly starting: boolean;
  readonly canStart: boolean;
  readonly capturing: boolean;
  readonly elapsedMs: number;
  readonly hasUnsecuredAudio: boolean;
  readonly hasPendingWork: boolean;
  readonly startProblem: string;
  readonly sections: readonly SectionSnapshot[];
}

export type RecordingEvent =
  | { type: "changed"; snapshot: RecordingSnapshot }
  | { type: "cue"; cue: Cue }
  | { type: "log"; message: string };

type StopReason = "user" | "duration" | "backlog" | "capture" | "oversize" | "conflict";
type CaptureLifecycle =
  | { kind: "preparing" }
  | { kind: "capturing"; startedAt: number; elapsedMs: number;
      progressTimer: ReturnType<typeof setInterval>; limitTimer: ReturnType<typeof setTimeout> }
  | { kind: "finalizing"; reason: StopReason; startedAt: number; elapsedMs: number }
  | { kind: "ended"; reason: StopReason; elapsedMs: number };
type RecoveryLifecycle = "automatic" | "waiting" | "requested" | "blocked-oversize" | "blocked-conflict";
type CloseCommand = { readonly kind: "complete"; readonly frameCount: number }
  | { readonly kind: "interrupt" };
type CloseLifecycle =
  | { kind: "open" }
  | { kind: "closing"; command: CloseCommand }
  | { kind: "failed"; command: CloseCommand; problem: string }
  | { kind: "closed"; command: CloseCommand; result: AudioAssetSummary };

class SectionSession {
  capture: RecordingCapture | null = null;
  delivery!: FrameDelivery;
  captureLifecycle: CaptureLifecycle = { kind: "preparing" };
  recoveryLifecycle: RecoveryLifecycle = "automatic";
  closeLifecycle: CloseLifecycle = { kind: "open" };
  lastFrameEndMs = 0;
  securedMs = 0;
  assignedFrames = 0;
  problem = "";
  authenticationSuspended = false;

  constructor(readonly created: CreatedSection) {}
}

interface CreationAttempt {
  readonly key: string;
  readonly title: string;
}

/** Owns one Project and all page-lifetime Section delivery state independently of its view. */
export class RecordingController {
  private project: ProjectSummary | null = null;
  private projectLoad: Promise<ProjectSummary> | null = null;
  private projectCreation: CreationAttempt | null = null;
  private sectionCreationKey: string | null = null;
  private readonly sections: SectionSession[] = [];
  private active: SectionSession | null = null;
  private starting = false;
  private startProblem = "";
  private disposed = false;
  private authenticationSuspended = false;
  private viewAttached = true;
  private readonly listeners = new Set<(event: RecordingEvent) => void>();
  private readonly uuid: () => string;
  private readonly wallClock: () => Date;

  constructor(
    private readonly config: RecordingConfig,
    private readonly dependencies: RecordingDependencies,
    readonly context: Readonly<ProjectRecordingContext>,
  ) {
    this.uuid = dependencies.uuid ?? (() => crypto.randomUUID());
    this.wallClock = dependencies.wallClock ?? (() => new Date());
    if (context.projectId) {
      void this.loadExistingProject().catch((error: unknown) => {
        if (this.disposed) return;
        this.startProblem = `Could not load Project: ${errorMessage(error)}`;
        this.changed();
      });
    }
  }

  get snapshot(): RecordingSnapshot {
    const active = this.active;
    return {
      project: this.project,
      starting: this.starting,
      canStart: !this.disposed && this.viewAttached && !this.authenticationSuspended && !this.starting
        && (!active || (active.captureLifecycle.kind !== "capturing"
          && active.captureLifecycle.kind !== "finalizing")),
      capturing: active?.captureLifecycle.kind === "capturing",
      elapsedMs: active ? captureElapsedMs(active.captureLifecycle) : 0,
      hasUnsecuredAudio: this.sections.some((section) => this.hasUnsecuredAudio(section)),
      hasPendingWork: this.starting || this.sections.some((section) => this.hasUnsecuredAudio(section)
        || (section.captureLifecycle.kind === "ended" && section.closeLifecycle.kind !== "closed")),
      startProblem: this.startProblem,
      sections: this.sections.map((section) => this.sectionSnapshot(section)),
    };
  }

  subscribe(listener: (event: RecordingEvent) => void): () => void {
    this.listeners.add(listener);
    listener({ type: "changed", snapshot: this.snapshot });
    return () => { this.listeners.delete(listener); };
  }

  /** Reconnect a view to this page-lifetime owner without replacing delivery state. */
  attachView(): void {
    if (this.disposed || this.viewAttached) return;
    this.viewAttached = true;
    this.changed();
  }

  /** Release active capture resources while retaining all Project and delivery state. */
  detachView(): void {
    if (this.disposed || !this.viewAttached) return;
    this.viewAttached = false;
    const active = this.active;
    if (active?.captureLifecycle.kind === "capturing") void this.stopCapture(active, "capture");
    this.changed();
  }

  async start(): Promise<void> {
    if (!this.snapshot.canStart) return;
    this.starting = true;
    this.startProblem = "";
    this.changed();
    let capture: RecordingCapture | null = null;
    let createdSession: SectionSession | null = null;
    try {
      capture = await this.dependencies.openCapture({
        intervalMs: this.config.frameIntervalMs,
        canContinue: () => this.canContinueCapture(),
        onError: (error) => {
          const active = this.active;
          if (active?.captureLifecycle.kind === "capturing") {
            this.write(active, `ERR ${error.message}`);
            void this.stopCapture(active, "capture");
          }
        },
      });
      if (!this.startMayContinue(capture)) return;
      const projectId = await this.ensureProject();
      if (!this.startMayContinue(capture)) return;
      this.sectionCreationKey ??= this.uuid();
      const created = await this.dependencies.backend.createSection(projectId, this.sectionCreationKey);
      this.sectionCreationKey = null;
      if (this.disposed) { capture.dispose(); return; }

      const section = new SectionSession(created);
      createdSession = section;
      section.capture = capture;
      section.delivery = this.createDelivery(section);
      this.sections.push(section);
      this.sections.sort((left, right) => left.created.position - right.created.position);
      this.active = section;
      this.starting = false;
      if (this.authenticationSuspended || !this.viewAttached) {
        capture.dispose();
        section.authenticationSuspended = this.authenticationSuspended;
        section.captureLifecycle = { kind: "ended", reason: "capture", elapsedMs: 0 };
        section.recoveryLifecycle = "requested";
        section.problem = "Capture did not start because its view or Application Session ended.";
        this.changed();
        await this.maybeClose(section);
        return;
      }

      const startedAt = this.dependencies.now();
      const progressTimer = setInterval(() => {
        if (section.captureLifecycle.kind !== "capturing") return;
        section.captureLifecycle.elapsedMs = this.dependencies.now() - section.captureLifecycle.startedAt;
        this.changed();
      }, 100);
      const limitTimer = setTimeout(() => void this.stopCapture(section, "duration"), this.config.maxRecordingMs);
      section.captureLifecycle = { kind: "capturing", startedAt, elapsedMs: 0, progressTimer, limitTimer };
      capture.start((frame) => this.acceptFrame(section, frame));
      this.emit({ type: "cue", cue: "start" });
      this.write(section, `section ${created.position + 1} ${created.audioAssetId.slice(0, 8)}`);
      this.changed();
    } catch (error) {
      capture?.dispose();
      if (this.disposed) return;
      if (createdSession) {
        this.clearCaptureTimers(createdSession.captureLifecycle);
        createdSession.captureLifecycle = { kind: "ended", reason: "capture", elapsedMs: 0 };
        createdSession.recoveryLifecycle = "requested";
        createdSession.problem = `Capture could not begin: ${errorMessage(error)}`;
        void this.maybeClose(createdSession);
      }
      this.starting = false;
      this.startProblem = `Could not start capture: ${errorMessage(error)}`;
      this.emit({ type: "log", message: `0.0 ERR ${this.startProblem}` });
      this.changed();
    }
  }

  stop(): Promise<void> {
    return this.active ? this.stopCapture(this.active, "user") : Promise.resolve();
  }

  uploadPendingAudio(sectionId: string): void {
    const section = this.findSection(sectionId);
    if (!section || section.captureLifecycle.kind !== "ended" || !section.delivery.pendingCount
      || section.recoveryLifecycle === "blocked-oversize"
      || section.recoveryLifecycle === "blocked-conflict") return;
    if (this.isInterrupted(section)) section.recoveryLifecycle = "requested";
    section.problem = "";
    section.delivery.retry();
    this.changed();
  }

  retryClose(sectionId: string): void {
    const section = this.findSection(sectionId);
    if (!section || section.closeLifecycle.kind !== "failed") return;
    void this.close(section, section.closeLifecycle.command);
  }

  suspendForAuthentication(): void {
    if (this.authenticationSuspended) return;
    this.authenticationSuspended = true;
    for (const section of this.sections) {
      section.authenticationSuspended = true;
      section.delivery.freeze();
    }
    this.changed();
  }

  resumeAfterAuthentication(accountId: string): void {
    if (!this.authenticationSuspended || accountId !== this.context.accountId) return;
    this.authenticationSuspended = false;
    for (const section of this.sections) {
      section.authenticationSuspended = false;
      if (section.delivery.pendingCount
        && (section.recoveryLifecycle === "automatic" || section.recoveryLifecycle === "requested")) {
        section.delivery.retry();
      }
      void this.maybeClose(section);
    }
    this.changed();
  }

  /** Permanently finish the page-lifetime module and release every resource. */
  dispose(): void {
    this.disposed = true;
    this.starting = false;
    for (const section of this.sections) {
      this.clearCaptureTimers(section.captureLifecycle);
      section.delivery.dispose();
      section.capture?.dispose();
    }
    this.listeners.clear();
  }

  private startMayContinue(capture: RecordingCapture): boolean {
    if (!this.disposed && !this.authenticationSuspended && this.viewAttached) return true;
    capture.dispose();
    this.starting = false;
    if (!this.disposed) this.changed();
    return false;
  }

  private async ensureProject(): Promise<string> {
    if (this.project) return this.project.id;
    if (this.context.projectId) return (await this.loadExistingProject()).id;
    this.projectCreation ??= {
      key: this.uuid(),
      title: localizedProjectTitle(this.wallClock(), this.dependencies.locale),
    };
    const created = await this.dependencies.backend.createProject(
      this.projectCreation.key, this.projectCreation.title,
    );
    this.project = created;
    this.projectCreation = null;
    this.changed();
    return created.id;
  }

  private loadExistingProject(): Promise<ProjectSummary> {
    if (this.project) return Promise.resolve(this.project);
    const projectId = this.context.projectId;
    if (!projectId) return Promise.reject(new Error("No existing Project ID was supplied."));
    if (!this.projectLoad) {
      const request = this.dependencies.backend.getProject(projectId).then((project) => {
        if (project.id !== projectId) throw new Error("The loaded Project identity did not match.");
        this.project = project;
        this.startProblem = "";
        this.changed();
        return project;
      });
      const tracked = request.finally(() => {
        if (this.projectLoad === tracked) this.projectLoad = null;
      });
      this.projectLoad = tracked;
    }
    return this.projectLoad;
  }

  private createDelivery(section: SectionSession): FrameDelivery {
    return new FrameDelivery({
      retryDelaysMs: [this.config.retryFirstMs, this.config.retrySecondMs, this.config.retryThirdMs],
      requestTimeoutMs: this.config.requestTimeoutMs,
      send: (frame, signal) => this.dependencies.backend.putFrame(section.created.audioAssetId, frame, signal),
      log: (message) => this.write(section, message),
      changed: () => {
        if (this.disposed) return;
        this.changed();
        void this.maybeClose(section);
      },
      acknowledged: (frame) => {
        section.securedMs = Math.max(section.securedMs, frame.endMs);
        if (section === this.active && section.captureLifecycle.kind === "capturing") {
          this.emit({ type: "cue", cue: "health" });
        }
      },
      delayed: () => {
        if (section === this.active && section.captureLifecycle.kind === "capturing") {
          this.emit({ type: "cue", cue: "delayed" });
        }
      },
      failed: (_frame, error) => this.handleFrameFailure(section, error),
    });
  }

  private handleFrameFailure(section: SectionSession, error: unknown): void {
    if (!(error instanceof ApiError) || error.status !== 409
      || section.recoveryLifecycle === "blocked-conflict") return;
    section.delivery.terminate();
    section.recoveryLifecycle = "blocked-conflict";
    section.problem = conflictProblem(error.problem);
    this.write(section, `frame conflict HTTP 409 code=${error.problem?.code ?? "none"}`
      + ` referenceId=${error.problem?.referenceId ?? "none"}`);
    if (section.captureLifecycle.kind === "capturing") {
      void this.stopCapture(section, "conflict");
    } else {
      this.changed();
    }
  }

  private acceptFrame(section: SectionSession, frame: TransportFrame): void {
    if (this.disposed || !this.sections.includes(section)) return;
    section.lastFrameEndMs = frame.endMs;
    section.assignedFrames++;
    if (frame.data.size > this.config.maxFrameBytes) {
      section.delivery.freeze();
      section.recoveryLifecycle = "blocked-oversize";
      section.problem = "Capture was interrupted because an audio frame is too large to upload. Audio remains in this page.";
      section.delivery.enqueue(frame);
      if (section.captureLifecycle.kind === "capturing") void this.stopCapture(section, "oversize");
      return;
    }
    section.delivery.enqueue(frame);
  }

  private canContinueCapture(): boolean {
    const section = this.active;
    if (!section || section.captureLifecycle.kind !== "capturing" || this.disposed) return false;
    if (section.delivery.pendingCount >= 2) {
      void this.stopCapture(section, "backlog");
      return false;
    }
    return true;
  }

  private async stopCapture(section: SectionSession, reason: StopReason): Promise<void> {
    const lifecycle = section.captureLifecycle;
    if (this.disposed || lifecycle.kind !== "capturing" || !section.capture) return;
    this.clearCaptureTimers(lifecycle);
    const elapsedMs = this.dependencies.now() - lifecycle.startedAt;
    section.captureLifecycle = { kind: "finalizing", reason, startedAt: lifecycle.startedAt, elapsedMs };
    this.write(section, `capture stop ${reason}`);
    if (this.isInterruptionReason(reason)) {
      if (section.recoveryLifecycle !== "blocked-oversize"
        && section.recoveryLifecycle !== "blocked-conflict") section.recoveryLifecycle = "waiting";
      section.problem ||= reason === "backlog"
        ? "Capture was interrupted because audio could not be uploaded."
        : reason === "oversize"
          ? "Capture was interrupted because an audio frame is too large to upload. Audio remains in this page."
          : "Capture was interrupted because the microphone became unavailable.";
      section.delivery.freeze();
      if (section === this.active) this.emit({ type: "cue", cue: "interrupted" });
    } else if (reason === "duration" && section === this.active) {
      this.emit({ type: "cue", cue: "stop" });
    }
    this.changed();
    await section.capture.stop();
    if (this.disposed) return;
    section.captureLifecycle = { kind: "ended", reason, elapsedMs };
    if (this.isInterruptionReason(reason) && section.delivery.pendingCount === 0
      && section.recoveryLifecycle !== "blocked-oversize"
      && section.recoveryLifecycle !== "blocked-conflict") {
      section.recoveryLifecycle = "requested";
    }
    if (!this.isInterruptionReason(reason) && reason === "user" && section === this.active) {
      this.emit({ type: "cue", cue: "stop" });
    }
    this.changed();
    await this.maybeClose(section);
  }

  private async maybeClose(section: SectionSession): Promise<void> {
    if (this.disposed || section.authenticationSuspended || section.captureLifecycle.kind !== "ended"
      || section.delivery.pendingCount || section.closeLifecycle.kind === "closed"
      || section.closeLifecycle.kind === "closing" || section.recoveryLifecycle === "blocked-oversize"
      || section.recoveryLifecycle === "blocked-conflict") return;
    if (this.isInterrupted(section) && section.recoveryLifecycle !== "requested") return;
    const command = section.closeLifecycle.kind === "failed"
      ? section.closeLifecycle.command
      : this.isInterrupted(section)
        ? { kind: "interrupt" } as const
        : { kind: "complete", frameCount: section.assignedFrames } as const;
    await this.close(section, command);
  }

  private async close(section: SectionSession, command: CloseCommand): Promise<void> {
    if (section.closeLifecycle.kind === "closing" || section.closeLifecycle.kind === "closed"
      || this.disposed) return;
    section.closeLifecycle = { kind: "closing", command };
    this.changed();
    try {
      let result: AudioAssetSummary;
      if (command.kind === "complete") {
        result = await this.dependencies.backend.complete(section.created.audioAssetId, command.frameCount);
      } else {
        result = await this.dependencies.backend.interrupt(section.created.audioAssetId);
      }
      if (this.disposed) return;
      section.closeLifecycle = result.isSealed
        ? { kind: "closed", command, result }
        : { kind: "failed", command, problem: command.kind === "complete"
          ? "Could not complete recording" : "Could not close recording" };
      if (result.isSealed) section.problem = "";
      this.write(section, command.kind === "complete" ? "complete sent" : "interrupt sent");
    } catch (error) {
      if (this.disposed) return;
      section.closeLifecycle = {
        kind: "failed", command, problem: command.kind === "complete"
          ? `Could not complete recording: ${errorMessage(error)}`
          : `Could not close recording: ${errorMessage(error)}`,
      };
      this.write(section, `ERR ${section.closeLifecycle.problem}`);
    } finally {
      if (!this.disposed) this.changed();
    }
  }

  private sectionSnapshot(section: SectionSession): SectionSnapshot {
    const unfinalizedCapturedMs = section.captureLifecycle.kind === "capturing"
      || section.captureLifecycle.kind === "finalizing"
      ? Math.max(0, captureElapsedMs(section.captureLifecycle) - section.lastFrameEndMs) : 0;
    const pendingFrames = section.delivery.pendingCount;
    const completion = this.completionStatus(section);
    const completionProblem = completion === "failed" ? this.completionProblem(section) : "";
    const completionFailure = section.closeLifecycle.kind === "failed"
      && section.closeLifecycle.command.kind === "complete";
    const closeProblem = section.closeLifecycle.kind === "failed"
      && section.closeLifecycle.command.kind === "interrupt" ? section.closeLifecycle.problem : "";
    return {
      sectionId: section.created.sectionId,
      audioAssetId: section.created.audioAssetId,
      position: section.created.position,
      capturing: section.captureLifecycle.kind === "capturing",
      finalizingCapture: section.captureLifecycle.kind === "finalizing",
      elapsedMs: captureElapsedMs(section.captureLifecycle),
      securedMs: section.securedMs,
      pendingMs: section.delivery.pendingMs + unfinalizedCapturedMs,
      pendingFrames,
      hasUnsecuredAudio: this.hasUnsecuredAudio(section),
      completion,
      completionProblem,
      canRetryCompletion: completionFailure,
      problem: section.problem || (section.delivery.delayed
        ? section.captureLifecycle.kind === "capturing" ? "Audio delivery is delayed. Capture continues."
          : "Audio delivery is delayed. Capture has stopped."
        : ""),
      canUploadPending: section.captureLifecycle.kind === "ended" && pendingFrames > 0
        && section.recoveryLifecycle !== "blocked-oversize"
        && section.recoveryLifecycle !== "blocked-conflict",
      closeProblem,
      canRetryClose: !!closeProblem,
    };
  }

  private completionStatus(section: SectionSession): CompletionStatus | null {
    const capture = section.captureLifecycle;
    if (capture.kind === "preparing" || capture.kind === "capturing"
      || this.isInterruptionReason(capture.reason)) return null;
    const close = section.closeLifecycle;
    if (close.kind === "open" || close.kind === "closing") return "pending";
    if (close.command.kind !== "complete") return null;
    if (close.kind === "failed") return "failed";
    return close.result.isSealed && close.result.sealReason === "completed" ? "succeeded" : "failed";
  }

  private completionProblem(section: SectionSession): string {
    const close = section.closeLifecycle;
    if (close.kind === "failed" && close.command.kind === "complete") {
      return "Could not complete recording.";
    }
    if (close.kind === "closed" && close.command.kind === "complete") {
      return close.result.sealReason === "interrupted"
        ? "Recording was sealed as interrupted and cannot be completed."
        : "Recording was sealed without normal completion and cannot be completed.";
    }
    return "";
  }

  private hasUnsecuredAudio(section: SectionSession): boolean {
    return section.captureLifecycle.kind === "capturing" || section.captureLifecycle.kind === "finalizing"
      || section.delivery.pendingCount > 0;
  }

  private isInterrupted(section: SectionSession): boolean {
    return section.captureLifecycle.kind === "ended"
      && this.isInterruptionReason(section.captureLifecycle.reason);
  }

  private isInterruptionReason(reason: StopReason): boolean {
    return reason === "backlog" || reason === "capture" || reason === "oversize" || reason === "conflict";
  }

  private findSection(sectionId: string): SectionSession | undefined {
    return this.sections.find((section) => section.created.sectionId === sectionId);
  }

  private clearCaptureTimers(lifecycle: CaptureLifecycle): void {
    if (lifecycle.kind !== "capturing") return;
    clearInterval(lifecycle.progressTimer);
    clearTimeout(lifecycle.limitTimer);
  }

  private changed(): void { this.emit({ type: "changed", snapshot: this.snapshot }); }

  private emit(event: RecordingEvent): void {
    for (const listener of this.listeners) listener(event);
  }

  private write(section: SectionSession, entry: string): void {
    if (this.disposed) return;
    const lifecycle = section.captureLifecycle;
    const startedAt = lifecycle.kind === "capturing" || lifecycle.kind === "finalizing"
      ? lifecycle.startedAt : this.dependencies.now();
    const seconds = ((this.dependencies.now() - startedAt) / 1000).toFixed(1);
    this.emit({ type: "log", message: `${seconds} s${section.created.position + 1} ${entry}` });
  }
}

function captureElapsedMs(lifecycle: CaptureLifecycle): number {
  return lifecycle.kind === "preparing" ? 0 : lifecycle.elapsedMs;
}

export function localizedProjectTitle(date: Date, locale?: string): string {
  return `Draft — ${new Intl.DateTimeFormat(locale, {
    dateStyle: "medium", timeStyle: "short",
  }).format(date)}`;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function conflictProblem(problem: Readonly<ProblemDetails> | null): string {
  const details = [
    problem?.title ? `Title: ${problem.title}` : "",
    problem?.detail ? `Detail: ${problem.detail}` : "",
    problem?.code ? `Code: ${problem.code}` : "",
    problem?.referenceId ? `Reference ID: ${problem.referenceId}` : "",
  ].filter(Boolean);
  return ["Capture stopped because audio delivery conflicted with stored data. Audio remains in this page.",
    ...details].join(" ");
}
