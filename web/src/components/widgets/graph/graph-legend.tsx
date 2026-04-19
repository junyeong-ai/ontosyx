"use client";

import { memo } from "react";
import { useTranslations } from "next-intl";

// ---------------------------------------------------------------------------
// Legend — type-color mapping + click-to-toggle visibility
// ---------------------------------------------------------------------------
//
// Each legend chip doubles as a toggle: clicking hides every node of that
// type (plus any edges whose endpoints are hidden). Hidden chips render
// muted so the user can always restore a type they didn't mean to drop.

interface LegendProps {
  typeColorIndex: Map<string, string>;
  /** Types the user has hidden. When empty, nothing is dimmed. */
  hiddenTypes?: ReadonlySet<string>;
  /** Called when a chip is clicked. Omit to render a static legend. */
  onToggleType?: (type: string) => void;
}

export const Legend = memo(function Legend({
  typeColorIndex,
  hiddenTypes,
  onToggleType,
}: LegendProps) {
  const t = useTranslations("widget.graph");
  if (typeColorIndex.size <= 1) return null;
  const entries = Array.from(typeColorIndex.entries());
  if (entries.length > 12) return null; // too many types, skip legend

  const interactive = !!onToggleType;

  return (
    <div className="absolute bottom-2 left-2 z-10 flex flex-wrap gap-x-3 gap-y-1 rounded-md bg-white/90 px-2 py-1.5 text-[10px] shadow-sm backdrop-blur dark:bg-zinc-800/90">
      {entries.map(([type, color]) => {
        const isHidden = hiddenTypes?.has(type) ?? false;
        const chip = (
          <span
            className={`inline-block h-2.5 w-2.5 rounded-full transition-opacity ${
              isHidden ? "opacity-30" : "opacity-100"
            }`}
            style={{ backgroundColor: color }}
          />
        );
        const label = (
          <span
            className={`transition-colors ${
              isHidden
                ? "text-muted-foreground line-through dark:text-zinc-500"
                : "text-zinc-600 dark:text-zinc-300"
            }`}
          >
            {type}
          </span>
        );
        if (!interactive) {
          return (
            <div key={type} className="flex items-center gap-1">
              {chip}
              {label}
            </div>
          );
        }
        return (
          <button
            key={type}
            type="button"
            onClick={() => onToggleType?.(type)}
            aria-pressed={!isHidden}
            aria-label={isHidden ? t("legendShowAria", { type }) : t("legendHideAria", { type })}
            className="flex cursor-pointer items-center gap-1 rounded px-0.5 transition-colors hover:bg-zinc-100 dark:hover:bg-zinc-700"
          >
            {chip}
            {label}
          </button>
        );
      })}
    </div>
  );
});
