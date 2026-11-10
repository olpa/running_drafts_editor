<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import ProjectBrowser from "../projects/ProjectBrowser.vue";
import { useProjects } from "../projects/projects.ts";
import { useLeaveGuard } from "../recording/leave-guard.ts";
import RecordingPanel from "../recording/RecordingPanel.vue";
import AllowanceStatus from "../recording/AllowanceStatus.vue";
import { useApplicationCapture } from "../recording/application-capture.ts";
import SessionPanel from "../session/SessionPanel.vue";
import { useSession } from "../session/session.ts";
import { useConfig } from "../shared/config.ts";
import RadarLogView from "../shared/RadarLogView.vue";
import UsageSummary from "../usage/UsageSummary.vue";

const session = useSession();
const recording = useApplicationCapture();
const projects = useProjects();
const config = useConfig();
useLeaveGuard(recording);

const activeView = ref<"record" | "projects">("record");
const signedIn = computed(() => session.state.value.status === "signedIn");
const showRecordingWorkspace = computed(
  () =>
    recording.focused.value ||
    (signedIn.value ? activeView.value === "record" : recording.hasRetainedWork.value),
);

watch(signedIn, (value) => {
  if (!value) activeView.value = "record";
});

async function showProjects(): Promise<void> {
  if (!signedIn.value) return;
  activeView.value = "projects";
  await projects.showList();
}

onMounted(() => void session.start());
</script>

<template>
  <SessionPanel v-if="!recording.focused.value" />
  <UsageSummary v-if="signedIn && !recording.focused.value" />
  <AllowanceStatus v-else />
  <nav
    v-if="signedIn && !recording.focused.value"
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
    <div v-if="signedIn && !recording.focused.value" data-testid="private-results">
      <ol class="transcript" data-testid="transcript"></ol>
      <RadarLogView
        :lines="recording.logView.value.lines"
        :cursor="recording.logView.value.cursor"
        :columns="config.logColumns"
        label="Diagnostic log"
        test-id="log"
      />
    </div>
  </div>
  <div
    v-if="signedIn && !recording.focused.value && activeView === 'projects'"
    data-testid="projects-workspace"
  >
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
</style>
