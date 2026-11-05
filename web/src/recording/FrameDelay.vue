<script setup lang="ts">
import { onMounted, ref } from "vue";
import type { OverdueFrame } from "./capture-and-delivery.ts";
const props = defineProps<{ frame: OverdueFrame }>();
const announcement = ref("");
// Establish an empty live region before adding its one announcement. Counter
// changes never modify this text or replace this keyed component.
onMounted(() => {
  announcement.value = message();
});
function message(): string {
  const seconds = Math.max(0, Math.floor(props.frame.elapsedSeconds ?? 0));
  return `Saving audio is delayed by ${seconds} ${seconds === 1 ? "second" : "seconds"}.`;
}
</script>

<template>
  <div class="frame-delay">
    <span class="sr-only" role="status" aria-live="polite">{{ announcement }}</span>
    <span aria-live="off" data-testid="frame-delay">{{ message() }}</span>
  </div>
</template>

<style scoped>
.frame-delay {
  flex-basis: 100%;
  color: var(--warning);
}
.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
</style>
