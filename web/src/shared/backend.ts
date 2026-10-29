declare global {
  interface Window {
    /** Browser-visible base URL for this environment's backend. */
    __BACKEND_BASE_URL__?: string;
  }
}

export const DEFAULT_BACKEND_BASE_URL = "http://localhost:18081";

export function backendBaseUrl(
  configured = window.__BACKEND_BASE_URL__ ?? DEFAULT_BACKEND_BASE_URL,
): string {
  return configured.replace(/\/$/, "");
}

export function backendUrl(path: `/${string}`, baseUrl = backendBaseUrl()): string {
  return `${baseUrl}${path}`;
}
