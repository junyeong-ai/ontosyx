"use client";

import type { AnalysisRecipe, RecipeStatus } from "@/types/api";

// ---------------------------------------------------------------------------
// Algorithm type badge colors
// ---------------------------------------------------------------------------

const ALGO_BADGE: Record<string, string> = {
  time_series:
    "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
  segmentation:
    "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400",
  statistical_analysis:
    "bg-cyan-100 text-cyan-700 dark:bg-cyan-900/30 dark:text-cyan-400",
  anomaly_detection:
    "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
  classification:
    "bg-indigo-100 text-indigo-700 dark:bg-indigo-900/30 dark:text-indigo-400",
  regression:
    "bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-400",
  custom:
    "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400",
};

const STATUS_BADGE: Record<RecipeStatus, string> = {
  draft:
    "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
  approved:
    "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
  deprecated:
    "bg-zinc-200 text-zinc-500 dark:bg-zinc-700 dark:text-zinc-400",
};

// ---------------------------------------------------------------------------
// Algorithm type icons (simple SVG)
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

// ---------------------------------------------------------------------------
// RecipeCard
// ---------------------------------------------------------------------------

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
  actionLabel = "Run Analysis",
}: RecipeCardProps) {
  const params = Object.entries(
    (recipe.parameters ?? {}) as Record<string, ParamDef>,
  );
  const algoKey = recipe.algorithm_type;
  const algoBadge = ALGO_BADGE[algoKey] ?? ALGO_BADGE.custom;
  const iconPath = ALGO_ICON[algoKey] ?? ALGO_ICON.custom;

  return (
    <div className="rounded-lg border border-zinc-200 bg-white p-4 transition-shadow hover:shadow-md dark:border-zinc-700 dark:bg-zinc-900">
      {/* Header row */}
      <div className="flex items-start justify-between gap-2">
        <div className="flex items-center gap-2">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            strokeWidth={1.5}
            stroke="currentColor"
            className="h-4 w-4 shrink-0 text-zinc-400"
          >
            <path strokeLinecap="round" strokeLinejoin="round" d={iconPath} />
          </svg>
          <h3 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200 line-clamp-1">
            {recipe.name}
          </h3>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <span
            className={`rounded-full px-2 py-0.5 text-[10px] font-medium ${algoBadge}`}
          >
            {algoKey.replace(/_/g, " ")}
          </span>
          <span
            className={`rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ${STATUS_BADGE[recipe.status]}`}
          >
            {recipe.status}
          </span>
        </div>
      </div>

      {/* Description */}
      <p className="mt-1.5 text-xs text-zinc-500 dark:text-zinc-400 line-clamp-2">
        {recipe.description}
      </p>

      {/* Parameters (non-compact only) */}
      {!compact && params.length > 0 && (
        <div className="mt-3">
          <h4 className="text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
            Parameters
          </h4>
          <div className="mt-1 flex flex-wrap gap-1.5">
            {params.map(([name, def]) => (
              <span
                key={name}
                className="inline-flex items-center gap-1 rounded bg-zinc-100 px-1.5 py-0.5 text-[11px] text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400"
              >
                <span className="font-medium">{name}</span>
                <span className="text-zinc-400 dark:text-zinc-500">
                  {String(def.default)}
                </span>
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Action */}
      {onRun && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onRun(recipe);
          }}
          className="mt-3 rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700 transition-colors"
        >
          {actionLabel}
        </button>
      )}
    </div>
  );
}
