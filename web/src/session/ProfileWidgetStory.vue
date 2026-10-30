<script setup lang="ts">
import {
  defineComponent,
  h,
  onBeforeUnmount,
  onMounted,
  provide,
  shallowRef,
  watch,
  type PropType,
  type WatchStopHandle,
} from "vue";
import { DEFAULT_CONFIG } from "../shared/config.ts";
import { RadarLog } from "../shared/radar-log.ts";
import RadarLogView from "../shared/RadarLogView.vue";
import ProfileWidget from "./ProfileWidget.vue";
import {
  createStorySessionGateway,
  createStorySessionLocation,
  type StorySessionInput,
} from "./story-gateway.ts";
import { createSession, sessionKey, type Session, type SessionState } from "./session.ts";

const props = defineProps<StorySessionInput>();

const SessionProvider = defineComponent({
  props: { session: { type: Object as PropType<Session>, required: true } },
  setup(providerProps) {
    provide(sessionKey, providerProps.session);
    return () => h(ProfileWidget);
  },
});

interface Scenario {
  id: number;
  session: Session;
}

const scenario = shallowRef<Scenario | null>(null);
const configurationError = shallowRef("");
const logView = shallowRef({ lines: [] as string[], cursor: 0 });
const log = new RadarLog(
  DEFAULT_CONFIG.logColumns,
  DEFAULT_CONFIG.logRows,
  DEFAULT_CONFIG.logGapChars,
);
const openedAt = globalThis.performance.now();
let scenarioNumber = 0;
let rebuildTimer: ReturnType<typeof globalThis.setTimeout> | null = null;
let stopStateWatch: WatchStopHandle | null = null;

function write(id: number, entry: string): void {
  const seconds = ((globalThis.performance.now() - openedAt) / 1000).toFixed(1);
  log.write(`${seconds} q${id} ${entry}`);
  logView.value = log.view();
}

function describeState(value: SessionState): string {
  switch (value.status) {
    case "loading":
      return "state loading";
    case "signedIn":
      return "state signed in";
    case "signedOut":
      return `state signed out: ${value.message}`;
    case "failure":
      return `state failure: ${value.message}`;
  }
}

function rebuild(): void {
  const id = ++scenarioNumber;
  write(id, "scenario setup");
  stopStateWatch?.();
  stopStateWatch = null;
  configurationError.value = "";

  try {
    const activity = (entry: string) => write(id, entry);
    const gateway = createStorySessionGateway(
      {
        initialState: props.initialState,
        displayName: props.displayName,
        verifiedEmail: props.verifiedEmail,
        accountId: props.accountId,
        providerKey: props.providerKey,
        sessionExpiresAt: props.sessionExpiresAt,
      },
      activity,
    );
    const session = createSession(gateway, createStorySessionLocation(activity));
    scenario.value = { id, session };
    let lastStateEntry = "";
    stopStateWatch = watch(
      session.state,
      (value) => {
        const entry = describeState(value);
        if (entry === lastStateEntry) return;
        lastStateEntry = entry;
        write(id, entry);
      },
      { immediate: true, flush: "sync" },
    );
    void session.bootstrap();
  } catch (error) {
    scenario.value = null;
    configurationError.value = error instanceof Error ? error.message : "Invalid story setup.";
    write(id, `ERR ${configurationError.value}`);
  }
}

function scheduleRebuild(): void {
  if (rebuildTimer !== null) globalThis.clearTimeout(rebuildTimer);
  rebuildTimer = globalThis.setTimeout(() => {
    rebuildTimer = null;
    rebuild();
  }, 250);
}

watch(
  () => [
    props.initialState,
    props.displayName,
    props.verifiedEmail,
    props.accountId,
    props.providerKey,
    props.sessionExpiresAt,
  ],
  scheduleRebuild,
);

onMounted(rebuild);
onBeforeUnmount(() => {
  if (rebuildTimer !== null) globalThis.clearTimeout(rebuildTimer);
  stopStateWatch?.();
});
</script>

<template>
  <div class="profile-story">
    <SessionProvider v-if="scenario" :key="scenario.id" :session="scenario.session" />
    <p v-else-if="configurationError" class="story-error" role="alert">
      {{ configurationError }}
    </p>
    <p v-else role="status">Preparing profile scenario…</p>
    <RadarLogView
      :lines="logView.lines"
      :cursor="logView.cursor"
      :columns="DEFAULT_CONFIG.logColumns"
      label="Story activity log"
      test-id="story-log"
    />
  </div>
</template>

<style scoped>
.profile-story {
  display: grid;
  gap: 1.25rem;
}
.profile-story > :deep(.radar-log) {
  margin: 0;
}
.story-error {
  max-width: 44rem;
  padding: 0.75rem;
  border-left: 4px solid var(--danger);
  background: #fff7f7;
  color: #9f2222;
}
</style>
