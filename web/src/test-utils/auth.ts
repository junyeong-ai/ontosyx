// Test fixture for `useAuth` mocks.
//
// Test files reach for `vi.mocked(useAuth).mockReturnValue(...)` to
// stub the auth subsystem. Constructing the full return shape by
// hand at every call-site invited a brittle pattern — the
// mismatched-shape errors that the discriminated `mode` field
// exposed got papered over with `as ReturnType<typeof useAuth>`
// casts that bypassed the type checker entirely.
//
// `mockAuth` builds the full shape from a single role keyword and an
// optional user override. The cast is gone, refactors of `useAuth`
// re-flow into tests automatically, and the failure mode for
// inconsistent inputs (e.g. "anonymous" + a user) is a compile error
// rather than a runtime surprise.
//
// `useAuth` returns `{ mode, user, loading, isAuthenticated,
// authEnabled, isAdmin, canWrite }` — every field is derived from
// the discriminated `AuthMode` union, so we follow the same
// derivation here.

import type { AuthUser } from "@/hooks/use-auth";

type AuthRole = "admin" | "designer" | "viewer";

type AuthVariant =
  /** /auth/me round-trip still pending. */
  | "loading"
  /** Auth enabled, no signed-in user — redirected to /login surface. */
  | "anonymous"
  /** Auth disabled (single-tenant / dev / on-prem stub principal). */
  | "disabled"
  /** Auth enabled + signed-in with the named role. */
  | { kind: "authenticated"; role: AuthRole };

interface AuthShape {
  mode:
    | { kind: "loading" }
    | { kind: "disabled"; user: AuthUser }
    | { kind: "authenticated"; user: AuthUser }
    | { kind: "unauthenticated" };
  user: AuthUser | null;
  loading: boolean;
  isAuthenticated: boolean;
  authEnabled: boolean;
  isAdmin: boolean;
  canWrite: boolean;
}

const DEFAULT_USER: Omit<AuthUser, "auth_enabled"> = {
  sub: "u1",
  email: "tester@example.com",
  name: "Tester",
  role: "viewer",
};

export function mockAuth(
  variant: AuthVariant,
  overrides: Partial<AuthUser> = {},
): AuthShape {
  if (variant === "loading") {
    return {
      mode: { kind: "loading" },
      user: null,
      loading: true,
      isAuthenticated: false,
      authEnabled: false,
      isAdmin: false,
      canWrite: false,
    };
  }
  if (variant === "anonymous") {
    return {
      mode: { kind: "unauthenticated" },
      user: null,
      loading: false,
      isAuthenticated: false,
      authEnabled: true,
      isAdmin: false,
      canWrite: false,
    };
  }
  if (variant === "disabled") {
    const user: AuthUser = {
      ...DEFAULT_USER,
      role: "admin",
      ...overrides,
      auth_enabled: false,
    };
    return {
      mode: { kind: "disabled", user },
      user,
      loading: false,
      isAuthenticated: true,
      authEnabled: false,
      isAdmin: user.role === "admin",
      canWrite: user.role === "admin" || user.role === "designer",
    };
  }
  // Authenticated.
  const user: AuthUser = {
    ...DEFAULT_USER,
    role: variant.role,
    ...overrides,
    auth_enabled: true,
  };
  return {
    mode: { kind: "authenticated", user },
    user,
    loading: false,
    isAuthenticated: true,
    authEnabled: true,
    isAdmin: user.role === "admin",
    canWrite: user.role === "admin" || user.role === "designer",
  };
}
