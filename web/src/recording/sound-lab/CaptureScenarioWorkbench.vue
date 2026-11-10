<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, shallowRef } from "vue";
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

const props = defineProps<{
  scenarioKind: ScenarioKind;
  initial: string;
  play: string;
  inspect: string;
  interaction: string;
}>();

interface Runtime {
  scenario: LabScenario;
  playback: ScenarioPlayback;
  cues: CuePlayer;
  dispose(): void;
}
const runtime = shallowRef<Runtime | null>(null);
const error = shallowRef("");
const revision = shallowRef(0);
const log = new RadarLog(
  DEFAULT_CONFIG.logColumns,
  DEFAULT_CONFIG.logRows,
  DEFAULT_CONFIG.logGapChars,
);
const logView = shallowRef(log.view());
const openedAt = globalThis.performance.now();
const overrides = computed(() => modifiedSoundAssignments());

function write(message: string): void {
  const seconds = ((globalThis.performance.now() - openedAt) / 1000).toFixed(1);
  log.write(`${seconds} q${revision.value} ${message}`);
  logView.value = log.view();
}

function rebuild(): Runtime {
  runtime.value?.dispose();
  revision.value++;
  error.value = "";
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
    await current.cues.prepare();
    await current.playback.play();
  } catch (value) {
    error.value = value instanceof Error ? value.message : String(value);
    write(`ERR ${error.value}`);
  }
}

onMounted(rebuild);
onBeforeUnmount(() => runtime.value?.dispose());
</script>

<template>
  <main class="capture-workbench">
    <header>
      <h1>Capture scenario workbench</h1>
      <p>Simulated Capture · No microphone, backend, credentials, or saved Project data</p>
    </header>
    <ScenarioReviewNote :initial :play :inspect :interaction />
    <aside v-if="overrides.length" class="sound-overrides" aria-label="Active sound overrides">
      <strong>Experimental sound assignments:</strong>
      <ul>
        <li v-for="value in overrides" :key="value.role">
          {{ feedbackRoleNames[value.role] }}: {{ value.candidateName }}
        </li>
      </ul>
    </aside>
    <div class="playback-actions">
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
    <p id="arranged-controls-note" class="interaction-note">
      This is an arranged review. Production controls are inactive; Acknowledge and Stop warning
      sound remain available when shown.
    </p>
    <p v-if="error" role="alert" class="story-error">{{ error }}</p>
    <ProductionCapture v-if="runtime" :key="revision" :scenario="runtime.scenario" />
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
