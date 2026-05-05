"use client";

// ConnectionStatusDot — minimal indicator for the collaboration
// WebSocket lifecycle. Sits in the header next to the user menu;
// hidden in the steady "ready" state so the chrome stays quiet
// when nothing's wrong.

import { useTranslations } from "next-intl";

import { selectConnectionState, useCollabStore } from "@/lib/collab";
import type { ConnectionState } from "@/lib/collab";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/cn";

const VISIBLE_STATES: ReadonlySet<ConnectionState> = new Set([
  "connecting",
  "authenticating",
  "reconnecting",
  "closed",
]);

const COLOR_CLASSES: Record<ConnectionState, string> = {
  idle: "bg-foreground-muted",
  connecting: "bg-warning-foreground animate-pulse",
  authenticating: "bg-warning-foreground animate-pulse",
  ready: "bg-success-foreground",
  reconnecting: "bg-warning-foreground animate-pulse",
  closed: "bg-danger-foreground",
};

export function ConnectionStatusDot({ className }: { className?: string }) {
  const state = useCollabStore(selectConnectionState);
  const t = useTranslations("collaboration.status");

  // Steady-state hides the dot entirely so users only see a marker
  // when something needs their attention.
  if (!VISIBLE_STATES.has(state)) return null;

  return (
    <Tooltip content={t(state)}>
      <div
        className={cn(
          "flex h-5 w-5 items-center justify-center",
          className,
        )}
        role="status"
        aria-label={t(state)}
      >
        <span
          className={cn(
            "block h-2 w-2 rounded-full",
            COLOR_CLASSES[state],
          )}
        />
      </div>
    </Tooltip>
  );
}
