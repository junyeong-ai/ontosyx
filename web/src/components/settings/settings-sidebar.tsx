"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import { useAuth } from "@/lib/use-auth";

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
      { labelKey: "recipes", href: "/settings/recipes" },
      { labelKey: "reports", href: "/settings/reports" },
      { labelKey: "schedules", href: "/settings/schedules", adminOnly: true },
      { labelKey: "knowledge", href: "/settings/knowledge", adminOnly: true },
    ],
  },
  {
    titleKey: "governance",
    items: [
      { labelKey: "qualityRules", href: "/settings/quality", adminOnly: true },
      { labelKey: "accessControl", href: "/settings/acl", adminOnly: true },
      { labelKey: "dataLineage", href: "/settings/lineage" },
      { labelKey: "auditLog", href: "/settings/audit", adminOnly: true },
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
    <aside className="flex w-52 shrink-0 flex-col border-r border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-950">
      {/* Back link */}
      <div className="border-b border-zinc-200 px-4 py-3 dark:border-zinc-800">
        <Link
          href="/"
          className="flex items-center gap-1.5 text-xs font-medium text-zinc-500 transition-colors hover:text-zinc-800 dark:text-muted-foreground dark:hover:text-zinc-200"
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
      <nav className="flex flex-col overflow-y-auto px-2 pb-4 pt-2">
        {visibleGroups.map((group) => (
          <div key={group.titleKey} className="flex flex-col gap-0.5">
            <span className="mt-4 mb-1 px-3 text-[10px] font-semibold uppercase tracking-wider text-zinc-600 dark:text-muted-foreground">
              {tSidebar(group.titleKey)}
            </span>
            {group.items.map((item) => {
              const isActive = pathname === item.href;
              return (
                <Link
                  key={item.href}
                  href={item.href}
                  className={cn(
                    "block rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
                    isActive
                      ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-900/20 dark:text-emerald-400"
                      : "text-zinc-600 hover:bg-zinc-100 hover:text-zinc-900 dark:text-muted-foreground dark:hover:bg-zinc-800 dark:hover:text-zinc-200",
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
