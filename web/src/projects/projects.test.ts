import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { axe } from "../test-support/axe.ts";
import type { ProjectsGateway, ProjectDetail } from "./gateway.ts";
import ProjectBrowser from "./ProjectBrowser.vue";
import { createProjects, projectsKey, type Projects } from "./projects.ts";

const createdId = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const newerId = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
const sectionId = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const assetId = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";

const detail: ProjectDetail = {
  id: createdId,
  title: "Canonical field notes",
  createdAt: "2026-10-26T10:00:00Z",
  updatedAt: "2026-10-26T10:01:00Z",
  document: {
    sections: [
      {
        id: sectionId,
        position: 0,
        audioAsset: {
          id: assetId,
          mediaType: "audio/webm;codecs=opus",
          isSealed: true,
          sealReason: "completed",
          sealedAt: "2026-10-26T10:01:00Z",
          frameCount: 3,
          observedDurationMs: 21_500,
          storedBytes: 123_456,
        },
      },
    ],
  },
};

function fakeGateway(overrides: Partial<ProjectsGateway> = {}): ProjectsGateway {
  return {
    listProjects: async () => [
      { id: newerId, title: "Newer Project", createdAt: "x", updatedAt: "2026-10-26T10:02:00Z" },
      { id: createdId, title: detail.title, createdAt: "x", updatedAt: detail.updatedAt },
    ],
    getProjectDetail: async () => detail,
    audioDownloadUrl: (id) => `https://api.example.test/api/v1/audio-assets/${id}/download`,
    ...overrides,
  };
}

function mountBrowser(projects: Projects) {
  return mount(ProjectBrowser, { global: { provide: { [projectsKey as symbol]: projects } } });
}

describe("Projects module", () => {
  it("lists Projects, opens one, and clears on request", async () => {
    const projects = createProjects(fakeGateway());
    await projects.showList();
    expect(projects.view.value).toMatchObject({ kind: "list" });
    await projects.showProject(createdId);
    expect(projects.view.value).toMatchObject({ kind: "detail" });
    expect(projects.status.value).toBe("");
    projects.clear();
    expect(projects.view.value).toEqual({ kind: "empty" });
  });

  it("ignores a stale response after a newer request", async () => {
    let release!: (value: ProjectDetail) => void;
    const projects = createProjects(
      fakeGateway({ getProjectDetail: () => new Promise((resolve) => (release = resolve)) }),
    );
    const pending = projects.showProject(createdId);
    projects.clear();
    release(detail);
    await pending;
    expect(projects.view.value).toEqual({ kind: "empty" });
  });
});

describe("ProjectBrowser", () => {
  it("shows Projects in backend order and opens a downloadable Section", async () => {
    const projects = createProjects(fakeGateway());
    const wrapper = mountBrowser(projects);
    await projects.showList();
    await flushPromises();

    const entries = wrapper.findAll("[data-testid=project-list-item]");
    expect(entries.map((entry) => entry.attributes("data-project-id"))).toEqual([
      newerId,
      createdId,
    ]);
    await entries[1]!.get("button").trigger("click");
    await flushPromises();

    const view = wrapper.get("[data-testid=project-detail]");
    expect(view.attributes("data-project-id")).toBe(createdId);
    expect(view.get("h2").text()).toBe("Canonical field notes");
    const section = view.get("[data-testid=project-section]");
    expect(section.attributes()).toMatchObject({
      "data-section-id": sectionId,
      "data-audio-asset-id": assetId,
      "data-sealed": "true",
      "data-seal-reason": "completed",
    });
    const download = section.get("[data-testid=audio-download]");
    expect(download.attributes("href")).toBe(
      `https://api.example.test/api/v1/audio-assets/${assetId}/download`,
    );
    expect(download.text()).toBe("Download original audio");
  });

  it("offers a visible retry after a failed load", async () => {
    let attempts = 0;
    const projects = createProjects(
      fakeGateway({
        listProjects: async () => {
          attempts++;
          if (attempts === 1) throw new Error("unavailable");
          return [];
        },
      }),
    );
    const wrapper = mountBrowser(projects);
    await projects.showList();
    await flushPromises();
    expect(wrapper.get("[role=status]").text()).toBe("Projects could not be loaded.");

    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(attempts).toBe(2);
    expect(wrapper.text()).toContain("No Projects yet.");
  });

  it("has no detectable accessibility violations", async () => {
    const projects = createProjects(fakeGateway());
    const wrapper = mountBrowser(projects);
    await projects.showList();
    await flushPromises();
    expect(await axe(wrapper.element as HTMLElement)).toHaveNoViolations();
  });
});
