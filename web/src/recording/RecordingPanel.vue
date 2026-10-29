<script setup lang="ts">
import { computed } from "vue";
import { formatClock } from "../shared/format.ts";
import { useConfig } from "../shared/config.ts";
import SectionCard from "./SectionCard.vue";
import { useRecording } from "./recording.ts";

const recording = useRecording();
const config = useConfig();

const snapshot = recording.snapshot;
const idle = computed(() => !recording.hasRecorder.value);
const busyStarting = computed(() => snapshot.value.starting);
const finalizing = computed(() =>
  snapshot.value.sections.some((section) => section.finalizingCapture),
);

const mode = computed(() => {
  if (idle.value) return "start";
  if (snapshot.value.capturing) return "stop";
  if (busyStarting.value || finalizing.value) return "busy";
  return snapshot.value.sections.length ? "start-new" : "start";
});
const label = computed(() => {
  if (idle.value) return "Start";
  if (snapshot.value.capturing) return "Stop current recording to start new";
  if (busyStarting.value) return "Starting…";
  if (finalizing.value) return "Finishing capture…";
  return snapshot.value.sections.length ? "Start new recording" : "Start";
});
const message = computed(() => {
  if (idle.value) return "Sign in to start capture.";
  if (snapshot.value.capturing) return "Capture is active. Speak now.";
  if (busyStarting.value) return "Preparing microphone capture. Please wait.";
  if (finalizing.value) return "Finalizing captured audio.";
  return snapshot.value.sections.length
    ? "Ready to capture another recording."
    : "Press Start when you are ready to capture audio.";
});
const disabled = computed(() => {
  if (idle.value) return true;
  if (snapshot.value.capturing) return false;
  if (busyStarting.value || finalizing.value) return true;
  return !snapshot.value.canStart || !recording.ownsCurrentSession.value;
});
const title = computed(
  () => (idle.value ? undefined : snapshot.value.project?.title) ?? "New Project",
);
const elapsed = computed(() => (idle.value ? 0 : snapshot.value.elapsedMs));
</script>

<template>
  <section aria-labelledby="project-recording-title">
    <header class="toolbar">
      <h2 id="project-recording-title" data-testid="project-title">{{ title }}</h2>
      <button
        type="button"
        class="primary primary-control"
        data-testid="start"
        :data-mode="mode"
        :disabled="disabled"
        @click="recording.toggle()"
      >
        {{ label }}
      </button>
      <span data-testid="elapsed" aria-label="Elapsed capture time">{{
        formatClock(elapsed)
      }}</span>
    </header>
    <div
      class="capture-indicator"
      data-testid="capture-indicator"
      :data-capturing="String(snapshot.capturing)"
      role="status"
    >
      <span class="capture-dot" aria-hidden="true"></span>
      <span data-testid="capture-message">{{ message }}</span>
    </div>
    <div v-if="snapshot.capturing" class="capture-progress" data-testid="capture-progress">
      <span data-testid="capture-time" aria-live="off"
        >{{ formatClock(snapshot.elapsedMs) }} / {{ formatClock(config.maxRecordingMs) }}</span
      >
      <progress
        data-testid="capture-duration"
        aria-label="Capture duration"
        :max="config.maxRecordingMs"
        :value="Math.min(snapshot.elapsedMs, config.maxRecordingMs)"
      ></progress>
    </div>
    <p
      class="start-problem"
      data-testid="start-problem"
      role="status"
      aria-live="polite"
      :hidden="!snapshot.startProblem"
    >
      {{ snapshot.startProblem }}
    </p>
    <p
      class="ownership-warning"
      data-testid="ownership-warning"
      role="status"
      aria-live="polite"
      :hidden="!recording.ownershipWarning.value"
    >
      {{ recording.ownershipWarning.value }}
    </p>
    <div
      class="section-deliveries"
      data-testid="sections"
      role="group"
      aria-label="Recording sections"
    >
      <SectionCard
        v-for="section in snapshot.sections"
        :key="section.sectionId"
        :section="section"
        @upload-pending="recording.uploadPendingAudio(section.sectionId)"
        @retry-close="recording.retryClose(section.sectionId)"
      />
    </div>
  </section>
</template>

<style scoped>
.toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.75rem;
}
.toolbar h2 {
  flex: 1 1 12rem;
  margin: 0;
}
.toolbar [data-testid="elapsed"] {
  margin-left: auto;
  font-variant-numeric: tabular-nums;
}
.primary-control {
  transition:
    background-color 180ms ease,
    border-color 180ms ease,
    color 180ms ease,
    transform 180ms ease;
}
.primary-control[data-mode="stop"] {
  border-color: var(--danger);
  background: var(--danger);
  transform: scale(1.02);
}
.capture-indicator {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-top: 1rem;
  padding: 1rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: #f8fafc;
  color: #334155;
  font-weight: 650;
}
.capture-indicator[data-capturing="true"] {
  border-color: #7cc99d;
  background: #f0f9f4;
  color: #166534;
}
.capture-dot {
  flex: 0 0 0.75rem;
  height: 0.75rem;
  border-radius: 50%;
  background: currentcolor;
}
.capture-progress {
  max-width: 24rem;
  margin-top: 0.75rem;
  font-variant-numeric: tabular-nums;
}
.capture-progress span {
  display: block;
  text-align: right;
}
.capture-progress progress {
  width: 100%;
  accent-color: var(--success);
}
.start-problem {
  margin-top: 0.75rem;
  padding: 0.75rem;
  border-left: 4px solid var(--danger);
  background: #fff7f7;
  color: #9f2222;
}
.ownership-warning {
  padding: 0.75rem;
  border-left: 4px solid var(--danger);
  background: #fff7f7;
  color: #9f2222;
}
.section-deliveries {
  display: grid;
  gap: 0.5rem;
  margin: 1rem 0 0;
  padding: 0;
}
@media (max-width: 560px) {
  .toolbar {
    align-items: stretch;
  }
  .toolbar > * {
    flex: 1 1 auto;
  }
}
@media (prefers-reduced-motion: reduce) {
  .primary-control {
    transition: none;
  }
}
</style>
