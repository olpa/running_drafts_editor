<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, shallowRef, watchEffect } from "vue";
import { createCaptureAlerts } from "../../alerts/capture-alerts.ts";
import { DEFAULT_CONFIG } from "../../shared/config.ts";
import { RadarLog } from "../../shared/radar-log.ts";
import RadarLogView from "../../shared/RadarLogView.vue";
import { CuePlayer } from "../cues.ts";
import ProductionCapture from "./ProductionCapture.vue";
import ScenarioReviewNote from "./ScenarioReviewNote.vue";
import { createScenarioPlayback, type ScenarioPlayback } from "./playback.ts";
import { createLabScenario, type LabScenario, type ScenarioKind } from "./scenario.ts";
import {
  feedbackRoleNames,
  modifiedSoundAssignments,
  selectedSoundAssignments,
} from "./sound-store.ts";

const props = withDefaults(
  defineProps<{
    scenarioKind: ScenarioKind;
    initial: string;
    play: string;
    inspect: string;
    interaction: string;
    reviewMode?: "experience" | "workbench";
  }>(),
  { reviewMode: "workbench" },
);

interface Runtime {
  scenario: LabScenario;
  playback: ScenarioPlayback;
  cues: CuePlayer;
  dispose(): void;
}
const runtime = shallowRef<Runtime | null>(null);
const error = shallowRef("");
const revision = shallowRef(0);
const modalOpen = shallowRef(false);
const capturedProjectTitle = shallowRef("");
const log = new RadarLog(
  DEFAULT_CONFIG.logColumns,
  DEFAULT_CONFIG.logRows,
  DEFAULT_CONFIG.logGapChars,
);
const logView = shallowRef(log.view());
const openedAt = globalThis.performance.now();
const overrides = computed(() => modifiedSoundAssignments());
const reviewTitle = computed(() =>
  props.reviewMode === "experience" ? "Capture experience" : "Capture scenario workbench",
);
const reviewSubtitle = computed(() =>
  props.reviewMode === "experience"
    ? "Production modal over a simulated Project · No microphone, backend, credentials, or saved data"
    : "Simulated Capture · No microphone, backend, credentials, or saved Project data",
);
const reviewInteraction = computed(() =>
  props.reviewMode === "experience"
    ? "Choose New recording once. The arranged scenario runs through the real production modal; its controls remain active. Switch Capture view to Workbench for direct inspection and Replay controls."
    : props.interaction,
);

function write(message: string): void {
  const seconds = ((globalThis.performance.now() - openedAt) / 1000).toFixed(1);
  log.write(`${seconds} q${revision.value} ${message}`);
  logView.value = log.view();
}

function rebuild(): Runtime {
  runtime.value?.dispose();
  revision.value++;
  error.value = "";
  modalOpen.value = false;
  capturedProjectTitle.value = "";
  const cues = new CuePlayer(
    (role, phase) => write(`cue ${role} ${phase}`),
    selectedSoundAssignments(),
  );
  const alerts = createCaptureAlerts(cues);
  const scenario = createLabScenario({
    name: props.scenarioKind,
    manual: true,
    intervalMs: 5000,
    deadlineMs: 2000,
    cues,
    alerts,
    log: write,
  });
  const playback = createScenarioPlayback(scenario, props.scenarioKind, write);
  const next = {
    scenario,
    playback,
    cues,
    dispose() {
      playback.dispose();
      scenario.dispose();
      alerts.dispose();
      cues.dispose();
    },
  };
  runtime.value = next;
  write("fresh complete scenario runtime");
  return next;
}

async function playScenario(replay = false): Promise<void> {
  const current = replay ? rebuild() : runtime.value;
  if (!current) return;
  try {
    if (props.reviewMode === "experience") {
      if (!current.scenario.app.prepare()) return;
      modalOpen.value = true;
      await nextTick();
    }
    await current.cues.prepare();
    await current.playback.play();
  } catch (value) {
    error.value = value instanceof Error ? value.message : String(value);
    write(`ERR ${error.value}`);
  }
}

function closeExperience(): void {
  const current = runtime.value;
  if (!current || !current.scenario.app.release()) return;
  modalOpen.value = false;
}

function discardExperience(): void {
  const current = runtime.value;
  if (!current) return;
  current.scenario.app.discardAll();
  modalOpen.value = false;
}

watchEffect(() => {
  const current = runtime.value;
  if (!current || props.reviewMode !== "experience" || !modalOpen.value) return;
  const snapshot = current.scenario.app.snapshot.value;
  const completed = snapshot.sections.some((section) => section.completion === "succeeded");
  if (
    !completed ||
    current.scenario.app.focused.value ||
    current.scenario.app.hasRetainedWork.value
  )
    return;
  capturedProjectTitle.value = snapshot.project?.title ?? "Captured Project";
  closeExperience();
});

onMounted(rebuild);
onBeforeUnmount(() => runtime.value?.dispose());
</script>

<template>
  <main class="capture-workbench">
    <header>
      <h1>{{ reviewTitle }}</h1>
      <p>{{ reviewSubtitle }}</p>
    </header>
    <ScenarioReviewNote :initial :play :inspect :interaction="reviewInteraction" />
    <aside v-if="overrides.length" class="sound-overrides" aria-label="Active sound overrides">
      <strong>Experimental sound assignments:</strong>
      <ul>
        <li v-for="value in overrides" :key="value.role">
          {{ feedbackRoleNames[value.role] }}: {{ value.candidateName }}
        </li>
      </ul>
    </aside>
    <div v-if="reviewMode === 'workbench'" class="playback-actions">
      <button
        v-if="runtime?.playback.status.value === 'ready'"
        type="button"
        class="primary"
        @click="playScenario(false)"
      >
        Play scenario
      </button>
      <p v-else-if="runtime?.playback.status.value === 'playing'" role="status">
        Scenario playing…
      </p>
      <button
        v-else-if="runtime?.playback.status.value === 'finished'"
        type="button"
        @click="playScenario(true)"
      >
        Replay scenario
      </button>
    </div>
    <p v-if="reviewMode === 'workbench'" id="arranged-controls-note" class="interaction-note">
      This is an arranged review. Production controls are inactive; Acknowledge and Stop warning
      sound remain available when shown.
    </p>
    <p v-if="error" role="alert" class="story-error">{{ error }}</p>
    <template v-if="reviewMode === 'experience'">
      <section class="experience-shell" data-testid="capture-experience-shell">
        <nav aria-label="Projects">
          <a href="#projects" @click.prevent>Projects</a>
          <button
            v-if="runtime?.playback.status.value === 'ready'"
            type="button"
            class="primary"
            @click="playScenario(false)"
          >
            New recording
          </button>
          <button
            v-else-if="runtime?.playback.status.value === 'finished' && !modalOpen"
            type="button"
            @click="playScenario(true)"
          >
            Replay scenario
          </button>
          <p v-else role="status">Capture scenario running…</p>
        </nav>
        <article aria-labelledby="experience-projects-title">
          <p class="eyebrow">Browser seed</p>
          <h2 id="experience-projects-title" tabindex="-1">
            {{ capturedProjectTitle || "Projects" }}
          </h2>
          <p v-if="capturedProjectTitle">
            Capture completed and the routed Project view refreshed.
          </p>
          <p v-else>Open New recording to review this scenario through the production modal.</p>
        </article>
      </section>
      <ProductionCapture
        v-if="runtime"
        :key="revision"
        :scenario="runtime.scenario"
        presentation="experience"
        :modal-open="modalOpen"
        @close="closeExperience"
        @discard="discardExperience"
      />
    </template>
    <ProductionCapture
      v-else-if="runtime"
      :key="revision"
      :scenario="runtime.scenario"
      presentation="workbench"
    />
    <RadarLogView
      :lines="logView.lines"
      :cursor="logView.cursor"
      :columns="DEFAULT_CONFIG.logColumns"
      label="Scenario activity log"
      test-id="scenario-log"
    />
  </main>
</template>

<style scoped>
.capture-workbench {
  max-width: 1000px;
  margin: auto;
  padding: 1.5rem;
  display: grid;
  gap: 1.25rem;
}
.experience-shell {
  overflow: hidden;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
  box-shadow: var(--shadow);
}
.experience-shell nav {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 1rem;
  border-bottom: 1px solid var(--line);
  background: #f8fafc;
}
.experience-shell nav a {
  margin-right: auto;
  color: var(--accent-dark);
  font-weight: 700;
}
.experience-shell nav p {
  margin: 0;
}
.experience-shell article {
  min-height: 15rem;
  padding: 2rem;
}
.experience-shell .eyebrow {
  margin-bottom: 0.4rem;
  color: var(--accent-dark);
  font-size: 0.75rem;
  font-weight: 750;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}
.playback-actions {
  min-height: 2.5rem;
  display: flex;
  align-items: center;
}
.interaction-note,
.sound-overrides {
  padding: 0.75rem;
  border-left: 4px solid #64748b;
  background: #f8fafc;
}
.sound-overrides ul {
  margin-bottom: 0;
}
.story-error {
  color: var(--danger);
  font-weight: 700;
}
</style>
