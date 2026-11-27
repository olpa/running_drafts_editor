import { computed, inject, shallowRef, type InjectionKey, type Ref } from "vue";
import { ApiError } from "../shared/http.ts";
import {
  authenticationCallbackFromSearch,
  authenticationMessage,
  hasAuthenticationCallback,
  withoutAuthenticationCallback,
  type AuthenticationCallback,
  type AuthenticationError,
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
      signInError?: Exclude<AuthenticationError, "cancelled">;
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

const SIGN_IN_RETURN = "running-drafts.sign-in-return";

export function browserLocation(): SessionLocation {
  restoreSignInReturn();
  return {
    current: () => ({
      path: `${window.location.pathname}${window.location.search}`,
      search: window.location.search,
      href: window.location.href,
    }),
    navigate(url) {
      try {
        const { pathname, search, hash } = window.location;
        window.sessionStorage.setItem(SIGN_IN_RETURN, pathname + search + hash);
      } catch {
        // Sign-in remains available when the browser disables storage.
      }
      window.location.assign(url);
    },
    replace: (url) => window.history.replaceState(window.history.state, "", url),
  };
}

/** A backend return omits the fragment. Expected provider failures use /login. */
function restoreSignInReturn(): void {
  try {
    const savedPath = window.sessionStorage.getItem(SIGN_IN_RETURN);
    if (!savedPath?.startsWith("/") || savedPath.startsWith("//")) return;

    const returned = new URL(window.location.href);
    const intended = new URL(savedPath, returned.origin);
    if (intended.origin !== returned.origin) return;

    const hasCallback = hasAuthenticationCallback(returned.search);
    const fallback = returned.pathname === "/login" && hasCallback;
    const cleanReturn = new URL(withoutAuthenticationCallback(returned.href), returned.origin);
    if (
      !fallback &&
      (cleanReturn.pathname !== intended.pathname ||
        cleanReturn.searchParams.toString() !== intended.searchParams.toString())
    )
      return;

    window.sessionStorage.removeItem(SIGN_IN_RETURN);
    for (const name of ["authError", "referenceId"]) {
      for (const value of returned.searchParams.getAll(name))
        intended.searchParams.append(name, value);
    }
    window.history.replaceState(
      window.history.state,
      "",
      intended.pathname + intended.search + intended.hash,
    );
  } catch {
    // An unavailable browser store does not prevent the normal session check.
  }
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
  loginUrl(providerKey: string): string;
  /** Allow a recording to start only with a long enough Application Session. */
  authorizeRecordingStart(): boolean;
}

export interface SessionOptions {
  /** Explicit logout may discard page-local private work. */
  confirmExplicitLogout?: () => boolean;
  /** Runs immediately after explicit logout is confirmed, before the request. */
  onExplicitLogout?: () => void;
}

export function createSession(
  gateway: SessionGateway,
  location: SessionLocation,
  options: SessionOptions = {},
): Session {
  const current = shallowRef<SessionState>({ status: "loading" });
  let requestId = 0;
  const noCallback: AuthenticationCallback = { error: null, referenceId: null };
  let pendingCallback = noCallback;

  function signedOut(
    message: string,
    options: {
      logoutUnconfirmed?: boolean;
      loginAllowed?: boolean;
      referenceId?: string | null;
      signInError?: Exclude<AuthenticationError, "cancelled">;
    } = {},
  ): SessionState {
    return {
      status: "signedOut",
      message,
      logoutUnconfirmed: options.logoutUnconfirmed ?? false,
      loginAllowed: options.loginAllowed ?? true,
      referenceId: options.referenceId ?? null,
      ...(options.signInError && { signInError: options.signInError }),
    };
  }

  function fail(error: unknown): void {
    current.value = {
      status: "failure",
      message: "Running Drafts could not check your session.",
      referenceId: error instanceof ApiError ? (error.problem?.referenceId ?? null) : null,
    };
  }

  async function bootstrap(callback = pendingCallback): Promise<void> {
    pendingCallback = callback;
    const id = ++requestId;
    current.value = { status: "loading" };
    try {
      const session = await gateway.current();
      if (id === requestId) {
        pendingCallback = noCallback;
        current.value = { status: "signedIn", session };
      }
    } catch (error) {
      if (id !== requestId) return;
      if (isAuthenticationRequired(error)) {
        pendingCallback = noCallback;
        const signInError = callback.error === "cancelled" ? null : callback.error;
        if (signInError) {
          console.warn("Running Drafts sign-in failed", {
            code: signInError,
            requestId: callback.referenceId,
          });
        }
        current.value = signedOut(authenticationMessage(callback.error), {
          referenceId: callback.referenceId,
          ...(signInError && { signInError }),
        });
      } else {
        fail(error);
      }
    }
  }

  function requireLogin(message = "Sign in to continue."): void {
    ++requestId;
    pendingCallback = noCallback;
    current.value = signedOut(message);
  }

  async function endSession(successMessage: string, explicit: boolean): Promise<void> {
    if (explicit && options.confirmExplicitLogout?.() === false) return;
    if (explicit) options.onExplicitLogout?.();
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
    logout: (successMessage = "Signed out.") => endSession(successMessage, true),
    login(providerKey) {
      location.navigate(session.loginUrl(providerKey));
    },
    loginUrl: (providerKey) => gateway.loginUrl(providerKey, location.current().path),
    authorizeRecordingStart() {
      const state = current.value;
      if (state.status !== "signedIn") return false;
      if (sessionAllowsRecording(state.session.sessionExpiresAt)) return true;
      requireLogin("Sign in again before recording. Your pending audio remains on this page.");
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
