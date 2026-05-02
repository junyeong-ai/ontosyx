"use client";

import { type ReactNode } from "react";
import { cn } from "@/lib/cn";
import { NumberTicker } from "@/components/motion/number-ticker";
import type { StatusTone } from "@/components/ui/status-badge";

type KpiTone = Exclude<StatusTone, "concept"> | "neutral";

interface KpiCardProps {
  label: string;
  value: number;
  tone?: KpiTone;
  /** Period-over-period delta. Positive → success tone, negative → danger.
   *  Pass `null`/omit for no delta. */
  delta?: number | null;
  /** Custom value formatter (default: thousand-separator integer). */
  format?: (n: number) => string;
  /** Trailing accessory rendered after the value (icon, sparkline, etc.). */
  trailing?: ReactNode;
  /** Skip the count-up animation. Default false. */
  staticValue?: boolean;
  className?: string;
}

const toneSurface: Record<KpiTone, string> = {
  neutral: "border-divider bg-surface-base",
  brand:   "border-brand-border bg-brand-surface",
  success: "border-success-border bg-success-surface",
  warning: "border-warning-border bg-warning-surface",
  danger:  "border-danger-border bg-danger-surface",
  info:    "border-info-border bg-info-surface",
};

const toneValueColor: Record<KpiTone, string> = {
  neutral: "text-foreground-strong",
  brand:   "text-brand-foreground-strong",
  success: "text-success-foreground",
  warning: "text-warning-foreground",
  danger:  "text-danger-foreground",
  info:    "text-info-foreground",
};

export function KpiCard({
  label,
  value,
  tone = "neutral",
  delta,
  format,
  trailing,
  staticValue = false,
  className,
}: KpiCardProps) {
  return (
    <div
      className={cn(
        "rounded-lg border p-4 transition-shadow duration-[var(--duration-base)] hover:shadow-1",
        toneSurface[tone],
        className,
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-xs font-medium text-foreground-muted">
            {label}
          </div>
          <div
            className={cn(
              "mt-1 text-2xl font-semibold tabular-nums tracking-tight",
              toneValueColor[tone],
            )}
          >
            {staticValue ? (
              format ? format(value) : value.toLocaleString()
            ) : (
              <NumberTicker value={value} format={format} />
            )}
          </div>
          {delta !== undefined && delta !== null && (
            <div
              className={cn(
                "mt-1 inline-flex items-center text-2xs font-medium tabular-nums",
                delta > 0 && "text-success-foreground",
                delta < 0 && "text-danger-foreground",
                delta === 0 && "text-foreground-muted",
              )}
            >
              {delta > 0 ? "↑" : delta < 0 ? "↓" : "—"} {Math.abs(delta)}
            </div>
          )}
        </div>
        {trailing && <div className="shrink-0">{trailing}</div>}
      </div>
    </div>
  );
}
