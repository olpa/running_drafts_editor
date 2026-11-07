import {
  captureAlertsKey,
  createCaptureAlerts,
  type CaptureAlerts,
} from "../alerts/capture-alerts.ts";
import {
  allowanceFeedbackKey,
  createAllowanceFeedback,
  type AllowanceFeedback,
} from "../alerts/allowance-feedback.ts";
import {
  createMicrophoneAccess,
  microphoneAccessKey,
  type MicrophoneAccess,
} from "../recording/microphone-access.ts";
import { createApp, watch, type App } from "vue";
import { createHealth, healthKey } from "../health/health.ts";
import { createHealthGateway, type HealthGateway } from "../health/gateway.ts";
import { createProjectsGateway, type ProjectsGateway } from "../projects/gateway.ts";
import { createProjects, projectsKey, type Projects } from "../projects/projects.ts";
import { openMicrophoneCapture } from "../recording/capture.ts";
import { CuePlayer, type Cues } from "../recording/cues.ts";
import { createRecordingGateway, type RecordingGateway } from "../recording/gateway.ts";
import type { RecordingEnvironment } from "../recording/project.ts";
import {
  createApplicationCapture,
  applicationCaptureKey,
  type ApplicationCapture,
} from "../recording/application-capture.ts";
import { createSessionGateway, type SessionGateway } from "../session/gateway.ts";
import {
  browserLocation,
  createSession,
  sessionKey,
  type Session,
  type SessionLocation,
} from "../session/session.ts";
import { configKey, loadConfig, type SeedConfig } from "../shared/config.ts";
import { createApiRequester } from "../shared/http.ts";
import { createUsageGateway, type UsageGateway } from "../usage/gateway.ts";
import { createUsage, usageKey, type Usage } from "../usage/usage.ts";
import AppShell from "./AppShell.vue";

/** Replaceable external adapters; every one defaults to its production implementation. */
export interface ApplicationAdapters {
  config?: SeedConfig;
  location?: SessionLocation;
  sessionGateway?: SessionGateway;
  recordingGateway?: RecordingGateway;
  projectsGateway?: ProjectsGateway;
  healthGateway?: HealthGateway;
  environment?: Partial<Omit<RecordingEnvironment, "gateway">>;
  cues?: Cues;
  microphoneAccess?: MicrophoneAccess;
  usageGateway?: UsageGateway;
  confirmDiscardPending?: () => boolean;
}

export interface Application {
  readonly app: App;
  readonly session: Session;
  readonly recording: ApplicationCapture;
  readonly projects: Projects;
  readonly alerts: CaptureAlerts;
  readonly microphoneAccess: MicrophoneAccess;
  readonly usage: Usage;
  readonly allowanceFeedback: AllowanceFeedback;
}

/** The composition root: wires adapters into modules and provides them to components. */
export function createApplication(adapters: ApplicationAdapters = {}): Application {
  const config = adapters.config ?? loadConfig();
  const session = createSession(
    adapters.sessionGateway ?? createSessionGateway(),
    adapters.location ?? browserLocation(),
    {
      confirmExplicitLogout: () =>
        !recording?.hasRetainedWork.value ||
        (
          adapters.confirmDiscardPending ??
          (() =>
            window.confirm(
              "Logging out will permanently discard pending audio on this page. Log out?",
            ))
        )(),
      onExplicitLogout: () => {
        recording?.discardAll();
        alerts?.acknowledgeAll();
        allowanceFeedback?.silence();
        usage?.setAccount(null);
        projects?.clear();
      },
    },
  );
  const api = createApiRequester({
    onAuthenticationExpired: () => session.authenticationExpired(),
  });
  const cues = adapters.cues ?? new CuePlayer();
  const usage = createUsage(adapters.usageGateway ?? createUsageGateway(api));
  const allowanceFeedback = createAllowanceFeedback(cues, adapters.environment?.scheduler);
  const alerts = createCaptureAlerts(cues);
  const microphoneAccess = adapters.microphoneAccess ?? createMicrophoneAccess();
  const recording = createApplicationCapture({
    config,
    environment: {
      gateway: adapters.recordingGateway ?? createRecordingGateway(api),
      openCapture: openMicrophoneCapture,
      now: () => performance.now(),
      microphoneAccess,
      ...adapters.environment,
    },
    cues,
    alerts,
    authorizeStart: () => session.authorizeRecordingStart(),
    usage,
    allowanceFeedback,
    onAccountSuspended: () =>
      session.requireLogin("Your Account is unavailable. Contact support for help."),
  });
  const projects = createProjects(adapters.projectsGateway ?? createProjectsGateway(api));
  const health = createHealth(adapters.healthGateway ?? createHealthGateway());

  // Private state follows the Account: recording is retained, everything else is dropped.
  watch(
    session.state,
    (state) => {
      if (state.status === "loading") return;
      if (state.status === "signedIn") {
        api.markAuthenticationUsable();
        usage.setAccount(state.session.account.id);
        recording.setSessionAccount(state.session.account.id);
        return;
      }
      recording.setSessionAccount(null);
      usage.setAccount(null);
      projects.clear();
      recording.clearLog();
    },
    { immediate: true, flush: "sync" },
  );

  const app = createApp(AppShell);
  app.onUnmount(() => {
    recording.dispose();
    alerts.dispose();
    microphoneAccess.dispose();
    cues.dispose();
    usage.dispose();
    allowanceFeedback.dispose();
  });
  app.provide(captureAlertsKey, alerts);
  app.provide(allowanceFeedbackKey, allowanceFeedback);
  app.provide(microphoneAccessKey, microphoneAccess);
  app.provide(configKey, config);
  app.provide(sessionKey, session);
  app.provide(applicationCaptureKey, recording);
  app.provide(projectsKey, projects);
  app.provide(usageKey, usage);
  app.provide(healthKey, health);
  return {
    app,
    session,
    recording,
    projects,
    alerts,
    microphoneAccess,
    usage,
    allowanceFeedback,
  };
}
