import { safeReferenceId } from "../shared/http.ts";

export type AuthenticationError = "cancelled" | "provider_unavailable" | "authentication_failed";

export interface AuthenticationCallback {
  error: AuthenticationError | null;
  referenceId: string | null;
}

const NO_CALLBACK: AuthenticationCallback = { error: null, referenceId: null };
const FAILED: AuthenticationCallback = { error: "authentication_failed", referenceId: null };

/** Read the backend's redirect result; anything unexpected becomes a generic failure. */
export function authenticationCallbackFromSearch(search: string): AuthenticationCallback {
  const parameters = new URLSearchParams(search);
  const errors = parameters.getAll("authError");
  const references = parameters.getAll("referenceId");
  if (errors.length === 0 && references.length === 0) return NO_CALLBACK;
  if (errors.length !== 1 || !isAuthenticationError(errors[0])) return FAILED;
  const error = errors[0];
  if (error !== "authentication_failed") {
    return references.length === 0 ? { error, referenceId: null } : FAILED;
  }
  if (references.length > 1) return FAILED;
  const referenceId = references.length === 1 ? safeReferenceId(references[0]) : null;
  return { error, referenceId };
}

export function withoutAuthenticationCallback(href: string): string {
  const url = new URL(href);
  url.searchParams.delete("authError");
  url.searchParams.delete("referenceId");
  return `${url.pathname}${url.search}${url.hash}`;
}

export function hasAuthenticationCallback(search: string): boolean {
  const parameters = new URLSearchParams(search);
  return parameters.has("authError") || parameters.has("referenceId");
}

export function authenticationMessage(error: AuthenticationError | null): string {
  if (error === "cancelled") return "Sign-in was cancelled.";
  if (error === "provider_unavailable") {
    return "The sign-in provider is temporarily unavailable. Please try again.";
  }
  if (error === "authentication_failed") return "Sign-in could not be verified. Please try again.";
  return "Sign in to continue.";
}

function isAuthenticationError(value: string | undefined): value is AuthenticationError {
  return (
    value === "cancelled" || value === "provider_unavailable" || value === "authentication_failed"
  );
}
