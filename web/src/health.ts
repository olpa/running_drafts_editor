export type HealthEndpoint = "live" | "ready";

export interface HealthResult {
  endpoint: HealthEndpoint;
  status: number;
  statusText: string;
  body: string;
}

declare global {
  interface Window {
    /** Replaced by the deployed development backend URL when it is available. */
    __HEALTH_BASE_URL__?: string;
  }
}

export const DEFAULT_HEALTH_BASE_URL = "http://localhost:18081";

export class HealthClient {
  constructor(
    private readonly baseUrl = window.__HEALTH_BASE_URL__ ?? DEFAULT_HEALTH_BASE_URL,
    private readonly request: typeof fetch = fetch,
  ) {}

  async check(endpoint: HealthEndpoint): Promise<HealthResult> {
    const response = await this.request(`${this.baseUrl.replace(/\/$/, "")}/health/${endpoint}`, {
      headers: { Accept: "application/json" },
      signal: AbortSignal.timeout(10_000),
    });
    const body = (await response.text()).trim();
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}${response.statusText ? ` ${response.statusText}` : ""}`);
    }
    return { endpoint, status: response.status, statusText: response.statusText, body };
  }
}
