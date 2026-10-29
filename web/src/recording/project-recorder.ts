import { computed, shallowRef, type Ref } from "vue";
import type { ProjectSummary } from "../shared/project-types.ts";
import type { CaptureOptions, MicrophoneCapture, TransportFrame } from "./capture.ts";
import type { Cue } from "./cues.ts";
import type { CreatedSection, RecordingGateway } from "./gateway.ts";
import { createSectionCapture, type SectionCapture } from "./section-capture.ts";
import {
  createSectionDelivery,
  isInterruptionReason,
  type CompletionStatus,
  type SectionDelivery,
  type StopReason,
} from "./section-delivery.ts";

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

export type { CompletionStatus };

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
  /** Capture has ended but the Section is not sealed yet. */
  readonly awaitingSeal: boolean;
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
}

/** Everything the page owns about recording one Project for one Account. */
export interface ProjectRecorder {
  /** Everything the UI shows, derived from the Sections; always current. */
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

interface Section {
  readonly created: CreatedSection;
  readonly capture: SectionCapture;
  readonly delivery: SectionDelivery;
  readonly snapshot: Readonly<Ref<SectionSnapshot>>;
}

interface CreationAttempt {
  readonly key: string;
  readonly title: string;
}

export function createProjectRecorder(
  config: RecordingConfig,
  environment: RecordingEnvironment,
  context: Readonly<ProjectRecordingContext>,
  sinks: RecordingSinks,
): ProjectRecorder {
  const { gateway } = environment;
  const uuid = environment.uuid ?? (() => crypto.randomUUID());
  const wallClock = environment.wallClock ?? (() => new Date());

  const project = shallowRef<ProjectSummary | null>(null);
  const sections = shallowRef<readonly Section[]>([]);
  const active = shallowRef<Section | null>(null);
  const starting = shallowRef(false);
  const startProblem = shallowRef("");
  const authenticationSuspended = shallowRef(false);
  const disposed = shallowRef(false);
  let projectLoad: Promise<ProjectSummary> | null = null;
  let projectCreation: CreationAttempt | null = null;
  let sectionCreationKey: string | null = null;

  const canStart = computed(() => {
    const kind = active.value?.capture.state.value.kind;
    return (
      !disposed.value &&
      !authenticationSuspended.value &&
      !starting.value &&
      kind !== "capturing" &&
      kind !== "finalizing"
    );
  });

  const snapshot = computed<RecordingSnapshot>(() => {
    const views = sections.value.map((section) => section.snapshot.value);
    const activeState = active.value?.capture.state.value;
    return {
      project: project.value,
      starting: starting.value,
      canStart: canStart.value,
      capturing: activeState?.kind === "capturing",
      elapsedMs: activeState && activeState.kind !== "preparing" ? activeState.elapsedMs : 0,
      hasUnsecuredAudio: views.some((view) => view.hasUnsecuredAudio),
      hasPendingWork:
        starting.value || views.some((view) => view.hasUnsecuredAudio || view.awaitingSeal),
      startProblem: startProblem.value,
      sections: views,
    };
  });

  function write(section: Section, entry: string): void {
    if (disposed.value) return;
    const state = section.capture.state.value;
    const startedAt =
      state.kind === "capturing" || state.kind === "finalizing"
        ? state.startedAt
        : environment.now();
    const seconds = ((environment.now() - startedAt) / 1000).toFixed(1);
    sinks.log(`${seconds} s${section.created.position + 1} ${entry}`);
  }

  function isActiveCapturing(section: Section): boolean {
    return section === active.value && section.capture.state.value.kind === "capturing";
  }

  function findSection(sectionId: string): Section | undefined {
    return sections.value.find((section) => section.created.sectionId === sectionId);
  }

  function createSection(created: CreatedSection, capture: SectionCapture): Section {
    const holder: { section?: Section } = {};
    const requestStop = (reason: StopReason) => {
      const section = holder.section!;
      if (section.capture.state.value.kind === "capturing") void stopCapture(section, reason);
    };
    const delivery = createSectionDelivery({
      audioAssetId: created.audioAssetId,
      retryDelaysMs: [config.retryFirstMs, config.retrySecondMs, config.retryThirdMs],
      requestTimeoutMs: config.requestTimeoutMs,
      maxFrameBytes: config.maxFrameBytes,
      gateway,
      now: environment.now,
      log: (message) => write(holder.section!, message),
      onAcknowledged: () => {
        if (isActiveCapturing(holder.section!)) sinks.cue("health");
      },
      onDelayed: () => {
        if (isActiveCapturing(holder.section!)) sinks.cue("delayed");
      },
      requestCaptureStop: requestStop,
    });
    const view = computed<SectionSnapshot>(() => {
      const capture = holder.section!.capture.state.value;
      const delivered = delivery.view.value;
      const running = capture.kind === "capturing" || capture.kind === "finalizing";
      const elapsedMs = capture.kind === "preparing" ? 0 : capture.elapsedMs;
      const unfinalizedCapturedMs = running ? Math.max(0, elapsedMs - delivered.lastFrameEndMs) : 0;
      return {
        sectionId: created.sectionId,
        audioAssetId: created.audioAssetId,
        position: created.position,
        capturing: capture.kind === "capturing",
        finalizingCapture: capture.kind === "finalizing",
        elapsedMs,
        securedMs: delivered.securedMs,
        pendingMs: delivered.pendingMs + unfinalizedCapturedMs,
        pendingFrames: delivered.pendingFrames,
        hasUnsecuredAudio: running || delivered.pendingFrames > 0,
        awaitingSeal: delivered.awaitingSeal,
        completion: delivered.completion,
        completionProblem: delivered.completionProblem,
        canRetryCompletion: delivered.canRetryCompletion,
        problem:
          delivered.problem ||
          (delivered.delayed
            ? capture.kind === "capturing"
              ? "Audio delivery is delayed. Capture continues."
              : "Audio delivery is delayed. Capture has stopped."
            : ""),
        canUploadPending: delivered.canUploadPending,
        closeProblem: delivered.closeProblem,
        canRetryClose: delivered.canRetryClose,
      };
    });
    const section: Section = { created, capture, delivery, snapshot: view };
    holder.section = section;
    return section;
  }

  function canContinueCapture(): boolean {
    const section = active.value;
    if (!section || section.capture.state.value.kind !== "capturing" || disposed.value)
      return false;
    if (section.delivery.backlogged) {
      void stopCapture(section, "backlog");
      return false;
    }
    return true;
  }

  function startMayContinue(capture: SectionCapture): boolean {
    if (!disposed.value && !authenticationSuspended.value) return true;
    capture.dispose();
    starting.value = false;
    return false;
  }

  async function ensureProject(): Promise<string> {
    if (project.value) return project.value.id;
    if (context.projectId) return (await loadExistingProject()).id;
    projectCreation ??= {
      key: uuid(),
      title: localizedProjectTitle(wallClock(), environment.locale),
    };
    const created = await gateway.createProject(projectCreation.key, projectCreation.title);
    project.value = created;
    projectCreation = null;
    return created.id;
  }

  function loadExistingProject(): Promise<ProjectSummary> {
    if (project.value) return Promise.resolve(project.value);
    const projectId = context.projectId;
    if (!projectId) return Promise.reject(new Error("No existing Project ID was supplied."));
    if (!projectLoad) {
      const request = gateway.getProject(projectId).then((loaded) => {
        if (loaded.id !== projectId) throw new Error("The loaded Project identity did not match.");
        project.value = loaded;
        startProblem.value = "";
        return loaded;
      });
      const tracked: Promise<ProjectSummary> = request.finally(() => {
        if (projectLoad === tracked) projectLoad = null;
      });
      projectLoad = tracked;
    }
    return projectLoad;
  }

  async function stopCapture(section: Section, reason: StopReason): Promise<void> {
    if (disposed.value) return;
    const result = await section.capture.stop(reason, () => {
      write(section, `capture stop ${reason}`);
      section.delivery.captureStopping(reason);
      if (isInterruptionReason(reason)) {
        if (section === active.value) sinks.cue("interrupted");
      } else if (reason === "duration" && section === active.value) {
        sinks.cue("stop");
      }
    });
    if (result.outcome === "skipped" || disposed.value) return;
    if (result.outcome === "failed") {
      const problem = `Capture was interrupted while finalizing: ${errorMessage(result.error)}`;
      section.delivery.captureFailedToFinalize(problem);
      if (section === active.value) sinks.cue("interrupted");
      write(section, `ERR ${problem}`);
    } else {
      section.delivery.captureEnded(reason);
      if (reason === "user" && section === active.value) sinks.cue("stop");
    }
    await section.delivery.closeIfReady();
  }

  async function start(): Promise<void> {
    if (!canStart.value) return;
    starting.value = true;
    startProblem.value = "";
    let capture: SectionCapture | null = null;
    let section: Section | null = null;
    try {
      const microphone = await environment.openCapture({
        intervalMs: config.frameIntervalMs,
        canContinue: canContinueCapture,
        now: environment.now,
        onError: (error) => {
          const current = active.value;
          if (current?.capture.state.value.kind === "capturing") {
            write(current, `ERR ${error.message}`);
            void stopCapture(current, "capture");
          }
        },
      });
      capture = createSectionCapture({
        capture: microphone,
        now: environment.now,
        maxRecordingMs: config.maxRecordingMs,
        onLimit: () => {
          if (section) void stopCapture(section, "duration");
        },
      });
      if (!startMayContinue(capture)) return;
      const projectId = await ensureProject();
      if (!startMayContinue(capture)) return;
      sectionCreationKey ??= uuid();
      const created = await gateway.createSection(projectId, sectionCreationKey);
      sectionCreationKey = null;
      if (disposed.value) {
        capture.dispose();
        return;
      }

      const current = createSection(created, capture);
      section = current;
      sections.value = [...sections.value, current].sort(
        (left, right) => left.created.position - right.created.position,
      );
      active.value = current;
      starting.value = false;
      if (authenticationSuspended.value) {
        current.delivery.suspend();
        current.capture.abandon();
        current.delivery.captureNeverStarted(
          "Capture did not start because its Application Session ended.",
        );
        await current.delivery.closeIfReady();
        return;
      }

      await current.capture.start((frame) => acceptFrame(current, frame));
      if (disposed.value || current.capture.state.value.kind !== "capturing") return;
      sinks.cue("start");
      write(current, `section ${created.position + 1} ${created.audioAssetId.slice(0, 8)}`);
    } catch (error) {
      capture?.dispose();
      if (disposed.value) return;
      if (section) {
        section.capture.abandon();
        section.delivery.captureNeverStarted(`Capture could not begin: ${errorMessage(error)}`);
        void section.delivery.closeIfReady();
      }
      starting.value = false;
      startProblem.value = `Could not start capture: ${errorMessage(error)}`;
      sinks.log(`0.0 ERR ${startProblem.value}`);
    }
  }

  function acceptFrame(section: Section, frame: TransportFrame): void {
    if (disposed.value || !sections.value.includes(section)) return;
    section.delivery.accept(frame);
  }

  if (context.projectId) {
    void loadExistingProject().catch((error: unknown) => {
      if (disposed.value) return;
      startProblem.value = `Could not load Project: ${errorMessage(error)}`;
    });
  }

  return {
    snapshot,
    context,
    start,
    stop: () => (active.value ? stopCapture(active.value, "user") : Promise.resolve()),
    uploadPendingAudio(sectionId) {
      findSection(sectionId)?.delivery.requestUpload();
    },
    retryClose(sectionId) {
      findSection(sectionId)?.delivery.retryClose();
    },
    suspendForAuthentication() {
      if (authenticationSuspended.value) return;
      authenticationSuspended.value = true;
      for (const section of sections.value) section.delivery.suspend();
    },
    resumeAfterAuthentication(accountId) {
      if (!authenticationSuspended.value || accountId !== context.accountId) return;
      authenticationSuspended.value = false;
      for (const section of sections.value) section.delivery.resume();
    },
    dispose() {
      disposed.value = true;
      starting.value = false;
      for (const section of sections.value) {
        section.delivery.dispose();
        section.capture.dispose();
      }
    },
  };
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
