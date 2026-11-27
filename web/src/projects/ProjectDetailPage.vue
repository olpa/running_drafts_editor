<script setup lang="ts">
import { computed, watchEffect } from "vue";
import { RouterLink, useRoute } from "vue-router";
import { useProjects } from "./projects.ts";
import ProjectSectionCard from "./ProjectSectionCard.vue";

const props = defineProps<{ projectId: string }>();
defineEmits<{ recordMore: [projectId: string] }>();

const projects = useProjects();
const route = useRoute();
const projectsLocation = computed(() => ({
  name: "projects",
  query: route.query,
  hash: route.hash,
}));

watchEffect(() => {
  const state = projects.detail.value;
  globalThis.document.title =
    state.kind === "ready"
      ? `${state.value.title} — Running Drafts`
      : state.kind === "notFound"
        ? "Project not found — Running Drafts"
        : "Running Drafts";
});
</script>

<template>
  <section
    class="project-detail-page"
    data-testid="project-detail-page"
    aria-labelledby="project-detail-title"
  >
    <div v-if="projects.detail.value.kind === 'loading'">
      <h1 id="project-detail-title" tabindex="-1">Project</h1>
      <div class="route-skeleton" aria-hidden="true"><span></span><span></span></div>
      <p v-if="projects.detail.value.slow" role="status">Loading Project…</p>
    </div>
    <div v-else-if="projects.detail.value.kind === 'notFound'" data-testid="project-not-found">
      <h1 id="project-detail-title" tabindex="-1">Project not found</h1>
      <p>This Project is unavailable.</p>
      <RouterLink class="back-link" :to="projectsLocation">
        <span aria-hidden="true">«</span>
        Back to all projects
      </RouterLink>
    </div>
    <div v-else-if="projects.detail.value.kind === 'failure'" role="alert">
      <h1 id="project-detail-title" tabindex="-1">Project could not be loaded</h1>
      <button type="button" @click="projects.loadProject(props.projectId)">Retry Project</button>
      <RouterLink class="back-link" :to="projectsLocation">
        <span aria-hidden="true">«</span>
        Back to all projects
      </RouterLink>
    </div>
    <article
      v-else-if="projects.detail.value.kind === 'ready'"
      data-testid="project-detail"
      :data-project-id="projects.detail.value.value.id"
    >
      <RouterLink class="back-link" :to="projectsLocation">
        <span aria-hidden="true">«</span>
        Back to all projects
      </RouterLink>
      <h1 id="project-detail-title" tabindex="-1">{{ projects.detail.value.value.title }}</h1>
      <p v-if="projects.detail.value.value.document.sections.length === 0" class="empty-sections">
        No recordings yet.
      </p>
      <ol v-else class="project-sections" aria-label="Project sections">
        <ProjectSectionCard
          v-for="section in projects.detail.value.value.document.sections"
          :key="section.audioAsset.id"
          :section="section"
          :server-time="projects.detail.value.value.serverTime"
        />
      </ol>
      <div class="record-more">
        <button
          type="button"
          class="primary"
          data-testid="record-more"
          aria-describedby="record-more-help"
          @click="$emit('recordMore', projects.detail.value.value.id)"
        >
          Record more
        </button>
        <p id="record-more-help">Adds audio to this Project.</p>
      </div>
      <RouterLink class="back-link" :to="projectsLocation">
        <span aria-hidden="true">«</span>
        Back to all projects
      </RouterLink>
    </article>
  </section>
</template>

<style scoped>
.project-detail-page {
  max-width: 48rem;
}
article {
  display: grid;
  gap: 1.25rem;
}
h1 {
  margin: 0;
  font-size: clamp(1.5rem, 4vw, 2rem);
  overflow-wrap: anywhere;
}
.back-link {
  display: inline-flex;
  align-items: center;
  justify-self: start;
  gap: 0.5rem;
  min-height: 48px;
  padding: 0.5rem 0.9rem;
  border: 1px solid #aabfdb;
  border-radius: 6px;
  background: var(--panel);
  color: var(--accent-dark);
  font-weight: 650;
  text-decoration: none;
}
.back-link:hover {
  border-color: var(--accent-dark);
  background: #e8f1ff;
}
.back-link span {
  font-size: 1.25em;
  line-height: 1;
}
.record-more {
  padding-block: 1rem;
  border-block: 1px solid var(--line);
}
.record-more button {
  min-height: 48px;
}
#record-more-help,
.empty-sections {
  margin: 0.5rem 0 0;
  color: var(--muted);
}
.project-sections {
  display: grid;
  gap: 0.75rem;
  margin: 0.75rem 0 0;
  padding: 0;
  list-style: none;
}
.route-skeleton {
  display: grid;
  gap: 0.75rem;
}
.route-skeleton span {
  height: 5rem;
  border-radius: 8px;
  background: #eef2f7;
}
</style>
