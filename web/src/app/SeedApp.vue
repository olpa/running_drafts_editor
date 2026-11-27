<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterView, useRoute, useRouter } from "vue-router";
import { useProjects } from "../projects/projects.ts";
import { useLeaveGuard } from "../recording/leave-guard.ts";
import AllowanceStatus from "../recording/AllowanceStatus.vue";
import CaptureModal from "../recording/CaptureModal.vue";
import { useApplicationCapture } from "../recording/application-capture.ts";
import FirstRunPage from "../session/FirstRunPage.vue";
import { useSession } from "../session/session.ts";

const session = useSession();
const recording = useApplicationCapture();
const projects = useProjects();
const route = useRoute();
const router = useRouter();
useLeaveGuard(recording);

const signedIn = computed(() => session.state.value.status === "signedIn");
const projectNotFound = computed(
  () => route.name === "project-detail" && projects.detail.value.kind === "notFound",
);
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
  if (projectId) await projects.loadProject(projectId);
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
  <main v-if="signedIn">
    <AllowanceStatus v-if="route.name !== 'account' && !projectNotFound" />
    <div :data-testid="route.name === 'account' ? undefined : 'projects-workspace'">
      <RouterView v-slot="{ Component }">
        <component :is="Component" @record-more="openCapture" @new-recording="openCapture()" />
      </RouterView>
    </div>
  </main>
  <FirstRunPage v-else />
  <CaptureModal v-if="modalOpen" @close="closeCapture" @discard="discardPending" />
</template>
