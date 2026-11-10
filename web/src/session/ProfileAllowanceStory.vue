<script setup lang="ts">
import { onBeforeUnmount, onMounted, provide, shallowRef, watch } from "vue";
import { createCaptureAlerts } from "../alerts/capture-alerts.ts";
import {
  createApplicationCapture,
  applicationCaptureKey,
} from "../recording/application-capture.ts";
import type { Cues } from "../recording/cues.ts";
import type { RecordingGateway } from "../recording/gateway.ts";
import { DEFAULT_CONFIG } from "../shared/config.ts";
import { RadarLog } from "../shared/radar-log.ts";
import RadarLogView from "../shared/RadarLogView.vue";
import type { UsageSnapshot } from "../usage/gateway.ts";
import { createUsage, usageKey } from "../usage/usage.ts";
import ProfileWidget from "./ProfileWidget.vue";
import { createStorySessionGateway, createStorySessionLocation } from "./story-gateway.ts";
import { createSession, sessionKey } from "./session.ts";

type ProfileAllowanceState = "free" | "pro" | "grants" | "warning";
const props = defineProps<{ state: ProfileAllowanceState }>();

const MIB = 1024 * 1024;
const snapshots: Record<ProfileAllowanceState, UsageSnapshot> = {
  free: {
    plan: "free",
    audioStorage: {
      usedBytes: 32 * MIB,
      grantedBytes: 0,
      warningAtBytes: 107_374_182,
      limitBytes: 134_217_728,
    },
    audioAssetDuration: { grantedMs: 0, warningAtMs: 2_880_000, limitMs: 3_600_000 },
  },
  pro: {
    plan: "pro",
    audioStorage: {
      usedBytes: 512 * MIB,
      grantedBytes: 0,
      warningAtBytes: 858_993_459,
      limitBytes: 1_073_741_824,
    },
    audioAssetDuration: { grantedMs: 0, warningAtMs: 11_520_000, limitMs: 14_400_000 },
  },
  grants: {
    plan: "free",
    audioStorage: {
      usedBytes: 64 * MIB,
      grantedBytes: 64 * MIB,
      warningAtBytes: 174_483_046,
      limitBytes: 201_326_592,
    },
    audioAssetDuration: {
      grantedMs: 1_800_000,
      warningAtMs: 4_680_000,
      limitMs: 5_400_000,
    },
  },
  warning: {
    plan: "free",
    audioStorage: {
      usedBytes: 107_374_182,
      grantedBytes: 0,
      warningAtBytes: 107_374_182,
      limitBytes: 134_217_728,
    },
    audioAssetDuration: { grantedMs: 0, warningAtMs: 2_880_000, limitMs: 3_600_000 },
  },
};

const log = new RadarLog(
  DEFAULT_CONFIG.logColumns,
  DEFAULT_CONFIG.logRows,
  DEFAULT_CONFIG.logGapChars,
);
const logView = shallowRef(log.view());
const openedAt = globalThis.performance.now();
function write(message: string): void {
  const seconds = ((globalThis.performance.now() - openedAt) / 1000).toFixed(1);
  log.write(`${seconds} ${message}`);
  logView.value = log.view();
}
function unexpected(message: string): never {
  write(`ERR unexpected ${message}`);
  throw new Error(`Unexpected Profile allowance story activity: ${message}`);
}

const session = createSession(
  createStorySessionGateway(
    {
      initialState: "signedIn",
      displayName: "Maya Chen",
      verifiedEmail: "maya.chen@example.com",
      accountId: "5728cdd2-909d-4207-8303-c04b270c308f",
      providerKey: "google",
      sessionExpiresAt: Date.UTC(2099, 0, 1),
    },
    (entry) => write(`Session gateway: ${entry}`),
  ),
  createStorySessionLocation((entry) => write(`Session location: ${entry}`)),
);
const usage = createUsage({
  async current() {
    const snapshot = snapshots[props.state];
    write(`Usage gateway: current ${snapshot.plan} ${props.state} snapshot`);
    return snapshot;
  },
});
const cues: Cues = {
  prepare() {},
  play(cue) {
    write(`ERR unexpected cue ${cue}`);
  },
  loopProblem() {
    return () => {};
  },
  loopAllowanceWarning() {
    write("ERR unexpected allowance warning sound");
    return () => {};
  },
  dispose() {},
};
const alerts = createCaptureAlerts(cues);
const gateway: RecordingGateway = {
  createProject: async () => unexpected("create Project"),
  getProject: async () => unexpected("get Project"),
  createSection: async () => unexpected("create Section"),
  putFrame: async () => unexpected("put frame"),
  complete: async () => unexpected("complete Audio Asset"),
  interrupt: async () => unexpected("interrupt Audio Asset"),
};
const recording = createApplicationCapture({
  config: DEFAULT_CONFIG,
  cues,
  alerts,
  usage,
  authorizeStart: () => session.authorizeRecordingStart(),
  environment: {
    gateway,
    now: () => globalThis.performance.now(),
    openCapture: async () => unexpected("open Capture"),
  },
});

provide(sessionKey, session);
provide(usageKey, usage);
provide(applicationCaptureKey, recording);

const stopSessionWatch = watch(
  session.state,
  (state) => {
    const accountId = state.status === "signedIn" ? state.session.account.id : null;
    usage.setAccount(accountId);
    recording.setSessionAccount(accountId);
  },
  { immediate: true, flush: "sync" },
);

onMounted(() => void session.bootstrap());
onBeforeUnmount(() => {
  stopSessionWatch();
  recording.dispose();
  usage.dispose();
  alerts.dispose();
});
</script>

<template>
  <main class="profile-allowance-story">
    <ProfileWidget />
    <RadarLogView
      :lines="logView.lines"
      :cursor="logView.cursor"
      :columns="DEFAULT_CONFIG.logColumns"
      label="Story activity log"
      test-id="story-log"
    />
  </main>
</template>

<style scoped>
.profile-allowance-story {
  display: grid;
  gap: 1.25rem;
  max-width: 64rem;
  margin: auto;
  padding: 1.5rem;
}
.profile-allowance-story > :deep(.radar-log) {
  margin: 0;
}
</style>
