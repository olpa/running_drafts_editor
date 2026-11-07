import type { ApiRequester } from "../shared/http.ts";

export type Plan = "free" | "pro";

export interface UsageSnapshot {
  readonly plan: Plan;
  readonly audioStorage: {
    readonly usedBytes: number;
    readonly grantedBytes: number;
    readonly warningAtBytes: number;
    readonly limitBytes: number;
  };
  readonly audioAssetDuration: {
    readonly grantedMs: number;
    readonly warningAtMs: number;
    readonly limitMs: number;
  };
}

export interface UsageGateway {
  current(signal: AbortSignal): Promise<UsageSnapshot>;
}

export function createUsageGateway(api: ApiRequester): UsageGateway {
  return {
    current: (signal) => api.json("GET", "/me/usage", { signal }),
  };
}
