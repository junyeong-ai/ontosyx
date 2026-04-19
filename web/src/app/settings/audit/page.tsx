"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { useTranslations } from "next-intl";
import { request } from "@/lib/api/client";
import { Spinner } from "@/components/ui/spinner";
import { SettingsSelect } from "@/components/ui/form-input";

interface AuditEntry {
  id: string;
  user_id: string | null;
  action: string;
  resource_type: string;
  resource_id: string | null;
  details: Record<string, unknown>;
  created_at: string;
}

export default function AuditSettingsPage() {
  const t = useTranslations("settings.audit");
  const datePickerT = useTranslations("settings.datePicker");
  const [entries, setEntries] = useState<AuditEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [days, setDays] = useState(30);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const from = new Date(Date.now() - days * 86400000).toISOString();
      const to = new Date().toISOString();
      const data = await request<{ items: AuditEntry[] }>(
        `/audit?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      );
      setEntries(data.items);
    } catch {
      toast.error(t("loadError"));
    } finally {
      setLoading(false);
    }
  }, [days, t]);

  useEffect(() => {
    load();
  }, [load]);

  const formatAction = (action: string) =>
    action.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());

  return (
    <div>
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            {t("title")}
          </h1>
          <p className="mt-1 text-sm text-zinc-500 dark:text-muted-foreground">
            {t("description")}
          </p>
        </div>
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

      {loading ? (
        <div className="mt-12 flex justify-center">
          <Spinner />
        </div>
      ) : (
        <div className="mt-6 overflow-x-auto -mx-6 px-6" tabIndex={0} role="region" aria-label="Table data — scroll horizontally">
          <table className="w-full min-w-[640px] text-sm">
            <thead>
              <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-muted-foreground dark:border-zinc-700">
                <th className="py-3 pr-6">{t("column.action")}</th>
                <th className="py-3 pr-6">{t("column.resourceType")}</th>
                <th className="py-3 pr-6">{t("column.resourceId")}</th>
                <th className="py-3 pr-6">{t("column.user")}</th>
                <th className="py-3 pr-6 text-right">{t("column.date")}</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <tr
                  key={entry.id}
                  className="border-b border-zinc-100 dark:border-zinc-800"
                >
                  <td className="py-3 pr-6 font-medium text-zinc-900 dark:text-zinc-100">
                    {formatAction(entry.action)}
                  </td>
                  <td className="py-3 pr-6 text-muted-foreground">
                    {entry.resource_type}
                  </td>
                  <td className="py-3 pr-6 font-mono text-xs text-muted-foreground">
                    {entry.resource_id
                      ? entry.resource_id.length > 12
                        ? entry.resource_id.slice(0, 12) + "..."
                        : entry.resource_id
                      : "\u2014"}
                  </td>
                  <td className="py-3 pr-6 font-mono text-xs text-muted-foreground">
                    {entry.user_id
                      ? entry.user_id.length > 12
                        ? entry.user_id.slice(0, 12) + "..."
                        : entry.user_id
                      : t("systemUser")}
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
              {entries.length === 0 && (
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
    </div>
  );
}
