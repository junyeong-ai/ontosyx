"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { ArrowRight } from "lucide-react";
import { findNavMatch } from "@/lib/constants/settings-nav";

/**
 * Breadcrumb derived from the active settings pathname using the same
 * `SETTINGS_NAV_GROUPS` source the sidebar reads. Renders
 * `Settings / <Group> / <Item>` so deep pages keep orientation when
 * the sidebar is collapsed or scrolled below the current group. The
 * leaf segment is rendered as plain text since the page heading
 * already names it.
 */
export function SettingsBreadcrumb() {
  const pathname = usePathname();
  const t = useTranslations("settings.chrome");
  const tSidebar = useTranslations("settings.chrome.sidebar");

  const match = findNavMatch(pathname);
  if (!match) return null;

  return (
    <nav
      aria-label={t("breadcrumbAriaLabel")}
      className="mb-1.5 flex items-center gap-1 text-2xs font-medium text-foreground-muted"
    >
      <Link
        href="/settings"
        className="rounded-sm transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40"
      >
        {t("rootTitle")}
      </Link>
      <ArrowRight className="h-3 w-3 shrink-0 text-foreground-subtle"
 
 aria-hidden="true" />
      <span className="text-foreground" aria-current="page">
        {tSidebar(match.group.titleKey)}
      </span>
    </nav>
  );
}
