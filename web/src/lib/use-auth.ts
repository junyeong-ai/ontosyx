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

async function fetchAuthMe(): Promise<AuthUser | null> {
  try {
    const response = await fetch("/auth/me");
    if (!response.ok) return null;
    const data = (await response.json()) as AuthUser;
    // Cache auth-enabled state so getPrincipalId() can short-circuit
    // without re-reading the user object.
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
    // Auth identity doesn't change mid-session; refetch only on
    // explicit invalidation (login/logout flows).
    staleTime: Infinity,
  });

  return {
    user,
    loading,
    isAuthenticated: !!user,
    authEnabled: user?.auth_enabled ?? false,
    isAdmin: user?.role === "admin",
    canWrite: user?.role === "admin" || user?.role === "designer",
  };
}
