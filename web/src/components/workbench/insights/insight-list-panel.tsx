"use client";

// ---------------------------------------------------------------------------
// InsightListPanel — Dashboard-side list of saved insights.
//
// Lists every insight authored by the caller; click to open
// (delegated upward via `onOpen`), trash icon to delete. The
// "Save Insight" creation flow lives on the Analyze surface where
// the answer + provenance are already in scope.
//
// Filter strip surfaces the `concept_anchors` axis the BE supports:
// stable ConceptId pills derived from visible insights. Clicking a
// pill scopes the list to that business concept.
// ---------------------------------------------------------------------------

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { BarChart3, Trash2 } from "lucide-react";
import { toast } from "@/components/ui/toast";

import { Button } from "@/components/ui/button";
import { Eyebrow } from "@/components/ui/eyebrow";
import { Spinner } from "@/components/ui/spinner";
import { Tooltip } from "@/components/ui/tooltip";
import { useConfirm } from "@/components/providers/confirm-provider";
import { useDeleteInsight, useInsights } from "@/hooks/api/use-insights";
import { localizePresent } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";
import type { InsightDef } from "@/types/api";

interface Props {
  /// Called when the user clicks an insight card. The parent
  /// decides what to do (open in Analyze, render in dashboard,
  /// etc.) so this panel stays presentation-only.
  onOpen?: (insight: InsightDef) => void;
}

export function InsightListPanel({ onOpen }: Props) {
  const t = useTranslations("workbench.insights");
  const confirm = useConfirm();
  const localeChain = useLocaleChain();

  // Active concept-anchor filter — `null` means "no filter, show
  // everything". Toggling the same chip clears it. Single-select
  // keeps the UX simple; the BE accepts multi-value already, so
  // wider selectors slot in without an API change.
  const [activeAnchor, setActiveAnchor] = useState<string | null>(null);

  const insightsQuery = useInsights({
    me: true,
    limit: 50,
    conceptAnchors: activeAnchor ? [activeAnchor] : undefined,
  });
  // Side fetch (no filter) used to build the chip list — chip set
  // stays stable as the user filters down. TanStack dedupes on
  // queryKey so this is a separate cached query.
  const allInsights = useInsights({ me: true, limit: 50 });
  const deleteMutation = useDeleteInsight();

  const items = insightsQuery.data?.items ?? [];
  const loading = insightsQuery.isLoading;

  const conceptAnchorChips = useMemo(() => {
    const seen = new Set<string>();
    for (const insight of allInsights.data?.items ?? []) {
      for (const anchor of insight.concept_anchors) {
        if (anchor) seen.add(anchor);
      }
    }
    return Array.from(seen).sort();
  }, [allInsights.data]);

  const handleDelete = async (insight: InsightDef) => {
    const title =
      localizePresent(insight.question, localeChain) ?? insight.id;
    const ok = await confirm({
      title: t("deleteTitle", { question: title }),
      description: t("deleteDescription"),
      confirmLabel: t("deleteConfirm"),
      variant: "danger",
    });
    if (!ok) return;
    try {
      await deleteMutation.mutateAsync(insight.id);
      toast.success(t("deleteSuccess"));
    } catch (err) {
      toast.error(
        err instanceof Error ? err.message : t("deleteFailed"),
      );
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-divider px-3 py-2">
        <BarChart3 className="h-3.5 w-3.5 text-brand-foreground" />
        <Eyebrow level={2} size="dense" tone="strong">
          {t("panelTitle")}
        </Eyebrow>
        <span className="ms-auto text-2xs text-foreground-muted">
          {t("countSummary", { count: items.length })}
        </span>
      </div>

      {conceptAnchorChips.length > 0 && (
        <div className="flex flex-wrap items-center gap-1 border-b border-divider-soft px-3 py-1.5">
          <span className="text-2xs uppercase tracking-wider text-foreground-muted">
            {t("filterByConcept")}
          </span>
          {conceptAnchorChips.map((anchor) => {
            const active = activeAnchor === anchor;
            return (
              <button type="button"
                key={anchor}
                onClick={() => setActiveAnchor(active ? null : anchor)}
                aria-pressed={active}
                className={
                  active
                    ? "rounded bg-brand-solid px-1.5 py-0.5 text-2xs font-medium text-foreground-onbrand"
                    : "rounded bg-surface-inset px-1.5 py-0.5 text-2xs text-foreground hover:bg-surface-inset"
                }
              >
                {anchor}
              </button>
            );
          })}
          {activeAnchor && (
            <button type="button"
              onClick={() => setActiveAnchor(null)}
              className="text-2xs text-foreground-muted hover:text-foreground-muted"
            >
              {t("filterClear")}
            </button>
          )}
        </div>
      )}

      {loading && (
        <div className="flex items-center justify-center py-8">
          <Spinner />
        </div>
      )}

      {!loading && items.length === 0 && (
        <p className="px-3 py-6 text-center text-xs text-foreground-muted">
          {t("emptyState")}
        </p>
      )}

      {!loading && items.length > 0 && (
        <ul className="flex-1 overflow-y-auto">
          {items.map((insight) => {
            const title =
              localizePresent(insight.question, localeChain) ?? insight.id;
            const desc = localizePresent(insight.description, localeChain);
            return (
              <li
                key={insight.id}
                className="border-b border-divider-soft last:border-b-0"
              >
                <div className="flex items-start gap-2 px-3 py-2">
                  <button type="button"
                    onClick={() => onOpen?.(insight)}
                    className="flex-1 text-start"
                  >
                    <p className="truncate text-xs font-medium text-foreground-strong">
                      {title}
                    </p>
                    {desc && (
                      <p className="mt-0.5 line-clamp-2 text-2xs text-foreground-muted">
                        {desc}
                      </p>
                    )}
                    {(insight.concept_anchors.length > 0 ||
                      insight.tags.length > 0) && (
                      <div className="mt-1 flex flex-wrap gap-1">
                        {insight.concept_anchors.map((anchor) => (
                          <span
                            key={`anchor-${anchor}`}
                            className="rounded bg-brand-surface-strong px-1 text-2xs font-medium text-brand-foreground-strong"
                            title={t("conceptAnchorTooltip")}
                          >
                            {anchor}
                          </span>
                        ))}
                        {insight.tags.map((tag) => (
                          <span
                            key={`tag-${tag}`}
                            className="rounded bg-surface-inset px-1 text-2xs text-foreground-subtle"
                          >
                            {tag}
                          </span>
                        ))}
                      </div>
                    )}
                  </button>
                  <Tooltip content={t("deleteTooltip")}>
                    <Button
                      size="xs"
                      variant="ghost"
                      onClick={() => handleDelete(insight)}
                      disabled={deleteMutation.isPending}
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  </Tooltip>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
