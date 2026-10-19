import { backendBaseUrl } from "./backend.ts";

export type HealthEndpoint = "live" | "ready";

export interface HealthResult {
  endpoint: HealthEndpoint;
  status: number;
  statusText: string;
  body: string;
}

export class HealthClient {
  constructor(
    private readonly baseUrl = backendBaseUrl(),
    private readonly request: typeof fetch = fetch.bind(window),
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
