<script setup lang="ts">
import { computed } from "vue";
import { formatDateTime } from "../shared/format.ts";
import { useSession } from "./session.ts";

const session = useSession();
const state = computed(() => session.state.value);

function identity(current: {
  account: { id: string };
  profile: { displayName: string | null; email: string | null };
}): string {
  return (
    current.profile.displayName ??
    current.profile.email ??
    `Account ${current.account.id.slice(0, 8)}`
  );
}
</script>

<template>
  <section class="authentication" data-testid="authentication" aria-live="polite">
    <p v-if="state.status === 'loading'">Checking your session…</p>

    <template v-else-if="state.status === 'signedIn'">
      <p>{{ identity(state.session) }} · {{ state.session.provider.displayName }}</p>
      <p>
        Session expires
        <time :datetime="state.session.sessionExpiresAt">
          {{ formatDateTime(state.session.sessionExpiresAt) }}
        </time>
      </p>
      <button type="button" @click="session.logout()">Log out</button>
    </template>

    <template v-else>
      <p>{{ state.message }}{{ state.referenceId ? ` Reference: ${state.referenceId}` : "" }}</p>
      <button v-if="state.status === 'failure'" type="button" @click="session.bootstrap()">
        Try again
      </button>
      <template v-else>
        <template v-if="state.loginAllowed">
          <div class="provider-actions">
            <button
              v-for="provider in session.providers"
              :key="provider.key"
              type="button"
              class="primary"
              @click="session.login(provider.key)"
            >
              Continue with {{ provider.displayName }}
            </button>
          </div>
          <p class="policy-links"><a href="/privacy">Privacy</a> · <a href="/terms">Terms</a></p>
        </template>
        <button v-if="state.logoutUnconfirmed" type="button" @click="session.logout()">
          Retry logout
        </button>
      </template>
    </template>
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
.provider-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}
.policy-links {
  flex-basis: 100%;
  color: var(--muted);
  font-size: 0.85rem;
}
.policy-links a {
  color: var(--accent-dark);
}
</style>
