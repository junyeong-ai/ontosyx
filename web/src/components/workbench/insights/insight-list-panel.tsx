"use client";

// ---------------------------------------------------------------------------
// InsightListPanel — Dashboard-side list of saved insights.
//
// Lists every insight authored by the caller; click to open
// (delegated upward via `onOpen`), trash icon to delete. The
// "Save Insight" creation flow lives on the Analyze surface where
// the answer + provenance are already in scope.
//
// Filter strip surfaces the `concept_anchors` axis the BE supports
// — typed `GlossaryTermId` pills derived from the visible insights,
// click to scope the list to that anchor. Mirrors the 1-pager's
// "용어 사전이 다리" promise: the same business-concept handle that
// lives in the glossary lets users navigate insights by concept.
// ---------------------------------------------------------------------------

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { Delete01Icon, Analytics01Icon } from "@hugeicons/core-free-icons";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { Tooltip } from "@/components/ui/tooltip";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { useDeleteInsight, useInsights } from "@/hooks/api/use-insights";
import { localizePresent } from "@/lib/locale/localize";
import { useLocaleChain } from "@/lib/use-locale-chain";
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
      <div className="flex items-center gap-2 border-b border-zinc-200 px-3 py-2 dark:border-zinc-800">
        <HugeiconsIcon
          icon={Analytics01Icon}
          className="h-3.5 w-3.5 text-emerald-600"
          size="100%"
        />
        <h3 className="text-xs font-semibold uppercase tracking-wider text-zinc-700 dark:text-zinc-300">
          {t("panelTitle")}
        </h3>
        <span className="ml-auto text-[10px] text-muted-foreground">
          {t("countSummary", { count: items.length })}
        </span>
      </div>

      {conceptAnchorChips.length > 0 && (
        <div className="flex flex-wrap items-center gap-1 border-b border-zinc-100 px-3 py-1.5 dark:border-zinc-800">
          <span className="text-[10px] uppercase tracking-wider text-muted-foreground">
            {t("filterByConcept")}
          </span>
          {conceptAnchorChips.map((anchor) => {
            const active = activeAnchor === anchor;
            return (
              <button
                key={anchor}
                onClick={() => setActiveAnchor(active ? null : anchor)}
                aria-pressed={active}
                className={
                  active
                    ? "rounded bg-emerald-600 px-1.5 py-0.5 text-[10px] font-medium text-white"
                    : "rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] text-zinc-700 hover:bg-zinc-200 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-zinc-700"
                }
              >
                {anchor}
              </button>
            );
          })}
          {activeAnchor && (
            <button
              onClick={() => setActiveAnchor(null)}
              className="text-[10px] text-muted-foreground hover:text-zinc-700 dark:hover:text-zinc-300"
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
        <p className="px-3 py-6 text-center text-xs text-muted-foreground">
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
                className="border-b border-zinc-100 last:border-b-0 dark:border-zinc-800"
              >
                <div className="flex items-start gap-2 px-3 py-2">
                  <button
                    onClick={() => onOpen?.(insight)}
                    className="flex-1 text-left"
                  >
                    <p className="truncate text-xs font-medium text-zinc-900 dark:text-zinc-100">
                      {title}
                    </p>
                    {desc && (
                      <p className="mt-0.5 line-clamp-2 text-[10px] text-muted-foreground">
                        {desc}
                      </p>
                    )}
                    {(insight.concept_anchors.length > 0 ||
                      insight.tags.length > 0) && (
                      <div className="mt-1 flex flex-wrap gap-1">
                        {insight.concept_anchors.map((anchor) => (
                          <span
                            key={`anchor-${anchor}`}
                            className="rounded bg-emerald-100 px-1 text-[9px] font-medium text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-300"
                            title={t("conceptAnchorTooltip")}
                          >
                            {anchor}
                          </span>
                        ))}
                        {insight.tags.map((tag) => (
                          <span
                            key={`tag-${tag}`}
                            className="rounded bg-zinc-100 px-1 text-[9px] text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400"
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
                      <HugeiconsIcon
                        icon={Delete01Icon}
                        className="h-3 w-3"
                        size="100%"
                      />
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
