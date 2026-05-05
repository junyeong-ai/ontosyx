"use client";

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "@/components/ui/toast";
import { useTranslations } from "next-intl";

import { ArrowDown, ArrowRight, Database, Link } from "lucide-react";
import { Circle } from "lucide-react";
import { request } from "@/lib/api/client";
import { SkeletonTable } from "@/components/ui/skeleton";
import { WorkbenchPageShell } from "@/components/workbench/workbench-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge";
import { useFormatters } from "@/hooks/use-formatters";
import { useWorkspaceOntology } from "@/hooks/api/use-workspace-ontology";
import { cn } from "@/lib/cn";
import { DynamicIcon } from "@/components/ui/dynamic-icon";

interface LineageSummary {
  graph_label: string;
  graph_element_type: string;
  source_count: number;
  total_records: number;
  last_loaded_at: string | null;
}

interface PropertyMapping {
  source_column: string;
  graph_property: string;
  transform: string | null;
  mapping_kind: string;
}

interface LabelMappings {
  label: string;
  element_type: string;
  mappings: PropertyMapping[];
}

interface LineageEntry {
  id: string;
  graph_label: string;
  graph_element_type: string;
  source_type: string;
  source_name: string;
  source_table: string | null;
  source_columns: string[] | null;
  property_mappings: LabelMappings[] | null;
  record_count: number;
  started_at: string;
  completed_at: string | null;
  status: string;
  error_message: string | null;
}

type KnownLineageStatus = "completed" | "running" | "failed" | "partial";
function isKnownLineageStatus(s: string): s is KnownLineageStatus {
  return s === "completed" || s === "running" || s === "failed" || s === "partial";
}

const COMPACT_DATE_OPTIONS: Intl.DateTimeFormatOptions = {
  month: "short",
  day: "numeric",
  year: "numeric",
  hour: "2-digit",
  minute: "2-digit",
};

const COMPACT_NUMBER_OPTIONS: Intl.NumberFormatOptions = {
  notation: "compact",
  maximumFractionDigits: 1,
};

function ExecutionStatusBadge({ status, label }: { status: string; label: string }) {
  const tone: StatusTone =
    status === "completed" ? "success"
      : status === "failed"  ? "danger"
      : status === "partial" ? "warning"
      : "info";
  return <StatusBadge tone={tone} size="md">{label}</StatusBadge>;
}

interface MappingGroup {
  sourceTable: string;
  graphLabel: string;
  elementType: string;
  mappings: PropertyMapping[];
}

function aggregateMappings(entries: LineageEntry[]): MappingGroup[] {
  const key = (src: string, label: string) => `${src}||${label}`;
  const groups = new Map<string, MappingGroup>();

  for (const entry of entries) {
    if (!entry.property_mappings) continue;
    for (const lm of entry.property_mappings) {
      const src = entry.source_table || entry.source_name || "unknown";
      const k = key(src, lm.label);
      if (!groups.has(k)) {
        groups.set(k, {
          sourceTable: src,
          graphLabel: lm.label,
          elementType: lm.element_type,
          mappings: [],
        });
      }
      const g = groups.get(k)!;
      for (const m of lm.mappings) {
        const exists = g.mappings.some(
          (x) =>
            x.source_column === m.source_column &&
            x.graph_property === m.graph_property,
        );
        if (!exists) g.mappings.push(m);
      }
    }
  }
  return Array.from(groups.values());
}

function MappingCard({
  group,
  expanded,
  onToggle,
}: {
  group: MappingGroup;
  expanded: boolean;
  onToggle: () => void;
}) {
  const t = useTranslations("lineage");
  const matchMappings = group.mappings.filter(
    (m) => m.mapping_kind === "match",
  );
  const setMappings = group.mappings.filter(
    (m) => m.mapping_kind === "set",
  );

  return (
    <div className="rounded-lg border border-divider overflow-hidden">
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center justify-between px-4 py-3 text-start hover:bg-surface-raised transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]"
      >
        <div className="flex items-center gap-3">
          <span className="inline-flex items-center gap-1.5 rounded-md bg-surface-inset px-2.5 py-1 text-sm font-mono font-medium text-foreground-muted">
            <Database className="h-3.5 w-3.5 text-foreground-muted" />
            {group.sourceTable}
          </span>

          <ArrowRight className="h-5 w-5 shrink-0 text-foreground-muted" />

          <span
            className={cn(
              "inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-sm font-mono font-medium",
              group.elementType === "node"
                ? "bg-info-surface text-info-foreground"
                : "bg-concept-surface text-concept-foreground",
            )}
          >
            <DynamicIcon as={group.elementType === "node" ? Circle : Link} className="h-3.5 w-3.5" />
            {group.graphLabel}
          </span>

          <span className="text-xs text-foreground-muted">
            {t("columnsCount", { count: group.mappings.length })}
          </span>
        </div>

        <ArrowDown className={cn(
 "h-4 w-4 text-foreground-muted transition-transform duration-[var(--duration-quick)] ease-[var(--ease-out)]",
 expanded && "rotate-180",
 )} />
      </button>

      {expanded && (
        <div className="border-t border-divider-soft px-4 py-3 bg-surface-raised/30">
          {matchMappings.length > 0 && (
            <div className="mb-3">
              <div className="text-2xs uppercase tracking-wider font-medium text-foreground-muted mb-1.5">
                {t("identity")}
              </div>
              <div className="space-y-1">
                {matchMappings.map((m, i) => (
                  <MappingRow key={`match-${i}`} mapping={m} isIdentity />
                ))}
              </div>
            </div>
          )}
          {setMappings.length > 0 && (
            <div>
              <div className="text-2xs uppercase tracking-wider font-medium text-foreground-muted mb-1.5">
                {t("properties")}
              </div>
              <div className="space-y-1">
                {setMappings.map((m, i) => (
                  <MappingRow key={`set-${i}`} mapping={m} />
                ))}
              </div>
            </div>
          )}
          {matchMappings.length === 0 && setMappings.length === 0 && (
            <p className="text-xs text-foreground-muted">{t("noMappings")}</p>
          )}
        </div>
      )}
    </div>
  );
}

function MappingRow({
  mapping,
  isIdentity,
}: {
  mapping: PropertyMapping;
  isIdentity?: boolean;
}) {
  const t = useTranslations("lineage");
  return (
    <div className="flex items-center gap-2 text-xs">
      <code
        className={cn(
          "rounded px-1.5 py-0.5 font-mono",
          isIdentity
            ? "bg-warning-surface text-warning-foreground"
            : "bg-surface-inset text-foreground",
        )}
      >
        {mapping.source_column}
      </code>
      <ArrowRight className="h-3 w-3 shrink-0 text-foreground-muted" />
      <code className="rounded bg-info-surface px-1.5 py-0.5 font-mono text-info-foreground">
        {mapping.graph_property}
      </code>
      {mapping.transform && (
        <span className="rounded bg-surface-inset px-1.5 py-0.5 text-2xs text-foreground-muted">
          {mapping.transform}
        </span>
      )}
      {isIdentity && (
        <span className="rounded bg-warning-surface px-1 py-0.5 text-2xs font-medium text-warning-foreground">
          {t("key")}
        </span>
      )}
    </div>
  );
}

const lineageKeys = {
  all: ["lineage"] as const,
  summary: () => [...lineageKeys.all, "summary"] as const,
  byLabel: (label: string) => [...lineageKeys.all, "label", label] as const,
};

export default function LineageSettingsPage() {
  const t = useTranslations("lineage");
  const tCommon = useTranslations("common");
  const fmt = useFormatters();
  const [expandedCard, setExpandedCard] = useState<string | null>(null);

  // Lineage is meaningful only after a canonical ontology exists —
  // graph labels and source loads both reference its NodeTypes /
  // ObjectMappings. Without one we render the same empty-state shape
  // as /mappings + the vocabulary tabs so the gate copy reads
  // consistently across surfaces.
  const ontologyQuery = useWorkspaceOntology();
  const hasCanonical = !!ontologyQuery.data;

  const summaryQuery = useQuery({
    queryKey: lineageKeys.summary(),
    queryFn: async () => {
      try {
        return await request<LineageSummary[]>("/lineage");
      } catch (err) {
        toast.error(t("toast.loadFailed"));
        throw err;
      }
    },
    enabled: hasCanonical,
  });

  const summary = summaryQuery.data ?? [];
  const uniqueLabels = [...new Set(summary.map((s) => s.graph_label))];

  const entriesQuery = useQuery({
    queryKey: [...lineageKeys.all, "entries", uniqueLabels],
    queryFn: async () => {
      const results = await Promise.all(
        uniqueLabels.map((label) =>
          request<LineageEntry[]>(`/lineage/label/${encodeURIComponent(label)}`),
        ),
      );
      return results.flat();
    },
    enabled: hasCanonical && summaryQuery.isSuccess,
  });

  const entries = entriesQuery.data ?? [];
  const isLoading =
    ontologyQuery.isLoading ||
    (hasCanonical && (summaryQuery.isLoading || entriesQuery.isLoading));
  const isError =
    ontologyQuery.isError || summaryQuery.isError || entriesQuery.isError;

  const totalRecords = summary.reduce((acc, l) => acc + l.total_records, 0);
  const totalSources = summary.reduce((acc, l) => acc + l.source_count, 0);
  const totalLabels = summary.length;

  const mappingGroups = aggregateMappings(entries);
  const historyEntries = [...entries].sort(
    (a, b) => new Date(b.started_at).getTime() - new Date(a.started_at).getTime(),
  );

  const statusLabel = (s: string) =>
    isKnownLineageStatus(s) ? t(`status.${s}`) : s;

  const pageState: PageState = isLoading
    ? { kind: "loading" }
    : isError
      ? {
          kind: "error",
          onRetry: () => {
            void ontologyQuery.refetch();
            void summaryQuery.refetch();
            void entriesQuery.refetch();
          },
        }
      : !hasCanonical
        ? { kind: "empty" }
        : { kind: "data" };

  return (
    <WorkbenchPageShell title={t("title")}>
      <PageStateView
          state={pageState}
          skeleton={<SkeletonTable rows={6} cols={5} />}
          error={{
            title: tCommon("loadError.title"),
            description: tCommon("loadError.description"),
            retryLabel: tCommon("retry"),
          }}
          empty={{
            title: t("noOntology"),
          }}
        >
            <div className="grid grid-cols-3 gap-4">
              <div className="rounded-lg border border-divider p-4">
                <div className="text-2xl font-bold text-foreground-strong">
                  {totalLabels}
                </div>
                <div className="text-xs text-foreground-muted">{t("summary.labels")}</div>
              </div>
              <div className="rounded-lg border border-divider p-4">
                <div className="text-2xl font-bold text-foreground-strong">
                  {totalSources}
                </div>
                <div className="text-xs text-foreground-muted">{t("summary.sources")}</div>
              </div>
              <div className="rounded-lg border border-divider p-4">
                <div className="text-2xl font-bold text-foreground-strong">
                  {fmt.number(totalRecords, COMPACT_NUMBER_OPTIONS)}
                </div>
                <div className="text-xs text-foreground-muted">{t("summary.records")}</div>
              </div>
            </div>

            {mappingGroups.length > 0 && (
              <div className="mt-8">
                <h2 className="text-sm font-semibold text-foreground mb-3">
                  {t("columnMappings")}
                </h2>
                <p className="text-xs text-foreground-muted mb-4">
                  {t("columnMappingsHint")}
                </p>
                <div className="space-y-2">
                  {mappingGroups.map((group) => {
                    const cardKey = `${group.sourceTable}||${group.graphLabel}`;
                    return (
                      <MappingCard
                        key={cardKey}
                        group={group}
                        expanded={expandedCard === cardKey}
                        onToggle={() =>
                          setExpandedCard((prev) =>
                            prev === cardKey ? null : cardKey,
                          )
                        }
                      />
                    );
                  })}
                </div>
              </div>
            )}

            <div className="mt-8">
              <h2 className="text-sm font-semibold text-foreground mb-3">
                {t("loadHistory")}
              </h2>
              <div className="overflow-x-auto rounded-lg border border-divider">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-divider bg-surface-raised text-start text-xs font-medium uppercase text-foreground-muted">
                      <th className="py-3 pe-6">{t("column.label")}</th>
                      <th className="py-3 pe-6">{t("column.source")}</th>
                      <th className="py-3 pe-6 text-end">{t("column.records")}</th>
                      <th className="py-3 pe-6 text-end">{t("column.started")}</th>
                      <th className="py-3 pe-6 text-end">{t("column.status")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {historyEntries.map((e) => (
                      <tr
                        key={e.id}
                        className="border-b border-divider-soft last:border-b-0"
                        title={e.error_message ?? undefined}
                      >
                        <td className="py-3 pe-6 font-medium text-foreground-strong">
                          <div className="flex items-center gap-1.5">
                            <span
                              className={cn(
                                "inline-block h-2 w-2 rounded-full",
                                e.graph_element_type === "node"
                                  ? "bg-info-foreground"
                                  : "bg-concept-foreground",
                              )}
                            />
                            {e.graph_label}
                          </div>
                        </td>
                        <td className="py-3 pe-6 text-foreground-muted">
                          <span className="font-mono text-xs">
                            {e.source_table || e.source_name}
                          </span>
                        </td>
                        <td className="py-3 pe-6 text-end text-foreground-muted">
                          {fmt.number(e.record_count, COMPACT_NUMBER_OPTIONS)}
                        </td>
                        <td className="py-3 pe-6 text-end text-foreground-muted text-xs">
                          {fmt.date(e.started_at, COMPACT_DATE_OPTIONS)}
                        </td>
                        <td className="py-3 pe-6 text-end">
                          <ExecutionStatusBadge status={e.status} label={statusLabel(e.status)} />
                        </td>
                      </tr>
                    ))}
                    {historyEntries.length === 0 && (
                      <tr>
                        <td
                          colSpan={5}
                          className="py-8 text-center text-foreground-muted"
                        >
                          {t("noHistory")}
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
            </div>
      </PageStateView>
    </WorkbenchPageShell>
  );
}
