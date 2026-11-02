import { computed, shallowRef, type Ref } from "vue";
import type { ProjectSummary } from "../shared/project-types.ts";
import type { CaptureOptions, MicrophoneCapture } from "./capture.ts";
import {
  createCaptureAndDelivery,
  type CaptureAndDelivery,
  type CaptureConfig,
  type OverdueFrame,
} from "./capture-and-delivery.ts";
import { browserScheduler, type Scheduler } from "./frame-timing.ts";
import type { CreatedSection, RecordingGateway } from "./gateway.ts";
import { createMicrophoneAccess, type MicrophoneAccess } from "./microphone-access.ts";
import type { SectionEvent } from "./section-events.ts";
import type { CompletionStatus } from "./section-delivery.ts";

export interface ProjectRecordingContext {
  accountId: string;
  projectId?: string;
}

export type { CompletionStatus, CaptureConfig };

/** Browser and clock seams of a recording run; production uses real adapters. */
export interface RecordingEnvironment {
  gateway: RecordingGateway;
  openCapture: (options: CaptureOptions) => Promise<MicrophoneCapture>;
  /** Monotonic elapsed-time clock. */
  now: () => number;
  scheduler?: Scheduler;
  microphoneAccess?: MicrophoneAccess;
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
  readonly overdueFrames: readonly OverdueFrame[];
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
  event(event: SectionEvent): void;
  log(message: string): void;
}

/** Everything the page owns about recording one Project for one Account. */
export interface Project {
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
  readonly captureAndDelivery: CaptureAndDelivery;
  readonly snapshot: Readonly<Ref<SectionSnapshot>>;
}
interface CreationAttempt {
  readonly key: string;
  readonly title: string;
}

/** Project identity and ordered Sections. Section workflows own all Capture and delivery. */
export function createProject(
  config: CaptureConfig,
  environment: RecordingEnvironment,
  context: Readonly<ProjectRecordingContext>,
  sinks: RecordingSinks,
): Project {
  const { gateway } = environment;
  const scheduler = environment.scheduler ?? { ...browserScheduler, now: environment.now };
  const microphoneAccess = environment.microphoneAccess ?? createMicrophoneAccess();
  const uuid = environment.uuid ?? (() => crypto.randomUUID());
  const wallClock = environment.wallClock ?? (() => new Date());
  const project = shallowRef<ProjectSummary | null>(null);
  const sections = shallowRef<readonly Section[]>([]);
  const active = shallowRef<Section | null>(null);
  const starting = shallowRef(false);
  const startProblem = shallowRef("");
  const authenticationSuspended = shallowRef(false);
  const disposed = shallowRef(false);
  let preparing: CaptureAndDelivery | null = null;
  let projectLoad: Promise<ProjectSummary> | null = null;
  let projectCreation: CreationAttempt | null = null;
  let sectionCreationKey: string | null = null;
  const canStart = computed(
    () =>
      !disposed.value &&
      !authenticationSuspended.value &&
      !starting.value &&
      !active.value?.snapshot.value.capturing,
  );
  const snapshot = computed<RecordingSnapshot>(() => {
    const views = sections.value.map((section) => section.snapshot.value);
    return {
      project: project.value,
      starting: starting.value,
      canStart: canStart.value,
      capturing: active.value?.snapshot.value.capturing ?? false,
      elapsedMs: active.value?.snapshot.value.elapsedMs ?? 0,
      hasUnsecuredAudio: views.some((view) => view.hasUnsecuredAudio),
      hasPendingWork:
        starting.value || views.some((view) => view.hasUnsecuredAudio || view.awaitingSeal),
      startProblem: startProblem.value,
      sections: views,
    };
  });
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

  async function start(): Promise<void> {
    if (!canStart.value) return;
    starting.value = true;
    startProblem.value = "";
    let identity: CreatedSection | null = null;
    const workflow = createCaptureAndDelivery({
      config,
      gateway,
      openCapture: environment.openCapture,
      scheduler,
      microphoneAccess,
      log(message) {
        if (!disposed.value)
          sinks.log(
            `${(workflow.view.value.elapsedMs / 1000).toFixed(1)} s${identity ? identity.position + 1 : "?"} ${message}`,
          );
      },
      onEvent: sinks.event,
      onCreated(created) {
        identity = created;
        const section: Section = {
          created,
          captureAndDelivery: workflow,
          snapshot: computed(() => ({
            ...workflow.view.value,
            sectionId: created.sectionId,
            audioAssetId: created.audioAssetId,
            position: created.position,
          })),
        };
        sections.value = [...sections.value, section].sort(
          (a, b) => a.created.position - b.created.position,
        );
        active.value = section;
      },
    });
    preparing = workflow;
    try {
      await workflow.start(async () => {
        const projectId = await ensureProject();
        if (disposed.value || authenticationSuspended.value)
          throw new Error("Capture start was cancelled.");
        sectionCreationKey ??= uuid();
        const created = await gateway.createSection(projectId, sectionCreationKey);
        sectionCreationKey = null;
        return created;
      });
    } catch (error) {
      if (!identity) workflow.dispose();
      if (!disposed.value) {
        startProblem.value = `Could not start capture: ${errorMessage(error)}`;
        sinks.log(`0.0 ERR ${startProblem.value}`);
      }
    } finally {
      if (preparing === workflow) preparing = null;
      starting.value = false;
    }
  }
  if (context.projectId) {
    void loadExistingProject().catch((error: unknown) => {
      if (!disposed.value) startProblem.value = `Could not load Project: ${errorMessage(error)}`;
    });
  }
  return {
    snapshot,
    context,
    start,
    stop: () => active.value?.captureAndDelivery.stop() ?? Promise.resolve(),
    uploadPendingAudio(sectionId) {
      sections.value
        .find((section) => section.created.sectionId === sectionId)
        ?.captureAndDelivery.uploadPendingAudio();
    },
    retryClose(sectionId) {
      sections.value
        .find((section) => section.created.sectionId === sectionId)
        ?.captureAndDelivery.retryClose();
    },
    suspendForAuthentication() {
      authenticationSuspended.value = true;
      preparing?.suspend();
      for (const section of sections.value) section.captureAndDelivery.suspend();
    },
    resumeAfterAuthentication(accountId) {
      if (!authenticationSuspended.value || accountId !== context.accountId) return;
      authenticationSuspended.value = false;
      preparing?.resume();
      for (const section of sections.value) section.captureAndDelivery.resume();
    },
    dispose() {
      disposed.value = true;
      preparing?.dispose();
      for (const section of sections.value) section.captureAndDelivery.dispose();
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
