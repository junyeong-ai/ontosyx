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
    <div className="absolute top-3 start-1/2 z-canvas flex -translate-x-1/2 items-center gap-1 rounded-lg border border-divider bg-surface-base px-2 py-1 shadow-2">
      <span className="me-2 text-xs text-foreground-muted">{t("label")}</span>
      {([1, 2, 3] as const).map((d) => (
        <button type="button"
          key={d}
          onClick={() => setNeighborhoodFocus({ nodeId, depth: d })}
          className={`rounded px-2 py-0.5 text-xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
            depth === d
              ? "bg-brand-surface-strong text-brand-foreground-strong"
              : "text-foreground-muted hover:bg-surface-inset"
          }`}
        >
          {t("hop", { depth: d })}
        </button>
      ))}
      <button type="button"
        onClick={() => setNeighborhoodFocus(null)}
        className="ms-1 rounded px-2 py-0.5 text-xs font-medium text-foreground-muted hover:bg-surface-inset"
      >
        {t("all")}
      </button>
      <span className="ms-1 text-2xs text-foreground-muted">{t("escHint")}</span>
    </div>
  );
}
