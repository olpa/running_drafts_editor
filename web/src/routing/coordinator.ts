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
  let projectRead: Promise<void> | undefined;
  const stops: WatchStopHandle[] = [];

  const scrollBehavior = router.options.scrollBehavior;
  if (scrollBehavior) {
    router.options.scrollBehavior = async (to, from, savedPosition) => {
      // A loading skeleton can be shorter than the saved scroll position.
      if (savedPosition?.top && to === router.currentRoute.value && projectRead) {
        await projectRead;
        await nextTick();
      }
      return scrollBehavior(to, from, savedPosition);
    };
  }

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
        if ((!route.meta.private && route.name !== "landing") || sessionStarted) return;
        sessionStarted = true;
        void session.start();
        // Authentication owns and removes only its callback parameters. Its
        // location adapter performs that cleanup synchronously; reflect the
        // cleaned browser address in the router before later links are built.
        const browserPath = `${globalThis.location.pathname}${globalThis.location.search}${globalThis.location.hash}`;
        if (route.path === globalThis.location.pathname && route.fullPath !== browserPath) {
          void router.replace(browserPath);
        }
      },
      { immediate: true, flush: "post" },
    ),
  );

  stops.push(
    watch(
      [router.currentRoute, session.state],
      ([route, state]) => {
        projectRead = undefined;
        if (route.name === "landing") {
          document.title = "Running Drafts";
          if (state.status === "signedIn") {
            const query = { ...route.query };
            delete query.authError;
            delete query.referenceId;
            void router.replace({ name: "projects", query, hash: route.hash });
          }
        }
        if (!route.meta.private) {
          projects.clear();
          return;
        }
        if (state.status !== "signedIn") {
          document.title = "Running Drafts";
          projects.clear();
          return;
        }
        if (route.name === "projects") {
          document.title = "Projects — Running Drafts";
          projects.clearDetail();
          projectRead = projects.loadList();
          return;
        }
        if (route.name === "account") {
          document.title = "Account — Running Drafts";
          projects.clear();
          return;
        }
        if (route.name === "project-detail") {
          const projectId = String(route.params.projectId);
          projectRead = projects.loadProject(projectId);
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
          '#projects-title, #project-detail-title, #account-title, [data-testid="page-not-found"] h1',
        )
        ?.focus({ preventScroll: true });
    });
  });

  return () => {
    if (scrollBehavior) router.options.scrollBehavior = scrollBehavior;
    removeGuard();
    removeAfterEach();
    globalThis.removeEventListener("popstate", markHistoryNavigation);
    for (const stop of stops) stop();
  };
}
