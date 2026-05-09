"use client";

import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "@/components/ui/toast";
import { useTranslations } from "next-intl";
import { History } from "lucide-react";

import { request } from "@/lib/api/client";
import { ErrorState } from "@/components/ui/error-state";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton, SkeletonTable } from "@/components/ui/skeleton";
import { FormSelect } from "@/components/ui/form-input";
import { DataTable, type ColumnDef } from "@/components/ui/data-table";
import { useFormatters } from "@/hooks/use-formatters";
import { useTableUrlState } from "@/hooks/use-table-url-state";
import { useChromeFilters } from "@/components/workbench/workbench-page-shell";

interface AuditEntry {
  id: string;
  user_id: string | null;
  action: string;
  resource_type: string;
  resource_id: string | null;
  details: Record<string, unknown>;
  created_at: string;
}

const auditKeys = {
  list: (days: number) => ["audit", "list", days] as const,
};

const VALID_DAYS = new Set([7, 30, 90]);

function formatAction(action: string): string {
  return action.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

function truncate(value: string | null, fallback: string): string {
  if (!value) return fallback;
  return value.length > 12 ? `${value.slice(0, 12)}…` : value;
}

export function UserAuditFacet() {
  const t = useTranslations("settings.governance.audit.user");
  const tCommon = useTranslations("common");
  const datePickerT = useTranslations("settings.datePicker");
  const fmt = useFormatters();

  const url = useTableUrlState({ filters: ["days"] });
  const days = (() => {
    const parsed = Number(url.filters.days);
    return VALID_DAYS.has(parsed) ? parsed : 30;
  })();

  const query = useQuery({
    queryKey: auditKeys.list(days),
    queryFn: async () => {
      const from = new Date(Date.now() - days * 86400000).toISOString();
      const to = new Date().toISOString();
      try {
        return await request<{ items: AuditEntry[] }>(
          `/audit?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`,
        );
      } catch (err) {
        toast.error(t("toast.loadFailed"));
        throw err;
      }
    },
  });

  const chromeFilters = useChromeFilters(
    <FormSelect
      density="settings"
      aria-label={datePickerT("daysLabel")}
      value={String(days)}
      onChange={(e) =>
        url.setFilter("days", e.target.value === "30" ? null : e.target.value)
      }
      className="w-auto"
    >
      <option value="7">{datePickerT("last7Days")}</option>
      <option value="30">{datePickerT("last30Days")}</option>
      <option value="90">{datePickerT("last90Days")}</option>
    </FormSelect>,
  );

  const items = query.data?.items ?? [];

  const columns = useMemo<ColumnDef<AuditEntry, unknown>[]>(
    () => [
      {
        id: "action",
        header: t("column.action"),
        accessorFn: (row) => row.action,
        cell: ({ getValue }) => (
          <span className="font-medium text-foreground-strong">
            {formatAction(getValue<string>())}
          </span>
        ),
      },
      {
        id: "resourceType",
        header: t("column.resourceType"),
        accessorFn: (row) => row.resource_type,
        cell: ({ getValue }) => (
          <span className="text-foreground-muted">{getValue<string>()}</span>
        ),
      },
      {
        id: "resourceId",
        header: t("column.resourceId"),
        accessorFn: (row) => row.resource_id ?? "",
        enableSorting: false,
        cell: ({ row }) => (
          <span className="font-mono text-2xs text-foreground-muted">
            {truncate(row.original.resource_id, "—")}
          </span>
        ),
      },
      {
        id: "user",
        header: t("column.user"),
        accessorFn: (row) => row.user_id ?? "",
        cell: ({ row }) => (
          <span className="font-mono text-2xs text-foreground-muted">
            {truncate(row.original.user_id, t("systemUser"))}
          </span>
        ),
      },
      {
        id: "date",
        header: t("column.date"),
        accessorFn: (row) => row.created_at,
        meta: { headerClass: "text-end", cellClass: "text-end" },
        cell: ({ getValue }) => (
          <span className="text-foreground-muted">
            {fmt.date(getValue<string>(), {
              month: "short",
              day: "numeric",
              hour: "2-digit",
              minute: "2-digit",
            })}
          </span>
        ),
      },
    ],
    [fmt, t],
  );

  return (
    <div className="flex flex-col gap-4">
      {chromeFilters}

      {query.isLoading ? (
        <div className="space-y-2">
          <div className="flex gap-3">
            <Skeleton className="h-3 w-1/5" />
            <Skeleton className="h-3 w-1/5" />
            <Skeleton className="h-3 w-1/5" />
            <Skeleton className="h-3 w-1/5" />
            <Skeleton className="h-3 w-1/5" />
          </div>
          <SkeletonTable rows={6} cols={5} />
        </div>
      ) : query.isError ? (
        <div className="py-8">
          <ErrorState
            title={tCommon("loadError.title")}
            description={tCommon("loadError.description")}
            onRetry={() => query.refetch()}
            retryLabel={tCommon("retry")}
          />
        </div>
      ) : (
        <DataTable<AuditEntry>
          columns={columns}
          data={items}
          sort={url.sort}
          onSortChange={url.setSort}
          ariaLabel={t("tableAria")}
          emptyState={
            <EmptyState
              kind="no-data"
              icon={History}
              title={t("empty")}
              description={t("emptyHint")}
            />
          }
        />
      )}
    </div>
  );
}
