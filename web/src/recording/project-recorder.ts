import { shallowRef, type Ref } from "vue";
import { ApiError, type ProblemDetails } from "../shared/http.ts";
import type { AudioAssetSummary, ProjectSummary } from "../shared/project-types.ts";
import type { CaptureOptions, MicrophoneCapture, TransportFrame } from "./capture.ts";
import type { Cue } from "./cues.ts";
import { FrameDelivery } from "./delivery.ts";
import type { CreatedSection, RecordingGateway } from "./gateway.ts";

export interface RecordingConfig {
  frameIntervalMs: number;
  maxRecordingMs: number;
  retryFirstMs: number;
  retrySecondMs: number;
  retryThirdMs: number;
  requestTimeoutMs: number;
  maxFrameBytes: number;
}

export interface ProjectRecordingContext {
  accountId: string;
  projectId?: string;
}

export type CompletionStatus = "pending" | "failed" | "succeeded";

/** Browser and clock seams of a recording run; production uses real adapters. */
export interface RecordingEnvironment {
  gateway: RecordingGateway;
  openCapture: (options: CaptureOptions) => Promise<MicrophoneCapture>;
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

export interface RecordingSinks {
  cue(cue: Cue): void;
  log(message: string): void;
  /** Called after every published snapshot, in the same turn as the change. */
  changed?(snapshot: RecordingSnapshot): void;
}

/** Everything the page owns about recording one Project for one Account. */
export interface ProjectRecorder {
  readonly snapshot: Readonly<Ref<RecordingSnapshot>>;
  readonly context: Readonly<ProjectRecordingContext>;
  start(): Promise<void>;
  stop(): Promise<void>;
  uploadPendingAudio(sectionId: string): void;
  retryClose(sectionId: string): void;
  suspendForAuthentication(): void;
  resumeAfterAuthentication(accountId: string): void;
  /** Permanently finish the recorder and release every resource. */
  dispose(): void;
}

type StopReason = "user" | "duration" | "backlog" | "capture" | "oversize" | "conflict";
type CaptureLifecycle =
  | { kind: "preparing" }
  | {
      kind: "capturing";
      startedAt: number;
      elapsedMs: number;
      progressTimer: ReturnType<typeof setInterval>;
      limitTimer: ReturnType<typeof setTimeout>;
    }
  | { kind: "finalizing"; reason: StopReason; startedAt: number; elapsedMs: number }
  | { kind: "ended"; reason: StopReason; elapsedMs: number };
type RecoveryLifecycle =
  "automatic" | "waiting" | "requested" | "blocked-oversize" | "blocked-conflict";
type CloseCommand =
  { readonly kind: "complete"; readonly frameCount: number } | { readonly kind: "interrupt" };
type CloseLifecycle =
  | { kind: "open" }
  | { kind: "closing"; command: CloseCommand }
  | { kind: "failed"; command: CloseCommand; problem: string }
  | { kind: "closed"; command: CloseCommand; result: AudioAssetSummary };

interface SectionSession {
  readonly created: CreatedSection;
  capture: MicrophoneCapture | null;
  delivery: FrameDelivery;
  captureLifecycle: CaptureLifecycle;
  recoveryLifecycle: RecoveryLifecycle;
  closeLifecycle: CloseLifecycle;
  lastFrameEndMs: number;
  securedMs: number;
  assignedFrames: number;
  problem: string;
  authenticationSuspended: boolean;
}

interface CreationAttempt {
  readonly key: string;
  readonly title: string;
}

const EMPTY_SNAPSHOT: RecordingSnapshot = {
  project: null,
  starting: false,
  canStart: false,
  capturing: false,
  elapsedMs: 0,
  hasUnsecuredAudio: false,
  hasPendingWork: false,
  startProblem: "",
  sections: [],
};

export function createProjectRecorder(
  config: RecordingConfig,
  environment: RecordingEnvironment,
  context: Readonly<ProjectRecordingContext>,
  sinks: RecordingSinks,
): ProjectRecorder {
  const { gateway } = environment;
  const uuid = environment.uuid ?? (() => crypto.randomUUID());
  const wallClock = environment.wallClock ?? (() => new Date());
  const sections: SectionSession[] = [];
  const snapshot = shallowRef<RecordingSnapshot>(EMPTY_SNAPSHOT);

  let project: ProjectSummary | null = null;
  let projectLoad: Promise<ProjectSummary> | null = null;
  let projectCreation: CreationAttempt | null = null;
  let sectionCreationKey: string | null = null;
  let active: SectionSession | null = null;
  let starting = false;
  let startProblem = "";
  let disposed = false;
  let authenticationSuspended = false;

  function currentSnapshot(): RecordingSnapshot {
    return {
      project,
      starting,
      canStart:
        !disposed &&
        !authenticationSuspended &&
        !starting &&
        (!active ||
          (active.captureLifecycle.kind !== "capturing" &&
            active.captureLifecycle.kind !== "finalizing")),
      capturing: active?.captureLifecycle.kind === "capturing",
      elapsedMs: active ? captureElapsedMs(active.captureLifecycle) : 0,
      hasUnsecuredAudio: sections.some(hasUnsecuredAudio),
      hasPendingWork:
        starting ||
        sections.some(
          (section) =>
            hasUnsecuredAudio(section) ||
            (section.captureLifecycle.kind === "ended" && section.closeLifecycle.kind !== "closed"),
        ),
      startProblem,
      sections: sections.map(sectionSnapshot),
    };
  }

  function changed(): void {
    snapshot.value = currentSnapshot();
    sinks.changed?.(snapshot.value);
  }

  function write(section: SectionSession, entry: string): void {
    if (disposed) return;
    const lifecycle = section.captureLifecycle;
    const startedAt =
      lifecycle.kind === "capturing" || lifecycle.kind === "finalizing"
        ? lifecycle.startedAt
        : environment.now();
    const seconds = ((environment.now() - startedAt) / 1000).toFixed(1);
    sinks.log(`${seconds} s${section.created.position + 1} ${entry}`);
  }

  function hasUnsecuredAudio(section: SectionSession): boolean {
    return (
      section.captureLifecycle.kind === "capturing" ||
      section.captureLifecycle.kind === "finalizing" ||
      section.delivery.pendingCount > 0
    );
  }

  function isInterrupted(section: SectionSession): boolean {
    return (
      section.captureLifecycle.kind === "ended" &&
      isInterruptionReason(section.captureLifecycle.reason)
    );
  }

  function clearCaptureTimers(lifecycle: CaptureLifecycle): void {
    if (lifecycle.kind !== "capturing") return;
    clearInterval(lifecycle.progressTimer);
    clearTimeout(lifecycle.limitTimer);
  }

  function findSection(sectionId: string): SectionSession | undefined {
    return sections.find((section) => section.created.sectionId === sectionId);
  }

  function startMayContinue(capture: MicrophoneCapture): boolean {
    if (!disposed && !authenticationSuspended) return true;
    capture.dispose();
    starting = false;
    if (!disposed) changed();
    return false;
  }

  async function ensureProject(): Promise<string> {
    if (project) return project.id;
    if (context.projectId) return (await loadExistingProject()).id;
    projectCreation ??= {
      key: uuid(),
      title: localizedProjectTitle(wallClock(), environment.locale),
    };
    const created = await gateway.createProject(projectCreation.key, projectCreation.title);
    project = created;
    projectCreation = null;
    changed();
    return created.id;
  }

  function loadExistingProject(): Promise<ProjectSummary> {
    if (project) return Promise.resolve(project);
    const projectId = context.projectId;
    if (!projectId) return Promise.reject(new Error("No existing Project ID was supplied."));
    if (!projectLoad) {
      const request = gateway.getProject(projectId).then((loaded) => {
        if (loaded.id !== projectId) throw new Error("The loaded Project identity did not match.");
        project = loaded;
        startProblem = "";
        changed();
        return loaded;
      });
      const tracked: Promise<ProjectSummary> = request.finally(() => {
        if (projectLoad === tracked) projectLoad = null;
      });
      projectLoad = tracked;
    }
    return projectLoad;
  }

  function createDelivery(section: () => SectionSession): FrameDelivery {
    return new FrameDelivery({
      retryDelaysMs: [config.retryFirstMs, config.retrySecondMs, config.retryThirdMs],
      requestTimeoutMs: config.requestTimeoutMs,
      send: (frame, signal) => gateway.putFrame(section().created.audioAssetId, frame, signal),
      log: (message) => write(section(), message),
      changed: () => {
        if (disposed) return;
        changed();
        void maybeClose(section());
      },
      acknowledged: (frame) => {
        const current = section();
        current.securedMs = Math.max(current.securedMs, frame.endMs);
        if (current === active && current.captureLifecycle.kind === "capturing") {
          sinks.cue("health");
        }
      },
      delayed: () => {
        const current = section();
        if (current === active && current.captureLifecycle.kind === "capturing") {
          sinks.cue("delayed");
        }
      },
      failed: (_frame, error) => handleFrameFailure(section(), error),
      now: environment.now,
    });
  }

  function handleFrameFailure(section: SectionSession, error: unknown): void {
    if (
      !(error instanceof ApiError) ||
      error.status !== 409 ||
      section.recoveryLifecycle === "blocked-conflict"
    ) {
      return;
    }
    section.delivery.terminate();
    section.recoveryLifecycle = "blocked-conflict";
    section.problem = conflictProblem(error.problem);
    write(
      section,
      `frame conflict HTTP 409 code=${error.problem?.code ?? "none"}` +
        ` referenceId=${error.problem?.referenceId ?? "none"}`,
    );
    if (section.captureLifecycle.kind === "capturing") {
      void stopCapture(section, "conflict");
    } else {
      changed();
    }
  }

  function acceptFrame(section: SectionSession, frame: TransportFrame): void {
    if (disposed || !sections.includes(section)) return;
    section.lastFrameEndMs = frame.endMs;
    section.assignedFrames++;
    if (frame.data.size > config.maxFrameBytes) {
      section.delivery.freeze();
      section.recoveryLifecycle = "blocked-oversize";
      section.problem =
        "Capture was interrupted because an audio frame is too large to upload. Audio remains in this page.";
      section.delivery.enqueue(frame);
      if (section.captureLifecycle.kind === "capturing") void stopCapture(section, "oversize");
      return;
    }
    section.delivery.enqueue(frame);
  }

  function canContinueCapture(): boolean {
    const section = active;
    if (!section || section.captureLifecycle.kind !== "capturing" || disposed) return false;
    if (section.delivery.pendingCount >= 2) {
      void stopCapture(section, "backlog");
      return false;
    }
    return true;
  }

  async function stopCapture(section: SectionSession, reason: StopReason): Promise<void> {
    const lifecycle = section.captureLifecycle;
    if (disposed || lifecycle.kind !== "capturing" || !section.capture) return;
    clearCaptureTimers(lifecycle);
    const elapsedMs = environment.now() - lifecycle.startedAt;
    section.captureLifecycle = {
      kind: "finalizing",
      reason,
      startedAt: lifecycle.startedAt,
      elapsedMs,
    };
    write(section, `capture stop ${reason}`);
    if (isInterruptionReason(reason)) {
      if (
        section.recoveryLifecycle !== "blocked-oversize" &&
        section.recoveryLifecycle !== "blocked-conflict"
      ) {
        section.recoveryLifecycle = "waiting";
      }
      section.problem ||=
        reason === "backlog"
          ? "Capture was interrupted because audio could not be uploaded."
          : reason === "oversize"
            ? "Capture was interrupted because an audio frame is too large to upload. Audio remains in this page."
            : "Capture was interrupted because the microphone became unavailable.";
      section.delivery.freeze();
      if (section === active) sinks.cue("interrupted");
    } else if (reason === "duration" && section === active) {
      sinks.cue("stop");
    }
    changed();
    try {
      await section.capture.stop();
    } catch (error) {
      if (disposed) return;
      section.captureLifecycle = { kind: "ended", reason: "capture", elapsedMs };
      section.recoveryLifecycle = section.delivery.pendingCount ? "waiting" : "requested";
      section.problem = `Capture was interrupted while finalizing: ${errorMessage(error)}`;
      section.delivery.freeze();
      if (section === active) sinks.cue("interrupted");
      write(section, `ERR ${section.problem}`);
      changed();
      await maybeClose(section);
      return;
    }
    if (disposed) return;
    section.captureLifecycle = { kind: "ended", reason, elapsedMs };
    if (
      isInterruptionReason(reason) &&
      section.delivery.pendingCount === 0 &&
      section.recoveryLifecycle !== "blocked-oversize" &&
      section.recoveryLifecycle !== "blocked-conflict"
    ) {
      section.recoveryLifecycle = "requested";
    }
    if (!isInterruptionReason(reason) && reason === "user" && section === active) {
      sinks.cue("stop");
    }
    changed();
    await maybeClose(section);
  }

  async function maybeClose(section: SectionSession): Promise<void> {
    if (
      disposed ||
      section.authenticationSuspended ||
      section.captureLifecycle.kind !== "ended" ||
      section.delivery.pendingCount ||
      section.closeLifecycle.kind === "closed" ||
      section.closeLifecycle.kind === "closing" ||
      section.recoveryLifecycle === "blocked-oversize" ||
      section.recoveryLifecycle === "blocked-conflict"
    ) {
      return;
    }
    if (isInterrupted(section) && section.recoveryLifecycle !== "requested") return;
    const command: CloseCommand =
      section.closeLifecycle.kind === "failed"
        ? section.closeLifecycle.command
        : isInterrupted(section)
          ? { kind: "interrupt" }
          : { kind: "complete", frameCount: section.assignedFrames };
    await close(section, command);
  }

  async function close(section: SectionSession, command: CloseCommand): Promise<void> {
    if (
      section.closeLifecycle.kind === "closing" ||
      section.closeLifecycle.kind === "closed" ||
      disposed
    ) {
      return;
    }
    section.closeLifecycle = { kind: "closing", command };
    changed();
    try {
      const result =
        command.kind === "complete"
          ? await gateway.complete(section.created.audioAssetId, command.frameCount)
          : await gateway.interrupt(section.created.audioAssetId);
      if (disposed) return;
      section.closeLifecycle = result.isSealed
        ? { kind: "closed", command, result }
        : {
            kind: "failed",
            command,
            problem:
              command.kind === "complete"
                ? "Could not complete recording"
                : "Could not close recording",
          };
      if (result.isSealed) section.problem = "";
      write(section, command.kind === "complete" ? "complete sent" : "interrupt sent");
    } catch (error) {
      if (disposed) return;
      section.closeLifecycle = {
        kind: "failed",
        command,
        problem:
          command.kind === "complete"
            ? `Could not complete recording: ${errorMessage(error)}`
            : `Could not close recording: ${errorMessage(error)}`,
      };
      write(section, `ERR ${section.closeLifecycle.problem}`);
    } finally {
      if (!disposed) changed();
    }
  }

  function sectionSnapshot(section: SectionSession): SectionSnapshot {
    const unfinalizedCapturedMs =
      section.captureLifecycle.kind === "capturing" ||
      section.captureLifecycle.kind === "finalizing"
        ? Math.max(0, captureElapsedMs(section.captureLifecycle) - section.lastFrameEndMs)
        : 0;
    const pendingFrames = section.delivery.pendingCount;
    const completion = completionStatus(section);
    const completionProblem = completion === "failed" ? completionProblemText(section) : "";
    const completionFailure =
      section.closeLifecycle.kind === "failed" &&
      section.closeLifecycle.command.kind === "complete";
    const closeProblem =
      section.closeLifecycle.kind === "failed" &&
      section.closeLifecycle.command.kind === "interrupt"
        ? section.closeLifecycle.problem
        : "";
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
      hasUnsecuredAudio: hasUnsecuredAudio(section),
      completion,
      completionProblem,
      canRetryCompletion: completionFailure,
      problem:
        section.problem ||
        (section.delivery.delayed
          ? section.captureLifecycle.kind === "capturing"
            ? "Audio delivery is delayed. Capture continues."
            : "Audio delivery is delayed. Capture has stopped."
          : ""),
      canUploadPending:
        section.captureLifecycle.kind === "ended" &&
        pendingFrames > 0 &&
        section.recoveryLifecycle !== "blocked-oversize" &&
        section.recoveryLifecycle !== "blocked-conflict",
      closeProblem,
      canRetryClose: !!closeProblem,
    };
  }

  function completionStatus(section: SectionSession): CompletionStatus | null {
    const capture = section.captureLifecycle;
    if (
      capture.kind === "preparing" ||
      capture.kind === "capturing" ||
      isInterruptionReason(capture.reason)
    ) {
      return null;
    }
    const closing = section.closeLifecycle;
    if (closing.kind === "open" || closing.kind === "closing") return "pending";
    if (closing.command.kind !== "complete") return null;
    if (closing.kind === "failed") return "failed";
    return closing.result.isSealed && closing.result.sealReason === "completed"
      ? "succeeded"
      : "failed";
  }

  function completionProblemText(section: SectionSession): string {
    const closing = section.closeLifecycle;
    if (closing.kind === "failed" && closing.command.kind === "complete") {
      return "Could not complete recording.";
    }
    if (closing.kind === "closed" && closing.command.kind === "complete") {
      return closing.result.sealReason === "interrupted"
        ? "Recording was sealed as interrupted and cannot be completed."
        : "Recording was sealed without normal completion and cannot be completed.";
    }
    return "";
  }

  async function start(): Promise<void> {
    if (!currentSnapshot().canStart) return;
    starting = true;
    startProblem = "";
    changed();
    let capture: MicrophoneCapture | null = null;
    let createdSession: SectionSession | null = null;
    try {
      capture = await environment.openCapture({
        intervalMs: config.frameIntervalMs,
        canContinue: canContinueCapture,
        now: environment.now,
        onError: (error) => {
          const current = active;
          if (current?.captureLifecycle.kind === "capturing") {
            write(current, `ERR ${error.message}`);
            void stopCapture(current, "capture");
          }
        },
      });
      if (!startMayContinue(capture)) return;
      const projectId = await ensureProject();
      if (!startMayContinue(capture)) return;
      sectionCreationKey ??= uuid();
      const created = await gateway.createSection(projectId, sectionCreationKey);
      sectionCreationKey = null;
      if (disposed) {
        capture.dispose();
        return;
      }

      // The delivery needs its section, and the section needs its delivery.
      const holder: { section?: SectionSession } = {};
      const section: SectionSession = {
        created,
        capture,
        delivery: createDelivery(() => holder.section!),
        captureLifecycle: { kind: "preparing" },
        recoveryLifecycle: "automatic",
        closeLifecycle: { kind: "open" },
        lastFrameEndMs: 0,
        securedMs: 0,
        assignedFrames: 0,
        problem: "",
        authenticationSuspended: false,
      };
      holder.section = section;
      createdSession = section;
      sections.push(section);
      sections.sort((left, right) => left.created.position - right.created.position);
      active = section;
      starting = false;
      if (authenticationSuspended) {
        capture.dispose();
        section.authenticationSuspended = true;
        section.captureLifecycle = { kind: "ended", reason: "capture", elapsedMs: 0 };
        section.recoveryLifecycle = "requested";
        section.problem = "Capture did not start because its Application Session ended.";
        changed();
        await maybeClose(section);
        return;
      }

      const startedAt = environment.now();
      const progressTimer = setInterval(() => {
        if (section.captureLifecycle.kind !== "capturing") return;
        section.captureLifecycle.elapsedMs = environment.now() - section.captureLifecycle.startedAt;
        changed();
      }, 100);
      const limitTimer = setTimeout(
        () => void stopCapture(section, "duration"),
        config.maxRecordingMs,
      );
      section.captureLifecycle = {
        kind: "capturing",
        startedAt,
        elapsedMs: 0,
        progressTimer,
        limitTimer,
      };
      await capture.start((frame) => acceptFrame(section, frame));
      if (disposed || section.captureLifecycle.kind !== "capturing") return;
      sinks.cue("start");
      write(section, `section ${created.position + 1} ${created.audioAssetId.slice(0, 8)}`);
      changed();
    } catch (error) {
      capture?.dispose();
      if (disposed) return;
      if (createdSession) {
        clearCaptureTimers(createdSession.captureLifecycle);
        createdSession.captureLifecycle = { kind: "ended", reason: "capture", elapsedMs: 0 };
        createdSession.recoveryLifecycle = "requested";
        createdSession.problem = `Capture could not begin: ${errorMessage(error)}`;
        void maybeClose(createdSession);
      }
      starting = false;
      startProblem = `Could not start capture: ${errorMessage(error)}`;
      sinks.log(`0.0 ERR ${startProblem}`);
      changed();
    }
  }

  if (context.projectId) {
    void loadExistingProject().catch((error: unknown) => {
      if (disposed) return;
      startProblem = `Could not load Project: ${errorMessage(error)}`;
      changed();
    });
  }
  changed();

  return {
    snapshot,
    context,
    start,
    stop: () => (active ? stopCapture(active, "user") : Promise.resolve()),
    uploadPendingAudio(sectionId) {
      const section = findSection(sectionId);
      if (
        !section ||
        section.captureLifecycle.kind !== "ended" ||
        !section.delivery.pendingCount ||
        section.recoveryLifecycle === "blocked-oversize" ||
        section.recoveryLifecycle === "blocked-conflict"
      ) {
        return;
      }
      if (isInterrupted(section)) section.recoveryLifecycle = "requested";
      section.problem = "";
      section.delivery.retry();
      changed();
    },
    retryClose(sectionId) {
      const section = findSection(sectionId);
      if (!section || section.closeLifecycle.kind !== "failed") return;
      void close(section, section.closeLifecycle.command);
    },
    suspendForAuthentication() {
      if (authenticationSuspended) return;
      authenticationSuspended = true;
      for (const section of sections) {
        section.authenticationSuspended = true;
        section.delivery.freeze();
      }
      changed();
    },
    resumeAfterAuthentication(accountId) {
      if (!authenticationSuspended || accountId !== context.accountId) return;
      authenticationSuspended = false;
      for (const section of sections) {
        section.authenticationSuspended = false;
        if (
          section.delivery.pendingCount &&
          (section.recoveryLifecycle === "automatic" || section.recoveryLifecycle === "requested")
        ) {
          section.delivery.retry();
        }
        void maybeClose(section);
      }
      changed();
    },
    dispose() {
      disposed = true;
      starting = false;
      for (const section of sections) {
        clearCaptureTimers(section.captureLifecycle);
        section.delivery.dispose();
        section.capture?.dispose();
      }
    },
  };
}

function isInterruptionReason(reason: StopReason): boolean {
  return (
    reason === "backlog" || reason === "capture" || reason === "oversize" || reason === "conflict"
  );
}

function captureElapsedMs(lifecycle: CaptureLifecycle): number {
  return lifecycle.kind === "preparing" ? 0 : lifecycle.elapsedMs;
}

export function localizedProjectTitle(date: Date, locale?: string): string {
  return `Draft — ${new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
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
  return [
    "Capture stopped because audio delivery conflicted with stored data. Audio remains in this page.",
    ...details,
  ].join(" ");
}
