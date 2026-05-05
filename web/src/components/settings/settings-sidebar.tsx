"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useMemo } from "react";
import { useTranslations } from "next-intl";
import { ArrowLeft } from "lucide-react";
import { cn } from "@/lib/cn";
import { useAuth } from "@/hooks/use-auth";
import { DynamicIcon } from "@/components/ui/dynamic-icon";
import { SETTINGS_NAV_GROUPS, type NavItem } from "@/lib/constants/settings-nav";

export function SettingsSidebar() {
  const t = useTranslations("settings.chrome");
  const tSidebar = useTranslations("settings.chrome.sidebar");
  const pathname = usePathname();
  const { authEnabled, isAdmin } = useAuth();

  const visibleGroups = useMemo(() => {
    const isItemVisible = (item: NavItem) =>
      (!item.authOnly || authEnabled) && (!item.adminOnly || isAdmin);
    return SETTINGS_NAV_GROUPS.map((group) => ({
      ...group,
      items: group.items.filter(isItemVisible),
    })).filter(
      // Keep groups that either have at least one visible sub-item or
      // are a single-page group (group label IS the link, no children).
      (group) => group.items.length > 0 || !!group.href,
    );
  }, [authEnabled, isAdmin]);

  return (
    <aside
      id="sidebar"
      tabIndex={-1}
      className="flex h-full w-60 shrink-0 flex-col border-e border-divider bg-surface-base outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40 focus-visible:ring-inset"
    >
      <div className="shrink-0 border-b border-divider px-4 py-3">
        <Link
          href="/"
          className="flex items-center gap-1.5 text-xs font-medium text-foreground-muted transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:text-foreground-strong"
        >
          <ArrowLeft className="h-3.5 w-3.5" />
          {t("backToWorkbench")}
        </Link>
      </div>

      <nav
        aria-label={t("navAriaLabel")}
        className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-2 pb-4 pt-2"
      >
        {visibleGroups.map((group) => {
          // Single-page group (Linear style): the group label IS the
          // navigation link, no children. Renders flat as a top-level
          // entry — no group→item nesting needed.
          if (group.href && group.items.length === 0) {
            const isActive =
              pathname === group.href || pathname.startsWith(`${group.href}/`);
            return (
              <Link
                key={group.titleKey}
                href={group.href}
                aria-current={isActive ? "page" : undefined}
                className={cn(
                  "relative mt-2 flex items-center gap-2 rounded-md px-3 py-1.5 text-sm font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground focus-visible:ring-offset-1",
                  isActive
                    ? "bg-brand-surface text-brand-foreground before:absolute before:start-0 before:top-1.5 before:bottom-1.5 before:w-0.5 before:rounded-full before:bg-brand-solid"
                    : "text-foreground-muted hover:text-foreground-strong",
                )}
              >
                <DynamicIcon as={group.icon} className="h-3.5 w-3.5 shrink-0"
 
 aria-hidden="true" />
                <span className="flex-1 text-start">
                  {tSidebar(group.titleKey)}
                </span>
              </Link>
            );
          }

          // Multi-item group: the group label is a non-interactive
          // section heading; children are always visible. Industry
          // pattern (Stripe / Linear / Notion) — collapsing 5–7
          // groups adds cognitive overhead without saving real
          // sidebar real estate.
          const headingId = `settings-group-${group.titleKey}`;
          return (
            <div
              key={group.titleKey}
              role="group"
              aria-labelledby={headingId}
              className="flex flex-col gap-0.5"
            >
              <h3
                id={headingId}
                className="mt-3 flex items-center gap-2 px-3 py-1 text-2xs font-semibold uppercase tracking-wider text-foreground-muted"
              >
                <DynamicIcon as={group.icon} className="h-3.5 w-3.5 shrink-0"
 
 aria-hidden="true" />
                <span className="flex-1 text-start">
                  {tSidebar(group.titleKey)}
                </span>
              </h3>
              <ul className="flex flex-col gap-0.5">
                {group.items.map((item) => {
                  const isActive = pathname === item.href;
                  return (
                    <li key={item.href}>
                      <Link
                        href={item.href}
                        aria-current={isActive ? "page" : undefined}
                        className={cn(
                          "relative block rounded-md ps-9 pe-3 py-1.5 text-sm font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground focus-visible:ring-offset-1",
                          isActive
                            ? "bg-brand-surface text-brand-foreground before:absolute before:start-0 before:top-1.5 before:bottom-1.5 before:w-0.5 before:rounded-full before:bg-brand-solid"
                            : "text-foreground-muted hover:bg-surface-inset hover:text-foreground-strong",
                        )}
                      >
                        {tSidebar(item.labelKey)}
                      </Link>
                    </li>
                  );
                })}
              </ul>
            </div>
          );
        })}
      </nav>
    </aside>
  );
}
