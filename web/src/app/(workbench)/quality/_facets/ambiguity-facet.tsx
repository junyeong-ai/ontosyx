"use client";

import { useEffect, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";

import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { SkeletonTable } from "@/components/ui/skeleton";
import { BulkActionBar } from "@/components/ui/bulk-action-bar";
import { Button } from "@/components/ui/button";
import { DataTable, type ColumnDef } from "@/components/ui/data-table";
import { FormSelect } from "@/components/ui/form-input";
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge";
import { useChromeFilters } from "@/components/workbench/workbench-page-shell";
import { useTableUrlState } from "@/hooks/use-table-url-state";
import {
  useAmbiguities,
  useBulkRevokeAmbiguities,
  useResolveAmbiguity,
  useRevokeAmbiguity,
} from "@/hooks/api/use-ambiguities";
import type {
  AmbiguityMapping,
  AmbiguitySummary,
} from "@/lib/api/ambiguity";
import { ResolutionModal } from "@/components/ambiguity/resolution-modal";

type StatusFilter = "pending" | "stale" | "resolved" | "all";

const STATUS_TONE: Record<Exclude<StatusFilter, "all">, StatusTone> = {
  pending: "warning",
  stale: "warning",
  resolved: "brand",
};

const VALID_STATUS = new Set<StatusFilter>([
  "pending",
  "stale",
  "resolved",
  "all",
]);

function classify(summary: AmbiguitySummary): Exclude<StatusFilter, "all"> {
  if (!summary.active_resolution) return "pending";
  if (
    summary.active_resolution.context_source_hash !==
    summary.context.detection_source_hash
  ) {
    return "stale";
  }
  return "resolved";
}

export function AmbiguityFacet() {
  const t = useTranslations("settings.quality.ambiguity");
  const tCommon = useTranslations("common");
  const url = useTableUrlState({ filters: ["status"] });
  const statusFilter: StatusFilter = (() => {
    const raw = url.filters.status;
    return raw && VALID_STATUS.has(raw as StatusFilter)
      ? (raw as StatusFilter)
      : "pending";
  })();

  const [editing, setEditing] = useState<AmbiguitySummary | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  // biome-ignore lint/correctness/useExhaustiveDependencies: filter drives reset
  useEffect(() => {
    setSelectedIds(new Set());
  }, [statusFilter]);

  const ambiguitiesQuery = useAmbiguities();
  const { data, isLoading, isError, refetch } = ambiguitiesQuery;
  const resolve = useResolveAmbiguity({
    onSuccess: () => {
      toast.success(t("toast.resolved"));
      setEditing(null);
    },
    onError: (err) =>
      toast.error(err instanceof Error ? err.message : t("toast.resolveFailed")),
  });
  const revoke = useRevokeAmbiguity({
    onError: (err) =>
      toast.error(err instanceof Error ? err.message : t("toast.revokeFailed")),
  });
  const bulkRevoke = useBulkRevokeAmbiguities();

  const items = data?.items ?? [];
  const counts = useMemo(() => {
    const tally: Record<Exclude<StatusFilter, "all">, number> = {
      pending: 0,
      stale: 0,
      resolved: 0,
    };
    for (const item of items) tally[classify(item)] += 1;
    return { ...tally, all: items.length };
  }, [items]);

  const visibleRows = useMemo(() => {
    if (statusFilter === "all") return items;
    return items.filter((row) => classify(row) === statusFilter);
  }, [items, statusFilter]);

  const allowBulk = statusFilter === "resolved";

  const chromeFilters = useChromeFilters(
    <FormSelect
      density="settings"
      aria-label={t("filter.statusLabel")}
      value={statusFilter}
      onChange={(e) =>
        url.setFilter(
          "status",
          e.target.value === "pending" ? null : e.target.value,
        )
      }
      className="w-auto"
    >
      <option value="pending">
        {t("filter.pending", { count: counts.pending })}
      </option>
      <option value="stale">
        {t("filter.stale", { count: counts.stale })}
      </option>
      <option value="resolved">
        {t("filter.resolved", { count: counts.resolved })}
      </option>
      <option value="all">{t("filter.all", { count: counts.all })}</option>
    </FormSelect>,
  );

  const columns = useMemo<ColumnDef<AmbiguitySummary, unknown>[]>(
    () => [
      {
        id: "source",
        header: t("columns.source"),
        accessorFn: (row) => row.context.source_id,
        cell: ({ getValue }) => (
          <span className="font-mono">{getValue<string>()}</span>
        ),
      },
      {
        id: "column",
        header: t("columns.column"),
        accessorFn: (row) =>
          `${row.context.column.relation}.${row.context.column.column}`,
        cell: ({ row }) => (
          <span>
            <span className="text-foreground-muted">
              {row.original.context.column.relation}.
            </span>
            <span className="font-medium">
              {row.original.context.column.column}
            </span>
          </span>
        ),
      },
      {
        id: "kind",
        header: t("columns.kind"),
        accessorFn: (row) => row.context.kind.kind,
        cell: ({ row }) => <KindBadge kind={row.original.context.kind.kind} />,
      },
      {
        id: "status",
        header: t("columns.status"),
        accessorFn: (row) => classify(row),
        cell: ({ row }) => {
          const status = classify(row.original);
          return (
            <StatusBadge tone={STATUS_TONE[status]} size="sm">
              {t(`statusLabel.${status}`)}
            </StatusBadge>
          );
        },
      },
      {
        id: "mapping",
        header: t("columns.mapping"),
        enableSorting: false,
        cell: ({ row }) =>
          row.original.active_resolution ? (
            <MappingBadge
              mapping={row.original.active_resolution.mapping}
            />
          ) : (
            <span className="text-foreground-muted">—</span>
          ),
      },
      {
        id: "detected",
        header: t("columns.detected"),
        accessorFn: (row) => row.context.detected_at,
        cell: ({ getValue }) => (
          <span className="text-foreground-muted">
            {new Date(getValue<string>()).toLocaleDateString()}
          </span>
        ),
      },
      {
        id: "actions",
        header: t("columns.actions"),
        enableSorting: false,
        meta: { headerClass: "text-end", cellClass: "text-end" },
        cell: ({ row }) => (
          <>
            <Button
              variant="ghost"
              size="xs"
              onClick={() => setEditing(row.original)}
            >
              {row.original.active_resolution
                ? t("actions.edit")
                : t("actions.resolve")}
            </Button>
            {row.original.active_resolution && (
              <Button
                variant="ghost"
                size="xs"
                className="ms-1 text-danger-foreground hover:bg-danger-surface"
                onClick={() => {
                  const mapping = row.original.active_resolution?.mapping;
                  if (!mapping) return;
                  revoke.mutate(row.original.context.id, {
                    onSuccess: () => {
                      toast.undoable({
                        message: t("toast.revoked"),
                        undoLabel: t("toast.undo"),
                        onUndo: () =>
                          resolve.mutate({
                            id: row.original.context.id,
                            mapping,
                          }),
                      });
                    },
                  });
                }}
                disabled={revoke.isPending}
              >
                {t("actions.revoke")}
              </Button>
            )}
          </>
        ),
      },
    ],
    [resolve, revoke, t],
  );

  return (
    <div className="flex flex-col gap-4">
      {chromeFilters}

      <PageStateView
        state={
          (isLoading
            ? { kind: "loading" }
            : isError
              ? { kind: "error", onRetry: () => void refetch() }
              : visibleRows.length === 0
                ? { kind: "empty" }
                : { kind: "data" }) satisfies PageState
        }
        skeleton={<SkeletonTable rows={4} cols={7} />}
        empty={{ title: t(`empty.${statusFilter}`) }}
        error={{
          title: tCommon("loadError.title"),
          description: tCommon("loadError.description"),
          retryLabel: tCommon("retry"),
        }}
      >
        <DataTable<AmbiguitySummary>
          columns={columns}
          data={visibleRows}
          rowId={(row) => row.context.id}
          sort={url.sort}
          onSortChange={url.setSort}
          selectedIds={allowBulk ? selectedIds : undefined}
          onSelectionChange={allowBulk ? setSelectedIds : undefined}
          isRowSelectable={(row) => !!row.active_resolution}
          selectionLabels={{
            selectAll: t("bulk.selectAllAria"),
            selectRow: t("bulk.selectRowGeneric"),
          }}
          ariaLabel={t("title")}
        />
      </PageStateView>

      {editing && (
        <ResolutionModal
          context={editing.context}
          active={editing.active_resolution}
          busy={resolve.isPending}
          onCancel={() => setEditing(null)}
          onSubmit={(mapping: AmbiguityMapping) => {
            resolve.mutate({ id: editing.context.id, mapping });
          }}
        />
      )}

      <BulkActionBar
        count={selectedIds.size}
        countLabel={t("bulk.selectedCount", { count: selectedIds.size })}
        clearLabel={t("bulk.clear")}
        ariaLabel={t("bulk.barLabel")}
        actions={[
          {
            key: "revoke",
            label: t("bulk.revoke"),
            variant: "danger",
            onClick: () => {
              const ids = Array.from(selectedIds);
              bulkRevoke.mutate(
                { ids },
                {
                  onSuccess: ({ revoked }) => {
                    toast.success(
                      t("bulk.revokedToast", { count: revoked }),
                    );
                    setSelectedIds(new Set());
                  },
                  onError: (err) =>
                    toast.error(
                      err instanceof Error
                        ? err.message
                        : t("toast.revokeFailed"),
                    ),
                },
              );
            },
          },
        ]}
        onClear={() => setSelectedIds(new Set())}
        pending={bulkRevoke.isPending}
      />
    </div>
  );
}

function KindBadge({
  kind,
}: {
  kind: "numeric_code" | "opaque_short_code" | "overloaded_name";
}) {
  const t = useTranslations("settings.quality.ambiguity.kind");
  const tone: Record<typeof kind, StatusTone> = {
    numeric_code: "info",
    opaque_short_code: "warning",
    overloaded_name: "danger",
  };
  return (
    <StatusBadge tone={tone[kind]} size="sm">
      {t(kind)}
    </StatusBadge>
  );
}

function MappingBadge({ mapping }: { mapping: AmbiguityMapping }) {
  const t = useTranslations("settings.quality.ambiguity.mappingBadge");
  if (mapping.kind === "value_map") {
    return (
      <span className="text-foreground-muted">
        {t("valueMap", { count: mapping.entries.length })}
      </span>
    );
  }
  if (mapping.kind === "code_system_ref") {
    return (
      <span className="font-mono text-2xs">CS: {mapping.code_system_id}</span>
    );
  }
  return <span className="font-mono text-2xs">C: {mapping.concept_id}</span>;
}
