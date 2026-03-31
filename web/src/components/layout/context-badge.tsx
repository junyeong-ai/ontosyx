"use client";

import { useAppStore } from "@/lib/store";

// ---------------------------------------------------------------------------
// ContextBadge — mode-appropriate metadata badge in the header
// ---------------------------------------------------------------------------

export function ContextBadge() {
  const workspaceMode = useAppStore((s) => s.workspaceMode);
  const ontology = useAppStore((s) => s.ontology);
  const dashboardWidgetCount = useAppStore((s) => s.dashboardWidgetCount);

  switch (workspaceMode) {
    case "design":
    case "analyze":
      if (!ontology) return null;
      return (
        <span className="rounded-full bg-emerald-50 px-2 py-0.5 text-[10px] font-medium text-emerald-600 dark:bg-emerald-900/50 dark:text-emerald-400">
          {ontology.node_types.length}N &middot; {ontology.edge_types.length}E
        </span>
      );
    case "dashboard":
      return (
        <span className="rounded-full bg-blue-50 px-2 py-0.5 text-[10px] font-medium text-blue-600 dark:bg-blue-900/50 dark:text-blue-400">
          {dashboardWidgetCount} widget{dashboardWidgetCount !== 1 ? "s" : ""}
        </span>
      );
    default:
      return null;
  }
}
