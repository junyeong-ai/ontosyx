"use client";

import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";

// ---------------------------------------------------------------------------
// Version diff overlay bar -- shown when comparing revisions
// ---------------------------------------------------------------------------

export function VersionDiffBar() {
  const t = useTranslations("workbench.canvas.versionDiff");
  const diffOverlay = useAppStore((s) => s.activeDiffOverlay);
  const setDiffOverlay = useAppStore((s) => s.setActiveDiffOverlay);

  if (!diffOverlay || !diffOverlay.summary.total_changes) return null;

  const { summary } = diffOverlay;

  return (
    <div className="absolute left-1/2 bottom-3 z-10 -translate-x-1/2">
      <div className="flex items-center gap-3 rounded-lg border border-concept-border bg-concept-surface/95 px-4 py-2 text-xs shadow-lg backdrop-blur-sm dark:border-concept-border">
        <span className="font-semibold text-concept-foreground dark:text-concept-foreground">
          {t("title")}
        </span>
        {summary.nodes_added > 0 && (
          <span className="text-brand-foreground">
            +{summary.nodes_added}N
          </span>
        )}
        {summary.nodes_removed > 0 && (
          <span className="text-danger-foreground">
            -{summary.nodes_removed}N
          </span>
        )}
        {summary.nodes_modified > 0 && (
          <span className="text-warning-foreground">
            ~{summary.nodes_modified}N
          </span>
        )}
        {summary.edges_added > 0 && (
          <span className="text-brand-foreground">
            +{summary.edges_added}E
          </span>
        )}
        {summary.edges_removed > 0 && (
          <span className="text-danger-foreground">
            -{summary.edges_removed}E
          </span>
        )}
        {summary.edges_modified > 0 && (
          <span className="text-warning-foreground">
            ~{summary.edges_modified}E
          </span>
        )}
        <button
          onClick={() => setDiffOverlay(null)}
          className="ml-1 rounded-md px-2 py-0.5 text-muted-foreground hover:bg-surface-base hover:text-foreground dark:hover:bg-surface-base/50"
        >
          {t("dismiss")}
        </button>
      </div>
    </div>
  );
}
