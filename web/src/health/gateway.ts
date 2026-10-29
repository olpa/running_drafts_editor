import { backendBaseUrl } from "../shared/backend.ts";

export type HealthEndpoint = "live" | "ready";

export interface HealthResult {
  endpoint: HealthEndpoint;
  status: number;
  statusText: string;
  body: string;
}

/** Backend liveness and readiness checks. */
export interface HealthGateway {
  check(endpoint: HealthEndpoint): Promise<HealthResult>;
}

export function createHealthGateway(
  baseUrl = backendBaseUrl(),
  request: typeof fetch = fetch.bind(window),
): HealthGateway {
  return {
    async check(endpoint) {
      const response = await request(`${baseUrl.replace(/\/$/, "")}/health/${endpoint}`, {
        headers: { Accept: "application/json" },
        signal: AbortSignal.timeout(10_000),
      });
      const body = (await response.text()).trim();
      if (!response.ok) {
        throw new Error(
          `HTTP ${response.status}${response.statusText ? ` ${response.statusText}` : ""}`,
        );
      }
      return { endpoint, status: response.status, statusText: response.statusText, body };
    },
  };
}
