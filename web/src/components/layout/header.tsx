"use client";

import { useTranslations } from "next-intl";
import { BrandLogo } from "@/components/brand/logo";
import { ContextSelector } from "@/components/layout/context-selector";
import { ContextBadge } from "@/components/layout/context-badge";
import { WorkspaceSwitcher } from "@/components/layout/workspace-switcher";
import { ModeActions } from "@/components/layout/mode-actions";
import { UserMenu } from "@/components/layout/user-menu";
import { PresenceAvatars } from "@/components/collab/presence-avatars";
import { ConnectionStatusDot } from "@/components/collab/connection-status-dot";
import { useAppStore } from "@/lib/store";
import { selectStateActiveOntologyDraft } from "@/lib/store/selectors";
import { useAuth } from "@/hooks/use-auth";

// ---------------------------------------------------------------------------
// Unified Header — [Branding] | [ContextSelector] [ContextBadge] | [Spacer] | [ModeActions] | [UserMenu]
// ---------------------------------------------------------------------------

/**
 * Brand mark — anchor link to the workspace home (`/design`). The
 * lockup (graph-triple mark + "Ontosyx" wordmark) is owned by
 * `BrandLogo` so favicon / apple-icon / OG image / chrome all render
 * the same geometry. Chrome branding is not a content heading, so it
 * is not an `<h1>`; each route owns its own page-level heading.
 */
function AppBranding() {
  const t = useTranslations("chrome.header");
  return <BrandLogo href="/design" ariaLabel={t("appTitle")} size={16} />;
}

export function Header() {
  const activeOntologyDraft = useAppStore(selectStateActiveOntologyDraft);
  const { user } = useAuth();
  return (
    <header className="relative z-chrome flex h-11 shrink-0 items-center justify-between border-b border-divider bg-surface-base px-3">
      {/* Left: Logo + Context */}
      <div className="flex min-w-0 items-center gap-3">
        <span className="shrink-0"><AppBranding /></span>
        <div className="mx-1 h-5 w-px bg-surface-inset" />
        <WorkspaceSwitcher />
        <div className="mx-1 h-5 w-px bg-surface-inset" />
        <ContextSelector />
        <ContextBadge />
      </div>

      {/* Right: Presence + Status + Actions + User */}
      <div className="flex shrink-0 items-center gap-2 ps-3">
        {activeOntologyDraft?.id && (
          <PresenceAvatars
            ontologyDraftId={activeOntologyDraft.id}
            excludeUserId={user?.sub}
            className="me-1"
          />
        )}
        <ConnectionStatusDot />
        <ModeActions />
        <div className="mx-1 h-4 w-px bg-surface-inset" />
        <UserMenu />
      </div>
    </header>
  );
}
