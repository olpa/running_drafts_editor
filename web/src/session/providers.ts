/** An authentication provider the editor can offer. */
export interface AuthProvider {
  /** Stable key shared with the backend routes and current-session responses. */
  readonly key: string;
  readonly displayName: string;
}

/**
 * Source-owned catalog. Adding a provider needs a coordinated backend and editor
 * release; the editor never discovers providers at runtime.
 */
export const AUTH_PROVIDERS: readonly AuthProvider[] = [{ key: "google", displayName: "Google" }];

/** Resolve a key from a session response; an unknown key is shown as received. */
export function resolveProvider(key: string): AuthProvider {
  return AUTH_PROVIDERS.find((provider) => provider.key === key) ?? { key, displayName: key };
}
