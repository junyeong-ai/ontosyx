"use client";

import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import { useWidgets } from "@/hooks/api/use-widgets";
import { useWorkspaceMode } from "@/hooks/use-workspace-mode";
import { arr } from "@/lib/ir-collections";

// ---------------------------------------------------------------------------
// ContextBadge — mode-appropriate metadata badge in the header
// ---------------------------------------------------------------------------

export function ContextBadge() {
  const t = useTranslations("chrome.contextBadge");
  const workspaceMode = useWorkspaceMode();
  const ontology = useAppStore((s) => s.ontology);
  const activeDashboardId = useAppStore((s) => s.activeDashboardId);
  // Derive the widget count straight from TanStack Query — keeping
  // it in Zustand was a sync footgun (effect rewriting store after
  // every fetch round). Suspended hook is fine: the badge sits in
  // the header where the dashboards query is already in flight.
  const { data: widgets } = useWidgets(activeDashboardId);

  switch (workspaceMode) {
    case "design":
    case "analyze":
      if (!ontology) return null;
      return (
        <span className="rounded-full bg-brand-surface px-2 py-0.5 text-2xs font-medium text-brand-foreground-strong">
          {t("nodesEdges", {
            nodes: arr(ontology.node_types).length,
            edges: arr(ontology.edge_types).length,
          })}
        </span>
      );
    case "dashboard":
      return (
        <span className="rounded-full bg-info-surface px-2 py-0.5 text-2xs font-medium text-info-foreground dark:bg-info-foreground/50 dark:text-info-foreground">
          {t("widgetCount", { count: widgets?.length ?? 0 })}
        </span>
      );
    default:
      return null;
  }
}
