<script setup lang="ts">
import { computed } from "vue";
import type { SectionSnapshot } from "./project.ts";

const props = defineProps<{ section: SectionSnapshot }>();
const emit = defineEmits<{ uploadPending: []; retryClose: [] }>();

const number = computed(() => props.section.position + 1);
const visible = computed(() => {
  const section = props.section;
  if (section.capturing || section.completion === "succeeded") return false;
  return (
    section.finalizingCapture ||
    section.completion !== null ||
    section.pendingMs > 0 ||
    !!section.problem ||
    section.diagnosticDetails.length > 0 ||
    !!section.closeProblem ||
    section.canUploadPending ||
    section.canRetryCompletion ||
    section.canRetryClose ||
    section.awaitingSeal
  );
});
const pendingText = computed(() => {
  const { pendingMs } = props.section;
  if (pendingMs <= 0) return "";
  const seconds = Math.ceil(pendingMs / 1000);
  return `${seconds} ${seconds === 1 ? "second" : "seconds"} of audio pending`;
});
const completionText = computed(() => {
  const { completion, finalizingCapture } = props.section;
  if (finalizingCapture) return "Finishing capture…";
  if (completion === "pending") return "Completing recording…";
  return "";
});
</script>

<template>
  <section v-if="visible" data-testid="section" :data-completion="section.completion ?? undefined">
    <h3>Section {{ number }}</h3>
    <span v-if="pendingText" class="section-status warning">{{ pendingText }}</span>
    <span
      class="section-status warning"
      :hidden="!completionText"
      role="status"
      aria-live="polite"
      >{{ completionText }}</span
    >
    <p
      v-if="section.completion === 'failed' && section.completionProblem"
      class="section-status error"
      data-testid="completion-problem"
    >
      {{ section.completionProblem }}
    </p>
    <p v-if="section.problem" class="section-status error" data-testid="section-problem">
      {{ section.problem }}
    </p>
    <p v-if="section.closeProblem" class="section-status error" data-testid="close-problem">
      {{ section.closeProblem }}
    </p>
    <ul v-if="section.diagnosticDetails.length" class="diagnostic-details" aria-label="Details">
      <li v-for="detail in section.diagnosticDetails" :key="detail" data-testid="problem-detail">
        {{ detail }}
      </li>
    </ul>
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
.diagnostic-details {
  flex-basis: 100%;
  margin: 0;
  color: var(--muted);
  font-size: 0.85rem;
}
.warning {
  color: var(--warning);
}
.error {
  color: var(--danger);
}
</style>
