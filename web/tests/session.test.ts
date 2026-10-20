import assert from "node:assert/strict";
import test from "node:test";
import { ApiError } from "../src/api.ts";
import { authenticationCallbackFromSearch, MINIMUM_RECORDING_SESSION_MS, safeReturnPath,
  SessionClient, SessionController, sessionAllowsRecording, withoutAuthenticationCallback,
  type AccountId, type CurrentSession } from "../src/session.ts";

const fullSession: CurrentSession = {
  account: { id: "5728cdd2-909d-4207-8303-c04b270c308f" as AccountId },
  provider: { id: "google", displayName: "Google" },
  profile: { email: "runner@example.test", emailVerified: true, displayName: "Runner",
    givenName: "Run", familyName: "Ner", pictureUrl: null, locale: "en" },
  sessionExpiresAt: "2026-10-21T00:00:00Z",
};

test("session client accepts a profile with every optional field absent", async () => {
  const responses = [
    new Response('{"providers":[{"id":"google","displayName":"Google"}]}'),
    new Response(JSON.stringify({ account: fullSession.account, provider: fullSession.provider,
      profile: {}, sessionExpiresAt: fullSession.sessionExpiresAt })),
  ];
  const requests: RequestInit[] = [];
  const client = new SessionClient("https://api.example.test", async (_input, init) => {
    requests.push(init ?? {}); return responses.shift()!;
  });

  assert.deepEqual(await client.providers(), [{ id: "google", displayName: "Google" }]);
  assert.deepEqual((await client.current()).profile, { email: null, emailVerified: null,
    displayName: null, givenName: null, familyName: null, pictureUrl: null, locale: null });
  assert.ok(requests.every((request) => request.credentials === "include"));
});

test("session client rejects an expiry that is not an RFC 3339 instant", async () => {
  const client = new SessionClient("https://api.example.test", async () => Response.json({
    ...fullSession,
    sessionExpiresAt: "2026-10-21",
  }));
  await assert.rejects(() => client.current(), /Malformed session expiry/u);
});

test("bootstrap distinguishes signed in, ordinary signed out, backend failure, and malformed data", async () => {
  const providers = async () => [{ id: "google", displayName: "Google" }];
  const signedIn = new SessionController({ providers, current: async () => fullSession, logout: async () => undefined });
  await signedIn.bootstrap();
  assert.equal(signedIn.state.status, "signedIn");

  const signedOut = new SessionController({ providers, current: async () => { throw new ApiError("GET", "/api/v1/me", 401,
    { code: "authentication_required" }); }, logout: async () => undefined });
  await signedOut.bootstrap({ error: "cancelled", referenceId: null });
  assert.deepEqual(signedOut.state, { status: "signedOut", providers: [{ id: "google", displayName: "Google" }],
    message: "Sign-in was cancelled.", logoutUnconfirmed: false, loginAllowed: true, referenceId: null });

  const failure = new SessionController({ providers: async () => { throw new ApiError("GET", "/providers", 503,
    { referenceId: "safe-ref" }); }, current: async () => fullSession, logout: async () => undefined });
  await failure.bootstrap();
  assert.deepEqual(failure.state, { status: "failure", providers: [],
    message: "Running Drafts could not check your session.", referenceId: "safe-ref" });

  const malformedClient = new SessionClient("https://api.example.test", async () => new Response('{"providers":[{"id":3}]}'));
  const malformed = new SessionController(malformedClient);
  await malformed.bootstrap();
  assert.equal(malformed.state.status, "failure");
});

test("logout hides the Account immediately and reports an unconfirmed server logout", async () => {
  let rejectLogout!: (error: Error) => void;
  const logout = new Promise<void>((_resolve, reject) => { rejectLogout = reject; });
  const controller = new SessionController({ providers: async () => [], current: async () => fullSession,
    logout: () => logout });
  await controller.bootstrap();
  const pending = controller.logout();
  assert.equal(controller.state.status, "signedOut");
  if (controller.state.status === "signedOut") assert.equal(controller.state.loginAllowed, false);
  rejectLogout(new Error("offline"));
  await pending;
  assert.equal(controller.state.status, "signedOut");
  if (controller.state.status === "signedOut") {
    assert.equal(controller.state.logoutUnconfirmed, true);
    assert.equal(controller.state.loginAllowed, false);
  }
});

test("login URL keeps only a local path and query", () => {
  const client = new SessionClient("https://api.example.test", async () => new Response());
  assert.equal(client.loginUrl("google", "/projects/one?tab=audio"),
    "https://api.example.test/auth/login/google?return_to=%2Fprojects%2Fone%3Ftab%3Daudio");
  assert.equal(safeReturnPath("//attacker.example/path"), "/");
  assert.equal(safeReturnPath("https://attacker.example/path"), "/");
  assert.deepEqual(authenticationCallbackFromSearch("?authError=provider_unavailable"),
    { error: "provider_unavailable", referenceId: null });
  assert.deepEqual(authenticationCallbackFromSearch("?authError=authentication_failed&referenceId=safe_REF-42"),
    { error: "authentication_failed", referenceId: "safe_REF-42" });
  assert.deepEqual(authenticationCallbackFromSearch("?authError=authentication_failed"),
    { error: "authentication_failed", referenceId: null });
  assert.deepEqual(authenticationCallbackFromSearch("?authError=authentication_failed&referenceId=%3Cscript%3E"),
    { error: "authentication_failed", referenceId: null });
  assert.deepEqual(authenticationCallbackFromSearch("?authError=raw-provider-message&referenceId=safe"),
    { error: "authentication_failed", referenceId: null });
  assert.deepEqual(authenticationCallbackFromSearch("?authError=cancelled&authError=cancelled"),
    { error: "authentication_failed", referenceId: null });
  assert.deepEqual(authenticationCallbackFromSearch("?authError=cancelled&authError=raw-provider-message"),
    { error: "authentication_failed", referenceId: null });
  assert.deepEqual(authenticationCallbackFromSearch("?authError=authentication_failed&referenceId=one&referenceId=two"),
    { error: "authentication_failed", referenceId: null });
  assert.deepEqual(authenticationCallbackFromSearch("?authError=cancelled&referenceId=attacker-selected"),
    { error: "authentication_failed", referenceId: null });
  assert.deepEqual(authenticationCallbackFromSearch("?referenceId=attacker-selected"),
    { error: "authentication_failed", referenceId: null });
  assert.equal(withoutAuthenticationCallback("https://editor.test/login?keep=1&authError=authentication_failed&referenceId=ref#top"),
    "/login?keep=1#top");
  assert.equal(withoutAuthenticationCallback("https://editor.test/login?keep=1&keep=2&authError=cancelled&authError=failed&referenceId=a&referenceId=b"),
    "/login?keep=1&keep=2");
});

test("recording session gate accepts exactly six hours and rejects less or malformed expiry", () => {
  const now = Date.parse("2026-10-20T00:00:00Z");
  assert.equal(sessionAllowsRecording(new Date(now + MINIMUM_RECORDING_SESSION_MS - 1).toISOString(), now), false);
  assert.equal(sessionAllowsRecording(new Date(now + MINIMUM_RECORDING_SESSION_MS).toISOString(), now), true);
  assert.equal(sessionAllowsRecording(new Date(now + MINIMUM_RECORDING_SESSION_MS + 1).toISOString(), now), true);
  assert.equal(sessionAllowsRecording("not-a-date", now), false);
});
