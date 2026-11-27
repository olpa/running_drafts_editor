<script setup lang="ts">
import { useSession } from "../session/session.ts";
import LegalFooter from "./LegalFooter.vue";
import ProductHeader from "./ProductHeader.vue";
import SeedApp from "./SeedApp.vue";

const session = useSession();
</script>

<template>
  <div :class="{ 'private-app-shell': session.state.value.status === 'signedIn' }">
    <ProductHeader v-if="session.state.value.status === 'signedIn'" />
    <SeedApp />
    <LegalFooter v-if="session.state.value.status === 'signedIn'" />
  </div>
</template>

<style scoped>
.private-app-shell {
  display: grid;
  align-content: start;
  gap: 1.25rem;
  width: min(100%, 72rem);
  min-height: 100vh;
  margin-inline: auto;
  padding: clamp(1rem, 3vw, 2rem);
}
</style>
