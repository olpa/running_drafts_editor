<script setup lang="ts">
import { computed } from "vue";
import { formatClock } from "../shared/format.ts";
import SectionCard from "./SectionCard.vue";
import { useApplicationCapture } from "./application-capture.ts";

const recording = useApplicationCapture();

const snapshot = recording.snapshot;
const idle = computed(() => !recording.hasRecorder.value);
const busyStarting = computed(() => snapshot.value.starting);

const mode = computed(() => {
  if (idle.value) return "start";
  if (snapshot.value.capturing) return "stop";
  if (busyStarting.value) return "busy";
  return snapshot.value.sections.length ? "start-new" : "start";
});
const label = computed(() => {
  if (idle.value) return "Start";
  if (snapshot.value.capturing) return "Stop current recording to start new";
  if (busyStarting.value) return "Starting…";
  return snapshot.value.sections.length ? "Start new recording" : "Start";
});
const message = computed(() => {
  if (idle.value) return "Sign in to start capture.";
  if (snapshot.value.capturing) return "Capture is active. Speak now.";
  if (busyStarting.value) return "Preparing microphone capture. Please wait.";
  return snapshot.value.sections.length
    ? "Ready to capture another recording."
    : "Press Start when you are ready to capture audio.";
});
const disabled = computed(() => {
  if (idle.value) return true;
  if (snapshot.value.capturing) return false;
  if (busyStarting.value) return true;
  return !snapshot.value.canStart || !recording.ownsCurrentSession.value;
});
const title = computed(
  () => (idle.value ? undefined : snapshot.value.project?.title) ?? "New Project",
);
const elapsed = computed(() => (idle.value ? 0 : snapshot.value.elapsedMs));
const microphoneLevel = computed(() => snapshot.value.microphoneLevel);
const microphoneValue = computed(() => microphoneLevel.value?.valuePercent ?? 0);
const microphonePeak = computed(() => microphoneLevel.value?.peakPercent ?? 0);
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
    <section class="audio-confidence" aria-label="Capture audio feedback">
      <div class="cue-volume">
        <label for="cue-volume">Cue volume</label>
        <input
          id="cue-volume"
          type="range"
          min="0"
          max="100"
          step="10"
          :value="recording.cueVolumePercent.value"
          data-testid="cue-volume"
          @input="recording.setCueVolume(Number(($event.target as HTMLInputElement).value))"
        />
        <output for="cue-volume" aria-hidden="true" data-testid="cue-volume-value">
          {{ recording.cueVolumePercent.value }}%
        </output>
      </div>
      <div class="microphone-level">
        <div class="microphone-level-heading">
          <span id="microphone-level-label">Microphone level</span>
          <span aria-hidden="true" data-testid="microphone-level-value">
            {{ microphoneLevel ? `${microphoneValue}%` : "—%" }}
          </span>
        </div>
        <div
          class="microphone-level-track"
          role="meter"
          aria-labelledby="microphone-level-label"
          aria-valuemin="0"
          aria-valuemax="100"
          :aria-valuenow="microphoneValue"
          :aria-valuetext="microphoneLevel ? `${microphoneValue}%` : 'Inactive'"
          :aria-disabled="!microphoneLevel"
          :data-active="String(!!microphoneLevel)"
          data-testid="microphone-level"
        >
          <span
            class="microphone-level-fill"
            aria-hidden="true"
            :style="{ width: `${microphoneValue}%` }"
          ></span>
          <span
            v-if="microphoneLevel"
            class="microphone-level-peak"
            aria-hidden="true"
            :style="{ left: `min(calc(100% - 2px), ${microphonePeak}%)` }"
          ></span>
        </div>
      </div>
    </section>
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
.audio-confidence {
  display: grid;
  grid-template-columns: minmax(12rem, 1fr) minmax(12rem, 1fr);
  gap: 1rem 1.5rem;
  margin-top: 1rem;
  padding: 1rem;
  border: 1px solid var(--line);
  border-radius: 8px;
}
.cue-volume,
.microphone-level {
  display: grid;
  gap: 0.5rem;
}
.cue-volume {
  grid-template-columns: 1fr auto;
  align-items: center;
}
.cue-volume input {
  grid-column: 1 / -1;
  width: 100%;
  margin: 0;
}
.cue-volume output,
.microphone-level-heading span:last-child {
  font-variant-numeric: tabular-nums;
}
.microphone-level-heading {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
}
.microphone-level-track {
  position: relative;
  height: 1rem;
  overflow: hidden;
  border: 1px solid #64748b;
  border-radius: 999px;
  background: #e2e8f0;
}
.microphone-level-track[data-active="false"] {
  border-style: dashed;
  opacity: 0.55;
}
.microphone-level-fill {
  display: block;
  height: 100%;
  background: #475569;
}
.microphone-level-peak {
  position: absolute;
  inset-block: 0;
  width: 2px;
  background: #0f172a;
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
  .audio-confidence {
    grid-template-columns: 1fr;
  }
}
@media (prefers-reduced-motion: reduce) {
  .primary-control {
    transition: none;
  }
}
</style>
