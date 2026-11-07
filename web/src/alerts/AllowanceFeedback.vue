<script setup lang="ts">
import { useAllowanceFeedback } from "./allowance-feedback.ts";

const feedback = useAllowanceFeedback();
</script>

<template>
  <div v-if="feedback.active.value" class="allowance-feedback" role="status">
    <span>Allowance warning sound is playing.</span>
    <span v-if="feedback.soundUnavailable.value"> Warning sound is unavailable.</span>
    <button type="button" @click="feedback.silence()">Silence warning</button>
  </div>
</template>

<style scoped>
.allowance-feedback {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.75rem;
  padding: 0.75rem 1rem;
  margin-bottom: 1rem;
  border: 2px solid #a16207;
  background: #fffbeb;
  color: #713f12;
}
.allowance-feedback button {
  margin-left: auto;
}
</style>
