import { backendUrl } from "./backend.ts";

export interface ProblemDetails {
  type?: string;
  title?: string;
  status?: number;
  detail?: string;
  instance?: string;
  code?: string;
  referenceId?: string;
}

export class ApiError extends Error {
  constructor(
    readonly method: string,
    readonly path: string,
    readonly status: number,
    readonly problem: Readonly<ProblemDetails> | null = null,
  ) {
    super(problem?.title ?? `${method} ${path} failed with HTTP ${status}`);
    this.name = "ApiError";
  }
}

export interface ApiRequesterOptions {
  baseUrl?: string;
  request?: typeof fetch;
  /** Called once until `markAuthenticationUsable` resets the expiry cycle. */
  onAuthenticationExpired?: () => void;
}

/** Credentialed JSON transport for the authenticated `/api/v1` contract. */
export interface ApiRequester {
  json<T>(method: string, path: string, init?: RequestInit): Promise<T>;
  url(path: string): string;
  /** Re-arm expiry reporting after a successful session bootstrap. */
  markAuthenticationUsable(): void;
}

export function createApiRequester(options: ApiRequesterOptions = {}): ApiRequester {
  const baseUrl = options.baseUrl ?? backendUrl("/api/v1");
  const send = options.request ?? fetch.bind(window);
  const onAuthenticationExpired = options.onAuthenticationExpired ?? (() => undefined);
  let expiryReported = false;

  async function request(method: string, path: string, init: RequestInit): Promise<Response> {
    const response = await send(baseUrl + path, {
      ...init,
      method,
      credentials: "include",
      signal: init.signal ?? AbortSignal.timeout(60_000),
    });
    if (response.ok) return response;
    const problem = await readProblemDetails(response);
    if (response.status === 401 && problem?.code === "authentication_required" && !expiryReported) {
      expiryReported = true;
      onAuthenticationExpired();
    }
    throw new ApiError(method, path, response.status, problem);
  }

  return {
    async json<T>(method: string, path: string, init: RequestInit = {}): Promise<T> {
      return (await (await request(method, path, init)).json()) as T;
    },
    url: (path) => baseUrl + path,
    markAuthenticationUsable: () => {
      expiryReported = false;
    },
  };
}

export function jsonHeaders(): Record<string, string> {
  return { "Content-Type": "application/json" };
}

export async function readProblemDetails(
  response: Response,
): Promise<Readonly<ProblemDetails> | null> {
  let value: unknown;
  try {
    value = await response.clone().json();
  } catch {
    return null;
  }
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
