"use client";

import { useTranslations } from "next-intl";
import type { AnalysisRecipe, RecipeStatus } from "@/types/api";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge";

// ---------------------------------------------------------------------------
// Algorithm type → tone mapping for the algorithm chip.
// ---------------------------------------------------------------------------

const ALGO_TONE: Record<string, StatusTone> = {
  time_series: "info",
  segmentation: "concept",
  statistical_analysis: "success",
  anomaly_detection: "danger",
  classification: "concept",
  regression: "warning",
  custom: "neutral",
};

const STATUS_TONE: Record<RecipeStatus, StatusTone> = {
  draft: "warning",
  approved: "success",
  deprecated: "neutral",
};

// ---------------------------------------------------------------------------
// Algorithm type icons (simple SVG path data)
// ---------------------------------------------------------------------------

const ALGO_ICON: Record<string, string> = {
  time_series: "M3 12h4l3-9 4 18 3-9h4",
  segmentation: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm0 4v6l4 4",
  statistical_analysis: "M4 20h16M4 16h4v4H4zM10 12h4v8h-4zM16 8h4v12h-4z",
  anomaly_detection: "M12 2L2 22h20L12 2Zm0 7v5m0 3h.01",
  classification: "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z",
  regression: "M3 20L21 4M3 20h18M3 20V4",
  custom: "M12 6v6l4 2M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z",
};

interface ParamDef {
  type: string;
  default: unknown;
  description?: string;
}

interface RecipeCardProps {
  recipe: AnalysisRecipe;
  compact?: boolean;
  onRun?: (recipe: AnalysisRecipe) => void;
  actionLabel?: string;
}

export function RecipeCard({
  recipe,
  compact = false,
  onRun,
  actionLabel,
}: RecipeCardProps) {
  const t = useTranslations("settings.recipes.card");
  const params = Object.entries(
    (recipe.parameters ?? {}) as Record<string, ParamDef>,
  );
  const algoKey = recipe.algorithm_type;
  const iconPath = ALGO_ICON[algoKey] ?? ALGO_ICON.custom;
  const resolvedActionLabel = actionLabel ?? t("defaultAction");

  return (
    <Card padding="md" className="transition-shadow duration-[var(--duration-base)] ease-[var(--ease-out)] hover:shadow-2">
      <div className="flex items-start justify-between gap-2">
        <div className="flex items-center gap-2">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            strokeWidth={1.5}
            stroke="currentColor"
            className="h-4 w-4 shrink-0 text-foreground-muted"
           aria-hidden="true">
            <path strokeLinecap="round" strokeLinejoin="round" d={iconPath} />
          </svg>
          <h3 className="line-clamp-1 text-sm font-semibold text-foreground-strong">
            {recipe.name}
          </h3>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <StatusBadge tone={ALGO_TONE[algoKey] ?? ALGO_TONE.custom}>
            {algoKey.replace(/_/g, " ")}
          </StatusBadge>
          <StatusBadge
            tone={STATUS_TONE[recipe.status]}
            className="font-semibold uppercase tracking-wider"
          >
            {recipe.status}
          </StatusBadge>
        </div>
      </div>

      <p className="mt-1.5 line-clamp-2 text-xs text-foreground-muted">
        {recipe.description}
      </p>

      {!compact && params.length > 0 && (
        <div className="mt-3">
          <h4 className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("parameters")}
          </h4>
          <div className="mt-1 flex flex-wrap gap-1.5">
            {params.map(([name, def]) => (
              <span
                key={name}
                className="inline-flex items-center gap-1 rounded bg-surface-inset px-1.5 py-0.5 text-2xs text-foreground"
              >
                <span className="font-medium">{name}</span>
                <span className="text-foreground-muted">
                  {String(def.default)}
                </span>
              </span>
            ))}
          </div>
        </div>
      )}

      {onRun && (
        <Button
          variant="primary"
          size="sm"
          className="mt-3"
          onClick={(e) => {
            e.stopPropagation();
            onRun(recipe);
          }}
        >
          {resolvedActionLabel}
        </Button>
      )}
    </Card>
  );
}
