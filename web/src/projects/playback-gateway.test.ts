import { describe, expect, it, vi } from "vitest";
import { createApiRequester } from "../shared/http.ts";
import { createProjectsGateway } from "./gateway.ts";

describe("playback gateway", () => {
  it("resolves latest with a credentialed HEAD and probes the immutable URL with one byte", async () => {
    const requests: Array<{ url: string; init: RequestInit }> = [];
    const send: typeof fetch = async (input, init) => {
      requests.push({ url: String(input), init: init! });
      if (init?.method === "HEAD") {
        return new Response(null, {
          status: 200,
          headers: { "Content-Type": "audio/webm" },
        });
      }
      return new Response(new Uint8Array([26]), {
        status: 206,
        headers: { "Content-Range": "bytes 0-0/100" },
      });
    };
    const api = createApiRequester({ baseUrl: "https://api.example.test/api/v1", request: send });
    const gateway = createProjectsGateway(api, send);
    const signal = new AbortController().signal;

    const resolved = await gateway.resolvePlaybackUrl("asset/id", signal);
    await gateway.probePlayback(resolved, signal);

    expect(resolved).toBe(
      "https://api.example.test/api/v1/audio-assets/asset%2Fid/playback/latest",
    );
    expect(requests).toHaveLength(2);
    expect(requests[0]!.init).toMatchObject({
      method: "HEAD",
      credentials: "include",
      redirect: "follow",
      signal,
    });
    expect(requests[1]!.init).toMatchObject({
      method: "GET",
      credentials: "include",
      headers: { Range: "bytes=0-0" },
      signal,
    });
  });

  it("does not report playback authentication failure through shared session expiry", async () => {
    const authenticationExpired = vi.fn();
    const send: typeof fetch = async () =>
      Response.json(
        { code: "authentication_required", referenceId: "safe_AUTH-2" },
        { status: 401 },
      );
    const api = createApiRequester({
      baseUrl: "https://api.example.test/api/v1",
      request: send,
      onAuthenticationExpired: authenticationExpired,
    });
    const gateway = createProjectsGateway(api, send);

    await expect(
      gateway.resolvePlaybackUrl("asset", new AbortController().signal),
    ).rejects.toMatchObject({ status: 401, problem: { referenceId: "safe_AUTH-2" } });
    expect(authenticationExpired).not.toHaveBeenCalled();
  });
});
