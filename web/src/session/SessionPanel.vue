<script setup lang="ts">
import { computed } from "vue";
import { formatDateTime } from "../shared/format.ts";
import { useSession } from "./session.ts";

const session = useSession();
const current = computed(() => {
  const state = session.state.value;
  return state.status === "signedIn" ? state.session : null;
});
const identity = computed(() => {
  if (!current.value) return "";
  const { account, profile } = current.value;
  return profile.displayName ?? profile.email ?? `Account ${account.id.slice(0, 8)}`;
});
</script>

<template>
  <section v-if="current" class="authentication" data-testid="authentication" aria-live="polite">
    <p>{{ identity }} · {{ current.provider.displayName }}</p>
    <p>
      Session expires
      <time :datetime="current.sessionExpiresAt">{{
        formatDateTime(current.sessionExpiresAt)
      }}</time>
    </p>
    <button type="button" @click="session.logout()">Log out</button>
  </section>
</template>

<style scoped>
.authentication {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.75rem;
  margin-bottom: 1rem;
  padding: 1rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: #f8fafc;
}
.authentication p {
  margin: 0;
}
.authentication > p:first-child {
  flex: 1 1 16rem;
}
.authentication time {
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}
</style>
