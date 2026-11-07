<script setup lang="ts">
import { computed } from "vue";
import { useApplicationCapture } from "./application-capture.ts";

const recording = useApplicationCapture();
const warning = computed(() => recording.allowanceWarnings.value);

function bytes(value: number): string {
  if (value >= 1024 * 1024) return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
  if (value >= 1024) return `${Math.round(value / 1024)} KiB`;
  return `${value} bytes`;
}

function time(value: number): string {
  const seconds = Math.ceil(value / 1000);
  if (seconds >= 60) return `${Math.ceil(seconds / 60)} minutes`;
  return `${seconds} seconds`;
}
</script>

<template>
  <div
    v-if="warning.storageRemainingBytes !== null || warning.durationRemainingMs !== null"
    class="allowance-status"
    data-testid="allowance-status"
    role="status"
  >
    <strong>Audio allowance is nearly full.</strong>
    <span v-if="warning.storageRemainingBytes !== null" data-testid="storage-warning">
      {{ bytes(warning.storageRemainingBytes) }} of stored-audio capacity remains.
    </span>
    <span v-if="warning.durationRemainingMs !== null" data-testid="duration-warning">
      This recording has approximately {{ time(warning.durationRemainingMs) }} remaining.
    </span>
  </div>
</template>

<style scoped>
.allowance-status {
  display: grid;
  gap: 0.35rem;
  padding: 0.75rem 1rem;
  margin-bottom: 1rem;
  border-left: 4px solid #a16207;
  background: #fffbeb;
  color: #713f12;
}
</style>
