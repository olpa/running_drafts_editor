<script setup lang="ts">
import { defineComponent, h, provide, shallowRef, watch, type PropType } from "vue";
import { ApiError, readProblemDetails } from "../shared/http.ts";
import { RadarLog } from "../shared/radar-log.ts";
import RadarLogView from "../shared/RadarLogView.vue";
import type { ProjectDetail, ProjectsGateway } from "./gateway.ts";
import { createProjects, projectsKey, type Projects, type ProjectsScheduler } from "./projects.ts";
import SectionAudioPlayer from "./SectionAudioPlayer.vue";

const props = defineProps<{
  authError: boolean;
  missing: boolean;
  overload: boolean;
  serverFailure: boolean;
  networkFailure: boolean;
  invalidMedia: boolean;
  slowInitialLoad: boolean;
  prolongedBuffering: boolean;
}>();

const PlayerProvider = defineComponent({
  props: { projects: { type: Object as PropType<Projects>, required: true } },
  setup(providerProps) {
    provide(projectsKey, providerProps.projects);
    return () =>
      h(SectionAudioPlayer, {
        audioAssetId: "storybook-audio-asset",
        frameCount: 720,
        sectionNumber: 1,
      });
  },
});

const log = new RadarLog(80, 7, 12);
const logView = shallowRef(log.view());
const openedAt = globalThis.performance.now();
const serverToken = globalThis.crypto.randomUUID();
let scenarioNumber = 0;

const acceleratedStoryScheduler: ProjectsScheduler = {
  schedule(delayMs, callback) {
    const storyDelay = delayMs === 15_000 ? 1_000 : Math.min(delayMs, 2_500);
    const timer = globalThis.setTimeout(callback, storyDelay);
    return () => globalThis.clearTimeout(timer);
  },
};

function write(id: number, message: string): void {
  const seconds = ((globalThis.performance.now() - openedAt) / 1_000).toFixed(1);
  log.write(`${seconds} q${id} ${message}`);
  logView.value = log.view();
}

function scenarioQuery(): string {
  const query = new globalThis.URLSearchParams();
  query.set("story", serverToken);
  if (props.authError) query.set("auth", "1");
  if (props.missing) query.set("missing", "1");
  if (props.overload) query.set("overload", "1");
  if (props.serverFailure) query.set("server", "1");
  if (props.networkFailure) query.set("network", "1");
  if (props.invalidMedia) query.set("invalid", "1");
  if (props.slowInitialLoad) query.set("slow", "1");
  if (props.prolongedBuffering) query.set("buffer", "1");
  const value = query.toString();
  return value ? `?${value}` : "";
}

async function send(
  method: "HEAD" | "GET",
  url: string,
  signal: globalThis.AbortSignal,
): Promise<globalThis.Response> {
  const init: globalThis.RequestInit = {
    method,
    credentials: "include",
    redirect: "follow",
    signal,
  };
  if (method === "GET") init.headers = { Range: "bytes=0-0" };
  const response = await globalThis.fetch(url, init);
  if (response.ok) return response;
  throw new ApiError(
    method,
    "storybook playback endpoint",
    response.status,
    await readProblemDetails(response),
  );
}

function storyGateway(id: number): ProjectsGateway {
  const unused = async (): Promise<ProjectDetail> => {
    throw new Error("Project reads are outside this player-only story.");
  };
  return {
    listProjects: async () => [],
    getProjectDetail: unused,
    audioDownloadUrl: () => "#",
    async resolvePlaybackUrl(_audioAssetId, signal) {
      write(id, "HEAD latest sent");
      try {
        const response = await send(
          "HEAD",
          `/__running-drafts/audio/latest${scenarioQuery()}`,
          signal,
        );
        write(id, `HEAD latest ${response.status}; immutable snapshot retained`);
        return response.url;
      } catch (error) {
        write(id, `HEAD latest failed: ${error instanceof ApiError ? error.status : "network"}`);
        throw error;
      }
    },
    async probePlayback(url, signal) {
      write(id, "GET bytes=0-0 diagnostic sent");
      try {
        const response = await send("GET", url, signal);
        write(id, `diagnostic ${response.status}; response byte discarded`);
        return { status: response.status, referenceId: null };
      } catch (error) {
        write(id, `diagnostic failed: ${error instanceof ApiError ? error.status : "network"}`);
        throw error;
      }
    },
  };
}

const scenario = shallowRef<{ id: number; projects: Projects } | null>(null);

function describePlaybackActivity(entry: Record<string, unknown>): string {
  const facts = [`state ${String(entry.phase)}`];
  if (entry.mediaErrorCode !== undefined) facts.push(`media=${String(entry.mediaErrorCode)}`);
  if (entry.probeStatus !== undefined) facts.push(`probe=${String(entry.probeStatus)}`);
  if (entry.status !== undefined) facts.push(`status=${String(entry.status)}`);
  return facts.join(" ");
}

async function configureScenario(id: number): Promise<void> {
  try {
    const response = await globalThis.fetch(`/__running-drafts/audio/scenario${scenarioQuery()}`, {
      method: "POST",
    });
    if (!response.ok) throw new Error(`Scenario update returned ${response.status}.`);
    write(id, "mock server conditions updated");
  } catch {
    write(id, "ERR mock server conditions unavailable");
  }
}

function rebuild(): void {
  const id = ++scenarioNumber;
  write(id, "scenario reset; one-hour virtual WebM ready");
  scenario.value = {
    id,
    projects: createProjects(storyGateway(id), acceleratedStoryScheduler, {
      signInAgain: () => write(id, "explicit Sign in again activated"),
      logPlayback: (entry) => write(id, describePlaybackActivity(entry)),
    }),
  };
}

rebuild();

watch(
  () => [
    props.authError,
    props.missing,
    props.overload,
    props.serverFailure,
    props.networkFailure,
    props.invalidMedia,
    props.slowInitialLoad,
    props.prolongedBuffering,
  ],
  () => {
    if (scenario.value) void configureScenario(scenario.value.id);
  },
);
</script>

<template>
  <main class="audio-story">
    <section class="story-section" aria-labelledby="story-section-title">
      <h3 id="story-section-title">Section 1</h3>
      <PlayerProvider v-if="scenario" :key="scenario.id" :projects="scenario.projects" />
    </section>
    <RadarLogView
      :lines="logView.lines"
      :cursor="logView.cursor"
      :columns="80"
      label="Story playback activity"
      test-id="story-log"
    />
  </main>
</template>

<style scoped>
.audio-story {
  display: grid;
  gap: 1.25rem;
  max-width: 58rem;
}
.story-section {
  padding: 1rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
}
.audio-story > :deep(.radar-log) {
  margin: 0;
}
</style>
