import { describe, expect, it } from "vitest";
import { createApiRequester } from "../shared/http.ts";
import { createProjectsGateway } from "../projects/gateway.ts";
import { TRANSPORT_FRAME_MEDIA_TYPE } from "./capture.ts";
import { createRecordingGateway } from "./gateway.ts";

const completedAsset = {
  id: "asset",
  mediaType: TRANSPORT_FRAME_MEDIA_TYPE,
  isSealed: true,
  sealReason: "completed" as const,
  sealedAt: "now",
  frameCount: 1,
  observedDurationMs: 1_234,
  storedBytes: 4_321,
};

describe("authenticated v1 wire contract", () => {
  it("uses the Project, Section, frame, and Audio Asset endpoints", async () => {
    const requests: { url: string; init: RequestInit }[] = [];
    const responses: unknown[] = [
      { id: "project", title: "Canonical title", createdAt: "now", updatedAt: "now" },
      { id: "project/id", title: "Existing title", createdAt: "before", updatedAt: "now" },
      { sectionId: "section", audioAssetId: "asset", position: 0 },
      { sequence: 0, acknowledged: true, sha256: "a".repeat(64), nextExpectedSequence: 1 },
      { id: "asset", isSealed: true, sealReason: "completed" },
      { id: "asset", isSealed: true, sealReason: "interrupted" },
      {
        projects: [{ id: "project", title: "Canonical title", createdAt: "now", updatedAt: "now" }],
      },
      {
        id: "project",
        title: "Canonical title",
        createdAt: "now",
        updatedAt: "now",
        document: { sections: [{ id: "section", position: 0, audioAsset: completedAsset }] },
      },
    ];
    const api = createApiRequester({
      baseUrl: "https://api.example.test/api/v1",
      request: async (input, init) => {
        requests.push({ url: String(input), init: init! });
        return Response.json(responses.shift());
      },
    });
    const recording = createRecordingGateway(api);
    const projects = createProjectsGateway(api);
    const frame = {
      seq: 0,
      startMs: 0,
      endMs: 1234,
      mediaType: TRANSPORT_FRAME_MEDIA_TYPE,
      data: new Blob(["exact bytes"], { type: TRANSPORT_FRAME_MEDIA_TYPE }),
    };

    expect((await recording.createProject("project-key", "Draft title")).title).toBe(
      "Canonical title",
    );
    expect((await recording.getProject("project/id")).title).toBe("Existing title");
    await recording.createSection("project/id", "section-key");
    await recording.putFrame("asset/id", frame, new AbortController().signal);
    await recording.complete("asset", 1);
    await recording.interrupt("asset");
    expect((await projects.listProjects())[0]!.id).toBe("project");
    expect((await projects.getProjectDetail("project")).document.sections[0]!.audioAsset.id).toBe(
      "asset",
    );
    expect(projects.audioDownloadUrl("asset/id")).toBe(
      "https://api.example.test/api/v1/audio-assets/asset%2Fid/download",
    );

    expect(requests.map((request) => request.url)).toEqual([
      "https://api.example.test/api/v1/projects",
      "https://api.example.test/api/v1/projects/project%2Fid",
      "https://api.example.test/api/v1/projects/project%2Fid/sections",
      "https://api.example.test/api/v1/audio-assets/asset%2Fid/frames/0",
      "https://api.example.test/api/v1/audio-assets/asset/complete",
      "https://api.example.test/api/v1/audio-assets/asset/interrupt",
      "https://api.example.test/api/v1/projects",
      "https://api.example.test/api/v1/projects/project",
    ]);
    expect(requests.every((request) => request.init.credentials === "include")).toBe(true);
    expect(JSON.parse(String(requests[0]!.init.body))).toEqual({
      idempotencyKey: "project-key",
      title: "Draft title",
    });
    expect(JSON.parse(String(requests[2]!.init.body))).toEqual({
      idempotencyKey: "section-key",
      mediaType: TRANSPORT_FRAME_MEDIA_TYPE,
    });
    const frameHeaders = requests[3]!.init.headers as Record<string, string>;
    expect(frameHeaders["Content-Type"]).toBe(TRANSPORT_FRAME_MEDIA_TYPE);
    expect(frameHeaders["X-Frame-Start-Ms"]).toBe("0");
    expect(requests[3]!.init.body).toBe(frame.data);
    expect(JSON.parse(String(requests[4]!.init.body))).toEqual({ frameCount: 1 });
    expect(requests[5]!.init.body).toBeUndefined();
  });
});
