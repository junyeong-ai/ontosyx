"use client";

import { Menu } from "lucide-react";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { SettingsSidebar } from "@/components/settings/settings-sidebar";
import { SettingsCommandSource } from "@/components/settings/settings-command-source";
import { MobileNavRoot } from "@/components/layout/mobile-nav-root";
import { useAppStore } from "@/lib/store";
import { useIsClient } from "@/hooks/use-is-client";
import { useNavigationShortcuts } from "@/hooks/use-navigation-shortcuts";
import { SessionExpiredOverlay } from "@/components/collab/session-expired-overlay";
import { AuthGuard } from "@/components/auth/auth-guard";
import { PageTransition } from "@/components/motion/page-transition";

/**
 * Derive a page title from the settings pathname. Runs at render time —
 * pathnames are stable strings, so the same URL always yields the same
 * title without a hydration-timing dependency. Returns `null` for the
 * settings root so the heading reads "Settings" alone instead of the
 * tautological "Settings — Settings".
 */
function deriveTitle(pathname: string): string | null {
  const slug = pathname.replace(/^\/settings\/?/, "").split("/")[0];
  if (!slug) return null;
  return slug
    .split("-")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

export default function SettingsLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const pathname = usePathname();
  const t = useTranslations("settings.chrome");
  const tSidebar = useTranslations("chrome.sidebar");
  const tSkip = useTranslations("chrome.skipLinks");
  const pageTitle = deriveTitle(pathname);
  const setMobileNavOpen = useAppStore((s) => s.setMobileNavOpen);

  // Prevent hydration mismatch: client-only state (useAuth, localStorage)
  // causes SSR/client tree divergence in the PAGE BODY. Defer only the
  // inner `children`, not the surrounding document structure — keeping
  // the `<h1>` rendered on the first paint satisfies the
  // `page-has-heading-one` a11y rule and gives screen readers an anchor
  // during the hydration gap.
  const mounted = useIsClient();
  useNavigationShortcuts();

  return (
    <AuthGuard>
    {/* Settings-scoped skip link, placed before the sidebar so it lands
        as the first focusable inside this layout — the root layout
        owns only `#main`, surface-specific targets live with the layout
        that mounts the corresponding landmark. */}
    <nav aria-label={tSkip("labelSettings")} className="contents">
      <a
        href="#sidebar"
        className="sr-only focus:not-sr-only focus:absolute focus:start-4 focus:top-4 focus:z-skip-link focus:rounded-md focus:bg-brand-solid focus:px-4 focus:py-2 focus:text-sm focus:font-medium focus:text-foreground-onbrand focus:outline-none focus:ring-2 focus:ring-brand-foreground/40 focus:ring-offset-2"
      >
        {tSkip("toSidebar")}
      </a>
    </nav>
    <div className="flex h-screen bg-surface-raised">
      <MobileNavRoot>
        <SettingsSidebar />
      </MobileNavRoot>
      <SessionExpiredOverlay />
      <div className="flex flex-1 flex-col overflow-hidden">
        <div className="flex h-11 shrink-0 items-center border-b border-divider bg-surface-base px-3 md:hidden">
          <button
            type="button"
            onClick={() => setMobileNavOpen(true)}
            aria-label={tSidebar("openMobileNav")}
            className="-ms-1 inline-flex h-8 w-8 items-center justify-center rounded-md text-foreground-muted transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset hover:text-foreground-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40"
          >
            <Menu className="h-4 w-4" aria-hidden />
          </button>
          <span className="ms-2 text-sm font-medium text-foreground-strong">
            {pageTitle ? t("pageTitle", { page: pageTitle }) : t("rootTitle")}
          </span>
        </div>
        <main
          id="main"
          tabIndex={0}
          className="flex-1 overflow-hidden outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/50 focus-visible:ring-inset"
        >
          <h1 className="sr-only">
            {pageTitle ? t("pageTitle", { page: pageTitle }) : t("rootTitle")}
          </h1>
          {mounted ? (
            <PageTransition motionKey={pathname}>{children}</PageTransition>
          ) : null}
        </main>
      </div>
    </div>
    <SettingsCommandSource />
    </AuthGuard>
  );
}
