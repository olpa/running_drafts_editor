<script setup lang="ts">
import { RouterLink, useRoute } from "vue-router";
import { formatDateTime } from "../shared/format.ts";
import { useProjects } from "./projects.ts";

const projects = useProjects();
const route = useRoute();
</script>

<template>
  <section data-testid="project-list-page" aria-labelledby="projects-title">
    <h2 id="projects-title" tabindex="-1">Projects</h2>
    <div v-if="projects.list.value.kind === 'loading'" class="route-skeleton" aria-hidden="true">
      <span></span><span></span><span></span>
    </div>
    <p
      v-if="projects.list.value.kind === 'loading' && projects.list.value.slow"
      data-testid="project-route-status"
      role="status"
    >
      Loading Projects…
    </p>
    <div v-else-if="projects.list.value.kind === 'failure'" role="alert">
      <p>Projects could not be loaded.</p>
      <button type="button" @click="projects.loadList()">Retry Projects</button>
    </div>
    <template v-else-if="projects.list.value.kind === 'ready'">
      <p v-if="projects.list.value.value.length === 0">No Projects yet.</p>
      <ol v-else class="project-list" data-testid="project-list">
        <li
          v-for="project in projects.list.value.value"
          :key="project.id"
          data-testid="project-list-item"
          :data-project-id="project.id"
        >
          <RouterLink
            :to="{
              name: 'project-detail',
              params: { projectId: project.id },
              query: route.query,
              hash: route.hash,
            }"
          >
            {{ project.title }}
          </RouterLink>
          <time :datetime="project.updatedAt">{{ formatDateTime(project.updatedAt) }}</time>
        </li>
      </ol>
    </template>
  </section>
</template>

<style scoped>
.project-list {
  display: grid;
  gap: 0.75rem;
  padding: 0;
  list-style: none;
}
.project-list > li {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 1rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
}
.project-list a {
  color: var(--accent-dark);
}
.project-list time {
  color: var(--muted);
  font-size: 0.85rem;
}
.route-skeleton {
  display: grid;
  gap: 0.75rem;
}
.route-skeleton span {
  height: 3.5rem;
  border-radius: 8px;
  background: #eef2f7;
}
@media (max-width: 560px) {
  .project-list > li {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
