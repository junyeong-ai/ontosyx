"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { request } from "@/lib/api/client";
import { Spinner } from "@/components/ui/spinner";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface LineageSummary {
  graph_label: string;
  source_count: number;
  total_records: number;
  last_loaded_at: string | null;
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function LineageSettingsPage() {
  const [lineage, setLineage] = useState<LineageSummary[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    try {
      const data = await request<LineageSummary[]>("/lineage");
      setLineage(data);
    } catch {
      toast.error("Failed to load lineage data");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const totalRecords = lineage.reduce((acc, l) => acc + l.total_records, 0);
  const totalSources = lineage.reduce((acc, l) => acc + l.source_count, 0);
  const totalLabels = lineage.length;

  const formatNumber = (n: number) => {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
    return n.toLocaleString();
  };

  const formatDate = (iso: string | null) => {
    if (!iso) return "-";
    const d = new Date(iso);
    return d.toLocaleString();
  };

  return (
    <div>
      <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
        Data Lineage
      </h1>
      <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
        Graph label to source table mapping and load history.
      </p>

      {loading ? (
        <Spinner />
      ) : (
        <>
          {/* Summary cards */}
          <div className="mt-6 grid grid-cols-3 gap-4">
            <div className="rounded-lg border border-zinc-200 p-4 dark:border-zinc-700">
              <div className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
                {totalLabels}
              </div>
              <div className="text-xs text-zinc-500">Graph Labels</div>
            </div>
            <div className="rounded-lg border border-zinc-200 p-4 dark:border-zinc-700">
              <div className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
                {totalSources}
              </div>
              <div className="text-xs text-zinc-500">Source Tables</div>
            </div>
            <div className="rounded-lg border border-zinc-200 p-4 dark:border-zinc-700">
              <div className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
                {formatNumber(totalRecords)}
              </div>
              <div className="text-xs text-zinc-500">Total Records</div>
            </div>
          </div>

          {/* Lineage table */}
          <div className="mt-6">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-zinc-500 dark:border-zinc-700">
                  <th className="py-2">Graph Label</th>
                  <th className="py-2 text-right">Source Tables</th>
                  <th className="py-2 text-right">Total Records</th>
                  <th className="py-2 text-right">Last Loaded</th>
                </tr>
              </thead>
              <tbody>
                {lineage.map((l) => (
                  <tr
                    key={l.graph_label}
                    className="border-b border-zinc-100 dark:border-zinc-800"
                  >
                    <td className="py-2 font-medium text-zinc-900 dark:text-zinc-100">
                      {l.graph_label}
                    </td>
                    <td className="py-2 text-right text-zinc-500">
                      {l.source_count}
                    </td>
                    <td className="py-2 text-right text-zinc-500">
                      {formatNumber(l.total_records)}
                    </td>
                    <td className="py-2 text-right text-zinc-500">
                      {formatDate(l.last_loaded_at)}
                    </td>
                  </tr>
                ))}
                {lineage.length === 0 && (
                  <tr>
                    <td colSpan={4} className="py-8 text-center text-zinc-400">
                      No lineage data available
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  );
}
