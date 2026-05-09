"use client";

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
// Unified Header — [WorkspaceSwitcher] [ContextSelector] [ContextBadge] | [Spacer] | [Presence] [Status] [ModeActions] [UserMenu]
// ---------------------------------------------------------------------------
//
// Brand identity (mark + wordmark) lives in the sidebar's top cell —
// the Linear / Slack / Discord pattern. The header opens straight on
// workspace + page context, so the operator's eye lands on "what
// am I working on" rather than re-reading the brand they already
// recognise from the sidebar tile.
// ---------------------------------------------------------------------------

export function Header() {
  const activeOntologyDraft = useAppStore(selectStateActiveOntologyDraft);
  const { user } = useAuth();
  return (
    <header className="relative z-chrome flex h-11 shrink-0 items-center justify-between border-b border-divider bg-surface-base px-3">
      {/* Left: Context */}
      <div className="flex min-w-0 items-center gap-3">
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
