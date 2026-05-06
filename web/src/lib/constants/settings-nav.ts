import type { LucideIcon } from "lucide-react";
import { Building, Cpu, Database, FlaskConical, ShieldCheck } from "lucide-react";
export interface NavItem {
  labelKey: string;
  href: string;
  authOnly?: boolean;
  adminOnly?: boolean;
}

export interface NavGroup {
  titleKey: string;
  icon: LucideIcon;
  /**
   * When set, the group label itself is a navigation link (Linear-style
   * single-page group). The `items` array stays for future growth, and
   * sidebar / breadcrumb / palette resolve `href` paths through it
   * regardless of whether the group is collapsible.
   */
  href?: string;
  items: NavItem[];
}

/**
 * Single source of truth for the settings IA — sidebar, breadcrumb,
 * and command palette all read from this list. Adding a new group or
 * item lands in every surface automatically; URLs, labels, and icons
 * stay in lockstep.
 */
export const SETTINGS_NAV_GROUPS: readonly NavGroup[] = [
  {
    titleKey: "workspace",
    icon: Building,
    items: [
      { labelKey: "general", href: "/settings/workspace/general", adminOnly: true },
      { labelKey: "members", href: "/settings/workspace/members", authOnly: true },
      { labelKey: "usage", href: "/settings/workspace/usage", adminOnly: true },
      { labelKey: "reports", href: "/settings/workspace/reports" },
      { labelKey: "schedules", href: "/settings/workspace/schedules", adminOnly: true },
    ],
  },
  {
    titleKey: "data",
    icon: Database,
    items: [
      { labelKey: "knowledgeBase", href: "/settings/knowledge/base", adminOnly: true },
      { labelKey: "federation", href: "/settings/knowledge/federation", adminOnly: true },
    ],
  },
  {
    titleKey: "runtime",
    icon: Cpu,
    items: [
      { labelKey: "config", href: "/settings/runtime", adminOnly: true },
      { labelKey: "providers", href: "/settings/runtime/providers" },
      { labelKey: "models", href: "/settings/runtime/models", adminOnly: true },
      { labelKey: "prompts", href: "/settings/runtime/prompts", adminOnly: true },
    ],
  },
  {
    titleKey: "quality",
    icon: ShieldCheck,
    href: "/settings/quality",
    items: [],
  },
  {
    titleKey: "evaluation",
    icon: FlaskConical,
    href: "/settings/evaluation",
    items: [
      { labelKey: "runs", href: "/settings/evaluation", adminOnly: true },
      {
        labelKey: "datasets",
        href: "/settings/evaluation/datasets",
        adminOnly: true,
      },
      { labelKey: "diff", href: "/settings/evaluation/diff", adminOnly: true },
    ],
  },
  {
    titleKey: "governance",
    icon: ShieldCheck,
    items: [
      { labelKey: "accessControl", href: "/settings/governance/acl", adminOnly: true },
      { labelKey: "routingMatrix", href: "/settings/governance/routing", adminOnly: true },
      { labelKey: "approvals", href: "/settings/governance/approvals", adminOnly: true },
      { labelKey: "audit", href: "/settings/governance/audit", adminOnly: true },
    ],
  },
] as const;

/**
 * Reverse lookup — given a pathname, returns the matching group + item.
 * Used by the breadcrumb to render `Settings / <Group> / <Item>` and
 * by the command palette to highlight the current entry. Accepts a
 * nullable pathname so render-time consumers (`usePathname()` returns
 * `null` outside an app-router context, including unit tests) don't
 * need a guard at every call site.
 */
export function findNavMatch(pathname: string | null | undefined): {
  group: NavGroup;
  item: NavItem | null;
} | null {
  if (!pathname) return null;
  for (const group of SETTINGS_NAV_GROUPS) {
    // Direct group match — single-page groups (Linear-style) where the
    // group label itself navigates.
    if (group.href === pathname || (group.href && pathname.startsWith(`${group.href}/`))) {
      return { group, item: null };
    }
    for (const item of group.items) {
      if (pathname === item.href || pathname.startsWith(`${item.href}/`)) {
        return { group, item };
      }
    }
  }
  return null;
}
