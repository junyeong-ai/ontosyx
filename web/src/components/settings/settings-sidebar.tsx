"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { ArrowLeft01Icon, ArrowDown01Icon } from "@hugeicons/core-free-icons";
import { cn } from "@/lib/cn";
import { useAuth } from "@/hooks/use-auth";
import {
  SETTINGS_NAV_GROUPS,
  findNavMatch,
  type NavItem,
} from "@/lib/constants/settings-nav";

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

  // The group whose URL is currently active starts expanded so the user
  // sees the rest of its siblings without an extra click. Other groups
  // stay collapsed by default — this caps visible row count and keeps
  // the sidebar inside a 720px-laptop viewport without scrolling.
  const activeGroup = findNavMatch(pathname)?.group.titleKey ?? null;
  const [expanded, setExpanded] = useState<Set<string>>(
    () => new Set(activeGroup ? [activeGroup] : []),
  );

  // When the user navigates between groups, auto-open the new group's
  // parent so the active item is visible without the user manually
  // expanding it. Already-open groups stay open — collapsing on
  // navigation would feel jumpy.
  useEffect(() => {
    if (!activeGroup) return;
    setExpanded((prev) => {
      if (prev.has(activeGroup)) return prev;
      return new Set(prev).add(activeGroup);
    });
  }, [activeGroup]);

  const toggleGroup = (key: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

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
          <HugeiconsIcon
            icon={ArrowLeft01Icon}
            className="h-3.5 w-3.5"
            size="100%"
          />
          {t("backToWorkbench")}
        </Link>
      </div>

      <nav
        aria-label={t("navAriaLabel")}
        className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-2 pb-4 pt-2"
      >
        {visibleGroups.map((group) => {
          // Single-page groups (Linear-style): the group label itself
          // is the navigation link, no collapsible children. Lets a
          // group with one consolidated page (e.g. Quality hub) read
          // as a flat top-level entry instead of a redundant
          // group→item nesting.
          if (group.href && group.items.length === 0) {
            const isActive =
              pathname === group.href || pathname.startsWith(`${group.href}/`);
            return (
              <Link
                key={group.titleKey}
                href={group.href}
                aria-current={isActive ? "page" : undefined}
                className={cn(
                  "relative mt-2 flex items-center gap-2 rounded-md px-3 py-1.5 text-2xs font-semibold uppercase tracking-wider transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground focus-visible:ring-offset-1",
                  isActive
                    ? "bg-brand-surface text-brand-foreground before:absolute before:start-0 before:top-1.5 before:bottom-1.5 before:w-0.5 before:rounded-full before:bg-brand-solid"
                    : "text-foreground-muted hover:text-foreground-strong",
                )}
              >
                <HugeiconsIcon
                  icon={group.icon}
                  className="h-3.5 w-3.5 shrink-0"
                  size="100%"
                  aria-hidden="true"
                />
                <span className="flex-1 text-start">
                  {tSidebar(group.titleKey)}
                </span>
              </Link>
            );
          }

          const isOpen = expanded.has(group.titleKey);
          const groupId = `settings-group-${group.titleKey}`;
          return (
            <div key={group.titleKey} className="flex flex-col gap-0.5">
              <button
                type="button"
                onClick={() => toggleGroup(group.titleKey)}
                aria-expanded={isOpen}
                aria-controls={groupId}
                className="mt-2 flex items-center gap-2 rounded-md px-3 py-1.5 text-2xs font-semibold uppercase tracking-wider text-foreground-muted transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:text-foreground-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40"
              >
                <HugeiconsIcon
                  icon={group.icon}
                  className="h-3.5 w-3.5 shrink-0"
                  size="100%"
                  aria-hidden="true"
                />
                <span className="flex-1 text-start">
                  {tSidebar(group.titleKey)}
                </span>
                <HugeiconsIcon
                  icon={ArrowDown01Icon}
                  className={cn(
                    "h-3 w-3 shrink-0 transition-transform duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                    !isOpen && "-rotate-90",
                  )}
                  size="100%"
                  aria-hidden="true"
                />
              </button>
              {isOpen && (
                <div id={groupId} className="flex flex-col gap-0.5">
                  {group.items.map((item) => {
                    const isActive = pathname === item.href;
                    return (
                      <Link
                        key={item.href}
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
                    );
                  })}
                </div>
              )}
            </div>
          );
        })}
      </nav>
    </aside>
  );
}
