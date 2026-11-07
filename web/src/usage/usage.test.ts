import { describe, expect, it } from "vitest";
import type { AccountId } from "../session/gateway.ts";
import { deferred } from "../test-support/deferred.ts";
import type { UsageSnapshot } from "./gateway.ts";
import { createUsage } from "./usage.ts";

const account = (value: string) => value as AccountId;
const sample = (usedBytes: number): UsageSnapshot => ({
  plan: "free",
  audioStorage: {
    usedBytes,
    grantedBytes: 0,
    warningAtBytes: 100,
    limitBytes: 125,
  },
  audioAssetDuration: { grantedMs: 0, warningAtMs: 1000, limitMs: 1250 },
});

describe("Usage", () => {
  it("loads on Account entry and clears and cancels on Account change", async () => {
    const first = deferred<UsageSnapshot>();
    const second = deferred<UsageSnapshot>();
    const signals: AbortSignal[] = [];
    const requests = [first, second];
    const usage = createUsage({
      current(signal) {
        signals.push(signal);
        return requests.shift()!.promise;
      },
    });

    usage.setAccount(account("one"));
    expect(usage.status.value).toBe("loading");
    usage.setAccount(account("two"));
    expect(signals[0]!.aborted).toBe(true);
    first.resolve(sample(10));
    second.resolve(sample(20));
    await Promise.resolve();
    await Promise.resolve();
    expect(usage.snapshot.value?.audioStorage.usedBytes).toBe(20);

    usage.setAccount(null);
    expect(usage.snapshot.value).toBeNull();
    expect(usage.status.value).toBe("idle");
  });

  it("keeps the last snapshot after failure and lets overlapping results publish in completion order", async () => {
    const initial = deferred<UsageSnapshot>();
    const failure = deferred<UsageSnapshot>();
    const olderResult = deferred<UsageSnapshot>();
    const newerResult = deferred<UsageSnapshot>();
    const requests = [initial, failure, olderResult, newerResult];
    const usage = createUsage({ current: () => requests.shift()!.promise });
    usage.setAccount(account("one"));
    initial.resolve(sample(10));
    await Promise.resolve();
    await Promise.resolve();

    const failed = usage.refresh();
    failure.reject(new Error("offline"));
    await failed;
    expect(usage.snapshot.value?.audioStorage.usedBytes).toBe(10);
    expect(usage.problem.value).toBe("Usage could not be refreshed.");

    const older = usage.refresh();
    const newer = usage.refresh();
    newerResult.resolve(sample(30));
    await newer;
    olderResult.resolve(sample(20));
    await older;
    expect(usage.snapshot.value?.audioStorage.usedBytes).toBe(20);
  });
});
