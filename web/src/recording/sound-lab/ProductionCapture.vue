<script setup lang="ts">
import { provide } from "vue";
import RecordingPanel from "../RecordingPanel.vue";
import CaptureAlertsView from "../../alerts/CaptureAlerts.vue";
import { captureAlertsKey } from "../../alerts/capture-alerts.ts";
import { applicationCaptureKey } from "../application-capture.ts";
import { configKey } from "../../shared/config.ts";
import type { LabScenario } from "./scenario.ts";
const props = defineProps<{ scenario: LabScenario }>();
provide(applicationCaptureKey, props.scenario.app);
provide(configKey, props.scenario.config);
provide(captureAlertsKey, props.scenario.alerts);
</script>

<template>
  <CaptureAlertsView />
  <div class="arranged-production" inert aria-describedby="arranged-controls-note">
    <RecordingPanel />
  </div>
</template>
