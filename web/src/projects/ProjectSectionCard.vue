<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import { formatDateTime } from "../shared/format.ts";
import type { SectionSummary } from "./gateway.ts";
import { useProjects } from "./projects.ts";
import { withSectionActionFocus } from "./section-action-focus.ts";
import SectionAudioPlayer from "./SectionAudioPlayer.vue";

const props = defineProps<{ section: SectionSummary; serverTime: string }>();
const projects = useProjects();
const resolution = projects.createAssetResolution({
  audioAsset: props.section.audioAsset,
  serverTime: props.serverTime,
});
const {
  asset,
  open,
  validCount,
  recent,
  lastActivity,
  busy,
  unavailable,
  auth,
  retryRead,
  message,
  announcement,
} = resolution;
const heading = ref<globalThis.HTMLElement | null>(null);
const action = ref<globalThis.HTMLButtonElement | null>(null);
const empty = computed(() => asset.value.frameCount === 0);
const metadataConflict = computed(() => {
  const reason = asset.value.sealReason;
  return (
    (asset.value.isSealed === false && reason != null) ||
    (asset.value.isSealed === true &&
      reason !== "completed" &&
      reason !== "interrupted" &&
      reason !== "user_resolved")
  );
});
const buttonLabel = computed(() => {
  if (unavailable.value || auth.value) return "";
  if (retryRead.value) return "Retry refresh";
  if (!open.value) return "";
  return recent.value ? "Check again" : "Confirm recording has ended";
});
const signInUrl = computed(() => projects.signInUrl());

function activate(event: globalThis.MouseEvent): Promise<void> {
  return withSectionActionFocus(
    event,
    retryRead.value || recent.value ? resolution.refresh : resolution.confirm,
    () => action.value ?? heading.value,
  );
}

onBeforeUnmount(resolution.dispose);
</script>

<template>
  <li
    class="project-section"
    data-testid="project-section"
    :data-section-id="section.id"
    :data-audio-asset-id="asset.id"
    :data-sealed="String(!open)"
    :data-seal-reason="asset.sealReason ?? (open ? 'open' : undefined)"
  >
    <h3 ref="heading" tabindex="-1">Section {{ section.position + 1 }}</h3>
    <div v-if="(open && !unavailable) || message || buttonLabel" class="unfinished">
      <template v-if="open && !unavailable">
        <h4>Unfinished audio</h4>
        <p>
          This recording may still be running in another browser, or its page may have closed before
          it finished.{{ empty ? "" : " Audio secured so far is available to play." }}
        </p>
        <p v-if="!empty">
          {{ (asset.observedDurationMs / 1000).toFixed(1) }} seconds secured
          <span v-if="lastActivity !== null">
            · Last activity {{ formatDateTime(new Date(lastActivity).toISOString()) }}
          </span>
        </p>
        <p v-if="!validCount" class="asset-error" role="alert">
          Could not load the frame count for this audio.
        </p>
        <p v-if="recent">
          Audio activity was recorded less than a minute ago. Check again before confirming that
          recording has ended.
        </p>
        <p v-else-if="validCount && !retryRead && !auth">
          Confirm only if you know recording has ended. This prevents any browser from adding more
          audio. Unsecured audio in another browser will not be included.
        </p>
      </template>
      <p v-if="message" class="asset-error" role="alert">{{ message }}</p>
      <a v-if="auth && signInUrl" :href="signInUrl">Sign in</a>
      <button
        v-if="buttonLabel"
        ref="action"
        type="button"
        :aria-disabled="busy || (!retryRead && !recent && !validCount)"
        @click="activate"
      >
        {{ buttonLabel }}
      </button>
    </div>
    <p role="status" class="seal-announcement">{{ announcement }}</p>
    <template v-if="!unavailable">
      <p v-if="metadataConflict" class="asset-error" role="alert">
        Some audio status information could not be loaded.
      </p>
      <p v-if="empty">No audio has been secured yet</p>
      <SectionAudioPlayer
        v-else
        :audio-asset-id="asset.id"
        :frame-count="asset.frameCount"
        :section-number="section.position + 1"
      />
      <a
        v-if="asset.isSealed !== false && !empty"
        class="download-control"
        data-testid="audio-download"
        :href="projects.audioDownloadUrl(asset.id)"
      >
        Download original audio
      </a>
    </template>
  </li>
</template>

<style scoped>
.project-section {
  padding: 1rem;
  border: 1px solid var(--line);
  border-radius: 8px;
}
.unfinished {
  padding: 0.75rem;
  border: 1px solid var(--line);
  border-radius: 6px;
  margin-bottom: 0.5rem;
  overflow-wrap: anywhere;
}
.unfinished h4 {
  margin-top: 0;
}
.unfinished p:last-child {
  margin-bottom: 0;
}
.asset-error {
  padding: 0.6rem 0.75rem;
  border-inline-start: 4px solid var(--danger);
  background: #fff7f7;
  color: var(--danger);
}
.seal-announcement {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
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
</style>
