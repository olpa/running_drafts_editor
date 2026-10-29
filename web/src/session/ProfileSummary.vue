<script setup lang="ts">
import { computed, useId } from "vue";
import { formatDateTime } from "../shared/format.ts";
import { useSession } from "./session.ts";

const session = useSession();
const titleId = useId();

const signedIn = computed(() => {
  const state = session.state.value;
  return state.status === "signedIn" ? state.session : null;
});
/** The email is shown only when the provider verified it. */
const verifiedEmail = computed(() => {
  const profile = signedIn.value?.profile;
  return profile?.emailVerified === true ? profile.email : null;
});
</script>

<template>
  <section v-if="signedIn" class="profile-summary" :aria-labelledby="titleId">
    <h2 :id="titleId">Profile</h2>
    <dl>
      <div v-if="signedIn.profile.displayName">
        <dt>Name</dt>
        <dd>{{ signedIn.profile.displayName }}</dd>
      </div>
      <div v-if="verifiedEmail">
        <dt>Email</dt>
        <dd>{{ verifiedEmail }}</dd>
      </div>
      <div>
        <dt>Account ID</dt>
        <dd>
          <code>{{ signedIn.account.id }}</code>
        </dd>
      </div>
      <div>
        <dt>Sign-in provider</dt>
        <dd>{{ signedIn.provider.displayName }}</dd>
      </div>
      <div>
        <dt>Session expires</dt>
        <dd>
          <time :datetime="signedIn.sessionExpiresAt">
            {{ formatDateTime(signedIn.sessionExpiresAt) }}
          </time>
        </dd>
      </div>
    </dl>
    <button type="button" class="link-button" @click="session.logout()">Log out</button>
  </section>
</template>

<style scoped>
.profile-summary {
  max-width: 36rem;
  padding: 1rem 1.25rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
}
dl {
  display: grid;
  gap: 0.6rem;
  margin: 0 0 1rem;
}
dl > div {
  display: grid;
  grid-template-columns: minmax(7rem, 10rem) 1fr;
  gap: 0.75rem;
}
dt {
  color: var(--muted);
}
dd {
  margin: 0;
  overflow-wrap: anywhere;
}
code {
  font-size: 0.9rem;
}
@media (max-width: 560px) {
  dl > div {
    grid-template-columns: 1fr;
    gap: 0;
  }
}
</style>
