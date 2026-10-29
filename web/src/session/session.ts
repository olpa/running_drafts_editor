import { computed, inject, shallowRef, type InjectionKey, type Ref } from "vue";
import { ApiError } from "../shared/http.ts";
import {
  authenticationCallbackFromSearch,
  authenticationMessage,
  hasAuthenticationCallback,
  withoutAuthenticationCallback,
  type AuthenticationCallback,
} from "./callback.ts";
import type { CurrentSession, SessionGateway } from "./gateway.ts";
import { AUTH_PROVIDERS, type AuthProvider } from "./providers.ts";

export type SessionState =
  | { status: "loading" }
  | {
      status: "signedOut";
      message: string;
      logoutUnconfirmed: boolean;
      loginAllowed: boolean;
      referenceId: string | null;
    }
  | { status: "signedIn"; session: CurrentSession }
  | { status: "failure"; message: string; referenceId: string | null };

/** The page address, as far as authentication needs it. */
export interface SessionLocation {
  /** Path and query of the current page. */
  current(): { path: string; search: string; href: string };
  /** Leave the application, for example to start provider sign-in. */
  navigate(url: string): void;
  /** Replace the address in place without reloading. */
  replace(url: string): void;
}

export function browserLocation(): SessionLocation {
  return {
    current: () => ({
      path: `${window.location.pathname}${window.location.search}`,
      search: window.location.search,
      href: window.location.href,
    }),
    navigate: (url) => window.location.assign(url),
    replace: (url) => window.history.replaceState(null, "", url),
  };
}

/** Application Session and sign-in state. */
export interface Session {
  readonly state: Readonly<Ref<SessionState>>;
  readonly providers: readonly AuthProvider[];
  /** Read the authentication callback from the address, clean it, and check the session. */
  start(): Promise<void>;
  /** Re-read the authoritative Account, for example after returning from authentication. */
  bootstrap(callback?: AuthenticationCallback): Promise<void>;
  authenticationExpired(): void;
  requireLogin(message?: string): void;
  logout(successMessage?: string): Promise<void>;
  /** Start provider sign-in and return to the current page afterwards. */
  login(providerKey: string): void;
  /** Allow a recording to start only with a long enough Application Session. */
  authorizeRecordingStart(): boolean;
}

export function createSession(gateway: SessionGateway, location: SessionLocation): Session {
  const current = shallowRef<SessionState>({ status: "loading" });
  let requestId = 0;

  function signedOut(
    message: string,
    options: {
      logoutUnconfirmed?: boolean;
      loginAllowed?: boolean;
      referenceId?: string | null;
    } = {},
  ): SessionState {
    return {
      status: "signedOut",
      message,
      logoutUnconfirmed: options.logoutUnconfirmed ?? false,
      loginAllowed: options.loginAllowed ?? true,
      referenceId: options.referenceId ?? null,
    };
  }

  function fail(error: unknown): void {
    current.value = {
      status: "failure",
      message: "Running Drafts could not check your session.",
      referenceId: error instanceof ApiError ? (error.problem?.referenceId ?? null) : null,
    };
  }

  async function bootstrap(
    callback: AuthenticationCallback = { error: null, referenceId: null },
  ): Promise<void> {
    const id = ++requestId;
    current.value = { status: "loading" };
    try {
      const session = await gateway.current();
      if (id === requestId) current.value = { status: "signedIn", session };
    } catch (error) {
      if (id !== requestId) return;
      if (isAuthenticationRequired(error)) {
        current.value = signedOut(authenticationMessage(callback.error), {
          referenceId: callback.referenceId,
        });
      } else {
        fail(error);
      }
    }
  }

  function requireLogin(message = "Sign in to continue."): void {
    ++requestId;
    current.value = signedOut(message);
  }

  const session: Session = {
    state: computed(() => current.value),
    providers: AUTH_PROVIDERS,
    start() {
      const { search, href } = location.current();
      const callback = authenticationCallbackFromSearch(search);
      if (hasAuthenticationCallback(search)) location.replace(withoutAuthenticationCallback(href));
      return bootstrap(callback);
    },
    bootstrap,
    authenticationExpired: () => requireLogin("Your session expired. Sign in again to continue."),
    requireLogin,
    async logout(successMessage = "Signed out.") {
      const id = ++requestId;
      current.value = signedOut("Signing out…", { loginAllowed: false });
      try {
        await gateway.logout();
        if (id === requestId) current.value = signedOut(successMessage);
      } catch {
        if (id !== requestId) return;
        current.value = signedOut("Private data was hidden, but server logout was not confirmed.", {
          logoutUnconfirmed: true,
          loginAllowed: false,
        });
      }
    },
    login(providerKey) {
      location.navigate(gateway.loginUrl(providerKey, location.current().path));
    },
    authorizeRecordingStart() {
      const state = current.value;
      if (state.status !== "signedIn") return false;
      if (sessionAllowsRecording(state.session.sessionExpiresAt)) return true;
      void session.logout("Sign in again before recording. Your previous session ended safely.");
      return false;
    },
  };
  return session;
}

export const MINIMUM_RECORDING_SESSION_MS = 6 * 60 * 60 * 1000;

export function sessionAllowsRecording(sessionExpiresAt: string, nowMs = Date.now()): boolean {
  const expiry = Date.parse(sessionExpiresAt);
  return Number.isFinite(expiry) && expiry - nowMs >= MINIMUM_RECORDING_SESSION_MS;
}

function isAuthenticationRequired(error: unknown): boolean {
  return (
    error instanceof ApiError &&
    error.status === 401 &&
    error.problem?.code === "authentication_required"
  );
}

export const sessionKey: InjectionKey<Session> = Symbol("session");

export function useSession(): Session {
  const session = inject(sessionKey);
  if (!session) throw new Error("The Session module was not provided.");
  return session;
}
