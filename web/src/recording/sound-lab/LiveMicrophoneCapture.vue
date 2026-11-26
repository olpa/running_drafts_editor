<script setup lang="ts">
import { onBeforeUnmount, provide, shallowRef } from "vue";
import { createCaptureAlerts } from "../../alerts/capture-alerts.ts";
import type { AccountId } from "../../session/gateway.ts";
import { DEFAULT_CONFIG, configKey } from "../../shared/config.ts";
import type { AudioAssetSummary } from "../../shared/project-types.ts";
import { createApplicationCapture, applicationCaptureKey } from "../application-capture.ts";
import {
  openMicrophoneCapture,
  type CaptureOptions,
  type MicrophoneInputSettings,
  type TransportFrame,
} from "../capture.ts";
import { CuePlayer } from "../cues.ts";
import type { RecordingGateway } from "../gateway.ts";
import { createMicrophoneAccess } from "../microphone-access.ts";
import RecordingPanel from "../RecordingPanel.vue";

const accountId = "5728cdd2-909d-4207-8303-c04b270c308f" as AccountId;
const cues = new CuePlayer();
const alerts = createCaptureAlerts(cues);
const microphoneAccess = createMicrophoneAccess();
let frameCount = 0;
let observedDurationMs = 0;
const inputSettings = shallowRef<Readonly<MicrophoneInputSettings> | null>(null);

function setting(value: boolean | string | null): string {
  if (value === null) return "Not reported";
  if (typeof value === "string") return value;
  return value ? "On" : "Off";
}

async function openLiveCapture(options: CaptureOptions) {
  const capture = await openMicrophoneCapture(options);
  inputSettings.value = capture.inputSettings?.() ?? null;
  return capture;
}

function asset(reason: "completed" | "interrupted"): AudioAssetSummary {
  return {
    id: "live-story-asset",
    mediaType: "audio/webm;codecs=opus",
    isSealed: true,
    sealReason: reason,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    frameCount,
    observedDurationMs,
    storedBytes: 0,
  };
}

const gateway: RecordingGateway = {
  async createProject() {
    return {
      id: "live-story-project",
      title: "Live microphone review",
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
  },
  async getProject() {
    throw new Error("The live microphone story does not load saved Projects.");
  },
  async createSection() {
    return {
      sectionId: "live-story-section",
      audioAssetId: "live-story-asset",
      position: 0,
    };
  },
  async putFrame(_audioAssetId, frame: TransportFrame) {
    frameCount = Math.max(frameCount, frame.seq + 1);
    observedDurationMs = Math.max(observedDurationMs, frame.endMs);
    return {
      sequence: frame.seq,
      acknowledged: true,
      sha256: "a".repeat(64),
      nextExpectedSequence: frame.seq + 1,
    };
  },
  async complete() {
    return asset("completed");
  },
  async interrupt() {
    return asset("interrupted");
  },
};

const recording = createApplicationCapture({
  config: DEFAULT_CONFIG,
  cues,
  alerts,
  authorizeStart: () => true,
  environment: {
    gateway,
    openCapture: openLiveCapture,
    now: () => globalThis.performance.now(),
    microphoneAccess,
    uuid: () => globalThis.crypto.randomUUID(),
  },
});
recording.setSessionAccount(accountId);

provide(applicationCaptureKey, recording);
provide(configKey, DEFAULT_CONFIG);

onBeforeUnmount(() => {
  recording.dispose();
  alerts.dispose();
  microphoneAccess.dispose();
  cues.dispose();
});
</script>

<template>
  <main class="live-microphone-story">
    <header>
      <h1>Live microphone level</h1>
      <p><strong>Uses your real microphone; audio is not saved.</strong></p>
      <p>
        Select Start, allow microphone access, and speak at different levels. Select Stop when the
        meter review is complete.
      </p>
      <dl v-if="inputSettings" class="input-settings" aria-label="Browser microphone processing">
        <div>
          <dt>Automatic gain</dt>
          <dd>{{ setting(inputSettings.autoGainControl) }}</dd>
        </div>
        <div>
          <dt>Noise suppression</dt>
          <dd>{{ setting(inputSettings.noiseSuppression) }}</dd>
        </div>
        <div>
          <dt>Echo cancellation</dt>
          <dd>{{ setting(inputSettings.echoCancellation) }}</dd>
        </div>
      </dl>
    </header>
    <RecordingPanel />
  </main>
</template>

<style scoped>
.live-microphone-story {
  width: min(52rem, calc(100% - 2rem));
  margin: 2rem auto;
  padding: 1.5rem;
  border-radius: 10px;
  background: var(--panel);
  box-shadow: var(--shadow);
}
.live-microphone-story header {
  margin-bottom: 1.5rem;
}
.live-microphone-story h1 {
  margin-top: 0;
}
.input-settings {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem 1.5rem;
  margin: 1rem 0 0;
}
.input-settings div {
  display: flex;
  gap: 0.4rem;
}
.input-settings dt {
  font-weight: 650;
}
.input-settings dd {
  margin: 0;
}
</style>
