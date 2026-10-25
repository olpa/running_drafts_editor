import { TRANSPORT_FRAME_MEDIA_TYPE, type TransportFrame } from "./capture.ts";
import { backendUrl } from "./backend.ts";

export { TRANSPORT_FRAME_MEDIA_TYPE } from "./capture.ts";

export interface ProjectSummary {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
}

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

export interface AudioAssetSummary {
  id: string;
  mediaType: string;
  isSealed: boolean;
  sealReason: "completed" | "interrupted" | null;
  sealedAt: string | null;
  frameCount: number;
  observedDurationMs: number;
  storedBytes: number;
}

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

/** HTTP adapter for the authenticated Project recording contract. */
export class ApiClient {
  private readonly baseUrl: string;
  private readonly requestImplementation: typeof fetch;
  private readonly onAuthenticationExpired: () => void;
  private authenticationExpiryReported = false;

  constructor(options: ApiClientOptions | string = {}) {
    const resolved = typeof options === "string" ? { baseUrl: options } : options;
    this.baseUrl = resolved.baseUrl ?? backendUrl("/api/v1");
    this.requestImplementation = resolved.request ?? fetch.bind(window);
    this.onAuthenticationExpired = resolved.onAuthenticationExpired ?? (() => undefined);
  }

  createProject(idempotencyKey: string, title: string): Promise<ProjectSummary> {
    return this.json("POST", "/projects", { headers: jsonHeaders(),
      body: JSON.stringify({ idempotencyKey, title }) });
  }

  getProject(projectId: string): Promise<ProjectSummary> {
    return this.json("GET", `/projects/${encodeURIComponent(projectId)}`, {});
  }

  createSection(projectId: string, idempotencyKey: string): Promise<CreatedSection> {
    return this.json("POST", `/projects/${encodeURIComponent(projectId)}/sections`, {
      headers: jsonHeaders(), body: JSON.stringify({ idempotencyKey, mediaType: TRANSPORT_FRAME_MEDIA_TYPE }),
    });
  }

  putFrame(audioAssetId: string, frame: TransportFrame, signal: AbortSignal): Promise<FrameAck> {
    return this.json("PUT", `/audio-assets/${encodeURIComponent(audioAssetId)}/frames/${frame.seq}`, {
      headers: { "Content-Type": TRANSPORT_FRAME_MEDIA_TYPE, "X-Frame-Start-Ms": String(frame.startMs),
        "X-Frame-End-Ms": String(frame.endMs) }, body: frame.data, signal,
    });
  }

  complete(audioAssetId: string, frameCount: number): Promise<AudioAssetSummary> {
    return this.json("POST", `/audio-assets/${encodeURIComponent(audioAssetId)}/complete`, {
      headers: jsonHeaders(), body: JSON.stringify({ frameCount }),
    });
  }

  interrupt(audioAssetId: string): Promise<AudioAssetSummary> {
    return this.json("POST", `/audio-assets/${encodeURIComponent(audioAssetId)}/interrupt`, {});
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

function jsonHeaders(): Record<string, string> {
  return { "Content-Type": "application/json" };
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
