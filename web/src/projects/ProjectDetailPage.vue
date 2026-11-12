<script setup lang="ts">
import { watchEffect } from "vue";
import { RouterLink, useRoute } from "vue-router";
import { formatDateTime } from "../shared/format.ts";
import type { AudioAssetSummary } from "../shared/project-types.ts";
import { useProjects } from "./projects.ts";

const props = defineProps<{ projectId: string }>();
defineEmits<{ recordMore: [projectId: string] }>();

const projects = useProjects();
const route = useRoute();

function assetState(asset: AudioAssetSummary): string {
  if (!asset.isSealed) return "Recording is still open and cannot be downloaded.";
  return asset.sealReason === "interrupted"
    ? "Interrupted audio is sealed and available to download."
    : "Completed audio is sealed and available to download.";
}

function assetDiagnostics(asset: AudioAssetSummary): string {
  const duration = (asset.observedDurationMs / 1_000).toFixed(1);
  const sealed = asset.sealedAt ? ` · sealed ${formatDateTime(asset.sealedAt)}` : "";
  return `${asset.frameCount} frames · ${asset.storedBytes} bytes · ${duration} seconds${sealed}`;
}

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
  <section data-testid="project-detail-page" aria-labelledby="project-detail-title">
    <div v-if="projects.detail.value.kind === 'loading'">
      <h2 id="project-detail-title" tabindex="-1">Project</h2>
      <div class="route-skeleton" aria-hidden="true"><span></span><span></span></div>
      <p v-if="projects.detail.value.slow" role="status">Loading Project…</p>
    </div>
    <div v-else-if="projects.detail.value.kind === 'notFound'" data-testid="project-not-found">
      <h2 id="project-detail-title" tabindex="-1">Project not found</h2>
      <p>This Project does not exist or is not available to this Account.</p>
      <RouterLink :to="{ name: 'projects', query: route.query, hash: route.hash }">
        Back to Projects
      </RouterLink>
    </div>
    <div v-else-if="projects.detail.value.kind === 'failure'" role="alert">
      <h2 id="project-detail-title" tabindex="-1">Project could not be loaded</h2>
      <button type="button" @click="projects.loadProject(props.projectId)">Retry Project</button>
      <RouterLink :to="{ name: 'projects', query: route.query, hash: route.hash }">
        Back to Projects
      </RouterLink>
    </div>
    <article
      v-else-if="projects.detail.value.kind === 'ready'"
      data-testid="project-detail"
      :data-project-id="projects.detail.value.value.id"
    >
      <header class="detail-heading">
        <h2 id="project-detail-title" tabindex="-1">{{ projects.detail.value.value.title }}</h2>
        <button
          type="button"
          class="primary"
          data-testid="record-more"
          @click="$emit('recordMore', projects.detail.value.value.id)"
        >
          Record more
        </button>
      </header>
      <p v-if="projects.detail.value.value.document.sections.length === 0">
        This Project has no Sections.
      </p>
      <ol v-else class="project-sections">
        <li
          v-for="section in projects.detail.value.value.document.sections"
          :key="section.id"
          data-testid="project-section"
          :data-section-id="section.id"
          :data-audio-asset-id="section.audioAsset.id"
          :data-sealed="String(section.audioAsset.isSealed)"
          :data-seal-reason="section.audioAsset.sealReason ?? 'open'"
        >
          <h3>Section {{ section.position + 1 }}</h3>
          <p>{{ assetState(section.audioAsset) }}</p>
          <p class="diagnostics">{{ assetDiagnostics(section.audioAsset) }}</p>
          <a
            v-if="section.audioAsset.isSealed"
            class="download-control"
            data-testid="audio-download"
            :href="projects.audioDownloadUrl(section.audioAsset.id)"
          >
            {{
              section.audioAsset.sealReason === "interrupted"
                ? "Download interrupted original audio"
                : "Download original audio"
            }}
          </a>
        </li>
      </ol>
    </article>
  </section>
</template>

<style scoped>
.detail-heading {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
}
.project-sections {
  display: grid;
  gap: 0.75rem;
  padding: 0;
  list-style: none;
}
.project-sections > li {
  padding: 1rem;
  border: 1px solid var(--line);
  border-radius: 8px;
}
.diagnostics {
  color: var(--muted);
  font-size: 0.85rem;
}
.download-control {
  display: inline-block;
  padding: 0.55rem 0.85rem;
  border-radius: 6px;
  background: var(--accent);
  color: white;
  font-weight: 650;
  text-decoration: none;
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
