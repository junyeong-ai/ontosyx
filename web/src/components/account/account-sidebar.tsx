"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";

import { cn } from "@/lib/cn";
import { ArrowLeft } from "lucide-react";
interface NavItem {
  labelKey: string;
  href: string;
}

const NAV_ITEMS: NavItem[] = [
  { labelKey: "profile", href: "/account/profile" },
  { labelKey: "notifications", href: "/account/notifications" },
  { labelKey: "sessions", href: "/account/sessions" },
];

/**
 * Account sidebar — user-scoped settings (profile, notifications,
 * sessions). Distinct from `<SettingsSidebar>` which owns
 * workspace-scoped admin surfaces. Splitting the two surfaces lines up
 * with the canonical SaaS pattern: account = me, settings = the
 * workspace I'm operating in.
 */
export function AccountSidebar() {
  const t = useTranslations("account.chrome");
  const tNav = useTranslations("account.chrome.sidebar");
  const pathname = usePathname();

  return (
    <aside
      id="sidebar"
      // `tabIndex={-1}` makes the skip-link target programmatically
      // focusable without adding the landmark itself to the tab cycle.
      tabIndex={-1}
      aria-label={t("sidebarAriaLabel")}
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
        className="flex min-h-0 flex-1 flex-col overflow-y-auto px-2 pb-4 pt-2"
      >
        <span className="mt-4 mb-1 px-3 text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {tNav("section")}
        </span>
        {NAV_ITEMS.map((item) => {
          const isActive = pathname === item.href;
          return (
            <Link
              key={item.href}
              href={item.href}
              aria-current={isActive ? "page" : undefined}
              className={cn(
                "relative block rounded-md px-3 py-1.5 text-sm font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground focus-visible:ring-offset-1",
                isActive
                  ? "bg-brand-surface text-brand-foreground before:absolute before:start-0 before:top-1.5 before:bottom-1.5 before:w-0.5 before:rounded-full before:bg-brand-solid"
                  : "text-foreground-muted hover:bg-surface-inset hover:text-foreground-strong",
              )}
            >
              {tNav(item.labelKey)}
            </Link>
          );
        })}
      </nav>
    </aside>
  );
}
