// AuthGuard — gate authenticated route segments.
//
// Wraps protected segments (workbench, settings) so that unauthenticated
// users are routed to `/login` instead of seeing transient flashes of
// workspace-scoped UI (onboarding modal, session-expired overlay,
// 401-fed error toasts). Mounts once per protected layout; the routing
// decision is owned here so individual pages don't repeat the dance.
//
// While the /auth/me round-trip is in flight we render `null` rather
// than the children — this keeps the layout shell stable but avoids
// the FOUC of post-login UI on the public hop.

"use client";

import { useEffect } from "react";
import { useRouter, usePathname } from "next/navigation";

import { useAuth } from "@/hooks/use-auth";

interface AuthGuardProps {
  children: React.ReactNode;
}

export function AuthGuard({ children }: AuthGuardProps) {
  const router = useRouter();
  const pathname = usePathname();
  const { user, loading } = useAuth();

  useEffect(() => {
    if (loading) return;
    if (user) return;
    // Preserve the intended destination so the OAuth callback can
    // bounce the user back after a successful sign-in.
    const next = encodeURIComponent(pathname);
    router.replace(`/login?next=${next}`);
  }, [loading, user, pathname, router]);

  if (loading || !user) return null;
  return <>{children}</>;
}
