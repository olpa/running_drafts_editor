import { computed, inject, shallowRef, watch, type InjectionKey, type Ref } from "vue";
import { RadarLog } from "../shared/radar-log.ts";
import type { AccountId } from "../session/gateway.ts";
import type { CaptureAlerts } from "../alerts/capture-alerts.ts";
import { createCaptureFeedback } from "./capture-feedback.ts";
import type { Cues } from "./cues.ts";
import type { AllowanceFeedback } from "../alerts/allowance-feedback.ts";
import type { Usage } from "../usage/usage.ts";
import type { UsageSnapshot } from "../usage/gateway.ts";
import type { SectionEvent } from "./section-events.ts";
import {
  createProject,
  type Project,
  type CaptureConfig,
  type RecordingEnvironment,
  type RecordingSnapshot,
} from "./project.ts";

export interface ApplicationCaptureOptions {
  config: CaptureConfig & { logColumns: number; logRows: number; logGapChars: number };
  environment: RecordingEnvironment;
  cues: Cues;
  alerts: CaptureAlerts;
  onEvent?: (sectionId: string, event: SectionEvent) => void;
  /** Decide whether the Application Session may start a recording now. */
  authorizeStart: () => boolean;
  /** Existing Project to extend instead of creating a new one. */
  projectId?: string;
  usage?: Usage;
  allowanceFeedback?: AllowanceFeedback;
  onAccountSuspended?: () => void;
}

/** The text view of the diagnostic log. */
export interface LogView {
  lines: string[];
  cursor: number;
}

const IDLE_SNAPSHOT: RecordingSnapshot = {
  project: null,
  starting: false,
  canStart: false,
  capturing: false,
  elapsedMs: 0,
  hasUnsecuredAudio: false,
  hasPendingWork: false,
  startProblem: "",
  sections: [],
  capturedBytes: 0,
  finalizingCapture: false,
};

export interface AllowanceWarningView {
  readonly storageRemainingBytes: number | null;
  readonly durationRemainingMs: number | null;
}

/**
 * Page-lifetime recording. It outlives every view, so retained audio is never lost by
 * navigation, and it follows the signed-in Account.
 */
export interface ApplicationCapture {
  readonly snapshot: Readonly<Ref<RecordingSnapshot>>;
  readonly ownershipWarning: Readonly<Ref<string>>;
  readonly logView: Readonly<Ref<LogView>>;
  /** A recorder exists for a signed-in Account; otherwise the panel is idle. */
  readonly hasRecorder: Readonly<Ref<boolean>>;
  /** Pending work exists, so the workspace must stay reachable. */
  readonly hasRetainedWork: Readonly<Ref<boolean>>;
  readonly hasUnsecuredAudio: Readonly<Ref<boolean>>;
  /** The signed-in Account owns the recorder in memory. */
  readonly ownsCurrentSession: Readonly<Ref<boolean>>;
  readonly focused: Readonly<Ref<boolean>>;
  readonly allowanceWarnings: Readonly<Ref<AllowanceWarningView>>;
  /** Prepare an idle modal workspace for a new or existing Project. */
  prepare(projectId?: string): boolean;
  /** Release an idle modal workspace. Active or pending work refuses release. */
  release(): boolean;
  setSessionAccount(accountId: AccountId | null): void;
  /** Stop when capturing; otherwise start if the Session and Account allow it. */
  toggle(): void;
  uploadPendingAudio(sectionId: string): void;
  retryClose(sectionId: string): void;
  clearLog(): void;
  /** Confirmed explicit logout permanently releases all page-local recording work. */
  discardAll(): void;
  /** Permanently finish the module. A temporary view removal must not call this. */
  dispose(): void;
}

export function createApplicationCapture(options: ApplicationCaptureOptions): ApplicationCapture {
  const { config, environment, cues } = options;
  const feedback = createCaptureFeedback(cues, options.alerts);
  const ownershipWarning = shallowRef("");
  const recorder = shallowRef<Project | null>(null);
  const moduleAccountId = shallowRef<AccountId | null>(null);
  const sessionAccountId = shallowRef<AccountId | null>(null);
  const logView = shallowRef<LogView>({ lines: [], cursor: 0 });
  let log = new RadarLog(config.logColumns, config.logRows, config.logGapChars);
  let stopHandover: (() => void) | null = null;
  // Keep an idle unbound recorder ready for existing module consumers. Preparing the
  // Capture modal replaces it with the selected Project context before any audio starts.
  let workspaceOpen = true;
  let targetProjectId = options.projectId;

  const snapshot = computed(() => recorder.value?.snapshot.value ?? IDLE_SNAPSHOT);
  const hasRecorder = computed(() => recorder.value !== null);
  const hasRetainedWork = computed(() => snapshot.value.hasPendingWork);
  const hasUnsecuredAudio = computed(() => snapshot.value.hasUnsecuredAudio);
  const ownsCurrentSession = computed(
    () => !!moduleAccountId.value && moduleAccountId.value === sessionAccountId.value,
  );
  const focused = computed(
    () => snapshot.value.starting || snapshot.value.capturing || snapshot.value.finalizingCapture,
  );
  const storageRemainingBytes = shallowRef<number | null>(null);
  const durationRemainingMs = shallowRef<number | null>(null);
  const allowanceWarnings = computed<AllowanceWarningView>(() => ({
    storageRemainingBytes: storageRemainingBytes.value,
    durationRemainingMs: durationRemainingMs.value,
  }));
  let lastUsageSnapshot: Readonly<UsageSnapshot> | null | undefined;

  if (options.usage) {
    watch(
      options.usage.snapshot,
      (usage) => {
        if (usage === lastUsageSnapshot) return;
        lastUsageSnapshot = usage;
        if (!usage) {
          storageRemainingBytes.value = null;
          durationRemainingMs.value = null;
          return;
        }
        storageRemainingBytes.value =
          usage.audioStorage.usedBytes >= usage.audioStorage.warningAtBytes
            ? Math.max(0, usage.audioStorage.limitBytes - usage.audioStorage.usedBytes)
            : null;
      },
      { immediate: true, flush: "sync" },
    );
    watch(
      snapshot,
      (current, previous) => {
        const usage = options.usage?.snapshot.value;
        if (!usage) {
          durationRemainingMs.value = null;
          return;
        }
        const durationWarned =
          current.capturing && current.elapsedMs >= usage.audioAssetDuration.warningAtMs;
        if (durationWarned) {
          if (durationRemainingMs.value === null && (previous?.capturing ?? false)) {
            options.allowanceFeedback?.play("duration");
          }
          durationRemainingMs.value = Math.max(
            0,
            usage.audioAssetDuration.limitMs - current.elapsedMs,
          );
        } else {
          durationRemainingMs.value = null;
        }
      },
      { flush: "sync" },
    );
  }

  function publishLog(): void {
    logView.value = log.view();
  }

  function clearLog(): void {
    log = new RadarLog(config.logColumns, config.logRows, config.logGapChars);
    publishLog();
  }

  function startRecorder(accountId: AccountId): void {
    moduleAccountId.value = accountId;
    const context = targetProjectId ? { accountId, projectId: targetProjectId } : { accountId };
    const created: Project = createProject(config, environment, context, {
      event(sectionId, event) {
        if (event.type === "transportFrameCaptured") {
          const usage = options.usage?.snapshot.value;
          if (usage) {
            const estimate = usage.audioStorage.usedBytes + event.totalBytes;
            if (estimate >= usage.audioStorage.warningAtBytes) {
              if (storageRemainingBytes.value === null && event.captureActive) {
                options.allowanceFeedback?.play("storage");
              }
              storageRemainingBytes.value = Math.max(0, usage.audioStorage.limitBytes - estimate);
            }
          }
        }
        if (event.type === "allowanceRejected") void options.usage?.refresh();
        if (event.type === "accountSuspended") options.onAccountSuspended?.();
        feedback(sectionId, event);
        options.onEvent?.(sectionId, event);
      },
      log(message) {
        if (!ownsCurrentSession.value) return;
        log.write(message);
        publishLog();
      },
    });
    // Once a former Account's work is finished, hand the module to the signed-in Account.
    stopHandover = watch(
      () => created.snapshot.value.hasPendingWork,
      (pending) => {
        if (
          pending ||
          recorder.value !== created ||
          !moduleAccountId.value ||
          sessionAccountId.value === moduleAccountId.value
        ) {
          return;
        }
        const next = sessionAccountId.value;
        resetRecorder();
        if (next) startRecorder(next);
      },
      { flush: "sync" },
    );
    recorder.value = created;
  }

  function resetRecorder(): void {
    stopHandover?.();
    stopHandover = null;
    recorder.value?.dispose();
    recorder.value = null;
    moduleAccountId.value = null;
    ownershipWarning.value = "";
  }

  clearLog();

  return {
    snapshot,
    ownershipWarning,
    logView,
    hasRecorder,
    hasRetainedWork,
    hasUnsecuredAudio,
    ownsCurrentSession,
    focused,
    allowanceWarnings,
    prepare(projectId) {
      if (focused.value || hasRetainedWork.value) return false;
      workspaceOpen = true;
      targetProjectId = projectId;
      resetRecorder();
      if (sessionAccountId.value) startRecorder(sessionAccountId.value);
      return true;
    },
    release() {
      if (focused.value || hasRetainedWork.value) return false;
      workspaceOpen = false;
      targetProjectId = undefined;
      resetRecorder();
      return true;
    },
    setSessionAccount(accountId) {
      sessionAccountId.value = accountId;
      const current = recorder.value;
      if (!current) {
        if (accountId && workspaceOpen) startRecorder(accountId);
        return;
      }
      if (accountId && accountId === moduleAccountId.value) {
        ownershipWarning.value = "";
        current.resumeAfterAuthentication(accountId);
        return;
      }
      if (hasRetainedWork.value) {
        ownershipWarning.value = accountId
          ? "Pending recording work belongs to a different Account. Sign in with the original Account to continue."
          : "Pending recording work is retained. Sign in with the original Account to continue.";
        current.suspendForAuthentication();
        return;
      }
      resetRecorder();
      if (accountId && workspaceOpen) startRecorder(accountId);
    },
    toggle() {
      const current = recorder.value;
      if (snapshot.value.capturing) {
        void current?.stop();
        return;
      }
      if (!options.authorizeStart() || !ownsCurrentSession.value) return;
      cues.prepare();
      void options.usage?.refresh();
      void current?.start();
    },
    uploadPendingAudio(sectionId) {
      if (ownsCurrentSession.value) {
        void options.usage?.refresh();
        recorder.value?.uploadPendingAudio(sectionId);
      }
    },
    retryClose(sectionId) {
      if (ownsCurrentSession.value) recorder.value?.retryClose(sectionId);
    },
    clearLog,
    discardAll() {
      workspaceOpen = false;
      targetProjectId = undefined;
      resetRecorder();
      clearLog();
    },
    dispose() {
      resetRecorder();
    },
  };
}

export const applicationCaptureKey: InjectionKey<ApplicationCapture> = Symbol("recording");

export function useApplicationCapture(): ApplicationCapture;
export function useApplicationCapture(optional: true): ApplicationCapture | null;
export function useApplicationCapture(optional = false): ApplicationCapture | null {
  const recording = inject(applicationCaptureKey, null);
  if (!recording && !optional) throw new Error("The ApplicationCapture module was not provided.");
  return recording;
}
