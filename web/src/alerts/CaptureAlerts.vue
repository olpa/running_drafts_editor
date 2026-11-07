<script setup lang="ts">
import { captureAlertId, useCaptureAlerts, type CaptureAlert } from "./capture-alerts.ts";

const captureAlerts = useCaptureAlerts();

function message(alert: CaptureAlert): string {
  switch (alert.kind) {
    case "transport-frame-overdue":
      return `Saving audio is delayed by ${alert.elapsedSeconds} ${alert.elapsedSeconds === 1 ? "second" : "seconds"}.`;
    case "capture-finalization-failed":
      return "Capture finalization failed. Recent audio may not be saved.";
    case "audio-asset-sealing-failed":
      return "Recording could not be completed.";
    case "unrequested-capture-stop":
      return "Capture stopped. Check the recording Section for retained audio and recovery details.";
  }
}
</script>

<template>
  <div
    v-if="captureAlerts.alerts.value.length"
    class="capture-alerts"
    role="alert"
    data-testid="capture-alerts"
  >
    <strong>Capture needs attention.</strong>
    <ul>
      <li v-for="alert in captureAlerts.alerts.value" :key="captureAlertId(alert)">
        {{ message(alert) }}
      </li>
    </ul>
    <span v-if="captureAlerts.soundUnavailable.value">Problem sound is unavailable.</span>
    <button type="button" @click="captureAlerts.acknowledgeAll()">Acknowledge</button>
  </div>
</template>

<style scoped>
.capture-alerts {
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
ul {
  flex-basis: 100%;
  margin: 0;
}
button {
  margin-left: auto;
}
</style>
