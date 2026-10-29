<script setup lang="ts">
import { formatDateTime } from "../shared/format.ts";
import type { AudioAssetSummary } from "../shared/project-types.ts";
import { useProjects } from "./projects.ts";

const projects = useProjects();

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
</script>

<template>
  <div class="project-browser">
    <p class="project-browser-status" data-testid="project-browser-status" role="status">
      {{ projects.status.value }}
    </p>
    <div>
      <button
        v-if="projects.view.value.kind === 'failure'"
        type="button"
        @click="projects.showList()"
      >
        {{ projects.view.value.retryLabel }}
      </button>

      <section v-else-if="projects.view.value.kind === 'list'" data-testid="project-list">
        <h2>Projects</h2>
        <p v-if="projects.view.value.projects.length === 0">No Projects yet.</p>
        <ol v-else class="project-list">
          <li
            v-for="project in projects.view.value.projects"
            :key="project.id"
            data-testid="project-list-item"
            :data-project-id="project.id"
          >
            <button type="button" @click="projects.showProject(project.id)">
              {{ project.title }}
            </button>
            <time :datetime="project.updatedAt">{{ formatDateTime(project.updatedAt) }}</time>
          </li>
        </ol>
      </section>

      <article
        v-else-if="projects.view.value.kind === 'detail'"
        data-testid="project-detail"
        :data-project-id="projects.view.value.project.id"
      >
        <button type="button" @click="projects.showList()">Back to Projects</button>
        <h2>{{ projects.view.value.project.title }}</h2>
        <p v-if="projects.view.value.project.document.sections.length === 0">
          This Project has no Sections.
        </p>
        <ol v-else class="project-sections">
          <li
            v-for="section in projects.view.value.project.document.sections"
            :key="section.id"
            data-testid="project-section"
            :data-section-id="section.id"
            :data-audio-asset-id="section.audioAsset.id"
            :data-sealed="String(section.audioAsset.isSealed)"
            :data-seal-reason="section.audioAsset.sealReason ?? 'open'"
          >
            <h3>Section {{ section.position + 1 }}</h3>
            <p class="project-section-state">{{ assetState(section.audioAsset) }}</p>
            <p class="project-section-diagnostics">{{ assetDiagnostics(section.audioAsset) }}</p>
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
    </div>
  </div>
</template>

<style scoped>
.project-browser-status {
  min-height: 1.5rem;
  color: var(--muted);
}
.project-browser-status:empty {
  min-height: 0;
}
.project-list,
.project-sections {
  display: grid;
  gap: 0.75rem;
  padding: 0;
  list-style: none;
}
.project-list > li,
.project-sections > li {
  padding: 1rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
}
.project-list > li {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
}
.project-list button {
  color: var(--accent-dark);
  text-align: left;
}
.project-list time,
.project-section-diagnostics {
  color: var(--muted);
  font-size: 0.85rem;
}
.project-sections {
  margin-top: 1rem;
}
.project-sections h3 {
  margin-bottom: 0.5rem;
}
.project-section-state,
.project-section-diagnostics {
  margin-bottom: 0.65rem;
}
.download-control {
  display: inline-block;
  min-height: 2.5rem;
  padding: 0.55rem 0.85rem;
  border: 1px solid var(--accent);
  border-radius: 6px;
  background: var(--accent);
  color: white;
  font-weight: 650;
  text-decoration: none;
}
.download-control:hover {
  background: var(--accent-dark);
  border-color: var(--accent-dark);
}
.download-control:focus-visible {
  outline: 3px solid var(--focus-ring);
  outline-offset: 2px;
}
@media (max-width: 560px) {
  .project-list > li {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
