import { createApp, watch, type App } from "vue";
import { createHealth, healthKey } from "../health/health.ts";
import { createHealthGateway, type HealthGateway } from "../health/gateway.ts";
import { createProjectsGateway, type ProjectsGateway } from "../projects/gateway.ts";
import { createProjects, projectsKey, type Projects } from "../projects/projects.ts";
import { openMicrophoneCapture } from "../recording/capture.ts";
import { CuePlayer, type Cues } from "../recording/cues.ts";
import { createRecordingGateway, type RecordingGateway } from "../recording/gateway.ts";
import type { RecordingEnvironment } from "../recording/project-recorder.ts";
import { createRecording, recordingKey, type Recording } from "../recording/recording.ts";
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
}

export interface Application {
  readonly app: App;
  readonly session: Session;
  readonly recording: Recording;
  readonly projects: Projects;
}

/** The composition root: wires adapters into modules and provides them to components. */
export function createApplication(adapters: ApplicationAdapters = {}): Application {
  const config = adapters.config ?? loadConfig();
  const session = createSession(
    adapters.sessionGateway ?? createSessionGateway(),
    adapters.location ?? browserLocation(),
  );
  const api = createApiRequester({
    onAuthenticationExpired: () => session.authenticationExpired(),
  });
  const recording = createRecording({
    config,
    environment: {
      gateway: adapters.recordingGateway ?? createRecordingGateway(api),
      openCapture: openMicrophoneCapture,
      now: () => performance.now(),
      ...adapters.environment,
    },
    cues: adapters.cues ?? new CuePlayer(),
    authorizeStart: () => session.authorizeRecordingStart(),
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
        recording.setSessionAccount(state.session.account.id);
        return;
      }
      recording.setSessionAccount(null);
      projects.clear();
      recording.clearLog();
    },
    { immediate: true, flush: "sync" },
  );

  const app = createApp(AppShell);
  app.provide(configKey, config);
  app.provide(sessionKey, session);
  app.provide(recordingKey, recording);
  app.provide(projectsKey, projects);
  app.provide(healthKey, health);
  return { app, session, recording, projects };
}
