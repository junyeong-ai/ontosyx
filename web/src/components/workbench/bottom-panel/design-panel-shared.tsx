"use client";

import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import { StatusBadge as StatusBadgePrimitive, type StatusTone } from "@/components/ui/status-badge";
import type { DesignSource } from "@/types/api";

// Known project statuses — localized via workbench.bottomPanel.workflow.stepXxx keys.
// Unknown statuses fall back to the raw wire string.
const KNOWN_STATUSES = ["analyzed", "designed", "completed"] as const;
type KnownStatus = (typeof KNOWN_STATUSES)[number];
function isKnownStatus(s: string): s is KnownStatus {
  return (KNOWN_STATUSES as readonly string[]).includes(s);
}

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
  "w-full rounded-md border border-divider bg-surface-base px-3 py-1.5 text-sm",
  "outline-none focus:border-brand-foreground focus:ring-1 focus:ring-brand-foreground/50",
  "dark:border-divider-strong",
  "dark:focus:border-brand-border dark:focus:ring-brand-foreground/50",
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

export function WorkflowStatusBadge({ status }: { status: string }) {
  const t = useTranslations("workbench.bottomPanel.workflow");
  const label = isKnownStatus(status)
    ? status === "analyzed"
      ? t("stepAnalyze")
      : status === "designed"
        ? t("stepDesign")
        : t("stepComplete")
    : status;
  const tone: StatusTone =
    status === "completed" ? "success"
      : status === "designed" ? "info"
      : "warning";
  return (
    <StatusBadgePrimitive
      tone={tone}
      className="shrink-0 font-medium uppercase"
    >
      {label}
    </StatusBadgePrimitive>
  );
}
