<script setup lang="ts">
import { modalFocus as vModalFocus } from "./modal-focus.ts";
import { inject } from "vue";
import { microphoneAccessKey } from "./microphone-access.ts";
const access = inject(microphoneAccessKey);
if (!access) throw new Error("MicrophoneAccess was not provided.");
</script>

<template>
  <div
    v-if="access.takeoverRequested.value"
    v-modal-focus="() => access.cancelTakeover()"
    class="microphone-modal"
  >
    <section role="dialog" aria-modal="true" aria-labelledby="microphone-access-title">
      <h2 id="microphone-access-title">Another Capture is active</h2>
      <p>Stop the previous Capture to use the microphone here.</p>
      <button type="button" @click="access.cancelTakeover()">Cancel</button>
      <button type="button" @click="access.confirmTakeover()">
        Stop previous Capture and start new
      </button>
    </section>
  </div>
</template>

<style scoped>
.microphone-modal {
  position: fixed;
  inset: 0;
  z-index: 10;
  display: grid;
  place-items: center;
  background: rgb(0 0 0 / 45%);
}
section {
  max-width: 32rem;
  margin: 1rem;
  padding: 2rem;
  background: white;
  border-radius: 8px;
}
button {
  margin: 0.5rem;
}
</style>
