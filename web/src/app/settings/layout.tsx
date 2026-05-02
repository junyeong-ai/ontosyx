"use client";

import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { SettingsSidebar } from "@/components/settings/settings-sidebar";
import { WIDE_SETTINGS_PAGES } from "@/lib/constants/settings";
import { cn } from "@/lib/cn";
import { useIsClient } from "@/lib/use-is-client";

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
  const isWide = WIDE_SETTINGS_PAGES.has(pathname);
  const pageTitle = deriveTitle(pathname);

  // Prevent hydration mismatch: client-only state (useAuth, localStorage)
  // causes SSR/client tree divergence in the PAGE BODY. Defer only the
  // inner `children`, not the surrounding document structure — keeping
  // the `<h1>` rendered on the first paint satisfies the
  // `page-has-heading-one` a11y rule and gives screen readers an anchor
  // during the hydration gap.
  const mounted = useIsClient();

  return (
    <div className="flex h-screen bg-zinc-50 dark:bg-zinc-950">
      <SettingsSidebar />
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
        className="flex-1 overflow-y-auto p-6 outline-none focus-visible:ring-2 focus-visible:ring-emerald-500/50 focus-visible:ring-inset lg:p-8"
      >
        <div className={cn("mx-auto", isWide ? "max-w-6xl" : "max-w-3xl")}>
          {/* Visually-hidden page title — subpages render their own
              human-facing heading via `SettingsSection`, which is now an
              `<h2>` to preserve the page hierarchy. */}
          <h1 className="sr-only">
            {pageTitle ? t("pageTitle", { page: pageTitle }) : t("rootTitle")}
          </h1>
          {mounted ? children : null}
        </div>
      </main>
    </div>
  );
}
