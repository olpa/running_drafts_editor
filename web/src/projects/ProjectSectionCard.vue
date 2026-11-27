<script setup lang="ts">
import { computed, onBeforeUnmount, ref, useId } from "vue";
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
const actions = ref<globalThis.HTMLButtonElement | null>(null);
const actionsOpen = ref(false);
const actionsId = useId();
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

function closeActions(restoreFocus = false): void {
  if (!actionsOpen.value) return;
  actionsOpen.value = false;
  if (restoreFocus) actions.value?.focus();
}

function handleActionsFocusOut(event: globalThis.FocusEvent): void {
  const container = event.currentTarget;
  if (
    container instanceof globalThis.HTMLElement &&
    event.relatedTarget instanceof globalThis.Node &&
    container.contains(event.relatedTarget)
  ) {
    return;
  }
  closeActions();
}

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
    <h2 ref="heading" class="section-title" tabindex="-1">Section {{ section.position + 1 }}</h2>
    <SectionAudioPlayer
      v-if="!unavailable && !empty"
      class="row-player"
      :audio-asset-id="asset.id"
      :frame-count="asset.frameCount"
      :section-number="section.position + 1"
    />
    <div
      v-if="!unavailable && asset.isSealed !== false && !empty"
      class="section-actions"
      @focusout="handleActionsFocusOut"
      @keydown.esc.stop="closeActions(true)"
    >
      <button
        ref="actions"
        type="button"
        :aria-label="`Actions for Section ${section.position + 1}`"
        :aria-expanded="actionsOpen"
        :aria-controls="actionsId"
        @click="actionsOpen = !actionsOpen"
      >
        <span aria-hidden="true">⋮</span>
      </button>
      <div v-show="actionsOpen" :id="actionsId" class="actions-popup">
        <a
          data-testid="audio-download"
          :href="projects.audioDownloadUrl(asset.id)"
          @click="closeActions(true)"
        >
          Download
        </a>
      </div>
    </div>
    <div v-if="(open && !unavailable) || message || buttonLabel" class="unfinished">
      <template v-if="open && !unavailable">
        <h3>Unfinished audio</h3>
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
      <p v-if="empty" class="empty-audio">No audio has been secured yet</p>
    </template>
  </li>
</template>

<style scoped>
.project-section {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 4.5rem 3rem;
  align-items: center;
  column-gap: 0.5rem;
  padding: 0.35rem 0.65rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
}
.section-title {
  grid-area: 1 / 1;
  margin: 0;
  padding-inline-start: 0.25rem;
  font-size: 1rem;
  overflow-wrap: anywhere;
}
.row-player {
  display: contents;
}
.row-player :deep(.play-audio) {
  grid-area: 1 / 2;
  justify-self: stretch;
}
.row-player :deep(.playback-controls),
.unfinished,
.asset-error,
.empty-audio {
  grid-column: 1 / -1;
  margin-block: 0.5rem;
}
.section-actions {
  position: relative;
  grid-area: 1 / 3;
}
.section-actions button {
  width: 100%;
  min-height: 48px;
  padding: 0;
  font-size: 1.35rem;
}
.actions-popup {
  position: absolute;
  z-index: 1;
  inset-block-start: calc(100% + 0.25rem);
  inset-inline-end: 0;
  min-width: 8rem;
  padding: 0.25rem;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--panel);
  box-shadow: var(--shadow);
}
.actions-popup a {
  display: flex;
  align-items: center;
  min-height: 44px;
  padding-inline: 0.75rem;
  border-radius: 4px;
  color: var(--accent-dark);
  text-decoration: none;
}
.actions-popup a:hover,
.actions-popup a:focus-visible {
  background: #e8f1ff;
}
.unfinished {
  padding: 0.75rem;
  border: 1px solid var(--line);
  border-radius: 6px;
  margin-bottom: 0.5rem;
  overflow-wrap: anywhere;
}
.unfinished h3 {
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
</style>
