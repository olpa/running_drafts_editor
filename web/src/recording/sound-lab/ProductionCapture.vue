<script setup lang="ts">
import { provide } from "vue";
import RecordingPanel from "../RecordingPanel.vue";
import AllowanceStatus from "../AllowanceStatus.vue";
import CaptureModal from "../CaptureModal.vue";
import CaptureAlertsView from "../../alerts/CaptureAlerts.vue";
import AllowanceFeedbackView from "../../alerts/AllowanceFeedback.vue";
import { captureAlertsKey } from "../../alerts/capture-alerts.ts";
import { allowanceFeedbackKey } from "../../alerts/allowance-feedback.ts";
import { applicationCaptureKey } from "../application-capture.ts";
import { microphoneAccessKey } from "../microphone-access.ts";
import { configKey } from "../../shared/config.ts";
import type { LabScenario } from "./scenario.ts";
const props = defineProps<{
  scenario: LabScenario;
  presentation?: "workbench" | "experience";
  modalOpen?: boolean;
}>();
defineEmits<{ close: []; discard: [] }>();
provide(applicationCaptureKey, props.scenario.app);
provide(configKey, props.scenario.config);
provide(captureAlertsKey, props.scenario.alerts);
provide(allowanceFeedbackKey, props.scenario.allowanceFeedback);
provide(microphoneAccessKey, props.scenario.microphoneAccess);
</script>

<template>
  <div class="global-feedback-layer">
    <CaptureAlertsView />
    <AllowanceFeedbackView />
  </div>
  <CaptureModal
    v-if="presentation === 'experience' && modalOpen"
    @close="$emit('close')"
    @discard="$emit('discard')"
  />
  <div
    v-else-if="presentation !== 'experience'"
    class="arranged-production"
    inert
    aria-describedby="arranged-controls-note"
  >
    <AllowanceStatus />
    <RecordingPanel />
  </div>
</template>

<style scoped>
.global-feedback-layer[inert] {
  visibility: hidden;
}
</style>
