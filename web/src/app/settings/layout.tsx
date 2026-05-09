"use client";

import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { SettingsSidebar } from "@/components/settings/settings-sidebar";
import { SettingsCommandSource } from "@/components/settings/settings-command-source";
import { isNarrowSettingsPage } from "@/lib/constants/settings";
import { cn } from "@/lib/cn";
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
  const tSkip = useTranslations("chrome.skipLinks");
  // Wide-by-default; only pure-form pages opt into narrow. Match is
  // prefix-aware so a deep-link inside an opted-in subtree stays
  // width-consistent with its parent.
  const isNarrow = isNarrowSettingsPage(pathname);
  const pageTitle = deriveTitle(pathname);

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
      <SettingsSidebar />
      <SessionExpiredOverlay />
      <main
        id="main"
        // `tabIndex={0}` makes the scroll container reachable for
        // keyboard users on read-only pages that have no focusable
        // children of their own (e.g. settings/providers — `region`
        // landmark must be focusable per WCAG 2.1 SC 2.1.1). The
        // `focus-visible:` ring keeps the indicator visible only for
        // keyboard activation, not mouse clicks — so casual scrolling
        // doesn't paint a ring.
        tabIndex={0}
        className="flex-1 overflow-y-auto p-6 outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/50 focus-visible:ring-inset lg:p-8"
      >
        <div className={cn("mx-auto", isNarrow ? "max-w-3xl" : "max-w-7xl")}>
          {/* Visually-hidden page title — subpages render their own
              human-facing heading via `SettingsSection`, which is now an
              `<h2>` to preserve the page hierarchy. */}
          <h1 className="sr-only">
            {pageTitle ? t("pageTitle", { page: pageTitle }) : t("rootTitle")}
          </h1>
          {mounted ? (
            <PageTransition motionKey={pathname}>{children}</PageTransition>
          ) : null}
        </div>
      </main>
    </div>
    <SettingsCommandSource />
    </AuthGuard>
  );
}
