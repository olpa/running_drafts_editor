<script setup lang="ts">
import { computed, onBeforeUnmount } from "vue";
import { connectBrowserPlaybackMedia, type PlaybackMedia } from "./playback-media.ts";
import {
  formatPlaybackTime,
  playbackFailureMessage,
  type JumpDirection,
} from "./section-playback.ts";
import { useProjects } from "./projects.ts";

const props = defineProps<{
  audioAssetId: string;
  frameCount: number;
  sectionNumber: number;
}>();

const playback = useProjects().createPlayback(props.audioAssetId);
const state = playback.state;
let connected: PlaybackMedia | null = null;
let connectedElement: globalThis.HTMLAudioElement | null = null;

const jumpOptions = [
  { seconds: 10, label: "10 seconds" },
  { seconds: 60, label: "1 minute" },
  { seconds: 300, label: "5 minutes" },
  { seconds: 600, label: "10 minutes" },
  { seconds: 900, label: "15 minutes" },
] as const;

const position = computed(() => {
  const showHours = state.value.duration >= 3_600;
  return `${formatPlaybackTime(state.value.position, showHours)} of ${formatPlaybackTime(state.value.duration, showHours)}`;
});
const jumpLabel = computed(() => {
  const option = jumpOptions.find((candidate) => candidate.seconds === state.value.jumpSeconds);
  return option?.label ?? `${state.value.jumpSeconds} seconds`;
});

function connectElement(value: unknown): void {
  const element = value instanceof globalThis.HTMLAudioElement ? value : null;
  if (element === connectedElement) return;

  if (connected) {
    playback.disconnect(connected);
    connected = null;
  }
  connectedElement = element;
  if (!element) return;
  connected = connectBrowserPlaybackMedia(element, playback.mediaEvents);
  playback.connect(connected);
}

function chooseJump(direction: JumpDirection, seconds: number): void {
  playback.jump(direction, seconds);
}

onBeforeUnmount(() => playback.dispose());
</script>

<template>
  <div class="section-player" @keydown.esc="playback.closeJumpMenu()">
    <p v-if="frameCount === 0" class="empty-audio">No audio is available yet.</p>
    <button
      v-else-if="!state.activated"
      type="button"
      class="primary"
      :aria-label="`Play audio for Section ${sectionNumber}`"
      @click="playback.activate()"
    >
      Play audio
    </button>

    <template v-else>
      <Transition name="player-reload" mode="out-in">
        <audio
          v-if="state.url"
          :key="state.generation"
          :ref="connectElement"
          class="native-player"
          :src="state.url"
          controls
          controlslist="nodownload"
          crossorigin="use-credentials"
          preload="metadata"
        >
          This browser does not support audio playback.
        </audio>
      </Transition>

      <p v-if="state.status" class="playback-status" role="status">{{ state.status }}</p>

      <div v-if="state.url" class="playback-toolbar" aria-label="Audio navigation controls">
        <div class="split-control">
          <button
            type="button"
            :disabled="!state.controlsReady || Boolean(state.failure)"
            @click="playback.jump('back')"
          >
            Back {{ jumpLabel }}
          </button>
          <button
            type="button"
            class="menu-arrow"
            aria-label="Choose back distance"
            aria-haspopup="menu"
            :aria-expanded="state.jumpMenu === 'back'"
            :disabled="!state.controlsReady || Boolean(state.failure)"
            @click="playback.toggleJumpMenu('back')"
          >
            ▾
          </button>
          <ul v-if="state.jumpMenu === 'back'" class="jump-menu" role="menu">
            <li v-for="option in jumpOptions" :key="option.seconds" role="none">
              <button type="button" role="menuitem" @click="chooseJump('back', option.seconds)">
                {{ option.label }}
              </button>
            </li>
          </ul>
        </div>

        <div class="split-control">
          <button
            type="button"
            :disabled="!state.controlsReady || Boolean(state.failure)"
            @click="playback.jump('forward')"
          >
            Forward {{ jumpLabel }}
          </button>
          <button
            type="button"
            class="menu-arrow"
            aria-label="Choose forward distance"
            aria-haspopup="menu"
            :aria-expanded="state.jumpMenu === 'forward'"
            :disabled="!state.controlsReady || Boolean(state.failure)"
            @click="playback.toggleJumpMenu('forward')"
          >
            ▾
          </button>
          <ul v-if="state.jumpMenu === 'forward'" class="jump-menu" role="menu">
            <li v-for="option in jumpOptions" :key="option.seconds" role="none">
              <button type="button" role="menuitem" @click="chooseJump('forward', option.seconds)">
                {{ option.label }}
              </button>
            </li>
          </ul>
        </div>

        <div class="go-to-control">
          <button
            type="button"
            :disabled="!state.controlsReady || Boolean(state.failure)"
            :aria-expanded="state.goToOpen"
            @click="playback.openGoTo()"
          >
            Go to time…
          </button>
          <form v-if="state.goToOpen" class="go-to-popover" @submit.prevent="playback.goToTime()">
            <label :for="`go-to-${audioAssetId}`">Time</label>
            <input
              :id="`go-to-${audioAssetId}`"
              :value="state.goToValue"
              placeholder="H:MM:SS"
              inputmode="numeric"
              :aria-describedby="state.goToError ? `go-to-error-${audioAssetId}` : undefined"
              @input="playback.setGoToValue(($event.target as HTMLInputElement).value)"
            />
            <p v-if="state.goToError" :id="`go-to-error-${audioAssetId}`" class="field-error">
              {{ state.goToError }}
            </p>
            <div class="popover-actions">
              <button type="submit">Go</button>
              <button type="button" @click="playback.closeGoTo()">Cancel</button>
            </div>
          </form>
        </div>

        <label class="speed-control">
          Speed
          <select
            :value="state.rate"
            :disabled="!state.controlsReady || Boolean(state.failure)"
            @change="playback.setRate(Number(($event.target as HTMLSelectElement).value))"
          >
            <option :value="0.75">0.75×</option>
            <option :value="1">1×</option>
            <option :value="1.25">1.25×</option>
            <option :value="1.5">1.5×</option>
            <option :value="2">2×</option>
          </select>
        </label>
        <p class="position">Position {{ position }}</p>
      </div>

      <section v-if="state.failure" class="playback-failure" role="alert">
        <h4>Audio could not be played</h4>
        <p>{{ playbackFailureMessage(state.failure.reason) }}</p>
        <p v-if="state.failure.checking">Checking the server…</p>
        <p v-if="state.failure.referenceId" class="reference">
          Reference ID: {{ state.failure.referenceId }}
        </p>
        <div class="failure-actions">
          <button type="button" @click="playback.retry()">Retry</button>
          <button
            v-if="state.failure.reason === 'auth'"
            type="button"
            @click="playback.signInAgain()"
          >
            Sign in again
          </button>
        </div>
      </section>
    </template>
  </div>
</template>

<style scoped>
.section-player {
  display: grid;
  gap: 0.65rem;
  margin: 0.9rem 0;
}
.native-player {
  width: 100%;
}
.player-reload-enter-active,
.player-reload-leave-active {
  transform-origin: center;
}
.player-reload-leave-active {
  transition:
    opacity 180ms ease-in,
    transform 180ms ease-in;
}
.player-reload-enter-active {
  transition:
    opacity 260ms ease-out 60ms,
    transform 260ms cubic-bezier(0.2, 0.8, 0.2, 1) 60ms;
  animation: player-reload-highlight 520ms ease-out 60ms;
}
.player-reload-enter-from {
  opacity: 0;
  transform: translateY(0.55rem) scale(0.9);
}
.player-reload-leave-to {
  opacity: 0;
  transform: translateY(-0.55rem) scale(0.9);
}
@keyframes player-reload-highlight {
  0%,
  30% {
    box-shadow: 0 0 0 0 rgb(31 111 235 / 0%);
  }
  55% {
    box-shadow: 0 0 0 4px rgb(31 111 235 / 42%);
  }
  100% {
    box-shadow: 0 0 0 7px rgb(31 111 235 / 0%);
  }
}
@media (prefers-reduced-motion: reduce) {
  .player-reload-enter-active,
  .player-reload-leave-active {
    transition: none;
    animation: none;
  }
}
.playback-status,
.position,
.empty-audio {
  margin: 0;
  color: var(--muted);
}
.playback-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  gap: 0.55rem;
}
.split-control,
.go-to-control {
  position: relative;
  display: flex;
}
.split-control > button:first-child {
  border-radius: 6px 0 0 6px;
}
.split-control > .menu-arrow {
  min-width: 2.5rem;
  padding-right: 0.55rem;
  padding-left: 0.55rem;
  border-left: 0;
  border-radius: 0 6px 6px 0;
}
.jump-menu,
.go-to-popover {
  position: absolute;
  z-index: 2;
  top: calc(100% + 0.25rem);
  left: 0;
  min-width: 11rem;
  margin: 0;
  padding: 0.35rem;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--panel);
  box-shadow: var(--shadow);
  list-style: none;
}
.jump-menu button {
  width: 100%;
  border: 0;
  text-align: left;
}
.go-to-popover {
  display: grid;
  gap: 0.35rem;
  min-width: 16rem;
  padding: 0.75rem;
}
.go-to-popover input,
.speed-control select {
  min-height: 2.5rem;
  padding: 0.45rem;
  border: 1px solid var(--line);
  border-radius: 6px;
  font: inherit;
}
.popover-actions,
.failure-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}
.speed-control {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  font-weight: 650;
}
.position {
  align-self: center;
  white-space: nowrap;
}
.field-error {
  margin: 0;
  color: var(--danger);
  font-size: 0.9rem;
}
.playback-failure {
  padding: 0.8rem;
  border-left: 4px solid var(--danger);
  background: #fff7f7;
}
.playback-failure h4,
.playback-failure p {
  margin: 0 0 0.5rem;
}
.reference {
  font-family: ui-monospace, monospace;
  font-size: 0.85rem;
}
</style>
