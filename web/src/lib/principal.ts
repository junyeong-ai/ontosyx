"use client";

/**
 * Client-side principal ID for data-scoping.
 *
 * When auth is enabled (JWT cookie-based), the principal ID is extracted
 * server-side in the API proxy — the client does not need to send it.
 * This function returns undefined in that case.
 *
 * When auth is disabled (dev mode), falls back to the localStorage UUID
 * for backward compatibility.
 */
const PRINCIPAL_STORAGE_KEY = "ontosyx.principal_id";

// Cached auth-enabled flag to avoid repeated cookie checks.
// Set by checkAuthEnabled() on first call.
let authEnabledCache: boolean | null = null;

/**
 * Check if auth is enabled by looking for the session cookie.
 * When the ontosyx_session cookie exists, auth is handled server-side
 * and we don't need to send x-principal-id from the client.
 */
function isAuthEnabledClient(): boolean {
  if (authEnabledCache !== null) return authEnabledCache;
  if (typeof document === "undefined") return false;

  // If the session cookie exists (even though we can't read its value
  // because it's HTTP-only), the cookie name still appears in document.cookie
  // only for non-httpOnly cookies. So instead, we rely on the /auth/me
  // endpoint result cached in sessionStorage.
  const cached = window.sessionStorage.getItem("ontosyx.auth_enabled");
  if (cached !== null) {
    authEnabledCache = cached === "true";
    return authEnabledCache;
  }

  // Not yet determined — assume dev mode (will be updated by useAuth hook)
  return false;
}

/** Called by useAuth hook to cache the auth-enabled state. */
export function setAuthEnabled(enabled: boolean): void {
  authEnabledCache = enabled;
  if (typeof window !== "undefined") {
    window.sessionStorage.setItem("ontosyx.auth_enabled", String(enabled));
  }
}

export function getPrincipalId(): string | undefined {
  if (typeof window === "undefined") {
    return undefined;
  }

  // When auth is enabled, principal is injected server-side from JWT
  if (isAuthEnabledClient()) {
    return undefined;
  }

  // Dev mode fallback: localStorage UUID
  const existing = window.localStorage.getItem(PRINCIPAL_STORAGE_KEY);
  if (existing) {
    return existing;
  }

  const generated = window.crypto.randomUUID();
  window.localStorage.setItem(PRINCIPAL_STORAGE_KEY, generated);
  return generated;
}
