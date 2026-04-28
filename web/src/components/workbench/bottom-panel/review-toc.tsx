"use client";

import { useTranslations } from "next-intl";

import { cn } from "@/lib/cn";

/**
 * Compact, sticky table-of-contents strip rendered above the
 * analysis review. Each pill links to a section anchor inside the
 * review and surfaces the count of unresolved items in that
 * section, so the operator can both navigate and prioritise from a
 * single glance.
 *
 * The component is intentionally render-only — it never owns gate
 * state. The parent supplies counts, the TOC translates them into
 * UI. Adding a new review section means one entry here plus an
 * anchor id on the rendered section.
 */
export interface ReviewTOCEntry {
  /** DOM element id this pill jumps to. */
  anchor: string;
  /** i18n key under `review.toc.label`. */
  labelKey:
    | "warnings"
    | "relationships"
    | "exclusions"
    | "pii"
    | "clarifications";
  /** Total items in the section; pill hidden when zero. */
  total: number;
  /** Items still requiring operator action. */
  unresolved: number;
}

export function ReviewTOC({
  entries,
  className,
}: {
  entries: ReadonlyArray<ReviewTOCEntry>;
  className?: string;
}) {
  const t = useTranslations("workbench.bottomPanel.review.toc");
  const visible = entries.filter((entry) => entry.total > 0);
  if (visible.length === 0) return null;

  return (
    <nav
      aria-label={t("ariaLabel")}
      className={cn(
        "sticky top-0 z-10 -mx-2 flex flex-wrap items-center gap-1.5",
        "border-b border-zinc-200 bg-white/95 px-2 py-1.5 backdrop-blur",
        "dark:border-zinc-800 dark:bg-zinc-900/95",
        className,
      )}
    >
      {visible.map((entry) => (
        <button
          key={entry.anchor}
          type="button"
          onClick={() => focusAnchor(entry.anchor)}
          className={cn(
            "flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] font-medium transition-colors",
            entry.unresolved === 0
              ? "border-emerald-200 bg-emerald-50 text-emerald-700 hover:bg-emerald-100 dark:border-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-300"
              : "border-amber-200 bg-amber-50 text-amber-700 hover:bg-amber-100 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-300",
          )}
        >
          <span>{t(`label.${entry.labelKey}`)}</span>
          {entry.unresolved > 0 ? (
            <span className="rounded-full bg-amber-500/80 px-1 text-[9px] font-bold text-white">
              {entry.unresolved}
            </span>
          ) : (
            <span className="rounded-full bg-emerald-500/80 px-1 text-[9px] font-bold text-white">
              ✓
            </span>
          )}
        </button>
      ))}
    </nav>
  );
}

function focusAnchor(anchor: string): void {
  if (typeof document === "undefined") return;
  const el = document.getElementById(anchor);
  if (!el) return;
  el.scrollIntoView({ behavior: "smooth", block: "start" });
  el.classList.add("ring-2", "ring-emerald-300", "ring-offset-2");
  window.setTimeout(() => {
    el.classList.remove("ring-2", "ring-emerald-300", "ring-offset-2");
  }, 1000);
}
