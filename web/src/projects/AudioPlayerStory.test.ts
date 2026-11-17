import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import AudioPlayerStory from "./AudioPlayerStory.vue";

const props = {
  authError: false,
  missing: false,
  overload: false,
  serverFailure: false,
  networkFailure: false,
  invalidMedia: false,
  slowInitialLoad: false,
  prolongedBuffering: false,
};

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("Audio player story", () => {
  it("does not poll HTTP activity before the player is activated", () => {
    vi.useFakeTimers();
    const fetch = vi.fn();
    vi.stubGlobal("fetch", fetch);
    const wrapper = mount(AudioPlayerStory, { props });

    vi.advanceTimersByTime(1_000);

    expect(fetch).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("does not make diagnostic activity requests during playback", async () => {
    vi.useFakeTimers();
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
    const fetch = vi.fn(async (input: string | URL | Request) => {
      expect(String(input)).not.toContain("/activity");
      return { ok: true, status: 200, url: "/snapshot/1" };
    });
    vi.stubGlobal("fetch", fetch);
    const wrapper = mount(AudioPlayerStory, { props });

    await wrapper.get("button").trigger("click");
    await flushPromises();
    const audio = wrapper.get("audio").element;
    audio.dispatchEvent(new Event("playing"));
    vi.advanceTimersByTime(1_000);
    await flushPromises();

    expect(fetch).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it("preserves the active player when a failure control changes", async () => {
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
    const fetch = vi.fn(async () => ({ ok: true, status: 200, url: "/snapshot/1" }));
    vi.stubGlobal("fetch", fetch);
    const wrapper = mount(AudioPlayerStory, { props });

    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.find("audio").exists()).toBe(true);

    await wrapper.setProps({ serverFailure: true });
    await flushPromises();

    expect(wrapper.find("audio").exists()).toBe(true);
    expect(wrapper.find("button[aria-label='Play audio for Section 1']").exists()).toBe(false);
    expect(fetch).toHaveBeenCalledWith(
      expect.stringMatching(/^\/__running-drafts\/audio\/scenario\?story=[^&]+&server=1$/u),
      { method: "POST" },
    );
    wrapper.unmount();
  });
});
