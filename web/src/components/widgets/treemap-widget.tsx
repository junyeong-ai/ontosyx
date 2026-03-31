"use client";

import { useMemo } from "react";
import type { QueryResult, WidgetSpec } from "@/types/api";
import { Treemap, ResponsiveContainer, Tooltip } from "recharts";
import { useIsDarkMode } from "@/lib/use-dark-mode";
import { resolveLabelField, resolveValueField, tooltipStyle, PALETTE_PRIMARY } from "./chart-utils";

interface TreemapWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

interface TreeNode {
  [key: string]: unknown;
  name: string;
  value?: number;
  children?: TreeNode[];
}

/** Custom content renderer for treemap cells */
function CustomizedContent(props: Record<string, unknown>) {
  const { x, y, width, height, index, name } = props as {
    x: number;
    y: number;
    width: number;
    height: number;
    index: number;
    name: string;
  };

  const fill = PALETTE_PRIMARY[index % PALETTE_PRIMARY.length];

  return (
    <g>
      <rect
        x={x}
        y={y}
        width={width}
        height={height}
        rx={4}
        ry={4}
        style={{ fill, stroke: "#fff", strokeWidth: 2, strokeOpacity: 0.3 }}
      />
      {width > 40 && height > 20 && (
        <text
          x={x + width / 2}
          y={y + height / 2}
          textAnchor="middle"
          dominantBaseline="central"
          fill="#fff"
          fontSize={11}
          fontWeight={600}
          style={{ pointerEvents: "none" }}
        >
          {String(name).length > Math.floor(width / 7)
            ? String(name).slice(0, Math.floor(width / 7)) + "\u2026"
            : name}
        </text>
      )}
    </g>
  );
}

export function TreemapWidget({ spec, data }: TreemapWidgetProps) {
  const isDark = useIsDarkMode();
  const nameField = resolveLabelField(spec, data);
  const valueField = resolveValueField(spec, data);

  // Check for optional parent column for hierarchy
  const parentCol = useMemo(() => {
    const lower = data.columns.map((c) => c.toLowerCase());
    const idx = lower.findIndex((c) => c === "parent" || c === "group" || c === "category");
    return idx >= 0 ? data.columns[idx] : undefined;
  }, [data.columns]);

  const treeData = useMemo(() => {
    if (!nameField || !valueField) return [];

    if (parentCol) {
      // Build hierarchical structure
      const groups = new Map<string, TreeNode>();
      const orphans: TreeNode[] = [];

      for (const row of data.rows) {
        const name = String(row[nameField] ?? "");
        const value = Number(row[valueField] ?? 0);
        const parent = row[parentCol] != null ? String(row[parentCol]) : null;

        if (parent) {
          if (!groups.has(parent)) {
            groups.set(parent, { name: parent, children: [] });
          }
          groups.get(parent)!.children!.push({ name, value });
        } else {
          orphans.push({ name, value });
        }
      }

      const result: TreeNode[] = [...groups.values(), ...orphans];
      return result;
    }

    // Flat list
    return data.rows.map((row) => ({
      name: String(row[nameField] ?? ""),
      value: Number(row[valueField] ?? 0),
    }));
  }, [data.rows, nameField, valueField, parentCol]);

  if (!nameField || !valueField || treeData.length === 0) {
    return <p className="text-xs text-zinc-400">Need name and value columns for treemap</p>;
  }

  return (
    <div className="space-y-2">
      {spec.title && (
        <h4 className="text-xs font-semibold text-zinc-600 dark:text-zinc-400">
          {spec.title}
        </h4>
      )}
      <div className="h-64 w-full overflow-hidden">
        <ResponsiveContainer width="100%" height="100%">
          <Treemap
            data={treeData}
            dataKey="value"
            nameKey="name"
            content={<CustomizedContent />}
          >
            <Tooltip contentStyle={tooltipStyle(isDark)} />
          </Treemap>
        </ResponsiveContainer>
      </div>
      <p className="text-[10px] text-zinc-400">{data.rows.length} items</p>
    </div>
  );
}
