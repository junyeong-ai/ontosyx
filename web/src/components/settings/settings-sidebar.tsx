"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import { useAuth } from "@/hooks/use-auth";

interface NavItem {
  labelKey: string;
  href: string;
  authOnly?: boolean;
  adminOnly?: boolean;
}

interface NavGroup {
  titleKey: string;
  items: NavItem[];
}

const NAV_GROUPS: NavGroup[] = [
  {
    titleKey: "organization",
    items: [
      { labelKey: "workspace", href: "/settings/workspace", adminOnly: true },
      { labelKey: "team", href: "/settings/team", authOnly: true },
      { labelKey: "profile", href: "/settings/profile" },
    ],
  },
  {
    titleKey: "systemCategory",
    items: [
      { labelKey: "system", href: "/settings/system", adminOnly: true },
      { labelKey: "providers", href: "/settings/providers" },
      { labelKey: "models", href: "/settings/models", adminOnly: true },
      { labelKey: "usage", href: "/settings/usage", adminOnly: true },
      { labelKey: "notifications", href: "/settings/notifications", adminOnly: true },
    ],
  },
  {
    titleKey: "data",
    items: [
      { labelKey: "reports", href: "/settings/reports" },
      { labelKey: "schedules", href: "/settings/schedules", adminOnly: true },
      { labelKey: "knowledge", href: "/settings/knowledge", adminOnly: true },
      { labelKey: "federation", href: "/settings/federation", adminOnly: true },
    ],
  },
  {
    titleKey: "governance",
    items: [
      { labelKey: "qualityRules", href: "/settings/quality", adminOnly: true },
      {
        labelKey: "qualitySignals",
        href: "/settings/quality/signals",
        adminOnly: true,
      },
      {
        labelKey: "staleConcepts",
        href: "/settings/quality/stale",
        adminOnly: true,
      },
      { labelKey: "ambiguity", href: "/settings/ambiguity", adminOnly: true },
      { labelKey: "accessControl", href: "/settings/acl", adminOnly: true },
      { labelKey: "dataLineage", href: "/settings/lineage" },
      { labelKey: "auditLog", href: "/settings/audit", adminOnly: true },
      {
        labelKey: "provenanceAudit",
        href: "/settings/governance/audit",
        adminOnly: true,
      },
      {
        labelKey: "routingMatrix",
        href: "/settings/governance/routing",
        adminOnly: true,
      },
      { labelKey: "approvals", href: "/settings/approvals", adminOnly: true },
    ],
  },
  {
    titleKey: "development",
    items: [
      { labelKey: "prompts", href: "/settings/prompts", adminOnly: true },
      { labelKey: "sessions", href: "/settings/sessions" },
    ],
  },
];

export function SettingsSidebar() {
  const t = useTranslations("settings.chrome");
  const tSidebar = useTranslations("settings.chrome.sidebar");
  const pathname = usePathname();
  const { authEnabled, isAdmin } = useAuth();

  const isItemVisible = (item: NavItem) =>
    (!item.authOnly || authEnabled) && (!item.adminOnly || isAdmin);

  const visibleGroups = NAV_GROUPS.map((group) => ({
    ...group,
    items: group.items.filter(isItemVisible),
  })).filter((group) => group.items.length > 0);

  return (
    <aside className="flex w-52 shrink-0 flex-col border-r border-divider bg-surface-base">
      {/* Back link */}
      <div className="border-b border-divider px-4 py-3">
        <Link
          href="/"
          className="flex items-center gap-1.5 text-xs font-medium text-foreground-muted transition-colors hover:text-foreground-strong"
        >
          <svg
            className="h-3.5 w-3.5"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M15 19l-7-7 7-7"
            />
          </svg>
          {t("backToWorkbench")}
        </Link>
      </div>

      {/* Grouped navigation */}
      <nav
        aria-label={t("navAriaLabel")}
        className="flex flex-col overflow-y-auto px-2 pb-4 pt-2"
      >
        {visibleGroups.map((group) => (
          <div key={group.titleKey} className="flex flex-col gap-0.5">
            <span className="mt-4 mb-1 px-3 text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {tSidebar(group.titleKey)}
            </span>
            {group.items.map((item) => {
              const isActive = pathname === item.href;
              return (
                <Link
                  key={item.href}
                  href={item.href}
                  aria-current={isActive ? "page" : undefined}
                  className={cn(
                    "relative block rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
                    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground focus-visible:ring-offset-1",
                    isActive
                      ? "bg-brand-surface text-brand-foreground before:absolute before:left-0 before:top-1.5 before:bottom-1.5 before:w-0.5 before:rounded-full before:bg-brand-solid"
                      : "text-foreground-muted hover:bg-surface-inset hover:text-foreground-strong",
                  )}
                >
                  {tSidebar(item.labelKey)}
                </Link>
              );
            })}
          </div>
        ))}
      </nav>
    </aside>
  );
}
