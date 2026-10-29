import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { axe } from "../test-support/axe.ts";
import type { SectionSnapshot } from "./project-recorder.ts";
import SectionCard from "./SectionCard.vue";

function snapshot(overrides: Partial<SectionSnapshot> = {}): SectionSnapshot {
  return {
    sectionId: "section-one",
    audioAssetId: "asset-one",
    position: 0,
    capturing: false,
    finalizingCapture: false,
    elapsedMs: 10_000,
    securedMs: 0,
    pendingMs: 0,
    pendingFrames: 0,
    hasUnsecuredAudio: false,
    completion: null,
    completionProblem: "",
    canRetryCompletion: false,
    problem: "",
    canUploadPending: false,
    closeProblem: "",
    canRetryClose: false,
    awaitingSeal: false,
    ...overrides,
  };
}

describe("SectionCard", () => {
  it("renders a semantic Section and updates its completion lifecycle in place", async () => {
    const wrapper = mount(SectionCard, { props: { section: snapshot() } });
    const root = wrapper.element;
    const liveStatus = wrapper.get("[role=status]").element;

    expect(root.localName).toBe("section");
    expect(wrapper.attributes("data-testid")).toBe("section");
    expect(wrapper.attributes("data-completion")).toBeUndefined();
    expect(wrapper.get("h3").text()).toBe("Section 1");

    await wrapper.setProps({
      section: snapshot({ completion: "pending", pendingMs: 2_000, pendingFrames: 1 }),
    });
    expect(wrapper.element).toBe(root);
    expect(wrapper.get("[role=status]").element).toBe(liveStatus);
    expect(wrapper.attributes("data-completion")).toBe("pending");
    expect(wrapper.text()).toContain("2 seconds of audio pending");
    expect(wrapper.text()).toContain("Completing recording…");

    await wrapper.setProps({ section: snapshot({ completion: "succeeded" }) });
    expect(wrapper.attributes("data-completion")).toBe("succeeded");
    expect(wrapper.text()).toContain("Recording complete");
    expect(wrapper.get("[role=status]").classes()).toContain("success");
  });

  it("separates a retryable completion problem from its action", async () => {
    const wrapper = mount(SectionCard, {
      props: {
        section: snapshot({
          completion: "failed",
          completionProblem: "Could not complete recording.",
          canRetryCompletion: true,
        }),
      },
    });

    expect(wrapper.attributes("data-completion")).toBe("failed");
    expect(wrapper.get("[role=status]").text()).toBe("Could not complete recording.");
    const retry = wrapper.get("button");
    expect(retry.text()).toBe("Try again");
    expect(retry.attributes("aria-label")).toBe("Try completing Section 1 again");
    await retry.trigger("click");
    expect(wrapper.emitted("retryClose")).toHaveLength(1);
  });

  it("renders a terminal completion mismatch without offering retry", () => {
    const wrapper = mount(SectionCard, {
      props: {
        section: snapshot({
          completion: "failed",
          completionProblem: "Recording was sealed as interrupted and cannot be completed.",
        }),
      },
    });
    expect(wrapper.text()).toContain("sealed as interrupted");
    expect(wrapper.findAll("button")).toHaveLength(0);
  });

  it("offers an upload of pending audio and has no accessibility violations", async () => {
    const wrapper = mount(SectionCard, {
      props: {
        section: snapshot({ canUploadPending: true, problem: "Audio delivery is delayed." }),
      },
    });
    await wrapper.get("button").trigger("click");
    expect(wrapper.emitted("uploadPending")).toHaveLength(1);
    expect(await axe(wrapper.element as HTMLElement)).toHaveNoViolations();
  });
});
