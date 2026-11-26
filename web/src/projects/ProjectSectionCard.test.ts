import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "../shared/http.ts";
import type { AudioAssetSummary } from "../shared/project-types.ts";
import { createProjects, projectsKey } from "./projects.ts";
import ProjectSectionCard from "./ProjectSectionCard.vue";
import SectionAudioPlayer from "./SectionAudioPlayer.vue";

const start = "2026-11-25T12:00:00Z";
const recent = "2026-11-25T12:00:15Z";
const old = "2026-11-25T12:01:00Z";
const asset: AudioAssetSummary = {
  id: "asset",
  createdAt: start,
  updatedAt: start,
  isSealed: false,
  sealReason: null,
  frameCount: 1,
  observedDurationMs: 1000,
  storedBytes: 100,
  mediaType: "audio/webm;codecs=opus",
};

function setup(initial = asset) {
  const unused = async () => {
    throw new Error("Unexpected request");
  };
  const getAudioAsset = vi.fn(async () => ({
    audioAsset: { ...initial, frameCount: 2, observedDurationMs: 2000 },
    serverTime: old,
  }));
  const confirmEnded = vi.fn(async () => initial);
  const gateway = {
    listProjects: vi.fn(unused),
    getProjectDetail: vi.fn(unused),
    getAudioAsset,
    confirmEnded,
    audioDownloadUrl: () => "/asset/download",
    resolvePlaybackUrl: unused,
    probePlayback: unused,
  };
  const projects = createProjects(gateway, undefined, { signInUrl: () => "/auth/login/provider" });
  const wrapper = mount(ProjectSectionCard, {
    props: { section: { id: "section", position: 0, audioAsset: initial }, serverTime: recent },
    global: { provide: { [projectsKey as symbol]: projects } },
    attachTo: document.body,
  });
  return { wrapper, gateway };
}

describe("Project Section card", () => {
  it("leaves the player mounted and uses just the asset read on recheck", async () => {
    const { wrapper, gateway } = setup();
    const player = wrapper.getComponent(SectionAudioPlayer).element;
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.getComponent(SectionAudioPlayer).element).toBe(player);
    expect(wrapper.text()).toContain("2.0 seconds secured");
    expect(wrapper.get("button").text()).toBe("Confirm recording has ended");
    expect(gateway.getAudioAsset).toHaveBeenCalledWith("asset");
    expect(gateway.listProjects).not.toHaveBeenCalled();
    expect(gateway.getProjectDetail).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("seals a zero-frame asset, announces it, and keeps focus on the card", async () => {
    const empty = { ...asset, frameCount: 0 };
    const { wrapper, gateway } = setup(empty);
    gateway.getAudioAsset
      .mockResolvedValueOnce({ audioAsset: empty, serverTime: old })
      .mockResolvedValueOnce({
        audioAsset: { ...empty, isSealed: true, sealReason: "user_resolved" },
        serverTime: old,
      });
    const button = wrapper.get("button");
    button.element.focus();
    await button.trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("No audio has been secured yet");
    expect(wrapper.findComponent(SectionAudioPlayer).exists()).toBe(false);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(gateway.confirmEnded).toHaveBeenCalledWith("asset", 0);
    expect(wrapper.attributes("data-seal-reason")).toBe("user_resolved");
    expect(wrapper.get('[role="status"]').text()).toBe("Recording sealed.");
    expect(document.activeElement).toBe(wrapper.get("h3").element);
    expect(wrapper.find("[data-testid=audio-download]").exists()).toBe(false);
    wrapper.unmount();
  });

  it.each(["completed", "interrupted", "user_resolved"] as const)(
    "shows the same download label for %s",
    (sealReason) => {
      const { wrapper } = setup({ ...asset, isSealed: true, sealReason });
      expect(wrapper.attributes("data-seal-reason")).toBe(sealReason);
      expect(wrapper.get("[data-testid=audio-download]").text()).toBe("Download original audio");
      expect(wrapper.text()).not.toContain("Unfinished audio");
      wrapper.unmount();
    },
  );

  it("keeps playback and download when lifecycle flags are missing", () => {
    const malformed = { ...asset };
    Reflect.deleteProperty(malformed, "isSealed");
    Reflect.deleteProperty(malformed, "frameCount");
    const { wrapper } = setup(malformed);
    expect(wrapper.text()).not.toContain("Unfinished audio");
    expect(wrapper.find("[data-testid=audio-download]").exists()).toBe(true);
    expect(wrapper.findComponent(SectionAudioPlayer).exists()).toBe(true);
    wrapper.unmount();
  });

  it("removes asset controls when a recheck returns not found", async () => {
    const { wrapper, gateway } = setup();
    gateway.getAudioAsset.mockRejectedValueOnce(new ApiError("GET", "/asset", 404));
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toBe("This audio is no longer available");
    expect(wrapper.find("button").exists()).toBe(false);
    expect(wrapper.findComponent(SectionAudioPlayer).exists()).toBe(false);
    expect(wrapper.find("[data-testid=audio-download]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("clears the recheck error after a successful retry", async () => {
    const { wrapper, gateway } = setup();
    gateway.getAudioAsset.mockRejectedValueOnce(new ApiError("GET", "/asset", 503));
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toBe("Could not refresh this audio.");
    expect(wrapper.get("button").text()).toBe("Retry refresh");
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.get("button").text()).toBe("Confirm recording has ended");
    wrapper.unmount();
  });

  it("keeps the confirmation error beside the same confirmation action", async () => {
    const { wrapper, gateway } = setup();
    gateway.confirmEnded.mockRejectedValueOnce(new ApiError("POST", "/asset/confirm-ended", 503));
    await wrapper.get("button").trigger("click");
    await flushPromises();
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toBe(
      "The server returned an error. Try confirming again.",
    );
    expect(wrapper.get("button").text()).toBe("Confirm recording has ended");
    expect(gateway.getAudioAsset).toHaveBeenCalledOnce();
    wrapper.unmount();
  });

  it("keeps the card and offers an inline sign-in link", async () => {
    const { wrapper, gateway } = setup();
    gateway.getAudioAsset.mockRejectedValueOnce(new ApiError("GET", "/asset", 401));
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.get("a").text()).toBe("Sign in");
    expect(wrapper.get("a").attributes("href")).toBe("/auth/login/provider");
    expect(wrapper.findComponent(SectionAudioPlayer).exists()).toBe(true);
    wrapper.unmount();
  });
});
