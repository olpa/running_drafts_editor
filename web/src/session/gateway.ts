import { backendBaseUrl } from "../shared/backend.ts";
import { ApiError, readProblemDetails } from "../shared/http.ts";
import { resolveProvider, type AuthProvider } from "./providers.ts";

declare const accountIdBrand: unique symbol;
export type AccountId = string & { readonly [accountIdBrand]: "AccountId" };

export interface AccountProfile {
  email: string | null;
  emailVerified: boolean | null;
  displayName: string | null;
  givenName: string | null;
  familyName: string | null;
  pictureUrl: string | null;
  locale: string | null;
}

export interface CurrentSession {
  account: { id: AccountId };
  provider: AuthProvider;
  profile: AccountProfile;
  sessionExpiresAt: string;
}

/** Backend operations the session module needs. */
export interface SessionGateway {
  current(): Promise<CurrentSession>;
  logout(): Promise<void>;
  loginUrl(providerKey: string, returnTo: string): string;
}

export function createSessionGateway(
  baseUrl = backendBaseUrl(),
  request: typeof fetch = fetch.bind(window),
): SessionGateway {
  const root = baseUrl.replace(/\/$/, "");

  async function send(method: string, path: string): Promise<Response> {
    const response = await request(`${root}${path}`, {
      method,
      credentials: "include",
      headers: { Accept: "application/json" },
      signal: AbortSignal.timeout(60_000),
    });
    if (response.ok) return response;
    throw new ApiError(method, path, response.status, await readProblemDetails(response));
  }

  return {
    async current() {
      return parseCurrentSession(await (await send("GET", "/api/v1/me")).json());
    },
    async logout() {
      await send("POST", "/auth/logout");
    },
    loginUrl: (providerKey, returnTo) =>
      `${root}/auth/login/${encodeURIComponent(providerKey)}?return_to=${encodeURIComponent(safeReturnPath(returnTo))}`,
  };
}

export function safeReturnPath(value: string): string {
  if (
    !value.startsWith("/") ||
    value.startsWith("//") ||
    value.includes("#") ||
    /[\r\n]/u.test(value)
  ) {
    return "/";
  }
  try {
    const parsed = new URL(value, "https://editor.invalid");
    return parsed.origin === "https://editor.invalid" ? `${parsed.pathname}${parsed.search}` : "/";
  } catch {
    return "/";
  }
}

export function parseCurrentSession(value: unknown): CurrentSession {
  const root = record(value, "current session");
  const account = record(root.account, "account");
  const provider = record(root.provider, "provider");
  const profile = record(root.profile, "profile");
  const sessionExpiresAt = requiredString(root.sessionExpiresAt, "session expiry");
  if (
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/u.test(
      sessionExpiresAt,
    ) ||
    !Number.isFinite(Date.parse(sessionExpiresAt))
  ) {
    throw new Error("Malformed session expiry");
  }
  return {
    account: { id: parseAccountId(account.id) },
    provider: resolveProvider(requiredString(provider.id, "provider key")),
    profile: {
      email: nullableString(profile.email, "email"),
      emailVerified: nullableBoolean(profile.emailVerified, "email verification"),
      displayName: nullableString(profile.displayName, "display name"),
      givenName: nullableString(profile.givenName, "given name"),
      familyName: nullableString(profile.familyName, "family name"),
      pictureUrl: nullableString(profile.pictureUrl, "picture URL"),
      locale: nullableString(profile.locale, "locale"),
    },
    sessionExpiresAt,
  };
}

function parseAccountId(value: unknown): AccountId {
  const id = requiredString(value, "account id");
  if (!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/iu.test(id)) {
    throw new Error("Malformed account id");
  }
  return id as AccountId;
}

function record(value: unknown, name: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`Malformed ${name}`);
  }
  return value as Record<string, unknown>;
}

function requiredString(value: unknown, name: string): string {
  if (typeof value !== "string" || value.length === 0) throw new Error(`Malformed ${name}`);
  return value;
}

function nullableString(value: unknown, name: string): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value !== "string") throw new Error(`Malformed ${name}`);
  return value;
}

function nullableBoolean(value: unknown, name: string): boolean | null {
  if (value === null || value === undefined) return null;
  if (typeof value !== "boolean") throw new Error(`Malformed ${name}`);
  return value;
}
