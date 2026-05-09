"use client";

import { useTranslations } from "next-intl";
import type { QueryExecutionSummary } from "@/types/api";
import { Card } from "@/components/ui/card";
import { BarChart, Table } from "lucide-react";
import { useFormatters } from "@/hooks/use-formatters";
import { formatModelIdentity } from "@/lib/model-identity";

// ---------------------------------------------------------------------------
// ExecutionCard — list item for query executions
// ---------------------------------------------------------------------------

export interface ExecutionCardProps {
  item: QueryExecutionSummary;
  onClick: () => void;
}

export function ExecutionCard({ item, onClick }: ExecutionCardProps) {
  const t = useTranslations("workbench.chat.execution");
  const fmt = useFormatters();
  const timeStr = fmt.date(item.created_at, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });

  return (
    <Card
      padding="sm"
      interactive
      onClick={onClick}
      aria-label={t("viewAria", { question: item.question.slice(0, 60) })}
      className="w-full text-start"
    >
      <p className="line-clamp-2 text-sm font-medium text-foreground-strong">
        {item.question}
      </p>
      <div className="mt-1.5 flex items-center gap-3 text-xs text-foreground-muted">
        <span className="flex items-center gap-1">
          {item.has_widget ? (
            <BarChart className="h-3 w-3" />
          ) : (
            <Table className="h-3 w-3" />
          )}
          {t("rowsSummary", { count: item.row_count })}
        </span>
        <span>{item.execution_time_ms}ms</span>
        <span className="truncate font-mono">
          {formatModelIdentity(item.model_provider, item.model)}
        </span>
        <span className="ms-auto">{timeStr}</span>
      </div>
    </Card>
  );
}
