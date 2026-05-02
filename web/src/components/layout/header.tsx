"use client";

import { useTranslations } from "next-intl";
import { ContextSelector } from "@/components/layout/context-selector";
import { ContextBadge } from "@/components/layout/context-badge";
import { WorkspaceSwitcher } from "@/components/layout/workspace-switcher";
import { ModeActions } from "@/components/layout/mode-actions";
import { UserMenu } from "@/components/layout/user-menu";
import { PresenceAvatars } from "@/components/collab/presence-avatars";
import { useAppStore } from "@/lib/store";
import { selectStateActiveProject } from "@/lib/store/selectors";

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
  const t = useTranslations("chrome.header");
  return (
    <h1 className="m-0 text-sm font-semibold tracking-tight text-foreground-strong">
      {t("appTitle")}
    </h1>
  );
}

export function Header() {
  const activeProject = useAppStore(selectStateActiveProject);
  return (
    <header className="relative z-20 flex h-11 shrink-0 items-center justify-between border-b border-divider bg-surface-base px-3">
      {/* Left: Logo + Context */}
      <div className="flex min-w-0 items-center gap-3">
        <span className="shrink-0"><AppBranding /></span>
        <div className="mx-1 h-5 w-px bg-surface-inset" />
        <WorkspaceSwitcher />
        <div className="mx-1 h-5 w-px bg-surface-inset" />
        <ContextSelector />
        <ContextBadge />
      </div>

      {/* Right: Presence + Actions + User */}
      <div className="flex shrink-0 items-center gap-2 pl-3">
        {activeProject?.id && (
          <PresenceAvatars projectId={activeProject.id} className="mr-1" />
        )}
        <ModeActions />
        <div className="mx-1 h-4 w-px bg-surface-inset" />
        <UserMenu />
      </div>
    </header>
  );
}
