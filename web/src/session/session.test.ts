import { describe, expect, it } from "vitest";
import { ApiError } from "../shared/http.ts";
import { authenticationCallbackFromSearch, withoutAuthenticationCallback } from "./callback.ts";
import {
  createSessionGateway,
  parseCurrentSession,
  safeReturnPath,
  type AccountId,
  type CurrentSession,
  type SessionGateway,
} from "./gateway.ts";
import { AUTH_PROVIDERS, resolveProvider } from "./providers.ts";
import {
  createSession,
  MINIMUM_RECORDING_SESSION_MS,
  sessionAllowsRecording,
  type SessionLocation,
} from "./session.ts";

const google = resolveProvider("google");
const fullSession: CurrentSession = {
  account: { id: "5728cdd2-909d-4207-8303-c04b270c308f" as AccountId },
  provider: google,
  profile: {
    email: "runner@example.test",
    emailVerified: true,
    displayName: "Runner",
    givenName: "Run",
    familyName: "Ner",
    pictureUrl: null,
    locale: "en",
  },
  sessionExpiresAt: "2026-10-21T00:00:00Z",
};

function gateway(overrides: Partial<SessionGateway> = {}): SessionGateway {
  return {
    current: async () => fullSession,
    logout: async () => undefined,
    loginUrl: (key, returnTo) => `https://api.example.test/auth/login/${key}?return_to=${returnTo}`,
    ...overrides,
  };
}

function fakeLocation(search = "") {
  const navigations: string[] = [];
  const replacements: string[] = [];
  const location: SessionLocation = {
    current: () => ({
      path: `/projects/one${search}`,
      search,
      href: `https://editor.test/projects/one${search}`,
    }),
    navigate: (url) => navigations.push(url),
    replace: (url) => replacements.push(url),
  };
  return { location, navigations, replacements };
}

describe("provider catalog", () => {
  it("is source-owned and contains Google only", () => {
    expect(AUTH_PROVIDERS).toEqual([{ key: "google", displayName: "Google" }]);
  });

  it("shows an unknown provider key as received", () => {
    expect(resolveProvider("microsoft")).toEqual({ key: "microsoft", displayName: "microsoft" });
  });
});

describe("session gateway", () => {
  const response = (profile: object, provider: object = { id: "google" }) =>
    JSON.stringify({
      account: fullSession.account,
      provider,
      profile,
      sessionExpiresAt: fullSession.sessionExpiresAt,
    });

  it("accepts a profile with every optional field absent and resolves the provider key", async () => {
    const requests: { url: string; init: RequestInit }[] = [];
    const client = createSessionGateway("https://api.example.test", async (input, init) => {
      requests.push({ url: String(input), init: init ?? {} });
      return new Response(response({}, { id: "google", displayName: "Ignored label" }));
    });

    const session = await client.current();
    expect(session.profile).toEqual({
      email: null,
      emailVerified: null,
      displayName: null,
      givenName: null,
      familyName: null,
      pictureUrl: null,
      locale: null,
    });
    expect(session.provider).toEqual({ key: "google", displayName: "Google" });
    expect(requests.map((request) => request.url)).toEqual(["https://api.example.test/api/v1/me"]);
    expect(requests.every((request) => request.init.credentials === "include")).toBe(true);
  });

  it("never asks the backend which providers exist", async () => {
    const urls: string[] = [];
    const client = createSessionGateway("https://api.example.test", async (input) => {
      urls.push(String(input));
      return new Response(response({}));
    });
    await client.current();
    await client.logout();
    expect(urls.some((url) => url.includes("/auth/providers"))).toBe(false);
  });

  it("rejects an expiry that is not an RFC 3339 instant", () => {
    expect(() => parseCurrentSession({ ...fullSession, sessionExpiresAt: "2026-10-21" })).toThrow(
      /Malformed session expiry/,
    );
  });

  it("rejects a session without a provider key", () => {
    expect(() =>
      parseCurrentSession({ ...fullSession, provider: { displayName: "Google" } }),
    ).toThrow(/Malformed provider key/);
  });

  it("keeps only a local path and query in the login URL", () => {
    const client = createSessionGateway("https://api.example.test", async () => new Response());
    expect(client.loginUrl("google", "/projects/one?tab=audio")).toBe(
      "https://api.example.test/auth/login/google?return_to=%2Fprojects%2Fone%3Ftab%3Daudio",
    );
    expect(safeReturnPath("//attacker.example/path")).toBe("/");
    expect(safeReturnPath("https://attacker.example/path")).toBe("/");
  });
});

describe("authentication callback", () => {
  it("accepts only expected values", () => {
    const failed = { error: "authentication_failed", referenceId: null };
    expect(authenticationCallbackFromSearch("?authError=provider_unavailable")).toEqual({
      error: "provider_unavailable",
      referenceId: null,
    });
    expect(
      authenticationCallbackFromSearch("?authError=authentication_failed&referenceId=safe_REF-42"),
    ).toEqual({ error: "authentication_failed", referenceId: "safe_REF-42" });
    expect(authenticationCallbackFromSearch("?authError=authentication_failed")).toEqual(failed);
    for (const search of [
      "?authError=authentication_failed&referenceId=%3Cscript%3E",
      "?authError=raw-provider-message&referenceId=safe",
      "?authError=cancelled&authError=cancelled",
      "?authError=cancelled&authError=raw-provider-message",
      "?authError=authentication_failed&referenceId=one&referenceId=two",
      "?authError=cancelled&referenceId=attacker-selected",
      "?referenceId=attacker-selected",
    ]) {
      expect(authenticationCallbackFromSearch(search), search).toEqual(failed);
    }
  });

  it("removes the callback parameters and keeps everything else", () => {
    expect(
      withoutAuthenticationCallback(
        "https://editor.test/login?keep=1&authError=authentication_failed&referenceId=ref#top",
      ),
    ).toBe("/login?keep=1#top");
    expect(
      withoutAuthenticationCallback(
        "https://editor.test/login?keep=1&keep=2&authError=cancelled&authError=failed&referenceId=a&referenceId=b",
      ),
    ).toBe("/login?keep=1&keep=2");
  });
});

describe("session module", () => {
  it("distinguishes signed in, ordinary signed out, backend failure, and malformed data", async () => {
    const { location } = fakeLocation();
    const signedIn = createSession(gateway(), location);
    expect(signedIn.state.value.status).toBe("loading");
    await signedIn.bootstrap();
    expect(signedIn.state.value).toEqual({ status: "signedIn", session: fullSession });

    const signedOut = createSession(
      gateway({
        current: async () => {
          throw new ApiError("GET", "/api/v1/me", 401, { code: "authentication_required" });
        },
      }),
      location,
    );
    await signedOut.bootstrap({ error: "cancelled", referenceId: null });
    expect(signedOut.state.value).toEqual({
      status: "signedOut",
      message: "Sign-in was cancelled.",
      logoutUnconfirmed: false,
      loginAllowed: true,
      referenceId: null,
    });

    const failure = createSession(
      gateway({
        current: async () => {
          throw new ApiError("GET", "/api/v1/me", 503, { referenceId: "safe-ref" });
        },
      }),
      location,
    );
    await failure.bootstrap();
    expect(failure.state.value).toEqual({
      status: "failure",
      message: "Running Drafts could not check your session.",
      referenceId: "safe-ref",
    });

    const malformed = createSession(
      createSessionGateway("https://api.example.test", async () => new Response('{"account":3}')),
      location,
    );
    await malformed.bootstrap();
    expect(malformed.state.value.status).toBe("failure");
  });

  it("starts from the address: reads the callback, cleans the URL, and checks the session", async () => {
    const { location, replacements } = fakeLocation("?authError=cancelled&keep=1");
    const session = createSession(
      gateway({
        current: async () => {
          throw new ApiError("GET", "/api/v1/me", 401, { code: "authentication_required" });
        },
      }),
      location,
    );
    await session.start();
    expect(replacements).toEqual(["/projects/one?keep=1"]);
    expect(session.state.value).toMatchObject({
      status: "signedOut",
      message: "Sign-in was cancelled.",
    });
  });

  it("hides the Account immediately and reports an unconfirmed server logout", async () => {
    let rejectLogout!: (error: Error) => void;
    const logout = new Promise<void>((_resolve, reject) => {
      rejectLogout = reject;
    });
    const session = createSession(gateway({ logout: () => logout }), fakeLocation().location);
    await session.bootstrap();
    const pending = session.logout();
    expect(session.state.value).toMatchObject({ status: "signedOut", loginAllowed: false });
    rejectLogout(new Error("offline"));
    await pending;
    expect(session.state.value).toMatchObject({
      status: "signedOut",
      logoutUnconfirmed: true,
      loginAllowed: false,
    });
  });

  it("returns to the current page after provider sign-in", () => {
    const { location, navigations } = fakeLocation("?tab=audio");
    createSession(gateway(), location).login("google");
    expect(navigations).toEqual([
      "https://api.example.test/auth/login/google?return_to=/projects/one?tab=audio",
    ]);
  });

  it("refuses a recording whose Session is about to end, and logs out", async () => {
    const now = Date.now();
    const session = createSession(
      gateway({
        current: async () => ({
          ...fullSession,
          sessionExpiresAt: new Date(now + MINIMUM_RECORDING_SESSION_MS - 60_000).toISOString(),
        }),
      }),
      fakeLocation().location,
    );
    expect(session.authorizeRecordingStart()).toBe(false);
    await session.bootstrap();
    expect(session.authorizeRecordingStart()).toBe(false);
    await Promise.resolve();
    expect(session.state.value).toMatchObject({
      status: "signedOut",
      message: expect.stringContaining("Sign in again before recording"),
    });
  });

  it("allows exactly six hours and rejects less or malformed expiry", () => {
    const now = Date.parse("2026-10-20T00:00:00Z");
    const at = (offset: number) => new Date(now + offset).toISOString();
    expect(sessionAllowsRecording(at(MINIMUM_RECORDING_SESSION_MS - 1), now)).toBe(false);
    expect(sessionAllowsRecording(at(MINIMUM_RECORDING_SESSION_MS), now)).toBe(true);
    expect(sessionAllowsRecording(at(MINIMUM_RECORDING_SESSION_MS + 1), now)).toBe(true);
    expect(sessionAllowsRecording("not-a-date", now)).toBe(false);
  });
});
