"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";

import { Heading } from "@/components/ui/heading";
import { cn } from "@/lib/cn";
import type { components } from "@/types/api.generated";

type EvaluationActual = components["schemas"]["EvaluationActual"];
type RetrievedAnchor = components["schemas"]["EvaluationRetrievedAnchor"];

/**
 * Side-by-side hybrid vs trigram baseline view for a single
 * `retrieval_comparison` evaluation case.
 *
 * Top section pairs the four canonical IR axes (precision@k /
 * recall@k / MRR / NDCG@k); each row shows both legs' scores
 * inline with a lift annotation. Color tone keys off the lift
 * sign — green when hybrid wins, danger tone when trigram wins,
 * muted at parity. Lift is computed at render time rather than
 * persisted so a re-run that changes one leg never leaves the
 * stored lift stale.
 *
 * Bottom section renders both legs' ranked top-K side-by-side
 * with a hit indicator dot keyed off the case's `expected_ids`.
 * The dot lights green when the row is in the gold-standard set,
 * letting operators eyeball "did hybrid promote a relevant row
 * the trigram path missed" without manual id matching.
 *
 * This component renders nothing for non-comparison `actual`
 * shapes — the standard metrics list at the case detail handles
 * those. The caller is expected to inline this view above the
 * generic metrics surface so the comparison reads first.
 */
export interface RetrievalComparisonViewProps {
  actual: EvaluationActual;
  expectedIds: readonly string[];
}

export function RetrievalComparisonView({
  actual,
  expectedIds,
}: RetrievalComparisonViewProps) {
  const t = useTranslations("settings.evaluation.detail.retrievalComparison");
  const expectedSet = useMemo(() => new Set(expectedIds), [expectedIds]);
  if (actual.kind !== "retrieval_comparison") return null;

  const axes = [
    {
      key: "precision_at_k" as const,
      label: t("axis.precisionAtK"),
      hybrid: actual.hybrid.metrics.precision_at_k,
      trigram: actual.trigram.metrics.precision_at_k,
    },
    {
      key: "recall_at_k" as const,
      label: t("axis.recallAtK"),
      hybrid: actual.hybrid.metrics.recall_at_k,
      trigram: actual.trigram.metrics.recall_at_k,
    },
    {
      key: "mrr" as const,
      label: t("axis.mrr"),
      hybrid: actual.hybrid.metrics.mrr,
      trigram: actual.trigram.metrics.mrr,
    },
    {
      key: "ndcg_at_k" as const,
      label: t("axis.ndcgAtK"),
      hybrid: actual.hybrid.metrics.ndcg_at_k,
      trigram: actual.trigram.metrics.ndcg_at_k,
    },
  ];

  return (
    <section className="rounded-xl border border-divider bg-surface-base p-4">
      <header className="mb-3 flex items-baseline justify-between gap-3">
        <Heading level={2} size={5}>
          {t("title")}
        </Heading>
        <span className="text-2xs text-foreground-muted">
          {t("surfaceTag", { surface: actual.surface })}
        </span>
      </header>
      <p className="mb-3 text-xs text-foreground-muted">
        {t("description", { k: actual.hybrid.metrics.k })}
      </p>

      <div className="grid grid-cols-[1fr_auto_auto_auto] items-baseline gap-x-4 gap-y-1.5 text-sm">
        <span className="text-2xs font-medium uppercase tracking-wide text-foreground-muted">
          {t("axisHeader")}
        </span>
        <span className="text-2xs font-medium uppercase tracking-wide text-foreground-muted">
          {t("hybridHeader")}
        </span>
        <span className="text-2xs font-medium uppercase tracking-wide text-foreground-muted">
          {t("trigramHeader")}
        </span>
        <span className="text-2xs font-medium uppercase tracking-wide text-foreground-muted">
          {t("liftHeader")}
        </span>
        {axes.map(({ key, ...row }) => (
          <AxisRow key={key} {...row} />
        ))}
      </div>

      <div className="mt-5 grid grid-cols-1 gap-4 md:grid-cols-2">
        <RankedColumn
          heading={t("hybridHeader")}
          hits={actual.hybrid.hits}
          expectedSet={expectedSet}
          accent="brand"
        />
        <RankedColumn
          heading={t("trigramHeader")}
          hits={actual.trigram.hits}
          expectedSet={expectedSet}
          accent="muted"
        />
      </div>
    </section>
  );
}

interface AxisRowProps {
  label: string;
  hybrid: number;
  trigram: number;
}

function AxisRow({ label, hybrid, trigram }: AxisRowProps) {
  const lift = hybrid - trigram;
  // Use `Number.EPSILON`-style tolerance so `0.5 vs 0.5` doesn't
  // flicker color due to f64 round-trip noise; anything inside
  // 1e-6 reads as parity.
  const tone = lift > 1e-6 ? "win" : lift < -1e-6 ? "loss" : "parity";
  return (
    <>
      <span className="font-medium text-foreground-strong">{label}</span>
      <span className="tabular-nums text-foreground-strong">
        {hybrid.toFixed(3)}
      </span>
      <span className="tabular-nums text-foreground-muted">
        {trigram.toFixed(3)}
      </span>
      <span
        className={cn(
          "tabular-nums font-medium",
          tone === "win" && "text-success-foreground",
          tone === "loss" && "text-danger-foreground",
          tone === "parity" && "text-foreground-muted",
        )}
        aria-label={`lift ${lift.toFixed(3)}`}
      >
        {tone === "parity" ? "±0.000" : (lift > 0 ? "+" : "") + lift.toFixed(3)}
      </span>
    </>
  );
}

interface RankedColumnProps {
  heading: string;
  hits: readonly RetrievedAnchor[];
  expectedSet: ReadonlySet<string>;
  accent: "brand" | "muted";
}

function RankedColumn({
  heading,
  hits,
  expectedSet,
  accent,
}: RankedColumnProps) {
  const t = useTranslations("settings.evaluation.detail.retrievalComparison");
  return (
    <div>
      <Heading level={3} size={6}>
        {heading}
      </Heading>
      {hits.length === 0 ? (
        <p className="mt-2 text-xs text-foreground-muted">{t("emptyLeg")}</p>
      ) : (
        <ol className="mt-2 space-y-1.5">
          {hits.map((hit, i) => {
            const isHit = expectedSet.has(hit.logical_id);
            return (
              <li
                key={`${hit.entity_kind}:${hit.logical_id}:${i}`}
                className="flex items-baseline gap-2 rounded-md bg-surface-inset px-2 py-1.5"
              >
                <span
                  aria-hidden
                  className={cn(
                    "inline-block h-2 w-2 shrink-0 rounded-full",
                    isHit
                      ? "bg-success-foreground"
                      : accent === "brand"
                        ? "bg-brand-foreground/40"
                        : "bg-foreground-muted/40",
                  )}
                />
                <span className="shrink-0 text-2xs tabular-nums text-foreground-muted">
                  {i + 1}
                </span>
                <span className="min-w-0 flex-1 truncate text-xs text-foreground">
                  {hit.doc.length > 0 ? hit.doc : hit.logical_id}
                </span>
                {isHit ? (
                  <span className="shrink-0 text-2xs font-medium text-success-foreground">
                    {t("goldenBadge")}
                  </span>
                ) : null}
              </li>
            );
          })}
        </ol>
      )}
    </div>
  );
}
