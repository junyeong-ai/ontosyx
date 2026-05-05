"use client";

import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { AccountSidebar } from "@/components/account/account-sidebar";
import { useIsClient } from "@/hooks/use-is-client";
import { useNavigationShortcuts } from "@/hooks/use-navigation-shortcuts";
import { SessionExpiredOverlay } from "@/components/collab/session-expired-overlay";
import { AuthGuard } from "@/components/auth/auth-guard";
import { PageTransition } from "@/components/motion/page-transition";

/**
 * Derive a page title from the account pathname for the visually-hidden
 * heading. Single segment slug — `/account/profile` → "Profile".
 */
function deriveTitle(pathname: string): string | null {
  const slug = pathname.replace(/^\/account\/?/, "").split("/")[0];
  if (!slug) return null;
  return slug.charAt(0).toUpperCase() + slug.slice(1);
}

/**
 * Account section layout — user-scoped surfaces (profile, notifications,
 * sessions). Lives at `/account/*`, distinct from `/settings/*` which
 * owns workspace-scoped admin. The split mirrors the canonical SaaS
 * IA: my data ≠ the workspace's configuration.
 */
export default function AccountLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const pathname = usePathname();
  const t = useTranslations("account.chrome");
  const tSkip = useTranslations("chrome.skipLinks");
  const pageTitle = deriveTitle(pathname);

  const mounted = useIsClient();
  useNavigationShortcuts();

  return (
    <AuthGuard>
      <a
        href="#sidebar"
        className="sr-only focus:not-sr-only focus:absolute focus:start-4 focus:top-4 focus:z-skip-link focus:rounded-md focus:bg-brand-solid focus:px-4 focus:py-2 focus:text-sm focus:font-medium focus:text-foreground-onbrand focus:outline-none focus:ring-2 focus:ring-brand-foreground/40 focus:ring-offset-2"
      >
        {tSkip("toSidebar")}
      </a>
      <div className="flex h-screen bg-surface-raised">
        <AccountSidebar />
        <SessionExpiredOverlay />
        <main
          id="main"
          tabIndex={0}
          className="flex-1 overflow-y-auto p-6 outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/50 focus-visible:ring-inset lg:p-8"
        >
          <div className="mx-auto max-w-3xl">
            <h1 className="sr-only">
              {pageTitle ? t("pageTitle", { page: pageTitle }) : t("rootTitle")}
            </h1>
            {mounted ? (
              <PageTransition motionKey={pathname}>{children}</PageTransition>
            ) : null}
          </div>
        </main>
      </div>
    </AuthGuard>
  );
}
