"use client";

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { useTranslations } from "next-intl";

import { request } from "@/lib/api/client";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton, SkeletonTable } from "@/components/ui/skeleton";
import { SettingsSelect } from "@/components/ui/form-input";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";

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

export default function AuditSettingsPage() {
  const t = useTranslations("settings.audit");
  const tCommon = useTranslations("common");
  const datePickerT = useTranslations("settings.datePicker");
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
    <SettingsPageShell
      title={t("title")}
      subtitle={t("description")}
      actions={
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
      }
    >
      {query.isLoading ? (
        <div className="mt-6 space-y-2">
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
        <div className="py-12">
          <ErrorState
            title={tCommon("loadError.title")}
            description={tCommon("loadError.description")}
            onRetry={() => query.refetch()}
            retryLabel={tCommon("retry")}
          />
        </div>
      ) : (
        <div
          className="-mx-6 mt-6 overflow-x-auto px-6"
          tabIndex={0}
          role="region"
          aria-label={t("tableAria")}
        >
          <table className="w-full min-w-[640px] text-sm">
            <thead>
              <tr className="border-b border-divider text-left text-xs font-medium uppercase text-muted-foreground">
                <th className="py-3 pr-6">{t("column.action")}</th>
                <th className="py-3 pr-6">{t("column.resourceType")}</th>
                <th className="py-3 pr-6">{t("column.resourceId")}</th>
                <th className="py-3 pr-6">{t("column.user")}</th>
                <th className="py-3 pr-6 text-right">{t("column.date")}</th>
              </tr>
            </thead>
            <tbody>
              {(query.data?.items ?? []).map((entry) => (
                <tr key={entry.id} className="border-b border-divider-soft">
                  <td className="py-3 pr-6 font-medium text-foreground-strong">
                    {formatAction(entry.action)}
                  </td>
                  <td className="py-3 pr-6 text-muted-foreground">
                    {entry.resource_type}
                  </td>
                  <td className="py-3 pr-6 font-mono text-xs text-muted-foreground">
                    {truncate(entry.resource_id, "—")}
                  </td>
                  <td className="py-3 pr-6 font-mono text-xs text-muted-foreground">
                    {truncate(entry.user_id, t("systemUser"))}
                  </td>
                  <td className="py-3 pr-6 text-right text-muted-foreground">
                    {new Date(entry.created_at).toLocaleString(undefined, {
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
                    className="py-8 text-center text-muted-foreground"
                  >
                    {t("empty")}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}
    </SettingsPageShell>
  );
}
