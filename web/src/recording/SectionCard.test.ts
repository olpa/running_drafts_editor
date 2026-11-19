import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { axe } from "../test-support/axe.ts";
import type { SectionSnapshot } from "./project.ts";
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
    overdueFrames: [],
    hasUnsecuredAudio: false,
    completion: null,
    completionProblem: "",
    canRetryCompletion: false,
    problem: "",
    diagnosticDetails: [],
    canUploadPending: false,
    closeProblem: "",
    canRetryClose: false,
    awaitingSeal: false,
    capturedBytes: 0,
    ...overrides,
  };
}

describe("SectionCard", () => {
  it("hides a Section with no visible work or outcome", () => {
    const wrapper = mount(SectionCard, { props: { section: snapshot() } });

    expect(wrapper.find('[data-testid="section"]').exists()).toBe(false);
  });

  it("hides a healthy Section and reveals it while Capture is finishing", async () => {
    const wrapper = mount(SectionCard, {
      props: { section: snapshot({ capturing: true, pendingMs: 2_000, pendingFrames: 1 }) },
    });

    expect(wrapper.find('[data-testid="section"]').exists()).toBe(false);

    await wrapper.setProps({
      section: snapshot({ finalizingCapture: true, pendingMs: 2_000, pendingFrames: 1 }),
    });

    expect(wrapper.get('[data-testid="section"]').element.localName).toBe("section");
    expect(wrapper.get("h3").text()).toBe("Section 1");
    expect(wrapper.get("[role=status]").text()).toBe("Finishing capture…");

    await wrapper.setProps({
      section: snapshot({ completion: "pending", awaitingSeal: true }),
    });
    expect(wrapper.get("[role=status]").text()).toBe("Completing recording…");

    await wrapper.setProps({ section: snapshot({ completion: "succeeded" }) });
    expect(wrapper.find('[data-testid="section"]').exists()).toBe(false);
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
    expect(wrapper.get('[data-testid="completion-problem"]').text()).toBe(
      "Could not complete recording.",
    );
    expect(wrapper.get('[data-testid="completion-problem"]').attributes("role")).toBeUndefined();
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

  it("prints backend diagnostics after the plain failure without announcing it twice", () => {
    const wrapper = mount(SectionCard, {
      props: {
        section: snapshot({
          problem: "Audio delivery stopped because stored data conflicted.",
          diagnosticDetails: [
            "Type: https://running-drafts.example/problems/frame-conflict",
            "Status: 409",
            "Code: frame_conflict",
            "Reference ID: ref-121",
          ],
        }),
      },
    });

    expect(wrapper.get('[data-testid="section-problem"]').text()).toBe(
      "Audio delivery stopped because stored data conflicted.",
    );
    expect(wrapper.get('[data-testid="section-problem"]').attributes("role")).toBeUndefined();
    expect(wrapper.findAll('[data-testid="problem-detail"]')).toHaveLength(4);
    expect(wrapper.text()).toContain("Reference ID: ref-121");
  });

  it("keeps an interruption sealing failure beside its retry action", async () => {
    const wrapper = mount(SectionCard, {
      props: {
        section: snapshot({
          closeProblem: "Could not close recording: Service unavailable",
          canRetryClose: true,
        }),
      },
    });

    expect(wrapper.get('[data-testid="close-problem"]').text()).toBe(
      "Could not close recording: Service unavailable",
    );
    await wrapper.get("button").trigger("click");
    expect(wrapper.emitted("retryClose")).toHaveLength(1);
  });
});

it("does not repeat modal-level delivery-delay details in a Section", () => {
  const wrapper = mount(SectionCard, {
    props: {
      section: snapshot({
        overdueFrames: [{ frameId: 0, elapsedSeconds: 7 }],
        pendingMs: 5_000,
        pendingFrames: 1,
        problem: "Capture was interrupted because audio could not be uploaded.",
        canUploadPending: true,
      }),
    },
  });

  expect(wrapper.text()).toContain("5 seconds of audio pending");
  expect(wrapper.text()).toContain("Capture was interrupted");
  expect(wrapper.find('[data-testid="frame-delay"]').exists()).toBe(false);
});
