"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { request } from "@/lib/api/client";
import { Spinner } from "@/components/ui/spinner";
import { SettingsSelect } from "@/components/ui/form-input";

interface UsageSummary {
  resource_type: string;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
  request_count: number;
}

export default function UsageSettingsPage() {
  const [usage, setUsage] = useState<UsageSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [days, setDays] = useState(30);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const from = new Date(Date.now() - days * 86400000).toISOString();
      const to = new Date().toISOString();
      const data = await request<UsageSummary[]>(
        `/usage?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      );
      setUsage(data);
    } catch {
      toast.error("Failed to load usage data");
    } finally {
      setLoading(false);
    }
  }, [days]);

  useEffect(() => { load(); }, [load]);

  const totalTokens = usage.reduce(
    (acc, u) => acc + u.total_input_tokens + u.total_output_tokens,
    0,
  );
  const totalCost = usage.reduce((acc, u) => acc + u.total_cost_usd, 0);
  const totalRequests = usage.reduce((acc, u) => acc + u.request_count, 0);

  const formatTokens = (n: number) => {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
    return n.toString();
  };

  return (
    <div>
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            Usage & Metering
          </h1>
          <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
            LLM token usage, compute costs, and request volumes.
          </p>
        </div>
        <SettingsSelect
          value={days}
          onChange={(e) => setDays(Number(e.target.value))}
        >
          <option value={7}>Last 7 days</option>
          <option value={30}>Last 30 days</option>
          <option value={90}>Last 90 days</option>
        </SettingsSelect>
      </div>

      {loading ? (
        <Spinner />
      ) : (
        <>
          {/* Summary cards */}
          <div className="mt-6 grid grid-cols-3 gap-4">
            <div className="rounded-lg border border-zinc-200 p-4 dark:border-zinc-700">
              <div className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
                {formatTokens(totalTokens)}
              </div>
              <div className="text-xs text-zinc-500">Total Tokens</div>
            </div>
            <div className="rounded-lg border border-zinc-200 p-4 dark:border-zinc-700">
              <div className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
                {totalRequests.toLocaleString()}
              </div>
              <div className="text-xs text-zinc-500">Requests</div>
            </div>
            <div className="rounded-lg border border-zinc-200 p-4 dark:border-zinc-700">
              <div className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
                ${totalCost.toFixed(4)}
              </div>
              <div className="text-xs text-zinc-500">Estimated Cost</div>
            </div>
          </div>

          {/* Breakdown table */}
          <div className="mt-6">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-zinc-500 dark:border-zinc-700">
                  <th className="py-3 pr-6">Resource Type</th>
                  <th className="py-3 pr-6 text-right">Input Tokens</th>
                  <th className="py-3 pr-6 text-right">Output Tokens</th>
                  <th className="py-3 pr-6 text-right">Requests</th>
                  <th className="py-3 pr-6 text-right">Cost</th>
                </tr>
              </thead>
              <tbody>
                {usage.map((u) => (
                  <tr key={u.resource_type} className="border-b border-zinc-100 dark:border-zinc-800">
                    <td className="py-3 pr-6 font-medium text-zinc-900 dark:text-zinc-100">
                      {u.resource_type}
                    </td>
                    <td className="py-3 pr-6 text-right text-zinc-500">
                      {formatTokens(u.total_input_tokens)}
                    </td>
                    <td className="py-3 pr-6 text-right text-zinc-500">
                      {formatTokens(u.total_output_tokens)}
                    </td>
                    <td className="py-3 pr-6 text-right text-zinc-500">
                      {u.request_count.toLocaleString()}
                    </td>
                    <td className="py-3 pr-6 text-right text-zinc-500">
                      ${u.total_cost_usd.toFixed(4)}
                    </td>
                  </tr>
                ))}
                {usage.length === 0 && (
                  <tr>
                    <td colSpan={5} className="py-8 text-center text-zinc-400">
                      No usage data for the selected period
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
