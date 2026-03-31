"use client";

import { cn } from "@/lib/cn";
import type { DesignSource } from "@/types/api";

export type GenerateSourceType = DesignSource["type"];

export function relationshipKey(rel: {
  from_table: string;
  from_column: string;
  to_table: string;
  to_column: string;
}) {
  return `${rel.from_table}.${rel.from_column}->${rel.to_table}.${rel.to_column}`;
}

export function columnKey(table: string, column: string) {
  return `${table}.${column}`;
}

export const selectClassName = cn(
  "w-full rounded-md border border-zinc-300 bg-white px-3 py-1.5 text-sm",
  "outline-none focus:border-emerald-500 focus:ring-1 focus:ring-emerald-500/50",
  "dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100",
  "dark:focus:border-emerald-400 dark:focus:ring-emerald-400/50",
);

export function formatGapLocation(loc: Record<string, unknown>): string {
  if (loc.ref_type === "node") return String(loc.label ?? "");
  if (loc.ref_type === "node_property") return `${loc.label}.${loc.property_name}`;
  if (loc.ref_type === "edge") return `[${loc.label}]`;
  if (loc.ref_type === "edge_property") return `[${loc.label}].${loc.property_name}`;
  if (loc.ref_type === "source_table") return String(loc.table ?? "");
  if (loc.ref_type === "source_column") return `${loc.table}.${loc.column}`;
  if (loc.ref_type === "source_foreign_key") return `${loc.from_table}.${loc.from_column} → ${loc.to_table}`;
  return "";
}

export function StatusBadge({ status }: { status: string }) {
  return (
    <span
      className={cn(
        "shrink-0 rounded-full px-1.5 py-0.5 text-[9px] font-medium uppercase",
        status === "completed"
          ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400"
          : status === "designed"
            ? "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400"
            : "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
      )}
    >
      {status}
    </span>
  );
}
