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

export function ReviewToc({
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
        "sticky top-0 z-canvas -mx-2 flex flex-wrap items-center gap-1.5",
        "border-b border-divider bg-surface-base px-2 py-1.5 backdrop-blur",
        className,
      )}
    >
      {visible.map((entry) => (
        <button
          key={entry.anchor}
          type="button"
          onClick={() => focusAnchor(entry.anchor)}
          className={cn(
            "flex items-center gap-1 rounded-full border px-2 py-0.5 text-2xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
            entry.unresolved === 0
              ? "border-brand-border bg-brand-surface text-brand-foreground hover:bg-brand-surface-strong/40"
              : "border-warning-border bg-warning-surface text-warning-foreground hover:bg-warning-surface/40",
          )}
        >
          <span>{t(`label.${entry.labelKey}`)}</span>
          {entry.unresolved > 0 ? (
            <span className="rounded-full bg-warning-foreground px-1 text-2xs font-bold text-foreground-onbrand">
              {entry.unresolved}
            </span>
          ) : (
            <span className="rounded-full bg-brand-solid/80 px-1 text-2xs font-bold text-foreground-onbrand">
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
  el.classList.add("ring-2", "ring-brand-foreground", "ring-offset-2");
  window.setTimeout(() => {
    el.classList.remove("ring-2", "ring-brand-foreground", "ring-offset-2");
  }, 1000);
}
