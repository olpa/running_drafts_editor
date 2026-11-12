<script setup lang="ts">
import { RouterLink, useRoute } from "vue-router";
import { useProjects } from "./projects.ts";

defineEmits<{ newRecording: [] }>();

const route = useRoute();
const projects = useProjects();
</script>

<template>
  <nav class="project-navigation" data-testid="private-navigation" aria-label="Projects">
    <div class="project-navigation-actions">
      <RouterLink :to="{ name: 'projects', query: route.query, hash: route.hash }">
        Projects
      </RouterLink>
      <button
        type="button"
        class="primary"
        data-testid="new-recording"
        @click="$emit('newRecording')"
      >
        New recording
      </button>
    </div>

    <p v-if="projects.list.value.kind === 'loading'" class="project-navigation-status">
      {{ projects.list.value.slow ? "Loading Projects…" : "" }}
    </p>
    <div v-else-if="projects.list.value.kind === 'failure'" class="project-navigation-status">
      <span>Projects could not be loaded.</span>
      <button type="button" @click="projects.loadList()">Retry</button>
    </div>
    <ol v-else-if="projects.list.value.kind === 'ready'" class="compact-project-list">
      <li v-for="project in projects.list.value.value" :key="project.id">
        <RouterLink
          :to="{
            name: 'project-detail',
            params: { projectId: project.id },
            query: route.query,
            hash: route.hash,
          }"
          :aria-current="route.params.projectId === project.id ? 'page' : undefined"
        >
          {{ project.title }}
        </RouterLink>
      </li>
    </ol>
  </nav>
</template>

<style scoped>
.project-navigation {
  display: grid;
  gap: 0.75rem;
  margin-bottom: 1rem;
  padding: 1rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: #f8fafc;
}
.project-navigation-actions,
.project-navigation-status {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.75rem;
}
.project-navigation-actions > a {
  color: var(--accent-dark);
  font-weight: 700;
}
.compact-project-list {
  display: flex;
  gap: 0.5rem;
  margin: 0;
  padding: 0;
  overflow-x: auto;
  list-style: none;
}
.compact-project-list a {
  display: block;
  padding: 0.45rem 0.65rem;
  border: 1px solid var(--line);
  border-radius: 6px;
  color: var(--accent-dark);
  text-decoration: none;
  white-space: nowrap;
}
.compact-project-list a[aria-current="page"] {
  border-color: var(--accent);
  background: #e8f1ff;
  color: var(--accent-dark);
}
.project-navigation-status {
  min-height: 1.5rem;
  margin: 0;
  color: var(--muted);
}
@media (max-width: 560px) {
  .compact-project-list {
    display: grid;
    overflow: visible;
  }
  .compact-project-list a {
    white-space: normal;
  }
}
</style>
