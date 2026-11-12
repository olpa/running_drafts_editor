import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import { describe, expect, it, vi } from "vitest";
import { createMicrophoneAccess, microphoneAccessKey } from "./microphone-access.ts";
import MicrophoneAccessPrompt from "./MicrophoneAccessPrompt.vue";

describe("MicrophoneAccess prompt", () => {
  it("allows cancellation and explicit takeover while restoring focus", async () => {
    const access = createMicrophoneAccess();
    const cleanup = vi.fn();
    access.acquire(cleanup);
    const previous = document.createElement("button");
    document.body.append(previous);
    previous.focus();
    const wrapper = mount(MicrophoneAccessPrompt, {
      attachTo: document.body,
      global: { provide: { [microphoneAccessKey as symbol]: access } },
    });
    const pending = access.requestTakeover();
    await nextTick();
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(wrapper.get(".microphone-prompt").attributes("aria-labelledby")).toBe(
      "microphone-access-title",
    );
    expect(document.activeElement).toBe(wrapper.findAll("button")[0]!.element);
    await wrapper.get("button").trigger("keydown", { key: "Escape" });
    expect(access.takeoverRequested.value).toBe(true);
    await wrapper.get("button").trigger("click");
    expect(await pending).toBe(false);
    expect(cleanup).not.toHaveBeenCalled();
    await nextTick();
    expect(document.activeElement).toBe(previous);
    const takeover = access.requestTakeover();
    await nextTick();
    await wrapper.findAll("button")[1]!.trigger("click");
    expect(await takeover).toBe(true);
    expect(cleanup).toHaveBeenCalledOnce();
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    wrapper.unmount();
    previous.remove();
    access.dispose();
  });
});
