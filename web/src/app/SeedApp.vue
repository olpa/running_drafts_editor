<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import ProjectBrowser from "../projects/ProjectBrowser.vue";
import { useProjects } from "../projects/projects.ts";
import { useLeaveGuard } from "../recording/leave-guard.ts";
import RecordingPanel from "../recording/RecordingPanel.vue";
import { useRecording } from "../recording/recording.ts";
import SessionPanel from "../session/SessionPanel.vue";
import { useSession } from "../session/session.ts";
import { useConfig } from "../shared/config.ts";

const session = useSession();
const recording = useRecording();
const projects = useProjects();
const config = useConfig();
useLeaveGuard(recording);

const activeView = ref<"record" | "projects">("record");
const signedIn = computed(() => session.state.value.status === "signedIn");
const showRecordingWorkspace = computed(() =>
  signedIn.value ? activeView.value === "record" : recording.hasRetainedWork.value,
);

const logParts = computed(() => {
  const { lines, cursor } = recording.logView.value;
  const text = lines.join("\n");
  return {
    before: text.slice(0, cursor),
    mark: text.charAt(cursor),
    after: text.slice(cursor + 1),
  };
});

watch(signedIn, (value) => {
  if (!value) activeView.value = "record";
});

async function showProjects(): Promise<void> {
  if (!signedIn.value) return;
  activeView.value = "projects";
  await projects.showList();
}

onMounted(() => void session.start());
onBeforeUnmount(() => recording.dispose());
</script>

<template>
  <SessionPanel />
  <nav
    v-if="signedIn"
    class="private-navigation"
    data-testid="private-navigation"
    aria-label="Private workspace"
  >
    <button type="button" data-testid="record-view" @click="activeView = 'record'">Record</button>
    <button type="button" data-testid="projects-view" @click="showProjects">Projects</button>
  </nav>
  <div v-if="showRecordingWorkspace" data-testid="recording-workspace">
    <div data-testid="recording-panel-host">
      <RecordingPanel />
    </div>
    <div v-if="signedIn" data-testid="private-results">
      <ol class="transcript" data-testid="transcript"></ol>
      <!-- prettier-ignore -->
      <pre
        class="radar-log"
        data-testid="log"
        aria-label="Diagnostic log"
        :style="{ '--log-columns': config.logColumns }"
      >{{ logParts.before }}<span class="cursor">{{ logParts.mark }}</span>{{ logParts.after }}</pre>
    </div>
  </div>
  <div v-if="signedIn && activeView === 'projects'" data-testid="projects-workspace">
    <ProjectBrowser />
  </div>
</template>

<style scoped>
.private-navigation {
  display: flex;
  gap: 0.5rem;
  margin-bottom: 1rem;
}
.private-navigation button {
  min-width: 7rem;
}
.transcript {
  min-height: 6rem;
  padding-left: 1.4rem;
}
.radar-log {
  width: min(100%, calc(var(--log-columns) * 1ch + 1rem));
  overflow-x: auto;
  padding: 0.75rem;
  border-radius: 6px;
  background: #111827;
  color: #86efac;
  line-height: 1.2;
}
.radar-log .cursor {
  background: #86efac;
  color: #111827;
}
</style>
