"use client";

import type { QueryExecutionSummary } from "@/types/api";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ChartColumnIcon,
  Table01Icon,
} from "@hugeicons/core-free-icons";

// ---------------------------------------------------------------------------
// ExecutionCard — list item for query executions
// ---------------------------------------------------------------------------

export interface ExecutionCardProps {
  item: QueryExecutionSummary;
  onClick: () => void;
}

export function ExecutionCard({ item, onClick }: ExecutionCardProps) {
  const date = new Date(item.created_at);
  const timeStr = date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });

  return (
    <button
      onClick={onClick}
      aria-label={`View execution: ${item.question.slice(0, 60)}`}
      className="w-full rounded-lg border border-zinc-200 bg-white p-3 text-left transition-colors hover:border-zinc-300 hover:bg-zinc-50 dark:border-zinc-800 dark:bg-zinc-900 dark:hover:border-zinc-700 dark:hover:bg-zinc-800/80"
    >
      <p className="text-sm font-medium text-zinc-800 dark:text-zinc-200 line-clamp-2">
        {item.question}
      </p>
      <div className="mt-1.5 flex items-center gap-3 text-xs text-zinc-400">
        <span className="flex items-center gap-1">
          {item.has_widget ? (
            <HugeiconsIcon icon={ChartColumnIcon} className="h-3 w-3" size="100%" />
          ) : (
            <HugeiconsIcon icon={Table01Icon} className="h-3 w-3" size="100%" />
          )}
          {item.row_count} rows
        </span>
        <span>{item.execution_time_ms}ms</span>
        <span className="ml-auto">{timeStr}</span>
      </div>
    </button>
  );
}
