"use client";

// AuthGuard — gate authenticated route segments.
//
// Wraps protected segments (workbench, settings) so unauthenticated
// users are routed to `/login?next=...` instead of seeing transient
// flashes of workspace-scoped UI. The routing decision is owned here
// so individual pages don't repeat the dance.
//
// While the /auth/me round-trip is in flight we render a neutral
// loading shell rather than `null` — keeping the layout stable
// preserves scroll, motion, and SSR-painted chrome instead of
// flashing white between hops.

import { useEffect } from "react";
import { useRouter, usePathname } from "next/navigation";

import { useAuth } from "@/hooks/use-auth";
import { Spinner } from "@/components/ui/spinner";

interface AuthGuardProps {
  children: React.ReactNode;
}

export function AuthGuard({ children }: AuthGuardProps) {
  const router = useRouter();
  const pathname = usePathname();
  const { mode } = useAuth();

  useEffect(() => {
    if (mode.kind === "unauthenticated") {
      // Preserve the intended destination so the OAuth callback can
      // bounce the user back after a successful sign-in.
      const next = encodeURIComponent(pathname);
      router.replace(`/login?next=${next}`);
    }
  }, [mode.kind, pathname, router]);

  if (mode.kind === "loading" || mode.kind === "unauthenticated") {
    return (
      <main
        id="main"
        tabIndex={0}
        className="flex h-dvh items-center justify-center bg-surface-base outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/50 focus-visible:ring-inset"
      >
        <Spinner size="lg" className="text-brand-foreground" />
      </main>
    );
  }
  return <>{children}</>;
}
