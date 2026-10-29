import { inject, reactive, type InjectionKey } from "vue";
import type { HealthEndpoint, HealthGateway } from "./gateway.ts";

export interface HealthCheck {
  state: "idle" | "busy" | "success" | "error";
  summary: string;
  detail: string;
}

export interface Health {
  readonly checks: Readonly<Record<HealthEndpoint, Readonly<HealthCheck>>>;
  check(endpoint: HealthEndpoint): Promise<void>;
}

const LABELS: Record<HealthEndpoint, { checking: string; success: string; idle: string }> = {
  live: {
    checking: "Checking liveness…",
    success: "Backend is live",
    idle: "Run the check to contact the backend.",
  },
  ready: {
    checking: "Checking readiness…",
    success: "Backend and database are ready",
    idle: "Run the check to contact the backend and database.",
  },
};

/** Runs and holds the two development-environment health checks. */
export function createHealth(gateway: HealthGateway): Health {
  const checks = reactive<Record<HealthEndpoint, HealthCheck>>({
    live: { state: "idle", summary: "Not checked", detail: LABELS.live.idle },
    ready: { state: "idle", summary: "Not checked", detail: LABELS.ready.idle },
  });

  return {
    checks,
    async check(endpoint) {
      const current = checks[endpoint];
      current.state = "busy";
      current.summary = LABELS[endpoint].checking;
      current.detail = `Contacting /health/${endpoint}`;
      try {
        const response = await gateway.check(endpoint);
        const status = `HTTP ${response.status}${response.statusText ? ` ${response.statusText}` : ""}`;
        current.state = "success";
        current.summary = LABELS[endpoint].success;
        current.detail = response.body ? `${status} · ${response.body}` : status;
      } catch (error) {
        current.state = "error";
        current.summary = "Check failed";
        current.detail = error instanceof Error ? error.message : String(error);
      }
    },
  };
}

export const healthKey: InjectionKey<Health> = Symbol("health");

export function useHealth(): Health {
  const health = inject(healthKey);
  if (!health) throw new Error("The Health module was not provided.");
  return health;
}
