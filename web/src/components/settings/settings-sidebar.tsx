"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { cn } from "@/lib/cn";
import { useAuth } from "@/lib/use-auth";

interface NavItem {
  label: string;
  href: string;
  /** Only show when auth is enabled */
  authOnly?: boolean;
  /** Only show for admin users */
  adminOnly?: boolean;
}

const NAV_ITEMS: NavItem[] = [
  { label: "Workspace", href: "/settings/workspace", adminOnly: true },
  { label: "Profile", href: "/settings/profile" },
  { label: "System", href: "/settings/system", adminOnly: true },
  { label: "Providers", href: "/settings/providers" },
  { label: "Team", href: "/settings/team", authOnly: true },
  { label: "Prompts", href: "/settings/prompts", adminOnly: true },
  { label: "Sessions", href: "/settings/sessions" },
  { label: "Recipes", href: "/settings/recipes" },
  { label: "Schedules", href: "/settings/schedules", adminOnly: true },
  { label: "Reports", href: "/settings/reports" },
  { label: "Quality Rules", href: "/settings/quality", adminOnly: true },
  { label: "Models", href: "/settings/models", adminOnly: true },
  { label: "Access Control", href: "/settings/acl", adminOnly: true },
  { label: "Data Lineage", href: "/settings/lineage" },
  { label: "Usage", href: "/settings/usage", adminOnly: true },
  { label: "Approvals", href: "/settings/approvals", adminOnly: true },
  { label: "Audit Log", href: "/settings/audit", adminOnly: true },
];

export function SettingsSidebar() {
  const pathname = usePathname();
  const { authEnabled, isAdmin } = useAuth();

  const visibleItems = NAV_ITEMS.filter(
    (item) =>
      (!item.authOnly || authEnabled) && (!item.adminOnly || isAdmin),
  );

  return (
    <aside className="flex w-52 shrink-0 flex-col border-r border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-950">
      {/* Back link */}
      <div className="border-b border-zinc-200 px-4 py-3 dark:border-zinc-800">
        <Link
          href="/"
          className="flex items-center gap-1.5 text-xs font-medium text-zinc-500 transition-colors hover:text-zinc-800 dark:text-zinc-400 dark:hover:text-zinc-200"
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
          Back to Workbench
        </Link>
      </div>

      {/* Section label */}
      <div className="px-4 pt-4 pb-2">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-zinc-400 dark:text-zinc-500">
          Settings
        </span>
      </div>

      {/* Navigation */}
      <nav className="flex flex-col gap-0.5 px-2">
        {visibleItems.map((item) => {
          const isActive = pathname === item.href;
          return (
            <Link
              key={item.href}
              href={item.href}
              className={cn(
                "rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
                isActive
                  ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-900/20 dark:text-emerald-400"
                  : "text-zinc-600 hover:bg-zinc-100 hover:text-zinc-900 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-zinc-200",
              )}
            >
              {item.label}
            </Link>
          );
        })}
      </nav>
    </aside>
  );
}
