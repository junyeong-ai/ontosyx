"use client";

import { ContextSelector } from "@/components/layout/context-selector";
import { ContextBadge } from "@/components/layout/context-badge";
import { WorkspaceSwitcher } from "@/components/layout/workspace-switcher";
import { ModeActions } from "@/components/layout/mode-actions";
import { UserMenu } from "@/components/layout/user-menu";

// ---------------------------------------------------------------------------
// Unified Header — [Branding] | [ContextSelector] [ContextBadge] | [Spacer] | [ModeActions] | [UserMenu]
// ---------------------------------------------------------------------------

/**
 * App branding rendered as the document's `<h1>`. Every client route
 * mounts this component, so using `<h1>` here ensures axe's
 * `page-has-heading-one` rule passes without sprinkling visually-hidden
 * headings into individual pages. Visual weight stays identical thanks
 * to the typography utilities; screen readers announce it as the main
 * landmark heading for the app.
 */
function AppBranding() {
  return (
    <h1 className="m-0 text-sm font-semibold tracking-tight text-zinc-800 dark:text-zinc-200">
      Ontosyx
    </h1>
  );
}

export function Header() {
  return (
    <header className="relative z-20 flex h-11 shrink-0 items-center justify-between border-b border-zinc-200 bg-white px-3 dark:border-zinc-800 dark:bg-zinc-950">
      {/* Left: Logo + Context */}
      <div className="flex min-w-0 items-center gap-3">
        <span className="shrink-0"><AppBranding /></span>
        <div className="mx-1 h-5 w-px bg-zinc-200 dark:bg-zinc-700" />
        <WorkspaceSwitcher />
        <div className="mx-1 h-5 w-px bg-zinc-200 dark:bg-zinc-700" />
        <ContextSelector />
        <ContextBadge />
      </div>

      {/* Right: Actions + User */}
      <div className="flex shrink-0 items-center gap-1 pl-3">
        <ModeActions />
        <div className="mx-1 h-4 w-px bg-zinc-200 dark:bg-zinc-700" />
        <UserMenu />
      </div>
    </header>
  );
}
