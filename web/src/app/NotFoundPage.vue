<script setup lang="ts">
import { onMounted } from "vue";
import { RouterLink, useRoute } from "vue-router";

import LegalFooter from "./LegalFooter.vue";
import ProductHeader from "./ProductHeader.vue";

const route = useRoute();

onMounted(() => {
  globalThis.document.title = "Page not found — Running Drafts";
});
</script>

<template>
  <div class="not-found-shell">
    <ProductHeader />
    <main class="not-found" data-testid="page-not-found">
      <h1 tabindex="-1">Page not found</h1>
      <p>The requested page does not exist.</p>
      <RouterLink :to="{ name: 'projects', query: route.query, hash: route.hash }">
        Go to Projects
      </RouterLink>
    </main>
    <LegalFooter />
  </div>
</template>

<style scoped>
.not-found-shell {
  display: grid;
  gap: 1.25rem;
  width: min(100%, 72rem);
  margin-inline: auto;
  padding: clamp(1rem, 3vw, 2rem);
}
.not-found {
  padding: 2rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
}
.not-found a {
  color: var(--accent-dark);
}
</style>
