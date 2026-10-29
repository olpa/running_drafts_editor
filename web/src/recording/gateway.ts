import { jsonHeaders, type ApiRequester } from "../shared/http.ts";
import type { AudioAssetSummary, ProjectSummary } from "../shared/project-types.ts";
import { TRANSPORT_FRAME_MEDIA_TYPE, type TransportFrame } from "./capture.ts";

export interface CreatedSection {
  sectionId: string;
  audioAssetId: string;
  position: number;
}

export interface FrameAck {
  sequence: number;
  acknowledged: true;
  sha256: string;
  nextExpectedSequence: number;
}

/** Backend operations the recording module needs. */
export interface RecordingGateway {
  createProject(idempotencyKey: string, title: string): Promise<ProjectSummary>;
  getProject(projectId: string): Promise<ProjectSummary>;
  createSection(projectId: string, idempotencyKey: string): Promise<CreatedSection>;
  putFrame(audioAssetId: string, frame: TransportFrame, signal: AbortSignal): Promise<FrameAck>;
  complete(audioAssetId: string, frameCount: number): Promise<AudioAssetSummary>;
  interrupt(audioAssetId: string): Promise<AudioAssetSummary>;
}

export function createRecordingGateway(api: ApiRequester): RecordingGateway {
  const asset = (id: string) => `/audio-assets/${encodeURIComponent(id)}`;
  const project = (id: string) => `/projects/${encodeURIComponent(id)}`;
  return {
    createProject: (idempotencyKey, title) =>
      api.json("POST", "/projects", {
        headers: jsonHeaders(),
        body: JSON.stringify({ idempotencyKey, title }),
      }),
    getProject: (projectId) => api.json("GET", project(projectId)),
    createSection: (projectId, idempotencyKey) =>
      api.json("POST", `${project(projectId)}/sections`, {
        headers: jsonHeaders(),
        body: JSON.stringify({ idempotencyKey, mediaType: TRANSPORT_FRAME_MEDIA_TYPE }),
      }),
    putFrame: (audioAssetId, frame, signal) =>
      api.json("PUT", `${asset(audioAssetId)}/frames/${frame.seq}`, {
        headers: {
          "Content-Type": TRANSPORT_FRAME_MEDIA_TYPE,
          "X-Frame-Start-Ms": String(frame.startMs),
          "X-Frame-End-Ms": String(frame.endMs),
        },
        body: frame.data,
        signal,
      }),
    complete: (audioAssetId, frameCount) =>
      api.json("POST", `${asset(audioAssetId)}/complete`, {
        headers: jsonHeaders(),
        body: JSON.stringify({ frameCount }),
      }),
    interrupt: (audioAssetId) => api.json("POST", `${asset(audioAssetId)}/interrupt`),
  };
}
