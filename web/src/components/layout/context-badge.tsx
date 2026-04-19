"use client";

import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import { useWorkspaceMode } from "@/lib/use-workspace-mode";

// ---------------------------------------------------------------------------
// ContextBadge — mode-appropriate metadata badge in the header
// ---------------------------------------------------------------------------

export function ContextBadge() {
  const t = useTranslations("chrome.contextBadge");
  const workspaceMode = useWorkspaceMode();
  const ontology = useAppStore((s) => s.ontology);
  const dashboardWidgetCount = useAppStore((s) => s.dashboardWidgetCount);

  switch (workspaceMode) {
    case "design":
    case "analyze":
      if (!ontology) return null;
      return (
        <span className="rounded-full bg-emerald-50 px-2 py-0.5 text-[10px] font-medium text-emerald-600 dark:bg-emerald-900/50 dark:text-emerald-400">
          {t("nodesEdges", {
            nodes: ontology.node_types.length,
            edges: ontology.edge_types.length,
          })}
        </span>
      );
    case "dashboard":
      return (
        <span className="rounded-full bg-blue-50 px-2 py-0.5 text-[10px] font-medium text-blue-600 dark:bg-blue-900/50 dark:text-blue-400">
          {t("widgetCount", { count: dashboardWidgetCount })}
        </span>
      );
    default:
      return null;
  }
}
