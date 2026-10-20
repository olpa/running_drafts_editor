import type { TransportFrame } from "./capture.js";
import { backendUrl } from "./backend.ts";

export type Language = "en" | "de" | "ru";
export interface CreatedRecording { recordingId: string; }
export interface FrameAck { seq: number; acknowledged: true; }
export interface Chunk { id: string; startMs: number; endMs: number; text: string; }
export interface RecordingStatus { status: "recording" | "finishing" | "complete"; chunks: Chunk[]; }

export interface ProblemDetails {
  type?: string; title?: string; status?: number; detail?: string; instance?: string;
  code?: string; referenceId?: string;
}

export class ApiError extends Error {
  constructor(readonly method: string, readonly path: string, readonly status: number,
    readonly problem: Readonly<ProblemDetails> | null = null) {
    super(problem?.title ?? `${method} ${path} failed with HTTP ${status}`);
    this.name = "ApiError";
  }
}

export interface ApiClientOptions {
  baseUrl?: string;
  request?: typeof fetch;
  /** Called once until a successful session bootstrap resets the expiry cycle. */
  onAuthenticationExpired?: () => void;
}

/** Shared application API boundary. Authentication remains in the HttpOnly cookie. */
export class ApiClient {
  private readonly baseUrl: string;
  private readonly requestImplementation: typeof fetch;
  private readonly onAuthenticationExpired: () => void;
  private authenticationExpiryReported = false;

  constructor(options: ApiClientOptions | string = {}) {
    const resolved = typeof options === "string" ? { baseUrl: options } : options;
    this.baseUrl = resolved.baseUrl ?? backendUrl("/api");
    this.requestImplementation = resolved.request ?? fetch.bind(window);
    this.onAuthenticationExpired = resolved.onAuthenticationExpired ?? (() => undefined);
  }

  createRecording(language: Language): Promise<CreatedRecording> {
    return this.json("POST", "/recordings", { headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ language }) });
  }

  putFrame(recordingId: string, frame: TransportFrame, signal: AbortSignal): Promise<FrameAck> {
    return this.json("PUT", `/recordings/${encodeURIComponent(recordingId)}/frames/${frame.seq}`, {
      headers: { "Content-Type": frame.mediaType, "X-Frame-Start-Ms": String(frame.startMs),
        "X-Frame-End-Ms": String(frame.endMs) }, body: frame.data, signal });
  }

  async finish(recordingId: string): Promise<void> {
    await this.request("POST", `/recordings/${encodeURIComponent(recordingId)}/finish`, {});
  }

  getRecording(recordingId: string): Promise<RecordingStatus> {
    return this.json("GET", `/recordings/${encodeURIComponent(recordingId)}`, {});
  }

  markAuthenticationUsable(): void { this.authenticationExpiryReported = false; }

  private async json<T>(method: string, path: string, init: RequestInit): Promise<T> {
    const response = await this.request(method, path, init);
    return (await response.json()) as T;
  }

  private async request(method: string, path: string, init: RequestInit): Promise<Response> {
    const response = await this.requestImplementation(this.baseUrl + path, { ...init, method,
      credentials: "include", signal: init.signal ?? AbortSignal.timeout(60_000) });
    if (response.ok) return response;
    const problem = await readProblemDetails(response);
    if (response.status === 401 && problem?.code === "authentication_required"
      && !this.authenticationExpiryReported) {
      this.authenticationExpiryReported = true;
      this.onAuthenticationExpired();
    }
    throw new ApiError(method, path, response.status, problem);
  }
}

export async function readProblemDetails(response: Response): Promise<Readonly<ProblemDetails> | null> {
  let value: unknown;
  try { value = await response.clone().json(); } catch { return null; }
  if (!isRecord(value)) return null;
  const problem: ProblemDetails = {};
  for (const key of ["type", "title", "detail", "instance", "code", "referenceId"] as const) {
    if (key === "referenceId") {
      const referenceId = safeReferenceId(value[key]);
      if (referenceId) problem.referenceId = referenceId;
    } else if (typeof value[key] === "string") problem[key] = value[key];
  }
  if (typeof value.status === "number") problem.status = value.status;
  return problem;
}

export function safeReferenceId(value: unknown): string | null {
  return typeof value === "string" && /^[A-Za-z0-9_-]{1,100}$/u.test(value) ? value : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
