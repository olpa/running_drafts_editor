<script setup lang="ts">
import { ref, watch } from "vue";
import { useMicrophoneAccess } from "./microphone-access.ts";

const access = useMicrophoneAccess();
const cancelButton = ref<InstanceType<typeof globalThis.HTMLButtonElement> | null>(null);
let previousFocus: InstanceType<typeof globalThis.HTMLElement> | null = null;

watch(
  access.takeoverRequested,
  (requested) => {
    if (requested) {
      previousFocus =
        globalThis.document.activeElement instanceof globalThis.HTMLElement
          ? globalThis.document.activeElement
          : null;
      cancelButton.value?.focus();
      return;
    }
    if (previousFocus?.isConnected) previousFocus.focus();
    previousFocus = null;
  },
  { flush: "post" },
);
</script>

<template>
  <section
    v-if="access.takeoverRequested.value"
    class="microphone-prompt"
    aria-labelledby="microphone-access-title"
  >
    <h3 id="microphone-access-title">Another Capture is active</h3>
    <p>Stop the previous Capture to use the microphone here.</p>
    <div class="microphone-prompt-actions">
      <button ref="cancelButton" type="button" @click="access.cancelTakeover()">Cancel</button>
      <button type="button" @click="access.confirmTakeover()">
        Stop previous Capture and start new
      </button>
    </div>
  </section>
</template>

<style scoped>
.microphone-prompt {
  margin-bottom: 1rem;
  padding: 1rem;
  border: 3px solid var(--accent-dark);
  border-radius: 8px;
  background: #e8f1ff;
}
.microphone-prompt h3 {
  margin-top: 0;
}
.microphone-prompt-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
}
.microphone-prompt-actions button:last-child {
  margin-left: auto;
}
</style>
