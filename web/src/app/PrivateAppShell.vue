<script setup lang="ts">
import { computed } from "vue";
import { RouterLink, useRoute } from "vue-router";
import { useProjects } from "../projects/projects.ts";
import { useSession } from "../session/session.ts";
import AccountMenu from "./AccountMenu.vue";
import LegalFooter from "./LegalFooter.vue";
import ProductHeader from "./ProductHeader.vue";
import SeedApp from "./SeedApp.vue";

const session = useSession();
const projects = useProjects();
const route = useRoute();
const showNavigation = computed(
  () => route.name !== "project-detail" || projects.detail.value.kind !== "notFound",
);
</script>

<template>
  <div :class="{ 'private-app-shell': session.state.value.status === 'signedIn' }">
    <ProductHeader v-if="session.state.value.status === 'signedIn'">
      <template v-if="showNavigation" #brand>
        <RouterLink :to="{ name: 'projects', query: route.query, hash: route.hash }">
          Running Drafts
        </RouterLink>
      </template>
      <template v-if="showNavigation" #navigation>
        <AccountMenu />
      </template>
    </ProductHeader>
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
