"use client";

import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";

export function NeighborhoodToolbar() {
  const t = useTranslations("workbench.canvas.neighborhood");
  const neighborhoodFocus = useAppStore((s) => s.neighborhoodFocus);
  const setNeighborhoodFocus = useAppStore((s) => s.setNeighborhoodFocus);

  if (!neighborhoodFocus) return null;

  const { nodeId, depth } = neighborhoodFocus;

  return (
    <div className="absolute top-3 left-1/2 z-10 flex -translate-x-1/2 items-center gap-1 rounded-lg border border-divider bg-surface-base px-2 py-1 shadow-md">
      <span className="mr-2 text-xs text-muted-foreground">{t("label")}</span>
      {([1, 2, 3] as const).map((d) => (
        <button
          key={d}
          onClick={() => setNeighborhoodFocus({ nodeId, depth: d })}
          className={`rounded px-2 py-0.5 text-xs font-medium transition-colors ${
            depth === d
              ? "bg-brand-surface-strong text-brand-foreground-strong-strong"
              : "text-muted-foreground hover:bg-surface-inset"
          }`}
        >
          {t("hop", { depth: d })}
        </button>
      ))}
      <button
        onClick={() => setNeighborhoodFocus(null)}
        className="ml-1 rounded px-2 py-0.5 text-xs font-medium text-muted-foreground hover:bg-surface-inset"
      >
        {t("all")}
      </button>
      <span className="ml-1 text-2xs text-muted-foreground">{t("escHint")}</span>
    </div>
  );
}
