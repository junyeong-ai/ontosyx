"use client";

// User-action audit log facet — generic record of admin actions on
// platform resources (workspace settings, role assignments, etc.).
// Mounted inside the consolidated Audit page's tab shell, so it owns
// only its filter row + table; the page chrome is supplied by the
// parent.

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "@/components/ui/toast";
import { useTranslations } from "next-intl";

import { request } from "@/lib/api/client";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton, SkeletonTable } from "@/components/ui/skeleton";
import { SettingsSelect } from "@/components/ui/form-input";
import { useFormatters } from "@/hooks/use-formatters";

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
  const [days, setDays] = useState(30);

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

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-end">
        <SettingsSelect
          label={datePickerT("daysLabel")}
          hideLabel
          value={days}
          onChange={(e) => setDays(Number(e.target.value))}
        >
          <option value={7}>{datePickerT("last7Days")}</option>
          <option value={30}>{datePickerT("last30Days")}</option>
          <option value={90}>{datePickerT("last90Days")}</option>
        </SettingsSelect>
      </div>

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
        <div
          className="-mx-6 overflow-x-auto px-6"
          tabIndex={0}
          role="region"
          aria-label={t("tableAria")}
        >
          <table className="w-full min-w-[640px] text-sm">
            <thead>
              <tr className="border-b border-divider text-start text-xs font-medium uppercase text-foreground-muted">
                <th className="py-3 pe-6">{t("column.action")}</th>
                <th className="py-3 pe-6">{t("column.resourceType")}</th>
                <th className="py-3 pe-6">{t("column.resourceId")}</th>
                <th className="py-3 pe-6">{t("column.user")}</th>
                <th className="py-3 pe-6 text-end">{t("column.date")}</th>
              </tr>
            </thead>
            <tbody>
              {(query.data?.items ?? []).map((entry) => (
                <tr key={entry.id} className="border-b border-divider-soft">
                  <td className="py-3 pe-6 font-medium text-foreground-strong">
                    {formatAction(entry.action)}
                  </td>
                  <td className="py-3 pe-6 text-foreground-muted">
                    {entry.resource_type}
                  </td>
                  <td className="py-3 pe-6 font-mono text-xs text-foreground-muted">
                    {truncate(entry.resource_id, "—")}
                  </td>
                  <td className="py-3 pe-6 font-mono text-xs text-foreground-muted">
                    {truncate(entry.user_id, t("systemUser"))}
                  </td>
                  <td className="py-3 pe-6 text-end text-foreground-muted">
                    {fmt.date(entry.created_at, {
                      month: "short",
                      day: "numeric",
                      hour: "2-digit",
                      minute: "2-digit",
                    })}
                  </td>
                </tr>
              ))}
              {(query.data?.items ?? []).length === 0 && (
                <tr>
                  <td
                    colSpan={5}
                    className="py-8 text-center text-foreground-muted"
                  >
                    {t("empty")}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
