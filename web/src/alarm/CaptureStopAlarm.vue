<script setup lang="ts">
import { useCaptureStopAlarm } from "./capture-stop-alarm.ts";
const alarm = useCaptureStopAlarm();
</script>

<template>
  <div
    v-if="alarm.active.value"
    class="capture-stop-alarm"
    role="alert"
    data-testid="capture-stop-alarm"
  >
    <strong>Capture stopped. Up to 30 seconds of recent audio may not be saved.</strong>
    <span v-if="alarm.soundUnavailable.value">Alarm sound is unavailable.</span>
    <button type="button" @click="alarm.acknowledge()">Acknowledge</button>
  </div>
</template>

<style scoped>
.capture-stop-alarm {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 1rem;
  padding: 1.25rem;
  margin-bottom: 1rem;
  border: 5px solid #991b1b;
  background: #fee2e2;
  color: #7f1d1d;
}
button {
  margin-left: auto;
}
</style>
