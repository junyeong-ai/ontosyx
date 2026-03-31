"use client";

import { useEffect, useState } from "react";
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

export function useAuth() {
  const [user, setUser] = useState<AuthUser | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetch("/auth/me")
      .then((r) => (r.ok ? r.json() : null))
      .then((data: AuthUser | null) => {
        setUser(data);
        // Cache auth-enabled state so getPrincipalId() knows whether
        // to send x-principal-id or let the server-side proxy handle it
        setAuthEnabled(data?.auth_enabled ?? false);
      })
      .catch(() => setUser(null))
      .finally(() => setLoading(false));
  }, []);

  return {
    user,
    loading,
    isAuthenticated: !!user,
    authEnabled: user?.auth_enabled ?? false,
    isAdmin: user?.role === "admin",
    canWrite: user?.role === "admin" || user?.role === "designer",
  };
}
