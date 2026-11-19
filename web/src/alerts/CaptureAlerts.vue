<script setup lang="ts">
import { computed } from "vue";
import { useApplicationCapture } from "../recording/application-capture.ts";
import { captureAlertId, useCaptureAlerts, type CaptureAlert } from "./capture-alerts.ts";

const captureAlerts = useCaptureAlerts();
const recording = useApplicationCapture();

const overdueAlerts = computed(() =>
  captureAlerts.alerts.value.filter(
    (alert): alert is Extract<CaptureAlert, { kind: "transport-frame-overdue" }> =>
      alert.kind === "transport-frame-overdue",
  ),
);
const otherAlerts = computed(() =>
  captureAlerts.alerts.value.filter((alert) => alert.kind !== "transport-frame-overdue"),
);
const delayDetails = computed(() => {
  const overdue = overdueAlerts.value;
  if (!overdue.length) return "";
  const affectedSections = new Set(overdue.map((alert) => alert.sectionId));
  const sections = recording.snapshot.value.sections.filter((section) =>
    affectedSections.has(section.sectionId),
  );
  const pending = sections.length
    ? sections.reduce((total, section) => total + section.pendingFrames, 0)
    : overdue.length;
  const currentDelays = sections.flatMap((section) =>
    section.overdueFrames
      .filter((frame) =>
        overdue.some(
          (alert) => alert.sectionId === section.sectionId && alert.frameId === frame.frameId,
        ),
      )
      .map((frame) => frame.elapsedSeconds ?? 0),
  );
  const oldest = Math.max(
    0,
    ...(currentDelays.length ? currentDelays : overdue.map((alert) => alert.elapsedSeconds)),
  );
  return `${pending} ${pending === 1 ? "piece" : "pieces"} pending · oldest delay ${oldest} ${oldest === 1 ? "second" : "seconds"}`;
});

function message(alert: CaptureAlert): string {
  switch (alert.kind) {
    case "capture-finalization-failed":
      return "Capture finalization failed. Recent audio may not be saved.";
    case "audio-asset-sealing-failed":
      return "Recording could not be completed.";
    case "unrequested-capture-stop":
      return recording.snapshot.value.hasPendingWork
        ? "Capture stopped. Check the recording Section for retained audio and recovery details."
        : "Capture stopped unexpectedly. No audio remains pending.";
    case "transport-frame-overdue":
      return "Saving audio is delayed. Capture continues.";
  }
}
</script>

<template>
  <div v-if="captureAlerts.alerts.value.length" class="capture-alerts" data-testid="capture-alerts">
    <div class="capture-alert-announcement" role="alert">
      <strong>Capture needs attention.</strong>
      <ul>
        <li v-if="overdueAlerts.length" data-testid="delay-message">
          Saving audio is delayed. Capture continues.
        </li>
        <li v-for="alert in otherAlerts" :key="captureAlertId(alert)">
          {{ message(alert) }}
        </li>
      </ul>
      <span v-if="captureAlerts.soundUnavailable.value">Problem sound is unavailable.</span>
    </div>
    <p v-if="delayDetails" data-testid="delay-details" aria-live="off">{{ delayDetails }}</p>
    <button type="button" @click="captureAlerts.acknowledgeAll()">Acknowledge</button>
  </div>
</template>

<style scoped>
.capture-alerts {
  display: grid;
  gap: 1rem;
  padding: 1.25rem;
  margin-bottom: 1rem;
  border: 5px solid #991b1b;
  background: #fee2e2;
  color: #7f1d1d;
}
.capture-alert-announcement {
  display: grid;
  gap: 1rem;
}
ul {
  margin: 0;
}
button {
  justify-self: end;
}
</style>
