import { ApiError, jsonHeaders, readProblemDetails, type ApiRequester } from "../shared/http.ts";
import type { AudioAssetSummary, ProjectSummary } from "../shared/project-types.ts";

export interface SectionSummary {
  id: string;
  position: number;
  audioAsset: AudioAssetSummary;
}

export interface ProjectDetail extends ProjectSummary {
  serverTime: string;
  document: { sections: SectionSummary[] };
}

export interface AudioAssetRead {
  audioAsset: AudioAssetSummary;
  serverTime: string;
}

export interface PlaybackProbeResult {
  status: number;
  referenceId: string | null;
}

/** Backend reads the Project browser needs. */
export interface ProjectsGateway {
  listProjects(): Promise<ProjectSummary[]>;
  getProjectDetail(projectId: string): Promise<ProjectDetail>;
  getAudioAsset(audioAssetId: string): Promise<AudioAssetRead>;
  confirmEnded(audioAssetId: string, frameCount: number): Promise<AudioAssetSummary>;
  audioDownloadUrl(audioAssetId: string): string;
  resolvePlaybackUrl(audioAssetId: string, signal: AbortSignal): Promise<string>;
  probePlayback(url: string, signal: AbortSignal): Promise<PlaybackProbeResult>;
}

export function createProjectsGateway(
  api: ApiRequester,
  request: typeof fetch = fetch.bind(window),
  // Asset actions report session expiry inline on the selected card.
  assetApi: ApiRequester = api,
): ProjectsGateway {
  async function playbackRequest(
    method: "HEAD" | "GET",
    url: string,
    signal: AbortSignal,
    headers?: HeadersInit,
  ): Promise<Response> {
    const init: RequestInit = {
      method,
      credentials: "include",
      redirect: "follow",
      signal,
    };
    if (headers) init.headers = headers;
    const response = await request(url, init);
    if (response.ok) return response;
    throw new ApiError(method, url, response.status, await readProblemDetails(response));
  }

  return {
    async listProjects() {
      const result = await api.json<{ projects: ProjectSummary[] }>("GET", "/projects");
      return result.projects;
    },
    getProjectDetail: (projectId) => api.json("GET", `/projects/${encodeURIComponent(projectId)}`),
    getAudioAsset: (id) => assetApi.json("GET", `/audio-assets/${encodeURIComponent(id)}`),
    confirmEnded: (id, frameCount) =>
      assetApi.json("POST", `/audio-assets/${encodeURIComponent(id)}/confirm-ended`, {
        headers: jsonHeaders(),
        body: JSON.stringify({ frameCount }),
      }),
    audioDownloadUrl: (audioAssetId) =>
      api.url(`/audio-assets/${encodeURIComponent(audioAssetId)}/download`),
    async resolvePlaybackUrl(audioAssetId, signal) {
      const latest = api.url(`/audio-assets/${encodeURIComponent(audioAssetId)}/playback/latest`);
      const response = await playbackRequest("HEAD", latest, signal);
      return response.url || latest;
    },
    async probePlayback(url, signal) {
      const response = await playbackRequest("GET", url, signal, { Range: "bytes=0-0" });
      return {
        status: response.status,
        referenceId: null,
      };
    },
  };
}
