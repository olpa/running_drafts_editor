import type { ApiRequester } from "../shared/http.ts";
import type { AudioAssetSummary, ProjectSummary } from "../shared/project-types.ts";

export interface SectionSummary {
  id: string;
  position: number;
  audioAsset: AudioAssetSummary;
}

export interface ProjectDetail extends ProjectSummary {
  document: { sections: SectionSummary[] };
}

/** Backend reads the Project browser needs. */
export interface ProjectsGateway {
  listProjects(): Promise<ProjectSummary[]>;
  getProjectDetail(projectId: string): Promise<ProjectDetail>;
  audioDownloadUrl(audioAssetId: string): string;
}

export function createProjectsGateway(api: ApiRequester): ProjectsGateway {
  return {
    async listProjects() {
      const result = await api.json<{ projects: ProjectSummary[] }>("GET", "/projects");
      return result.projects;
    },
    getProjectDetail: (projectId) => api.json("GET", `/projects/${encodeURIComponent(projectId)}`),
    audioDownloadUrl: (audioAssetId) =>
      api.url(`/audio-assets/${encodeURIComponent(audioAssetId)}/download`),
  };
}
