import assert from "node:assert/strict";
import test from "node:test";
import { ApiClient, ApiError } from "../src/api.ts";

test("application requests include the session cookie credential mode", async () => {
  let init: RequestInit | undefined;
  const client = new ApiClient({ baseUrl: "https://api.example.test/api", request: async (_input, options) => {
    init = options;
    return new Response('{"status":"complete","chunks":[]}');
  }});

  await client.getRecording("recording 1");

  assert.equal(init?.credentials, "include");
  assert.equal(init?.method, "GET");
});

test("retains Problem Details and coalesces concurrent authentication expiry", async () => {
  let expiries = 0;
  const request = async () => new Response(JSON.stringify({ title: "Authentication required",
    code: "authentication_required", referenceId: "ref-safe" }), {
    status: 401, headers: { "Content-Type": "application/problem+json" },
  });
  const client = new ApiClient({ baseUrl: "https://api.example.test/api", request,
    onAuthenticationExpired: () => expiries++ });

  const results = await Promise.allSettled([
    client.getRecording("one"), client.getRecording("two"), client.getRecording("three"),
  ]);

  assert.equal(expiries, 1);
  const error = results[0]!.status === "rejected" ? results[0]!.reason as unknown : null;
  assert.ok(error instanceof ApiError);
  assert.equal(error.problem?.code, "authentication_required");
  assert.equal(error.problem?.referenceId, "ref-safe");
  client.markAuthenticationUsable();
  await assert.rejects(() => client.getRecording("four"));
  assert.equal(expiries, 2);
});

test("does not report ordinary recovery statuses as authentication expiry", async () => {
  let expiries = 0;
  for (const status of [409, 413, 429, 503]) {
    const client = new ApiClient({ baseUrl: "https://api.example.test/api",
      request: async () => new Response('{"code":"temporary"}', { status }),
      onAuthenticationExpired: () => expiries++ });
    await assert.rejects(() => client.getRecording("one"), (error: unknown) =>
      error instanceof ApiError && error.status === status);
  }
  assert.equal(expiries, 0);
});

test("drops an untrusted Problem Details reference", async () => {
  const client = new ApiClient({ baseUrl: "https://api.example.test/api",
    request: async () => Response.json({ title: "Failed", referenceId: "exception: database password" }, { status: 500 }) });
  await assert.rejects(() => client.getRecording("one"), (error: unknown) =>
    error instanceof ApiError && error.problem?.referenceId === undefined);
});
