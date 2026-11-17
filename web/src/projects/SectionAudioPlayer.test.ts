import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import type { ProjectsGateway } from "./gateway.ts";
import { createProjects, projectsKey } from "./projects.ts";
import SectionAudioPlayer from "./SectionAudioPlayer.vue";

function gateway(resolvePlaybackUrl = vi.fn(async () => "/snapshot/1")): ProjectsGateway {
  return {
    listProjects: async () => [],
    getProjectDetail: async () => {
      throw new Error("unused");
    },
    audioDownloadUrl: () => "unused",
    resolvePlaybackUrl,
    probePlayback: async () => ({ status: 206, referenceId: null }),
  };
}

describe("Section audio player", () => {
  it("does not create a media source or request playback before explicit activation", async () => {
    const resolve = vi.fn(async () => "/snapshot/1");
    const wrapper = mount(SectionAudioPlayer, {
      props: { audioAssetId: "asset", frameCount: 20, sectionNumber: 2 },
      global: { provide: { [projectsKey as symbol]: createProjects(gateway(resolve)) } },
    });

    expect(wrapper.find("audio").exists()).toBe(false);
    expect(resolve).not.toHaveBeenCalled();
    expect(wrapper.get("button").attributes("aria-label")).toBe("Play audio for Section 2");
  });

  it("shows the zero-frame explanation without offering activation", () => {
    const resolve = vi.fn(async () => "/snapshot/1");
    const wrapper = mount(SectionAudioPlayer, {
      props: { audioAssetId: "asset", frameCount: 0, sectionNumber: 1 },
      global: { provide: { [projectsKey as symbol]: createProjects(gateway(resolve)) } },
    });

    expect(wrapper.text()).toContain("No audio is available yet.");
    expect(wrapper.find("button").exists()).toBe(false);
    expect(resolve).not.toHaveBeenCalled();
  });
});
