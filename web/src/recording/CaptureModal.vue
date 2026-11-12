<script setup lang="ts">
import { computed } from "vue";
import CaptureAlerts from "../alerts/CaptureAlerts.vue";
import AllowanceFeedback from "../alerts/AllowanceFeedback.vue";
import AllowanceStatus from "./AllowanceStatus.vue";
import MicrophoneAccessPrompt from "./MicrophoneAccessPrompt.vue";
import RecordingPanel from "./RecordingPanel.vue";
import { useApplicationCapture } from "./application-capture.ts";
import { useMicrophoneAccess } from "./microphone-access.ts";
import { modalFocus as vModalFocus } from "./modal-focus.ts";

defineEmits<{ close: []; discard: [] }>();

const recording = useApplicationCapture();
const microphoneAccess = useMicrophoneAccess();
const mayClose = computed(() => !recording.focused.value && !recording.hasRetainedWork.value);
</script>

<template>
  <div v-modal-focus="() => undefined" class="capture-modal" data-testid="capture-modal">
    <section role="dialog" aria-modal="true" aria-labelledby="capture-modal-title">
      <header class="capture-modal-heading">
        <h2 id="capture-modal-title">Capture audio</h2>
        <button
          v-if="mayClose && !microphoneAccess.takeoverRequested.value"
          type="button"
          data-testid="close-capture"
          @click="$emit('close')"
        >
          {{ recording.snapshot.value.sections.length === 0 ? "Cancel" : "Close" }}
        </button>
      </header>
      <MicrophoneAccessPrompt />
      <div :inert="microphoneAccess.takeoverRequested.value">
        <CaptureAlerts />
        <AllowanceStatus />
        <AllowanceFeedback />
        <RecordingPanel />
        <button
          v-if="recording.hasRetainedWork.value && !recording.focused.value"
          type="button"
          class="danger-action"
          data-testid="discard-pending-audio"
          @click="$emit('discard')"
        >
          Discard pending audio
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.capture-modal {
  position: fixed;
  inset: 0;
  z-index: 20;
  display: grid;
  place-items: center;
  padding: 1rem;
  overflow: auto;
  background: rgb(0 0 0 / 55%);
}
.capture-modal > section {
  width: min(52rem, 100%);
  max-height: calc(100vh - 2rem);
  padding: 1.5rem;
  overflow: auto;
  border-radius: 10px;
  background: var(--panel);
  box-shadow: var(--shadow);
}
.capture-modal-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  margin-bottom: 1rem;
}
.capture-modal-heading h2 {
  margin: 0;
}
.capture-modal [inert] {
  opacity: 0.55;
}
.danger-action {
  margin-top: 1rem;
  border-color: var(--danger);
  color: var(--danger);
}
</style>
