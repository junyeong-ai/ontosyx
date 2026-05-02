"use client";

/**
 * `useAuth` — fetches the current authenticated user from `/auth/me`
 * once per session and exposes role-derived helpers.
 *
 * Backed by TanStack Query so N components calling this hook share a
 * single fetch — important because some surfaces (chat tool-call
 * cards) are multi-instance. A naive useEffect-driven implementation
 * would fire one /auth/me request per mounted instance.
 *
 * Side effect: caches the `auth_enabled` flag into the principal
 * module so `getPrincipalId()` knows whether to inject
 * `x-principal-id` header on dev-mode requests.
 */

import { useQuery } from "@tanstack/react-query";
import { setAuthEnabled } from "@/lib/principal";

export interface AuthUser {
  sub: string;
  email: string;
  name: string;
  role: string;
  /** Profile picture URL. Available when fetched from backend /auth/me. */
  picture?: string;
  auth_enabled: boolean;
}

/**
 * Discriminated union over the auth subsystem state. Encoding the
 * three meaningful situations as distinct variants — instead of two
 * unrelated booleans (`isAuthenticated`, `authEnabled`) — forces
 * call-sites to handle each one explicitly and prevents the
 * "auth_enabled = false despite no user" footgun the boolean shape
 * silently hid.
 */
export type AuthMode =
  /** /auth/me round-trip still pending. */
  | { kind: "loading" }
  /** Single-tenant / on-prem / dev: backend has no auth provider configured. */
  | { kind: "disabled"; user: AuthUser }
  /** Multi-tenant: backend has auth, user signed in. */
  | { kind: "authenticated"; user: AuthUser }
  /** Multi-tenant: backend has auth, no session. */
  | { kind: "unauthenticated" };

async function fetchAuthMe(): Promise<AuthUser | null> {
  try {
    const response = await fetch("/auth/me");
    if (!response.ok) return null;
    const data = (await response.json()) as AuthUser;
    setAuthEnabled(data.auth_enabled);
    return data;
  } catch {
    return null;
  }
}

export const authKeys = {
  all: ["auth"] as const,
  me: () => [...authKeys.all, "me"] as const,
};

export function useAuth() {
  const { data: user = null, isLoading: loading } = useQuery({
    queryKey: authKeys.me(),
    queryFn: fetchAuthMe,
    staleTime: Infinity,
  });

  const mode: AuthMode = (() => {
    if (loading) return { kind: "loading" };
    if (!user) return { kind: "unauthenticated" };
    return user.auth_enabled
      ? { kind: "authenticated", user }
      : { kind: "disabled", user };
  })();

  return {
    mode,
    user,
    loading,
    isAuthenticated: mode.kind === "authenticated" || mode.kind === "disabled",
    authEnabled: mode.kind === "authenticated" || mode.kind === "unauthenticated",
    isAdmin: user?.role === "admin",
    canWrite: user?.role === "admin" || user?.role === "designer",
  };
}
