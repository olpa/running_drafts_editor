import { describe, expect, it } from "vitest";
import { backendBaseUrl, backendUrl } from "./backend.ts";
import { ApiError, createApiRequester } from "./http.ts";

function problemResponse(status: number, body: object): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/problem+json" },
  });
}

describe("backend URL", () => {
  it("uses one configured base URL for every backend path", () => {
    window.__BACKEND_BASE_URL__ = "https://configured.example.test/";
    try {
      expect(backendBaseUrl()).toBe("https://configured.example.test");
      expect(backendUrl("/api")).toBe("https://configured.example.test/api");
      expect(backendUrl("/health/live")).toBe("https://configured.example.test/health/live");
    } finally {
      delete window.__BACKEND_BASE_URL__;
    }
  });
});

describe("ApiRequester", () => {
  it("retains Problem Details and coalesces concurrent authentication expiry", async () => {
    let expiries = 0;
    const api = createApiRequester({
      baseUrl: "https://api.example.test/api/v1",
      request: async () =>
        problemResponse(401, {
          title: "Authentication required",
          code: "authentication_required",
          referenceId: "ref-safe",
        }),
      onAuthenticationExpired: () => expiries++,
    });

    const results = await Promise.allSettled([
      api.json("POST", "/projects"),
      api.json("POST", "/projects"),
      api.json("POST", "/projects"),
    ]);
    expect(expiries).toBe(1);
    const first = results[0]!;
    const error = first.status === "rejected" ? (first.reason as unknown) : null;
    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).problem).toMatchObject({
      code: "authentication_required",
      referenceId: "ref-safe",
    });

    api.markAuthenticationUsable();
    await expect(api.json("POST", "/projects")).rejects.toBeInstanceOf(ApiError);
    expect(expiries).toBe(2);
  });

  it("does not report ordinary recovery statuses as authentication expiry", async () => {
    let expiries = 0;
    for (const status of [409, 413, 429, 503]) {
      const api = createApiRequester({
        baseUrl: "https://api.example.test/api/v1",
        request: async () => new Response('{"code":"temporary"}', { status }),
        onAuthenticationExpired: () => expiries++,
      });
      await expect(api.json("POST", "/projects")).rejects.toMatchObject({ status });
    }
    expect(expiries).toBe(0);
  });

  it("drops an untrusted Problem Details reference", async () => {
    const api = createApiRequester({
      baseUrl: "https://api.example.test/api/v1",
      request: async () =>
        Response.json(
          { title: "Failed", referenceId: "exception: database password" },
          { status: 500 },
        ),
    });
    await expect(api.json("POST", "/projects")).rejects.toSatisfy(
      (error: unknown) => error instanceof ApiError && error.problem?.referenceId === undefined,
    );
  });

  it("sends credentials and resolves URLs against the API base", async () => {
    const seen: RequestInit[] = [];
    const api = createApiRequester({
      baseUrl: "https://api.example.test/api/v1",
      request: async (_input, init) => {
        seen.push(init ?? {});
        return Response.json({});
      },
    });
    await api.json("GET", "/projects");
    expect(seen[0]?.credentials).toBe("include");
    expect(api.url("/audio-assets/a/download")).toBe(
      "https://api.example.test/api/v1/audio-assets/a/download",
    );
  });
});
