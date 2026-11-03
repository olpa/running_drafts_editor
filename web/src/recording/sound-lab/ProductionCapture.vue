<script setup lang="ts">
import { provide } from "vue";
import RecordingPanel from "../RecordingPanel.vue";
import { applicationCaptureKey } from "../application-capture.ts";
import { configKey } from "../../shared/config.ts";
import type { LabScenario } from "./scenario.ts";
const props = defineProps<{ scenario: LabScenario }>();
provide(applicationCaptureKey, props.scenario.app);
provide(configKey, props.scenario.config);
</script>

<template>
  <RecordingPanel />
</template>
