"use client";

import { useMemo } from "react";
import { cn } from "@/lib/cn";
import type { QueryResult, WidgetSpec } from "@/types/api";

interface TimelineWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

const DATE_PATTERNS = ["date", "timestamp", "time", "created", "updated", "occurred", "at"];
const LABEL_PATTERNS = ["event", "label", "name", "title", "action", "type"];
const DESC_PATTERNS = ["description", "desc", "detail", "details", "message", "note", "body"];

function findColumn(columns: string[], patterns: string[]): string | undefined {
  const lower = columns.map((c) => c.toLowerCase());
  for (const pat of patterns) {
    const idx = lower.findIndex((c) => c === pat || c.includes(pat));
    if (idx >= 0) return columns[idx];
  }
  return undefined;
}

function formatDate(value: unknown): string {
  if (value == null) return "\u2014";
  const d = new Date(String(value));
  if (isNaN(d.getTime())) return String(value);
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function TimelineWidget({ spec, data }: TimelineWidgetProps) {
  const { columns, rows } = data;

  const dateCol = useMemo(
    () => spec.x_axis?.field ?? findColumn(columns, DATE_PATTERNS) ?? columns[0],
    [spec, columns],
  );
  const labelCol = useMemo(
    () => spec.data_mapping?.label ?? findColumn(columns, LABEL_PATTERNS) ?? columns[1],
    [spec, columns],
  );
  const descCol = useMemo(
    () => findColumn(columns, DESC_PATTERNS),
    [columns],
  );

  const events = useMemo(() => {
    if (!dateCol || !labelCol) return [];
    return rows
      .map((row) => ({
        date: row[dateCol],
        label: String(row[labelCol] ?? ""),
        description: descCol ? String(row[descCol] ?? "") : undefined,
      }))
      .sort((a, b) => {
        const da = new Date(String(a.date ?? ""));
        const db = new Date(String(b.date ?? ""));
        return da.getTime() - db.getTime();
      });
  }, [rows, dateCol, labelCol, descCol]);

  if (!dateCol || !labelCol || events.length === 0) {
    return <p className="text-xs text-zinc-400">Need date and event columns for timeline</p>;
  }

  return (
    <div className="space-y-2">
      {spec.title && (
        <h4 className="text-xs font-semibold text-zinc-600 dark:text-zinc-400">
          {spec.title}
        </h4>
      )}
      <div className="max-h-80 overflow-y-auto pl-4">
        <div className="relative border-l-2 border-zinc-300 dark:border-zinc-600">
          {events.map((evt, i) => (
            <div key={i} className="relative mb-4 ml-4 last:mb-0">
              {/* Dot */}
              <div
                className={cn(
                  "absolute -left-[21px] top-1.5 h-2.5 w-2.5 rounded-full",
                  "border-2 border-emerald-500 bg-white dark:bg-zinc-900",
                )}
              />
              {/* Date */}
              <p className="text-[10px] font-medium text-zinc-400 dark:text-zinc-500">
                {formatDate(evt.date)}
              </p>
              {/* Label */}
              <p className="text-xs font-semibold text-zinc-800 dark:text-zinc-200">
                {evt.label}
              </p>
              {/* Description */}
              {evt.description && (
                <p className="mt-0.5 text-[11px] text-zinc-500 dark:text-zinc-400">
                  {evt.description}
                </p>
              )}
            </div>
          ))}
        </div>
      </div>
      <p className="text-[10px] text-zinc-400">{events.length} events</p>
    </div>
  );
}
