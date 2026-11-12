import { computed, inject, shallowRef, type InjectionKey, type Ref } from "vue";
import { ApiError } from "../shared/http.ts";
import type { ProjectSummary } from "../shared/project-types.ts";
import type { ProjectDetail, ProjectsGateway } from "./gateway.ts";

export type ProjectLoad<T> =
  | { kind: "idle" }
  | { kind: "loading"; slow: boolean }
  | { kind: "ready"; value: T }
  | { kind: "notFound" }
  | { kind: "failure" };

export interface ProjectsScheduler {
  schedule(delayMs: number, callback: () => void): () => void;
}

/** Authenticated Project collection and complete Project-detail data. */
export interface Projects {
  readonly list: Readonly<Ref<ProjectLoad<readonly ProjectSummary[]>>>;
  readonly detail: Readonly<Ref<ProjectLoad<ProjectDetail>>>;
  loadList(): Promise<void>;
  loadProject(projectId: string): Promise<void>;
  clearDetail(): void;
  /** Forget all Account-owned data, for example when the Account signs out. */
  clear(): void;
  audioDownloadUrl(audioAssetId: string): string;
}

const browserScheduler: ProjectsScheduler = {
  schedule(delayMs, callback) {
    const timer = window.setTimeout(callback, delayMs);
    return () => window.clearTimeout(timer);
  },
};

export function createProjects(
  gateway: ProjectsGateway,
  scheduler: ProjectsScheduler = browserScheduler,
): Projects {
  const list = shallowRef<ProjectLoad<readonly ProjectSummary[]>>({ kind: "idle" });
  const detail = shallowRef<ProjectLoad<ProjectDetail>>({ kind: "idle" });
  let listGeneration = 0;
  let detailGeneration = 0;

  async function load<T>(
    generation: number,
    currentGeneration: () => number,
    target: Ref<ProjectLoad<T>>,
    fetch: () => Promise<T>,
    notFound: (error: unknown) => boolean,
  ): Promise<void> {
    target.value = { kind: "loading", slow: false };
    const cancelSlow = scheduler.schedule(1_000, () => {
      if (generation === currentGeneration() && target.value.kind === "loading") {
        target.value = { kind: "loading", slow: true };
      }
    });
    try {
      const value = await fetch();
      if (generation === currentGeneration()) target.value = { kind: "ready", value };
    } catch (error) {
      if (generation !== currentGeneration()) return;
      target.value = notFound(error) ? { kind: "notFound" } : { kind: "failure" };
    } finally {
      cancelSlow();
    }
  }

  return {
    list: computed(() => list.value),
    detail: computed(() => detail.value),
    loadList() {
      const generation = ++listGeneration;
      return load(
        generation,
        () => listGeneration,
        list,
        () => gateway.listProjects(),
        () => false,
      );
    },
    loadProject(projectId) {
      const generation = ++detailGeneration;
      return load(
        generation,
        () => detailGeneration,
        detail,
        () => gateway.getProjectDetail(projectId),
        projectMissing,
      );
    },
    clearDetail() {
      detailGeneration++;
      detail.value = { kind: "idle" };
    },
    clear() {
      listGeneration++;
      detailGeneration++;
      list.value = { kind: "idle" };
      detail.value = { kind: "idle" };
    },
    audioDownloadUrl: (audioAssetId) => gateway.audioDownloadUrl(audioAssetId),
  };
}

function projectMissing(error: unknown): boolean {
  return error instanceof ApiError && [400, 403, 404, 422].includes(error.status);
}

export const projectsKey: InjectionKey<Projects> = Symbol("projects");

export function useProjects(): Projects {
  const projects = inject(projectsKey);
  if (!projects) throw new Error("The Projects module was not provided.");
  return projects;
}
