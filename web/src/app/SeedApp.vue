<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterView, useRoute, useRouter } from "vue-router";
import ProjectNavigation from "../projects/ProjectNavigation.vue";
import { useProjects } from "../projects/projects.ts";
import { useLeaveGuard } from "../recording/leave-guard.ts";
import CaptureModal from "../recording/CaptureModal.vue";
import { useApplicationCapture } from "../recording/application-capture.ts";
import SessionPanel from "../session/SessionPanel.vue";
import { useSession } from "../session/session.ts";
import UsageSummary from "../usage/UsageSummary.vue";

const session = useSession();
const recording = useApplicationCapture();
const projects = useProjects();
const route = useRoute();
const router = useRouter();
useLeaveGuard(recording);

const signedIn = computed(() => session.state.value.status === "signedIn");
const modalOpen = ref(false);
const attemptedCapture = ref(false);
const targetProjectId = ref<string | undefined>();

function openCapture(projectId?: string): void {
  if (!recording.prepare(projectId)) return;
  targetProjectId.value = projectId;
  attemptedCapture.value = false;
  modalOpen.value = true;
}

async function closeCapture(): Promise<void> {
  const projectId = recording.snapshot.value.project?.id ?? targetProjectId.value;
  if (!recording.release()) return;
  modalOpen.value = false;
  attemptedCapture.value = false;
  targetProjectId.value = undefined;
  if (projectId && route.params.projectId !== projectId) {
    await router.push({
      name: "project-detail",
      params: { projectId },
      query: route.query,
      hash: route.hash,
    });
    return;
  }
  if (projectId) await Promise.all([projects.loadList(), projects.loadProject(projectId)]);
}

function discardPending(): void {
  if (!globalThis.confirm("Discard pending audio from this page? This cannot be undone.")) return;
  const projectId = recording.snapshot.value.project?.id ?? targetProjectId.value;
  recording.discardAll();
  modalOpen.value = false;
  attemptedCapture.value = false;
  targetProjectId.value = undefined;
  if (projectId && route.params.projectId !== projectId) {
    void router.push({
      name: "project-detail",
      params: { projectId },
      query: route.query,
      hash: route.hash,
    });
  }
}

watch(
  recording.snapshot,
  (snapshot) => {
    if (!modalOpen.value) return;
    if (snapshot.starting || snapshot.capturing || snapshot.sections.length > 0) {
      attemptedCapture.value = true;
    }
    const completed = snapshot.sections.some((section) => section.completion === "succeeded");
    if (
      attemptedCapture.value &&
      completed &&
      !recording.focused.value &&
      !recording.hasRetainedWork.value
    ) {
      void closeCapture();
    }
  },
  { flush: "post" },
);
</script>

<template>
  <SessionPanel />
  <template v-if="signedIn">
    <UsageSummary />
    <ProjectNavigation @new-recording="openCapture()" />
    <div data-testid="projects-workspace">
      <RouterView v-slot="{ Component }">
        <component :is="Component" @record-more="openCapture" />
      </RouterView>
    </div>
  </template>
  <div
    v-else-if="session.state.value.status === 'loading'"
    class="private-skeleton"
    aria-hidden="true"
  >
    <span></span><span></span><span></span>
  </div>
  <CaptureModal v-if="modalOpen" @close="closeCapture" @discard="discardPending" />
</template>

<style scoped>
.private-skeleton {
  display: grid;
  gap: 0.75rem;
}
.private-skeleton span {
  height: 4rem;
  border-radius: 8px;
  background: #eef2f7;
}
</style>
