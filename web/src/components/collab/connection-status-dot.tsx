"use client";

// ConnectionStatusDot — minimal indicator for the collaboration
// WebSocket lifecycle. Sits in the header next to the user menu;
// hidden in the steady "ready" state so the chrome stays quiet
// when nothing's wrong.

import { useTranslations } from "next-intl";

import { selectStateConnectionState, useCollabStore } from "@/lib/collab";
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
  const state = useCollabStore(selectStateConnectionState);
  const t = useTranslations("collaboration.status");

  // Steady-state hides the dot entirely so users only see a marker
  // when something needs their attention.
  if (!VISIBLE_STATES.has(state)) return null;

  // The terminal `closed` state is the most consequential — the
  // collaboration session has stopped trying to reconnect. A bare
  // red dot communicates only via colour, which fails WCAG 1.4.1
  // for users with colour-vision deficiencies. Promote the marker
  // to an inline pill with a label + dot so the meaning is legible
  // without hover. Transient states (`connecting`, `reconnecting`)
  // stay as the smaller pulse-only dot — the indicator is meant to
  // *fade away* once the session steadies.
  const isCritical = state === "closed";

  return (
    <Tooltip content={t(state)}>
      <div
        className={cn(
          "flex items-center gap-1.5",
          isCritical
            ? "rounded-full bg-danger-surface px-2 py-0.5 ring-1 ring-inset ring-danger-border"
            : "h-5 w-5 justify-center",
          className,
        )}
        role="status"
        aria-label={t(state)}
      >
        <span
          className={cn(
            "block h-2 w-2 shrink-0 rounded-full",
            COLOR_CLASSES[state],
          )}
        />
        {isCritical && (
          <span className="text-2xs font-medium text-danger-foreground">
            {t(state)}
          </span>
        )}
      </div>
    </Tooltip>
  );
}
