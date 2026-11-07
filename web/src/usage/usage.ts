import { computed, inject, shallowRef, type InjectionKey, type Ref } from "vue";
import type { AccountId } from "../session/gateway.ts";
import type { UsageGateway, UsageSnapshot } from "./gateway.ts";

export type UsageStatus = "idle" | "loading" | "ready" | "failure";

export interface Usage {
  readonly snapshot: Readonly<Ref<Readonly<UsageSnapshot> | null>>;
  readonly status: Readonly<Ref<UsageStatus>>;
  readonly problem: Readonly<Ref<string>>;
  /** Change the Account whose private usage may be published. */
  setAccount(accountId: AccountId | null): void;
  /** Refresh without blocking the action that requested it. */
  refresh(): Promise<void>;
  dispose(): void;
}

export function createUsage(gateway: UsageGateway): Usage {
  const snapshot = shallowRef<Readonly<UsageSnapshot> | null>(null);
  const status = shallowRef<UsageStatus>("idle");
  const problem = shallowRef("");
  const requests = new Set<AbortController>();
  let accountId: AccountId | null = null;
  let generation = 0;
  let disposed = false;

  function cancel(): void {
    generation++;
    for (const controller of requests) controller.abort();
    requests.clear();
  }

  async function refresh(): Promise<void> {
    const requestedAccount = accountId;
    if (disposed || requestedAccount === null) return;
    const requestedGeneration = generation;
    const controller = new AbortController();
    requests.add(controller);
    if (snapshot.value === null) status.value = "loading";
    problem.value = "";
    try {
      const next = await gateway.current(controller.signal);
      if (
        disposed ||
        controller.signal.aborted ||
        requestedGeneration !== generation ||
        requestedAccount !== accountId
      ) {
        return;
      }
      snapshot.value = next;
      status.value = "ready";
    } catch {
      if (
        disposed ||
        controller.signal.aborted ||
        requestedGeneration !== generation ||
        requestedAccount !== accountId
      ) {
        return;
      }
      status.value = "failure";
      problem.value = "Usage could not be refreshed.";
    } finally {
      requests.delete(controller);
    }
  }

  return {
    snapshot: computed(() => snapshot.value),
    status: computed(() => status.value),
    problem: computed(() => problem.value),
    setAccount(next) {
      if (disposed || next === accountId) return;
      cancel();
      accountId = next;
      snapshot.value = null;
      problem.value = "";
      status.value = next === null ? "idle" : "loading";
      if (next !== null) void refresh();
    },
    refresh,
    dispose() {
      if (disposed) return;
      disposed = true;
      cancel();
      accountId = null;
      snapshot.value = null;
      problem.value = "";
      status.value = "idle";
    },
  };
}

export const usageKey: InjectionKey<Usage> = Symbol("usage");

export function useUsage(optional = false): Usage | null {
  const usage = inject(usageKey, null);
  if (!usage && !optional) throw new Error("The Usage module was not provided.");
  return usage;
}
