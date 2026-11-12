<script setup lang="ts">
import CaptureAlerts from "../alerts/CaptureAlerts.vue";
import AllowanceFeedback from "../alerts/AllowanceFeedback.vue";
import { RouterView } from "vue-router";
</script>

<template>
  <div class="global-feedback-layer">
    <CaptureAlerts />
    <AllowanceFeedback />
  </div>
  <RouterView />
</template>

<style scoped>
.global-feedback-layer[inert] {
  visibility: hidden;
}
</style>
