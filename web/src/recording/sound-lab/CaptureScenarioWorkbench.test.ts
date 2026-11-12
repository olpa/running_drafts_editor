import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import { settle } from "../../test-support/manual-scheduler.ts";
import CaptureScenarioWorkbench from "./CaptureScenarioWorkbench.vue";
import type { ScenarioKind } from "./scenario.ts";

class Buffer {
  copyToChannel() {}
}
class Source extends EventTarget {
  loop = false;
  buffer: Buffer | null = null;
  connect() {}
  disconnect() {}
  start() {}
  stop() {
    this.dispatchEvent(new Event("ended"));
  }
}
class Audio extends EventTarget {
  state = "running";
  sampleRate = 8000;
  destination = {};
  async resume() {}
  async close() {
    this.state = "closed";
  }
  createBuffer() {
    return new Buffer();
  }
  createBufferSource() {
    return new Source();
  }
}

const common = {
  initial: "Capture is ready.",
  play: "Run the arranged actions.",
  inspect: "Inspect the final state.",
  interaction: "Only Acknowledge remains active.",
};

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("Capture scenario workbench", () => {
  async function play(scenarioKind: ScenarioKind) {
    vi.useFakeTimers();
    vi.stubGlobal("AudioContext", Audio);
    const wrapper = mount(CaptureScenarioWorkbench, {
      props: { scenarioKind, ...common },
    });
    await flushPromises();
    await wrapper.get(".playback-actions button").trigger("click");
    await vi.runAllTimersAsync();
    await settle();
    await flushPromises();
    return wrapper;
  }

  it("requires explicit Play, preserves semantic guidance, and offers Replay only when finished", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("AudioContext", Audio);
    const wrapper = mount(CaptureScenarioWorkbench, {
      props: { scenarioKind: "short", ...common },
    });
    await flushPromises();

    expect(wrapper.get('[data-review-field="initial"]').text()).toContain(common.initial);
    expect(wrapper.get(".arranged-production").attributes()).toHaveProperty("inert");
    expect(wrapper.findAll("button").map((button) => button.text())).toContain("Play scenario");
    expect(wrapper.findAll("button").map((button) => button.text())).not.toContain(
      "Replay scenario",
    );

    await wrapper.get(".playback-actions button").trigger("click");
    await vi.runAllTimersAsync();
    await settle();
    await flushPromises();
    expect(wrapper.findAll("button").map((button) => button.text())).toContain("Replay scenario");
    expect(wrapper.text()).toContain("Recording complete");
    expect(wrapper.find('[data-testid="scenario-log"]').exists()).toBe(true);

    await wrapper.get(".playback-actions button").trigger("click");
    await vi.runAllTimersAsync();
    await settle();
    await flushPromises();
    expect(wrapper.findAll("button").map((button) => button.text())).toContain("Replay scenario");
    expect(wrapper.get('[data-testid="scenario-log"]').text()).toContain("q2");
    wrapper.unmount();
  });

  it("keeps production Acknowledge interactive outside the inert scenario surface", async () => {
    const wrapper = await play("allowance-duration-rejection");
    const acknowledge = wrapper.findAll("button").find((button) => button.text() === "Acknowledge");
    expect(acknowledge).toBeDefined();
    expect(acknowledge!.element.closest("[inert]")).toBeNull();
    await acknowledge!.trigger("click");
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("shows both warning dimensions and keeps only the sound action interactive", async () => {
    const wrapper = await play("allowance-overlap");

    expect(wrapper.get('[data-testid="storage-warning"]').text()).toContain("remains");
    expect(wrapper.get('[data-testid="duration-warning"]').text()).toContain("remaining");
    const stopSound = wrapper
      .findAll("button")
      .find((button) => button.text() === "Stop warning sound");
    expect(stopSound).toBeDefined();
    expect(stopSound!.element.closest("[inert]")).toBeNull();
    await stopSound!.trigger("click");
    expect(wrapper.findAll("button").some((button) => button.text() === "Stop warning sound")).toBe(
      false,
    );
    expect(wrapper.find('[data-testid="allowance-status"]').exists()).toBe(true);
    wrapper.unmount();
  });

  it("shows zero duration remaining without stopping Capture", async () => {
    const wrapper = await play("allowance-duration-zero");

    expect(wrapper.get('[data-testid="duration-warning"]').text()).toContain("0 seconds remaining");
    expect(wrapper.get('[data-testid="capture-indicator"]').attributes("data-capturing")).toBe(
      "true",
    );
    wrapper.unmount();
  });

  it("retains rejected audio while the production upload action stays inert", async () => {
    const wrapper = await play("allowance-storage-rejection");

    expect(wrapper.get('[data-testid="section"]').text()).toContain(
      "storage allowance rejected this audio",
    );
    expect(wrapper.get('[data-testid="section"]').text()).toContain("audio pending");
    const upload = wrapper
      .findAll("button")
      .find((button) => button.text() === "Upload pending audio");
    expect(upload).toBeDefined();
    expect(upload!.element.closest("[inert]")).not.toBeNull();
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(true);
    wrapper.unmount();
  });

  it("runs a successful story through the real modal experience", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("AudioContext", Audio);
    const wrapper = mount(CaptureScenarioWorkbench, {
      attachTo: document.body,
      props: { scenarioKind: "short", reviewMode: "experience", ...common },
    });
    await flushPromises();

    const trigger = wrapper.get(".experience-shell button");
    expect(trigger.text()).toBe("New recording");
    await trigger.trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="capture-modal"]').exists()).toBe(true);
    expect(wrapper.find(".arranged-production").exists()).toBe(false);
    const shell = wrapper.get(".experience-shell").element;
    let background: HTMLElement | null = shell instanceof HTMLElement ? shell : null;
    let isInert = false;
    while (background) {
      isInert ||= background.inert;
      background = background.parentElement;
    }
    expect(isInert).toBe(true);

    await wrapper.get('[data-testid="capture-modal"]').trigger("keydown", { key: "Escape" });
    expect(wrapper.find('[data-testid="capture-modal"]').exists()).toBe(true);

    await vi.runAllTimersAsync();
    await settle();
    await flushPromises();
    expect(wrapper.find('[data-testid="capture-modal"]').exists()).toBe(false);
    expect(wrapper.get("#experience-projects-title").text()).toBe("Simulated Capture");
    expect(wrapper.get(".experience-shell button").text()).toBe("Replay scenario");
    wrapper.unmount();
  });

  it("keeps a failed Capture in the modal for review", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("AudioContext", Audio);
    const wrapper = mount(CaptureScenarioWorkbench, {
      attachTo: document.body,
      props: { scenarioKind: "finalization", reviewMode: "experience", ...common },
    });
    await flushPromises();
    await wrapper.get(".experience-shell button").trigger("click");
    await vi.runAllTimersAsync();
    await settle();
    await flushPromises();

    const modal = wrapper.get('[data-testid="capture-modal"]');
    expect(wrapper.text()).toContain("Capture finalization failed");
    expect(modal.findAll("button").some((button) => button.text() === "Acknowledge")).toBe(true);
    wrapper.unmount();
  });

  it("keeps the warning-sound action inside the Storybook modal experience", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("AudioContext", Audio);
    const wrapper = mount(CaptureScenarioWorkbench, {
      attachTo: document.body,
      props: { scenarioKind: "allowance-overlap", reviewMode: "experience", ...common },
    });
    await flushPromises();
    await wrapper.get(".experience-shell button").trigger("click");
    await vi.runAllTimersAsync();
    await settle();
    await flushPromises();

    const modal = wrapper.get('[data-testid="capture-modal"]');
    const stopSound = modal
      .findAll("button")
      .find((button) => button.text() === "Stop warning sound");
    expect(stopSound).toBeDefined();
    await stopSound!.trigger("click");
    expect(modal.findAll("button").some((button) => button.text() === "Stop warning sound")).toBe(
      false,
    );
    wrapper.unmount();
  });
});
