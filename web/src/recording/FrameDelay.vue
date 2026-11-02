<script setup lang="ts">
import { onMounted, ref } from "vue";
import type { OverdueFrame } from "./capture-and-delivery.ts";
defineProps<{ frame: OverdueFrame }>();
const announcement = ref("");
// Establish an empty live region before adding its one announcement. Counter
// changes never modify this text or replace this keyed component.
onMounted(() => {
  announcement.value = "Saving audio is delayed.";
});
</script>

<template>
  <div class="frame-delay">
    <span class="sr-only" role="status" aria-live="polite">{{ announcement }}</span>
    <span aria-live="off" data-testid="frame-delay">{{
      frame.elapsedSeconds === null
        ? "Saving audio is delayed."
        : `Saving audio is delayed by ${frame.elapsedSeconds} ${frame.elapsedSeconds === 1 ? "second" : "seconds"}.`
    }}</span>
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
