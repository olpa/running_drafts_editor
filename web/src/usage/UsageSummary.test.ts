import { mount } from "@vue/test-utils";
import { computed } from "vue";
import { describe, expect, it } from "vitest";
import { axe } from "../test-support/axe.ts";
import UsageSummary from "./UsageSummary.vue";
import { usageKey, type Usage } from "./usage.ts";

describe("UsageSummary", () => {
  it("presents backend-reported Plan, grants, usage, and effective thresholds", async () => {
    const usage: Usage = {
      snapshot: computed(() => ({
        plan: "pro" as const,
        audioStorage: {
          usedBytes: 524_288,
          grantedBytes: 1_024,
          warningAtBytes: 858_993_459,
          limitBytes: 1_073_741_824,
        },
        audioAssetDuration: {
          grantedMs: 60_000,
          warningAtMs: 11_520_000,
          limitMs: 14_400_000,
        },
      })),
      status: computed(() => "ready" as const),
      problem: computed(() => ""),
      setAccount() {},
      async refresh() {},
      dispose() {},
    };
    const wrapper = mount(UsageSummary, {
      global: { provide: { [usageKey as symbol]: usage } },
    });

    expect(wrapper.text()).toContain("Pro");
    expect(wrapper.text()).toContain("512 KiB used");
    expect(wrapper.text()).toContain("1.0 GiB");
    expect(wrapper.text()).toContain("4 hours");
    expect(await axe(wrapper.element as HTMLElement)).toHaveNoViolations();
  });
});
