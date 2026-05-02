"use client";

import { memo, useCallback } from "react";
import { useTranslations } from "next-intl";
import { type NodeProps } from "@xyflow/react";
import { useAppStore } from "@/lib/store";
import { cn } from "@/lib/cn";

// ---------------------------------------------------------------------------
// Group node — collapsible container for organizing schema nodes
// ---------------------------------------------------------------------------

export interface GroupNodeData {
  groupId: string;
  name: string;
  nodeCount: number;
  collapsed: boolean;
  color?: string;
}

type GroupNodeProps = NodeProps & { data: GroupNodeData };

export const NodeGroup = memo(function NodeGroup({ data }: GroupNodeProps) {
  const t = useTranslations("workbench.canvas.group");
  const { groupId, name, nodeCount, collapsed, color } = data;
  const toggleGroupCollapse = useAppStore((s) => s.toggleGroupCollapse);
  const removeGroup = useAppStore((s) => s.removeGroup);

  const onToggle = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      toggleGroupCollapse(groupId);
    },
    [groupId, toggleGroupCollapse],
  );

  const onRemove = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      removeGroup(groupId);
    },
    [groupId, removeGroup],
  );

  const borderColor = color ?? "#6366f1"; // indigo default

  return (
    <div
      className={cn(
        "rounded-xl border-2 border-dashed",
        collapsed ? "min-w-[180px]" : "min-w-[300px] min-h-[200px]",
      )}
      style={{ borderColor, backgroundColor: `${borderColor}08` }}
    >
      {/* Group header */}
      <div
        className="flex items-center gap-2 rounded-t-lg px-3 py-1.5"
        style={{ backgroundColor: `${borderColor}15` }}
      >
        {/* Collapse/expand toggle */}
        <button
          onClick={onToggle}
          className="flex h-5 w-5 items-center justify-center rounded text-xs hover:bg-black/10 dark:hover:bg-surface-base"
          style={{ color: borderColor }}
        >
          {collapsed ? "\u25B6" : "\u25BC"}
        </button>

        <span
          className="text-xs font-semibold tracking-wide"
          style={{ color: borderColor }}
        >
          {name}
        </span>

        <span className="ml-1 rounded-full bg-surface-base px-1.5 py-0.5 text-2xs font-medium text-foreground-muted dark:text-muted-foreground">
          {nodeCount}
        </span>

        {/* Ungroup button */}
        <button
          onClick={onRemove}
          className="ml-auto flex h-5 w-5 items-center justify-center rounded text-2xs text-muted-foreground hover:bg-black/10 hover:text-foreground dark:hover:bg-surface-base dark:hover:text-foreground-muted"
          title={t("ungroupTitle")}
        >
          ✕
        </button>
      </div>

      {/* When collapsed, show compact summary */}
      {collapsed && (
        <div className="px-3 py-1 text-2xs text-muted-foreground">
          {t("collapsedSummary", { count: nodeCount })}
        </div>
      )}
    </div>
  );
});
