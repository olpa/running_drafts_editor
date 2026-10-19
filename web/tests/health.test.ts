import assert from "node:assert/strict";
import test from "node:test";

globalThis.window = { __BACKEND_BASE_URL__: "https://configured.example.test" } as Window & typeof globalThis;

const { HealthClient } = await import("../src/health.ts");
const { backendBaseUrl, backendUrl } = await import("../src/backend.ts");

test("uses one configured base URL for every backend path", () => {
  assert.equal(backendBaseUrl(), "https://configured.example.test");
  assert.equal(backendUrl("/api"), "https://configured.example.test/api");
  assert.equal(backendUrl("/health/live"), "https://configured.example.test/health/live");
});

test("calls the browser fetch implementation with Window as its receiver", async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = function (this: unknown): Promise<Response> {
    if (this !== window) {
      throw new TypeError("'fetch' called on an object that does not implement interface Window.");
    }
    return Promise.resolve(new Response('{"status":"ok"}', { status: 200 }));
  } as typeof fetch;
  try {
    const client = new HealthClient("https://api.example.test");
    const result = await client.check("live");
    assert.equal(result.status, 200);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("checks liveness at the configured backend", async () => {
  let requestedUrl = "";
  const request = async (input: string | URL | Request): Promise<Response> => {
    requestedUrl = String(input);
    return new Response('{"status":"ok"}', { status: 200, statusText: "OK" });
  };
  const client = new HealthClient("http://localhost:18081/", request as typeof fetch);

  const result = await client.check("live");

  assert.equal(requestedUrl, "http://localhost:18081/health/live");
  assert.deepEqual(result, {
    endpoint: "live", status: 200, statusText: "OK", body: '{"status":"ok"}',
  });
});

test("keeps readiness distinct and reports an unhealthy response", async () => {
  let requestedUrl = "";
  const request = async (input: string | URL | Request): Promise<Response> => {
    requestedUrl = String(input);
    return new Response("database unavailable", { status: 503, statusText: "Service Unavailable" });
  };
  const client = new HealthClient("https://api.example.test", request as typeof fetch);

  await assert.rejects(() => client.check("ready"), /HTTP 503 Service Unavailable/);
  assert.equal(requestedUrl, "https://api.example.test/health/ready");
});
