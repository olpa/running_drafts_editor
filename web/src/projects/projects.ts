import { computed, inject, shallowRef, type InjectionKey, type Ref } from "vue";
import type { ProjectSummary } from "../shared/project-types.ts";
import type { ProjectDetail, ProjectsGateway } from "./gateway.ts";

export type ProjectsView =
  | { kind: "empty" }
  | { kind: "list"; projects: readonly ProjectSummary[] }
  | { kind: "detail"; project: ProjectDetail }
  | { kind: "failure"; retryLabel: string };

/** Authenticated Project list and complete Project-detail browsing state. */
export interface Projects {
  readonly status: Readonly<Ref<string>>;
  readonly view: Readonly<Ref<ProjectsView>>;
  showList(): Promise<void>;
  showProject(projectId: string): Promise<void>;
  /** Forget everything loaded, for example when the Account signs out. */
  clear(): void;
  audioDownloadUrl(audioAssetId: string): string;
}

export function createProjects(gateway: ProjectsGateway): Projects {
  const status = shallowRef("");
  const view = shallowRef<ProjectsView>({ kind: "empty" });
  let generation = 0;

  async function load<T>(
    message: string,
    failure: { message: string; retryLabel: string },
    fetch: () => Promise<T>,
    show: (value: T) => ProjectsView,
  ): Promise<void> {
    const current = ++generation;
    status.value = message;
    try {
      const value = await fetch();
      if (current !== generation) return;
      view.value = show(value);
      status.value = "";
    } catch {
      if (current !== generation) return;
      status.value = failure.message;
      view.value = { kind: "failure", retryLabel: failure.retryLabel };
    }
  }

  return {
    status: computed(() => status.value),
    view: computed(() => view.value),
    showList: () =>
      load(
        "Loading Projects…",
        { message: "Projects could not be loaded.", retryLabel: "Retry Projects" },
        () => gateway.listProjects(),
        (projects) => ({ kind: "list", projects }),
      ),
    showProject: (projectId) =>
      load(
        "Loading Project…",
        { message: "The Project could not be loaded.", retryLabel: "Back to Projects" },
        () => gateway.getProjectDetail(projectId),
        (project) => ({ kind: "detail", project }),
      ),
    clear() {
      generation++;
      status.value = "";
      view.value = { kind: "empty" };
    },
    audioDownloadUrl: (audioAssetId) => gateway.audioDownloadUrl(audioAssetId),
  };
}

export const projectsKey: InjectionKey<Projects> = Symbol("projects");

export function useProjects(): Projects {
  const projects = inject(projectsKey);
  if (!projects) throw new Error("The Projects module was not provided.");
  return projects;
}
