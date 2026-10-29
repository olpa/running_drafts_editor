import { computed, inject, shallowRef, watch, type InjectionKey, type Ref } from "vue";
import { RadarLog } from "../shared/radar-log.ts";
import type { AccountId } from "../session/gateway.ts";
import type { Cues } from "./cues.ts";
import {
  createProjectRecorder,
  type ProjectRecorder,
  type RecordingConfig,
  type RecordingEnvironment,
  type RecordingSnapshot,
} from "./project-recorder.ts";

export interface RecordingOptions {
  config: RecordingConfig & { logColumns: number; logRows: number; logGapChars: number };
  environment: RecordingEnvironment;
  cues: Cues;
  /** Decide whether the Application Session may start a recording now. */
  authorizeStart: () => boolean;
  /** Existing Project to extend instead of creating a new one. */
  projectId?: string;
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
};

/**
 * Page-lifetime recording. It outlives every view, so retained audio is never lost by
 * navigation, and it follows the signed-in Account.
 */
export interface Recording {
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
  setSessionAccount(accountId: AccountId | null): void;
  /** Stop when capturing; otherwise start if the Session and Account allow it. */
  toggle(): void;
  uploadPendingAudio(sectionId: string): void;
  retryClose(sectionId: string): void;
  clearLog(): void;
  /** Permanently finish the module. A temporary view removal must not call this. */
  dispose(): void;
}

export function createRecording(options: RecordingOptions): Recording {
  const { config, environment, cues } = options;
  const ownershipWarning = shallowRef("");
  const recorder = shallowRef<ProjectRecorder | null>(null);
  const moduleAccountId = shallowRef<AccountId | null>(null);
  const sessionAccountId = shallowRef<AccountId | null>(null);
  const logView = shallowRef<LogView>({ lines: [], cursor: 0 });
  let log = new RadarLog(config.logColumns, config.logRows, config.logGapChars);
  let stopHandover: (() => void) | null = null;

  const snapshot = computed(() => recorder.value?.snapshot.value ?? IDLE_SNAPSHOT);
  const hasRecorder = computed(() => recorder.value !== null);
  const hasRetainedWork = computed(() => snapshot.value.hasPendingWork);
  const hasUnsecuredAudio = computed(() => snapshot.value.hasUnsecuredAudio);
  const ownsCurrentSession = computed(
    () => !!moduleAccountId.value && moduleAccountId.value === sessionAccountId.value,
  );

  function publishLog(): void {
    logView.value = log.view();
  }

  function clearLog(): void {
    log = new RadarLog(config.logColumns, config.logRows, config.logGapChars);
    publishLog();
  }

  function startRecorder(accountId: AccountId): void {
    moduleAccountId.value = accountId;
    const context = options.projectId ? { accountId, projectId: options.projectId } : { accountId };
    const created: ProjectRecorder = createProjectRecorder(config, environment, context, {
      cue(cue) {
        cues.play(cue);
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
    setSessionAccount(accountId) {
      sessionAccountId.value = accountId;
      const current = recorder.value;
      if (!current) {
        if (accountId) startRecorder(accountId);
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
      if (accountId) startRecorder(accountId);
    },
    toggle() {
      const current = recorder.value;
      if (snapshot.value.capturing) {
        void current?.stop();
        return;
      }
      if (!options.authorizeStart() || !ownsCurrentSession.value) return;
      cues.prepare();
      void current?.start();
    },
    uploadPendingAudio(sectionId) {
      if (ownsCurrentSession.value) recorder.value?.uploadPendingAudio(sectionId);
    },
    retryClose(sectionId) {
      if (ownsCurrentSession.value) recorder.value?.retryClose(sectionId);
    },
    clearLog,
    dispose() {
      resetRecorder();
      cues.dispose();
    },
  };
}

export const recordingKey: InjectionKey<Recording> = Symbol("recording");

export function useRecording(): Recording {
  const recording = inject(recordingKey);
  if (!recording) throw new Error("The Recording module was not provided.");
  return recording;
}
