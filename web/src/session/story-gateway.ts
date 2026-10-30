import type { AccountId, CurrentSession, SessionGateway } from "./gateway.ts";
import { resolveProvider } from "./providers.ts";
import { ApiError } from "../shared/http.ts";
import type { SessionLocation } from "./session.ts";

export interface StorySessionInput {
  initialState: "signedIn" | "signedOut";
  displayName: string;
  verifiedEmail: string;
  accountId: string;
  providerKey: string;
  sessionExpiresAt: number;
}

export type StoryActivity = (entry: string) => void;

const ACCOUNT_ID_PATTERN = /^[A-Za-z0-9-]+$/u;
const PROVIDER_KEY_PATTERN = /^[A-Za-z0-9_-]+$/u;

export function createStorySessionGateway(
  input: StorySessionInput,
  activity: StoryActivity,
): SessionGateway {
  const currentSession = input.initialState === "signedIn" ? sessionFromInput(input) : null;
  return {
    async current() {
      activity("current sent");
      if (currentSession === null) {
        activity("current authentication required");
        throw new ApiError("GET", "/api/v1/me", 401, { code: "authentication_required" });
      }
      activity("current ok");
      return currentSession;
    },
    async logout() {
      activity("logout sent");
      activity("logout ok");
    },
    loginUrl(providerKey, returnTo) {
      activity(`login URL sent ${providerKey}`);
      const url = `/storybook/login/${encodeURIComponent(providerKey)}?return_to=${encodeURIComponent(returnTo)}`;
      activity("login URL ok");
      return url;
    },
  };
}

export function createStorySessionLocation(activity: StoryActivity): SessionLocation {
  return {
    current() {
      activity("location read");
      return {
        path: "/storybook/profile",
        search: "",
        href: "http://storybook.local/storybook/profile",
      };
    },
    navigate(url) {
      activity(`navigate ${url}`);
    },
    replace() {
      activity("ERR unexpected location replacement");
      throw new Error("The ProfileWidget story did not declare location replacement.");
    },
  };
}

function sessionFromInput(input: StorySessionInput): CurrentSession {
  if (!ACCOUNT_ID_PATTERN.test(input.accountId)) {
    throw new Error("Account ID must contain only ASCII letters, digits, and hyphens.");
  }
  if (!PROVIDER_KEY_PATTERN.test(input.providerKey)) {
    throw new Error(
      "Provider key must contain only ASCII letters, digits, underscores, and hyphens.",
    );
  }
  if (!Number.isFinite(input.sessionExpiresAt)) {
    throw new Error("Session expiry must be a valid date and time.");
  }
  const expiry = new Date(input.sessionExpiresAt);
  if (!Number.isFinite(expiry.getTime())) {
    throw new Error("Session expiry must be a valid date and time.");
  }

  const email = input.verifiedEmail === "" ? null : input.verifiedEmail;
  return {
    account: { id: input.accountId as AccountId },
    provider: resolveProvider(input.providerKey),
    profile: {
      email,
      emailVerified: email === null ? null : true,
      displayName: input.displayName === "" ? null : input.displayName,
      givenName: null,
      familyName: null,
      pictureUrl: null,
      locale: null,
    },
    sessionExpiresAt: expiry.toISOString(),
  };
}
