<script setup lang="ts">
import { computed, useId } from "vue";
import ProfileSummary from "./ProfileSummary.vue";
import { useSession } from "./session.ts";

const session = useSession();
const state = computed(() => session.state.value);
const titleId = useId();

const ROUTINE_SIGNED_OUT_MESSAGES = new Set(["Signed out.", "Sign in to continue."]);

const message = computed(() => {
  const current = state.value;
  if (current.status === "loading") return "Checking your session…";
  if (current.status === "failure") return current.message;
  if (current.status === "signedOut") {
    return ROUTINE_SIGNED_OUT_MESSAGES.has(current.message)
      ? "Sign in to view your profile."
      : current.message;
  }
  return "";
});
</script>

<template>
  <div class="profile-widget-state" aria-live="polite" aria-atomic="true">
    <ProfileSummary v-if="state.status === 'signedIn'" />
    <section v-else class="profile-widget" :aria-labelledby="titleId">
      <h2 :id="titleId">Profile</h2>
      <p>{{ message }}</p>
      <div v-if="state.status === 'signedOut' && state.loginAllowed" class="provider-actions">
        <button
          v-for="provider in session.providers"
          :key="provider.key"
          type="button"
          class="primary"
          @click="session.login(provider.key)"
        >
          Sign in with {{ provider.displayName }}
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.profile-widget {
  max-width: 36rem;
  padding: 1rem 1.25rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
}
.profile-widget p {
  margin: 0 0 1rem;
}
.provider-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}
</style>
