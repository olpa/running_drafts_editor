import { ApiError, readProblemDetails, safeReferenceId } from "./api.ts";
import { backendBaseUrl } from "./backend.ts";

export interface AuthProvider { id: string; displayName: string; }
declare const accountIdBrand: unique symbol;
export type AccountId = string & { readonly [accountIdBrand]: "AccountId" };
export interface AccountProfile {
  email: string | null; emailVerified: boolean | null; displayName: string | null;
  givenName: string | null; familyName: string | null; pictureUrl: string | null; locale: string | null;
}
export interface CurrentSession {
  account: { id: AccountId }; provider: AuthProvider; profile: AccountProfile; sessionExpiresAt: string;
}
export type AuthenticationError = "cancelled" | "provider_unavailable" | "authentication_failed";
export interface AuthenticationCallback {
  error: AuthenticationError | null;
  referenceId: string | null;
}
export type SessionState =
  | { status: "loading"; providers: readonly AuthProvider[] }
  | { status: "signedOut"; providers: readonly AuthProvider[]; message: string;
      logoutUnconfirmed: boolean; loginAllowed: boolean; referenceId: string | null }
  | { status: "signedIn"; providers: readonly AuthProvider[]; session: CurrentSession }
  | { status: "failure"; providers: readonly AuthProvider[]; message: string; referenceId: string | null };
type SessionListener = (state: Readonly<SessionState>) => void;

export class SessionClient {
  constructor(private readonly baseUrl = backendBaseUrl(),
    private readonly request: typeof fetch = fetch.bind(window)) {}

  async providers(): Promise<readonly AuthProvider[]> {
    return parseProviders(await (await this.send("GET", "/api/v1/auth/providers")).json());
  }
  async current(): Promise<CurrentSession> {
    return parseCurrentSession(await (await this.send("GET", "/api/v1/me")).json());
  }
  async logout(): Promise<void> { await this.send("POST", "/auth/logout"); }
  loginUrl(providerId: string, returnTo: string): string {
    return `${this.baseUrl.replace(/\/$/, "")}/auth/login/${encodeURIComponent(providerId)}?return_to=${encodeURIComponent(safeReturnPath(returnTo))}`;
  }
  private async send(method: string, path: string): Promise<Response> {
    const response = await this.request(`${this.baseUrl.replace(/\/$/, "")}${path}`, { method,
      credentials: "include", headers: { Accept: "application/json" }, signal: AbortSignal.timeout(60_000) });
    if (response.ok) return response;
    throw new ApiError(method, path, response.status, await readProblemDetails(response));
  }
}

export class SessionController {
  private listeners = new Set<SessionListener>();
  private requestId = 0;
  private providers: readonly AuthProvider[] = [];
  private currentState: SessionState = { status: "loading", providers: [] };
  constructor(private readonly client: Pick<SessionClient, "providers" | "current" | "logout">) {}
  get state(): Readonly<SessionState> { return this.currentState; }
  subscribe(listener: SessionListener): () => void {
    this.listeners.add(listener); listener(this.currentState); return () => this.listeners.delete(listener);
  }
  async bootstrap(callback: AuthenticationCallback = { error: null, referenceId: null }): Promise<void> {
    const requestId = ++this.requestId;
    this.set({ status: "loading", providers: this.providers });
    try {
      const providers = await this.client.providers();
      if (requestId !== this.requestId) return;
      this.providers = providers;
      try {
        const session = await this.client.current();
        if (requestId === this.requestId) this.set({ status: "signedIn", providers, session });
      } catch (error) {
        if (requestId !== this.requestId) return;
        if (isAuthenticationRequired(error)) {
          this.set({ status: "signedOut", providers, message: authenticationMessage(callback.error),
            logoutUnconfirmed: false, loginAllowed: true, referenceId: callback.referenceId });
        } else this.fail(error);
      }
    } catch (error) { if (requestId === this.requestId) this.fail(error); }
  }
  authenticationExpired(): void {
    this.requireLogin("Your session expired. Sign in again to continue.");
  }
  requireLogin(message = "Sign in to continue."): void {
    ++this.requestId;
    this.set({ status: "signedOut", providers: this.providers, message, logoutUnconfirmed: false,
      loginAllowed: true, referenceId: null });
  }
  async logout(successMessage = "Signed out."): Promise<void> {
    const requestId = ++this.requestId;
    this.set({ status: "signedOut", providers: this.providers, message: "Signing out…",
      logoutUnconfirmed: false, loginAllowed: false, referenceId: null });
    try {
      await this.client.logout();
      if (requestId !== this.requestId) return;
      this.set({ status: "signedOut", providers: this.providers, message: successMessage,
        logoutUnconfirmed: false, loginAllowed: true, referenceId: null });
    } catch {
      if (requestId !== this.requestId) return;
      this.set({ status: "signedOut", providers: this.providers,
        message: "Private data was hidden, but server logout was not confirmed.",
        logoutUnconfirmed: true, loginAllowed: false, referenceId: null });
    }
  }
  private fail(error: unknown): void {
    const referenceId = error instanceof ApiError ? error.problem?.referenceId ?? null : null;
    this.set({ status: "failure", providers: this.providers,
      message: "Running Drafts could not check your session.", referenceId });
  }
  private set(state: SessionState): void {
    this.currentState = state; for (const listener of this.listeners) listener(state);
  }
}

export const MINIMUM_RECORDING_SESSION_MS = 6 * 60 * 60 * 1000;
export function sessionAllowsRecording(sessionExpiresAt: string, nowMs = Date.now()): boolean {
  const expiry = Date.parse(sessionExpiresAt);
  return Number.isFinite(expiry) && expiry - nowMs >= MINIMUM_RECORDING_SESSION_MS;
}
export function safeReturnPath(value: string): string {
  if (!value.startsWith("/") || value.startsWith("//") || value.includes("#") || /[\r\n]/u.test(value)) return "/";
  try {
    const parsed = new URL(value, "https://editor.invalid");
    return parsed.origin === "https://editor.invalid" ? `${parsed.pathname}${parsed.search}` : "/";
  } catch { return "/"; }
}
export function authenticationCallbackFromSearch(search: string): AuthenticationCallback {
  const parameters = new URLSearchParams(search);
  const errors = parameters.getAll("authError");
  const references = parameters.getAll("referenceId");
  if (errors.length === 0 && references.length === 0) return { error: null, referenceId: null };
  if (errors.length !== 1 || !isAuthenticationError(errors[0])) {
    return { error: "authentication_failed", referenceId: null };
  }
  const error = errors[0];
  if (error !== "authentication_failed") {
    return references.length === 0 ? { error, referenceId: null }
      : { error: "authentication_failed", referenceId: null };
  }
  if (references.length > 1) return { error: "authentication_failed", referenceId: null };
  const referenceId = references.length === 1 ? safeReferenceId(references[0]) : null;
  return { error, referenceId };
}
export function withoutAuthenticationCallback(href: string): string {
  const url = new URL(href);
  url.searchParams.delete("authError");
  url.searchParams.delete("referenceId");
  return `${url.pathname}${url.search}${url.hash}`;
}
function authenticationMessage(error: AuthenticationError | null): string {
  if (error === "cancelled") return "Sign-in was cancelled.";
  if (error === "provider_unavailable") return "The sign-in provider is temporarily unavailable. Please try again.";
  if (error === "authentication_failed") return "Sign-in could not be verified. Please try again.";
  return "Sign in to continue.";
}
function isAuthenticationError(value: string | undefined): value is AuthenticationError {
  return value === "cancelled" || value === "provider_unavailable" || value === "authentication_failed";
}
function isAuthenticationRequired(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401 && error.problem?.code === "authentication_required";
}
function parseProviders(value: unknown): readonly AuthProvider[] {
  const root = record(value, "provider response");
  if (!Array.isArray(root.providers)) throw new Error("Malformed provider response");
  return root.providers.map(parseProvider);
}
function parseProvider(value: unknown): AuthProvider {
  const item = record(value, "provider");
  return { id: requiredString(item.id, "provider id"), displayName: requiredString(item.displayName, "provider display name") };
}
function parseCurrentSession(value: unknown): CurrentSession {
  const root = record(value, "current session"); const account = record(root.account, "account");
  const profile = record(root.profile, "profile"); const sessionExpiresAt = requiredString(root.sessionExpiresAt, "session expiry");
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/u.test(sessionExpiresAt)
    || !Number.isFinite(Date.parse(sessionExpiresAt))) throw new Error("Malformed session expiry");
  return { account: { id: parseAccountId(account.id) }, provider: parseProvider(root.provider),
    profile: { email: nullableString(profile.email, "email"),
      emailVerified: nullableBoolean(profile.emailVerified, "email verification"),
      displayName: nullableString(profile.displayName, "display name"),
      givenName: nullableString(profile.givenName, "given name"), familyName: nullableString(profile.familyName, "family name"),
      pictureUrl: nullableString(profile.pictureUrl, "picture URL"), locale: nullableString(profile.locale, "locale") },
    sessionExpiresAt };
}
function parseAccountId(value: unknown): AccountId {
  const id = requiredString(value, "account id");
  if (!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/iu.test(id)) {
    throw new Error("Malformed account id");
  }
  return id as AccountId;
}
function record(value: unknown, name: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error(`Malformed ${name}`);
  return value as Record<string, unknown>;
}
function requiredString(value: unknown, name: string): string {
  if (typeof value !== "string" || value.length === 0) throw new Error(`Malformed ${name}`); return value;
}
function nullableString(value: unknown, name: string): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value !== "string") throw new Error(`Malformed ${name}`); return value;
}
function nullableBoolean(value: unknown, name: string): boolean | null {
  if (value === null || value === undefined) return null;
  if (typeof value !== "boolean") throw new Error(`Malformed ${name}`); return value;
}
