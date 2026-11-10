<script setup lang="ts">
import { provide } from "vue";
import RecordingPanel from "../RecordingPanel.vue";
import AllowanceStatus from "../AllowanceStatus.vue";
import CaptureAlertsView from "../../alerts/CaptureAlerts.vue";
import AllowanceFeedbackView from "../../alerts/AllowanceFeedback.vue";
import { captureAlertsKey } from "../../alerts/capture-alerts.ts";
import { allowanceFeedbackKey } from "../../alerts/allowance-feedback.ts";
import { applicationCaptureKey } from "../application-capture.ts";
import { configKey } from "../../shared/config.ts";
import type { LabScenario } from "./scenario.ts";
const props = defineProps<{ scenario: LabScenario }>();
provide(applicationCaptureKey, props.scenario.app);
provide(configKey, props.scenario.config);
provide(captureAlertsKey, props.scenario.alerts);
provide(allowanceFeedbackKey, props.scenario.allowanceFeedback);
</script>

<template>
  <CaptureAlertsView />
  <AllowanceFeedbackView />
  <div class="arranged-production" inert aria-describedby="arranged-controls-note">
    <AllowanceStatus />
    <RecordingPanel />
  </div>
</template>
