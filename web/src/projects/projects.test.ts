import { describe, expect, it } from "vitest";
import { ApiError } from "../shared/http.ts";
import type { ProjectsGateway, ProjectDetail } from "./gateway.ts";
import { createProjects, type ProjectsScheduler } from "./projects.ts";

const projectId = "project-a";
const detail: ProjectDetail = {
  id: projectId,
  title: "Canonical field notes",
  createdAt: "2026-10-26T10:00:00Z",
  updatedAt: "2026-10-26T10:01:00Z",
  document: { sections: [] },
};

function fakeGateway(overrides: Partial<ProjectsGateway> = {}): ProjectsGateway {
  return {
    listProjects: async () => [detail],
    getProjectDetail: async () => detail,
    audioDownloadUrl: (id) => `https://api.example.test/audio-assets/${id}/download`,
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => (resolve = yes));
  return { promise, resolve };
}

describe("Projects module", () => {
  it("loads collection and detail independently", async () => {
    const projects = createProjects(fakeGateway());
    await Promise.all([projects.loadList(), projects.loadProject(projectId)]);
    expect(projects.list.value).toEqual({ kind: "ready", value: [detail] });
    expect(projects.detail.value).toEqual({ kind: "ready", value: detail });

    projects.clearDetail();
    expect(projects.detail.value).toEqual({ kind: "idle" });
    expect(projects.list.value.kind).toBe("ready");
  });

  it("ignores a stale detail response after a newer request", async () => {
    const first = deferred<ProjectDetail>();
    const second = deferred<ProjectDetail>();
    const requests = [first, second];
    const projects = createProjects(
      fakeGateway({ getProjectDetail: () => requests.shift()!.promise }),
    );

    const stale = projects.loadProject("first");
    const current = projects.loadProject("second");
    first.resolve({ ...detail, id: "first" });
    second.resolve({ ...detail, id: "second" });
    await Promise.all([stale, current]);

    expect(projects.detail.value).toEqual({
      kind: "ready",
      value: { ...detail, id: "second" },
    });
  });

  for (const status of [400, 403, 404, 422]) {
    it(`maps backend Project rejection ${status} to the privacy-preserving not-found state`, async () => {
      const projects = createProjects(
        fakeGateway({
          getProjectDetail: async () => {
            throw new ApiError("GET", "/projects/opaque", status);
          },
        }),
      );
      await projects.loadProject("opaque");
      expect(projects.detail.value).toEqual({ kind: "notFound" });
    });
  }

  it("delays the visible loading status", async () => {
    let showSlow!: () => void;
    const scheduler: ProjectsScheduler = {
      schedule(_delay, callback) {
        showSlow = callback;
        return () => undefined;
      },
    };
    const pending = deferred<never[]>();
    const projects = createProjects(
      fakeGateway({ listProjects: () => pending.promise }),
      scheduler,
    );
    const load = projects.loadList();
    expect(projects.list.value).toEqual({ kind: "loading", slow: false });
    showSlow();
    expect(projects.list.value).toEqual({ kind: "loading", slow: true });
    pending.resolve([]);
    await load;
    expect(projects.list.value).toEqual({ kind: "ready", value: [] });
  });
});
