import { nextTick, watch, type WatchStopHandle } from "vue";
import type { Router } from "vue-router";
import type { Projects } from "../projects/projects.ts";
import type { ApplicationCapture } from "../recording/application-capture.ts";
import type { Session } from "../session/session.ts";

/** Connect URL/history to application modules without moving their behavior into route views. */
export function coordinateRoutes(
  router: Router,
  session: Session,
  projects: Projects,
  recording: ApplicationCapture,
): () => void {
  let sessionStarted = false;
  let restoringHistory = false;
  const stops: WatchStopHandle[] = [];

  const markHistoryNavigation = () => {
    restoringHistory = true;
  };
  globalThis.addEventListener("popstate", markHistoryNavigation);

  const removeGuard = router.beforeEach((to, from) => {
    if (
      from.name &&
      to.fullPath !== from.fullPath &&
      (recording.focused.value || recording.hasRetainedWork.value)
    ) {
      return false;
    }
    return true;
  });

  stops.push(
    watch(
      router.currentRoute,
      (route) => {
        if (!route.meta.private || sessionStarted) return;
        sessionStarted = true;
        void session.start().then(async () => {
          // Authentication owns and removes only its callback parameters. Its
          // location adapter performs that cleanup synchronously; reflect the
          // cleaned browser address in the router before later links are built.
          const browserPath = `${globalThis.location.pathname}${globalThis.location.search}${globalThis.location.hash}`;
          if (router.currentRoute.value.fullPath !== browserPath) {
            await router.replace(browserPath);
          }
        });
      },
      { immediate: true, flush: "post" },
    ),
  );

  stops.push(
    watch(
      [router.currentRoute, session.state],
      ([route, state]) => {
        if (!route.meta.private) {
          projects.clear();
          return;
        }
        if (route.name === "projects") {
          document.title = "Projects — Running Drafts";
        } else if (state.status !== "signedIn") {
          document.title = "Running Drafts";
        }
        if (state.status !== "signedIn") {
          projects.clear();
          return;
        }
        if (route.name === "projects") {
          projects.clearDetail();
          void projects.loadList();
          return;
        }
        if (route.name === "project-detail") {
          const projectId = String(route.params.projectId);
          void Promise.all([projects.loadList(), projects.loadProject(projectId)]);
        }
      },
      { immediate: true, flush: "post" },
    ),
  );

  const removeAfterEach = router.afterEach((to, from, failure) => {
    const restore = restoringHistory;
    restoringHistory = false;
    if (failure || restore || !from.name || to.fullPath === from.fullPath) return;
    void nextTick(() => {
      globalThis.document
        .querySelector<HTMLElement>(
          '#projects-title, #project-detail-title, [data-testid="page-not-found"] h1',
        )
        ?.focus({ preventScroll: true });
    });
  });

  return () => {
    removeGuard();
    removeAfterEach();
    globalThis.removeEventListener("popstate", markHistoryNavigation);
    for (const stop of stops) stop();
  };
}
