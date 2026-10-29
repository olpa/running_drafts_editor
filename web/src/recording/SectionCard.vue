<script setup lang="ts">
import { computed } from "vue";
import type { SectionSnapshot } from "./project-recorder.ts";

const props = defineProps<{ section: SectionSnapshot }>();
const emit = defineEmits<{ uploadPending: []; retryClose: [] }>();

const number = computed(() => props.section.position + 1);
const pendingText = computed(() => {
  const { pendingMs } = props.section;
  if (pendingMs <= 0) return "";
  const seconds = Math.ceil(pendingMs / 1000);
  return `${seconds} ${seconds === 1 ? "second" : "seconds"} of audio pending`;
});
const completionText = computed(() => {
  const { completion, completionProblem } = props.section;
  if (completion === "pending") return "Completing recording…";
  if (completion === "succeeded") return "Recording complete";
  return completion === "failed" ? completionProblem : "";
});
const completionClass = computed(() => {
  const { completion } = props.section;
  return completion === "succeeded" ? "success" : completion === "failed" ? "error" : "warning";
});
</script>

<template>
  <section data-testid="section" :data-completion="section.completion ?? undefined">
    <h3>Section {{ number }}</h3>
    <span v-if="pendingText" class="section-status warning">{{ pendingText }}</span>
    <span
      class="section-status"
      :class="completionClass"
      :hidden="!completionText"
      role="status"
      aria-live="polite"
      >{{ completionText }}</span
    >
    <span v-if="section.problem" class="section-status error">{{ section.problem }}</span>
    <button v-if="section.canUploadPending" type="button" @click="emit('uploadPending')">
      Upload pending audio
    </button>
    <button
      v-if="section.canRetryCompletion"
      type="button"
      :aria-label="`Try completing Section ${number} again`"
      @click="emit('retryClose')"
    >
      Try again
    </button>
    <button v-if="section.canRetryClose" type="button" @click="emit('retryClose')">
      Could not close recording — Try again
    </button>
  </section>
</template>

<style scoped>
section {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.6rem;
  padding: 0.7rem;
  border: 1px solid var(--line);
  border-radius: 6px;
}
h3 {
  margin: 0;
  font-size: 1rem;
}
button {
  min-height: 2rem;
  padding: 0.3rem 0.6rem;
}
.section-status {
  font-size: 0.9rem;
}
.warning {
  color: var(--warning);
}
.success {
  color: var(--success);
}
.error {
  color: var(--danger);
}
</style>
