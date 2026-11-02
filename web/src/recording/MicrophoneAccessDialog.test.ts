import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import { describe, expect, it, vi } from "vitest";
import { createMicrophoneAccess, microphoneAccessKey } from "./microphone-access.ts";
import MicrophoneAccessDialog from "./MicrophoneAccessDialog.vue";

describe("MicrophoneAccess dialog", () => {
  it("allows cancellation and explicit takeover with keyboard focus contained", async () => {
    const access = createMicrophoneAccess();
    const cleanup = vi.fn();
    access.acquire(cleanup);
    const wrapper = mount(MicrophoneAccessDialog, {
      attachTo: document.body,
      global: { provide: { [microphoneAccessKey as symbol]: access } },
    });
    const pending = access.requestTakeover();
    await nextTick();
    expect(wrapper.get('[role="dialog"]').attributes("aria-modal")).toBe("true");
    expect(document.activeElement).toBe(wrapper.findAll("button")[0]!.element);
    await wrapper.findAll("button")[0]!.trigger("keydown", { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(wrapper.findAll("button")[1]!.element);
    await wrapper.get("button").trigger("keydown", { key: "Escape" });
    expect(await pending).toBe(false);
    expect(cleanup).not.toHaveBeenCalled();
    const takeover = access.requestTakeover();
    await nextTick();
    await wrapper.findAll("button")[1]!.trigger("click");
    expect(await takeover).toBe(true);
    expect(cleanup).toHaveBeenCalledOnce();
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    wrapper.unmount();
    access.dispose();
  });
});
