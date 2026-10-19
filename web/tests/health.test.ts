import assert from "node:assert/strict";
import test from "node:test";

globalThis.window = {} as Window & typeof globalThis;

const { HealthClient } = await import("../src/health.ts");

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
