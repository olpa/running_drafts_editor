import assert from "node:assert/strict";
import test from "node:test";
import { ApiClient, ApiError, TRANSPORT_FRAME_MEDIA_TYPE } from "../src/api.ts";

const completedAsset = {
  id: "asset", mediaType: TRANSPORT_FRAME_MEDIA_TYPE, isSealed: true,
  sealReason: "completed" as const, sealedAt: "now", frameCount: 1,
  observedDurationMs: 1_234, storedBytes: 4_321,
};

test("uses the authenticated v1 Project and Audio Asset wire contract", async () => {
  const requests: { url: string; init: RequestInit }[] = [];
  const responses = [
    { id: "project", title: "Canonical title", createdAt: "now", updatedAt: "now" },
    { id: "project/id", title: "Existing title", createdAt: "before", updatedAt: "now",
      document: { sections: [] } },
    { sectionId: "section", audioAssetId: "asset", position: 0 },
    { sequence: 0, acknowledged: true, sha256: "a".repeat(64), nextExpectedSequence: 1 },
    { id: "asset", isSealed: true, sealReason: "completed" },
    { id: "asset", isSealed: true, sealReason: "interrupted" },
    { projects: [
      { id: "project", title: "Canonical title", createdAt: "now", updatedAt: "now" },
    ] },
    { id: "project", title: "Canonical title", createdAt: "now", updatedAt: "now",
      document: { sections: [{ id: "section", position: 0, audioAsset: completedAsset }] } },
  ];
  const client = new ApiClient({ baseUrl: "https://api.example.test/api/v1", request: async (input, init) => {
    requests.push({ url: String(input), init: init! });
    return Response.json(responses.shift());
  }});
  const signal = new AbortController().signal;
  const frame = { seq: 0, startMs: 0, endMs: 1234, mediaType: TRANSPORT_FRAME_MEDIA_TYPE,
    data: new Blob(["exact bytes"], { type: TRANSPORT_FRAME_MEDIA_TYPE }) };

  const project = await client.createProject("project-key", "Draft title");
  const existing = await client.getProject("project/id");
  await client.createSection("project/id", "section-key");
  await client.putFrame("asset/id", frame, signal);
  await client.complete("asset", 1);
  await client.interrupt("asset");
  const projects = await client.listProjects();
  const detail = await client.getProjectDetail("project");

  assert.equal(project.title, "Canonical title");
  assert.equal(existing.title, "Existing title");
  assert.equal(projects[0]!.id, "project");
  assert.equal(detail.document.sections[0]!.audioAsset.id, "asset");
  assert.equal(client.audioDownloadUrl("asset/id"),
    "https://api.example.test/api/v1/audio-assets/asset%2Fid/download");
  assert.deepEqual(requests.map((request) => request.url), [
    "https://api.example.test/api/v1/projects",
    "https://api.example.test/api/v1/projects/project%2Fid",
    "https://api.example.test/api/v1/projects/project%2Fid/sections",
    "https://api.example.test/api/v1/audio-assets/asset%2Fid/frames/0",
    "https://api.example.test/api/v1/audio-assets/asset/complete",
    "https://api.example.test/api/v1/audio-assets/asset/interrupt",
    "https://api.example.test/api/v1/projects",
    "https://api.example.test/api/v1/projects/project",
  ]);
  assert.ok(requests.every((request) => request.init.credentials === "include"));
  assert.deepEqual(JSON.parse(String(requests[0]!.init.body)),
    { idempotencyKey: "project-key", title: "Draft title" });
  assert.deepEqual(JSON.parse(String(requests[2]!.init.body)),
    { idempotencyKey: "section-key", mediaType: TRANSPORT_FRAME_MEDIA_TYPE });
  assert.equal((requests[3]!.init.headers as Record<string, string>)["Content-Type"], TRANSPORT_FRAME_MEDIA_TYPE);
  assert.equal((requests[3]!.init.headers as Record<string, string>)["X-Frame-Start-Ms"], "0");
  assert.equal(requests[3]!.init.body, frame.data);
  assert.deepEqual(JSON.parse(String(requests[4]!.init.body)), { frameCount: 1 });
  assert.equal(requests[5]!.init.body, undefined);
});

test("retains Problem Details and coalesces concurrent authentication expiry", async () => {
  let expiries = 0;
  const request = async () => new Response(JSON.stringify({ title: "Authentication required",
    code: "authentication_required", referenceId: "ref-safe" }), {
    status: 401, headers: { "Content-Type": "application/problem+json" },
  });
  const client = new ApiClient({ baseUrl: "https://api.example.test/api/v1", request,
    onAuthenticationExpired: () => expiries++ });

  const results = await Promise.allSettled([
    client.createProject("one", "One"), client.createProject("two", "Two"),
    client.createProject("three", "Three"),
  ]);
  assert.equal(expiries, 1);
  const error = results[0]!.status === "rejected" ? results[0]!.reason as unknown : null;
  assert.ok(error instanceof ApiError);
  assert.equal(error.problem?.code, "authentication_required");
  assert.equal(error.problem?.referenceId, "ref-safe");
  client.markAuthenticationUsable();
  await assert.rejects(() => client.createProject("four", "Four"));
  assert.equal(expiries, 2);
});

test("does not report ordinary recovery statuses as authentication expiry", async () => {
  let expiries = 0;
  for (const status of [409, 413, 429, 503]) {
    const client = new ApiClient({ baseUrl: "https://api.example.test/api/v1",
      request: async () => new Response('{"code":"temporary"}', { status }),
      onAuthenticationExpired: () => expiries++ });
    await assert.rejects(() => client.createProject("key", "Title"), (error: unknown) =>
      error instanceof ApiError && error.status === status);
  }
  assert.equal(expiries, 0);
});

test("drops an untrusted Problem Details reference", async () => {
  const client = new ApiClient({ baseUrl: "https://api.example.test/api/v1",
    request: async () => Response.json({ title: "Failed", referenceId: "exception: database password" }, { status: 500 }) });
  await assert.rejects(() => client.createProject("key", "Title"), (error: unknown) =>
    error instanceof ApiError && error.problem?.referenceId === undefined);
});
