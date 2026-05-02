"use client";

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { useTranslations } from "next-intl";

import { request } from "@/lib/api/client";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton, SkeletonTable } from "@/components/ui/skeleton";
import { SettingsSelect } from "@/components/ui/form-input";
import { KpiCard } from "@/components/ui/kpi-card";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";

interface UsageSummary {
  resource_type: string;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
  request_count: number;
}

const usageKeys = {
  list: (days: number) => ["usage", "list", days] as const,
};

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}

export default function UsageSettingsPage() {
  const t = useTranslations("settings.usage");
  const tCommon = useTranslations("common");
  const datePickerT = useTranslations("settings.datePicker");
  const [days, setDays] = useState(30);

  const query = useQuery({
    queryKey: usageKeys.list(days),
    queryFn: async () => {
      const from = new Date(Date.now() - days * 86400000).toISOString();
      const to = new Date().toISOString();
      try {
        return await request<UsageSummary[]>(
          `/usage?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`,
        );
      } catch (err) {
        toast.error(t("toast.loadFailed"));
        throw err;
      }
    },
  });

  const usage = query.data ?? [];
  const totalTokens = usage.reduce(
    (acc, u) => acc + u.total_input_tokens + u.total_output_tokens,
    0,
  );
  const totalCost = usage.reduce((acc, u) => acc + u.total_cost_usd, 0);
  const totalRequests = usage.reduce((acc, u) => acc + u.request_count, 0);

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
        <div className="space-y-6">
          <div className="grid grid-cols-3 gap-4">
            <Skeleton className="h-24" />
            <Skeleton className="h-24" />
            <Skeleton className="h-24" />
          </div>
          <SkeletonTable rows={5} cols={5} />
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
        <>
          <div className="grid grid-cols-3 gap-4">
            <KpiCard
              label={t("summary.totalTokens")}
              value={totalTokens}
              format={formatTokens}
            />
            <KpiCard label={t("summary.requests")} value={totalRequests} />
            <KpiCard
              label={t("summary.estimatedCost")}
              value={totalCost}
              format={(n) => `$${n.toFixed(4)}`}
            />
          </div>

          <div className="mt-6">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-divider text-left text-xs font-medium uppercase text-muted-foreground">
                  <th className="py-3 pr-6">{t("column.resourceType")}</th>
                  <th className="py-3 pr-6 text-right">
                    {t("column.inputTokens")}
                  </th>
                  <th className="py-3 pr-6 text-right">
                    {t("column.outputTokens")}
                  </th>
                  <th className="py-3 pr-6 text-right">
                    {t("column.requests")}
                  </th>
                  <th className="py-3 pr-6 text-right">{t("column.cost")}</th>
                </tr>
              </thead>
              <tbody>
                {usage.map((u) => (
                  <tr
                    key={u.resource_type}
                    className="border-b border-divider-soft"
                  >
                    <td className="py-3 pr-6 font-medium text-foreground-strong">
                      {u.resource_type}
                    </td>
                    <td className="py-3 pr-6 text-right text-muted-foreground">
                      {formatTokens(u.total_input_tokens)}
                    </td>
                    <td className="py-3 pr-6 text-right text-muted-foreground">
                      {formatTokens(u.total_output_tokens)}
                    </td>
                    <td className="py-3 pr-6 text-right text-muted-foreground">
                      {u.request_count.toLocaleString()}
                    </td>
                    <td className="py-3 pr-6 text-right text-muted-foreground">
                      ${u.total_cost_usd.toFixed(4)}
                    </td>
                  </tr>
                ))}
                {usage.length === 0 && (
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
        </>
      )}
    </SettingsPageShell>
  );
}
