"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { request } from "@/lib/api/client";
import { Spinner } from "@/components/ui/spinner";

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
      toast.error("Failed to load audit log");
    } finally {
      setLoading(false);
    }
  }, [days]);

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
            Audit Log
          </h1>
          <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
            Track user actions, resource changes, and system events.
          </p>
        </div>
        <select
          value={days}
          onChange={(e) => setDays(Number(e.target.value))}
          className="rounded-md border border-zinc-300 bg-white px-3 py-1.5 text-sm dark:border-zinc-700 dark:bg-zinc-900"
        >
          <option value={7}>Last 7 days</option>
          <option value={30}>Last 30 days</option>
          <option value={90}>Last 90 days</option>
        </select>
      </div>

      {loading ? (
        <div className="mt-12 flex justify-center">
          <Spinner />
        </div>
      ) : (
        <div className="mt-6">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-zinc-500 dark:border-zinc-700">
                <th className="py-2">Action</th>
                <th className="py-2">Resource Type</th>
                <th className="py-2">Resource ID</th>
                <th className="py-2">User</th>
                <th className="py-2 text-right">Date</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <tr
                  key={entry.id}
                  className="border-b border-zinc-100 dark:border-zinc-800"
                >
                  <td className="py-2 font-medium text-zinc-900 dark:text-zinc-100">
                    {formatAction(entry.action)}
                  </td>
                  <td className="py-2 text-zinc-500">
                    {entry.resource_type}
                  </td>
                  <td className="py-2 font-mono text-xs text-zinc-400">
                    {entry.resource_id
                      ? entry.resource_id.length > 12
                        ? entry.resource_id.slice(0, 12) + "..."
                        : entry.resource_id
                      : "\u2014"}
                  </td>
                  <td className="py-2 font-mono text-xs text-zinc-400">
                    {entry.user_id
                      ? entry.user_id.length > 12
                        ? entry.user_id.slice(0, 12) + "..."
                        : entry.user_id
                      : "system"}
                  </td>
                  <td className="py-2 text-right text-zinc-500">
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
                    className="py-8 text-center text-zinc-400"
                  >
                    No audit entries for the selected period
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
