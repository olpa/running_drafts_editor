<script setup lang="ts">
import { onMounted } from "vue";
import { RouterLink, useRoute } from "vue-router";

const route = useRoute();

onMounted(() => {
  globalThis.document.title = "Page not found — Running Drafts";
});
</script>

<template>
  <main class="not-found" data-testid="page-not-found">
    <h1 tabindex="-1">Page not found</h1>
    <p>The requested page does not exist.</p>
    <RouterLink :to="{ name: 'projects', query: route.query, hash: route.hash }">
      Go to Projects
    </RouterLink>
  </main>
</template>

<style scoped>
.not-found {
  width: min(40rem, calc(100% - 2rem));
  margin: 10vh auto;
  padding: 2rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
}
.not-found a {
  color: var(--accent-dark);
}
</style>
