// Assumed seed backend contract. Proposed on
// https://github.com/olpa/running_drafts_backend/issues/1 and mocked by the
// browser test harness until #78 connects the real backend.

import type { TransportFrame } from "./capture.js";

export type Language = "en" | "de" | "ru";

export interface CreatedRecording {
  recordingId: string;
}

export interface FrameAck {
  seq: number;
  acknowledged: true;
}

export interface Chunk {
  id: string;
  startMs: number;
  endMs: number;
  text: string;
}

export interface RecordingStatus {
  status: "recording" | "finishing" | "complete";
  chunks: Chunk[];
}

export class ApiError extends Error {
  constructor(
    readonly method: string,
    readonly path: string,
    readonly status: number,
  ) {
    super(`${method} ${path} failed with HTTP ${status}`);
    this.name = "ApiError";
  }
}

export class ApiClient {
  constructor(private readonly baseUrl = "/api") {}

  createRecording(language: Language): Promise<CreatedRecording> {
    return this.json("POST", "/recordings", {
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ language }),
    });
  }

  putFrame(recordingId: string, frame: TransportFrame, signal: AbortSignal): Promise<FrameAck> {
    return this.json("PUT", `/recordings/${encodeURIComponent(recordingId)}/frames/${frame.seq}`, {
      headers: {
        "Content-Type": frame.mediaType,
        "X-Frame-Start-Ms": String(frame.startMs),
        "X-Frame-End-Ms": String(frame.endMs),
      },
      body: frame.data,
      signal,
    });
  }

  async finish(recordingId: string): Promise<void> {
    await this.request("POST", `/recordings/${encodeURIComponent(recordingId)}/finish`, {});
  }

  getRecording(recordingId: string): Promise<RecordingStatus> {
    return this.json("GET", `/recordings/${encodeURIComponent(recordingId)}`, {});
  }

  private async json<T>(method: string, path: string, init: RequestInit): Promise<T> {
    const response = await this.request(method, path, init);
    return (await response.json()) as T;
  }

  private async request(method: string, path: string, init: RequestInit): Promise<Response> {
    const response = await fetch(this.baseUrl + path, {
      ...init, method, signal: init.signal ?? AbortSignal.timeout(60_000),
    });
    if (!response.ok) throw new ApiError(method, path, response.status);
    return response;
  }
}
