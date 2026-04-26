"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { SettingsSelect } from "@/components/ui/form-input";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { cn } from "@/lib/cn";
import type { MetricWindow } from "@/lib/api/quality";
import type {
  MetricValue,
  QualityMetricsReport,
  ShaclFailureCount,
  ShaclFailureKind,
  StaleTypeEntry,
} from "@/types/api";
import {
  useQualityMetrics,
  useShaclFailures,
  useStaleTypes,
} from "@/hooks/api/use-quality";
import {
  useOntologies,
  useOntologyDetail,
} from "@/hooks/api/use-ontologies";
import { useApplyOntologyEdits } from "@/hooks/api/use-ontology-edits";
import type { OntologyEditOp } from "@/lib/api/edit-ops";

/**
 * Ontology-quality signal dashboard — the patent's "6 창" surface.
 *
 * Lives at `/settings/quality/signals` so the existing
 * `/settings/quality` (data-quality rule CRUD) stays untouched. This
 * page reads from `QualitySignalStore`: 6 tiles + SHACL failure
 * distribution + stale-type candidates.
 */
export default function QualitySignalsPage() {
  const t = useTranslations("settings.qualitySignals");
  const [windowChoice, setWindowChoice] = useState<MetricWindow>("7d");
  const [staleDays, setStaleDays] = useState(180);

  const metrics = useQualityMetrics(windowChoice);
  const failures = useShaclFailures(windowChoice);
  const stale = useStaleTypes(staleDays);

  return (
    <div className="flex flex-col gap-6">
      <header className="flex items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            {t("title")}
          </h1>
          <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">
            {t("subtitle")}
          </p>
        </div>
        <SettingsSelect
          label={t("window.label")}
          hideLabel
          value={windowChoice}
          onChange={(e) => setWindowChoice(e.target.value as MetricWindow)}
        >
          <option value="7d">{t("window.last7d")}</option>
          <option value="30d">{t("window.last30d")}</option>
          <option value="90d">{t("window.last90d")}</option>
        </SettingsSelect>
      </header>

      <MetricsGrid report={metrics.data} loading={metrics.isLoading} />

      <section>
        <h2 className="mb-3 text-sm font-semibold text-zinc-800 dark:text-zinc-200">
          {t("shacl.title")}
        </h2>
        <ShaclFailureBars
          rows={failures.data ?? []}
          loading={failures.isLoading}
        />
      </section>

      <section>
        <div className="mb-3 flex items-end justify-between gap-4">
          <div>
            <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">
              {t("stale.title")}
            </h2>
            <p className="mt-0.5 text-xs text-zinc-500 dark:text-zinc-400">
              {t("stale.subtitle", { days: staleDays })}
            </p>
          </div>
          <SettingsSelect
            label={t("stale.thresholdLabel")}
            hideLabel
            value={staleDays}
            onChange={(e) => setStaleDays(Number(e.target.value))}
          >
            <option value={90}>{t("stale.days90")}</option>
            <option value={180}>{t("stale.days180")}</option>
            <option value={365}>{t("stale.days365")}</option>
          </SettingsSelect>
        </div>
        <StaleTypesTable rows={stale.data ?? []} loading={stale.isLoading} />
      </section>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 6-tile metrics grid
// ---------------------------------------------------------------------------

function MetricsGrid({
  report,
  loading,
}: {
  report: QualityMetricsReport | undefined;
  loading: boolean;
}) {
  const t = useTranslations("settings.qualitySignals");

  if (loading) {
    return (
      <div className="rounded-lg border border-zinc-200 bg-white p-8 text-center dark:border-zinc-800 dark:bg-zinc-950">
        <Spinner />
      </div>
    );
  }

  const tiles = [
    { key: "anchor_match_rate", value: report?.anchor_match_rate },
    { key: "glossary_hit_rate", value: report?.glossary_hit_rate },
    {
      key: "clarification_success_rate",
      value: report?.clarification_success_rate,
    },
    { key: "query_reproducibility", value: report?.query_reproducibility },
    { key: "shacl_pass_rate", value: report?.shacl_pass_rate },
    { key: "stale_concept_ratio", value: report?.stale_concept_ratio },
  ] as const;

  return (
    <section>
      <div className="mb-3 flex items-baseline justify-between">
        <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">
          {t("tiles.title")}
        </h2>
        {report && (
          <span className="text-[11px] text-zinc-500 dark:text-zinc-400">
            {t("tiles.sampleSize", { n: report.sample_size })}
          </span>
        )}
      </div>
      <div className="grid grid-cols-1 gap-3 md:grid-cols-2 lg:grid-cols-3">
        {tiles.map(({ key, value }) => (
          <MetricTile
            key={key}
            label={t(`tiles.${key}.label`)}
            description={t(`tiles.${key}.description`)}
            metric={value}
          />
        ))}
      </div>
    </section>
  );
}

function MetricTile({
  label,
  description,
  metric,
}: {
  label: string;
  description: string;
  metric: MetricValue | undefined;
}) {
  const hasData = !!metric && (metric.upper_bound_95 > 0 || metric.value > 0);
  const pct = hasData ? Math.round(metric!.value * 1000) / 10 : null;
  const trend = metric?.trend_delta ?? 0;
  const band = metric ? metric.upper_bound_95 - metric.lower_bound_95 : 0;
  // Wide Wilson CI flags small samples the UI shouldn't read as decisive.
  // Threshold 0.3 = a ±15-point band on the value estimate.
  const bandWide = hasData && band > 0.3;

  return (
    <article className="rounded-lg border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
      <header className="mb-2 flex items-start justify-between gap-2">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-zinc-500 dark:text-zinc-400">
          {label}
        </h3>
        <TrendBadge delta={trend} />
      </header>
      <div className="flex items-baseline gap-2">
        <span
          className={cn(
            "text-2xl font-semibold tabular-nums",
            hasData
              ? "text-zinc-900 dark:text-zinc-100"
              : "text-zinc-400 dark:text-zinc-600",
          )}
        >
          {pct !== null ? `${pct.toFixed(1)}%` : "—"}
        </span>
        {hasData && (
          <span className="text-[11px] text-zinc-500 dark:text-zinc-400">
            {`[${(metric!.lower_bound_95 * 100).toFixed(1)}%, ${(
              metric!.upper_bound_95 * 100
            ).toFixed(1)}%]`}
          </span>
        )}
      </div>
      {bandWide && (
        <p className="mt-1 text-[10px] italic text-amber-600 dark:text-amber-400">
          wide confidence band — limited samples
        </p>
      )}
      <p className="mt-2 text-[11px] leading-snug text-zinc-500 dark:text-zinc-400">
        {description}
      </p>
    </article>
  );
}

function TrendBadge({ delta }: { delta: number }) {
  // Dead-band below ±0.5pp renders flat so noise doesn't flip arrows.
  if (Math.abs(delta) < 0.005) {
    return (
      <span className="rounded px-1.5 py-0.5 text-[10px] font-medium text-zinc-500 dark:text-zinc-400">
        = flat
      </span>
    );
  }
  const up = delta > 0;
  return (
    <span
      className={cn(
        "rounded px-1.5 py-0.5 text-[10px] font-medium tabular-nums",
        up
          ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-400"
          : "bg-rose-50 text-rose-700 dark:bg-rose-950/40 dark:text-rose-400",
      )}
    >
      {up ? "▲" : "▼"} {(Math.abs(delta) * 100).toFixed(1)} pp
    </span>
  );
}

// ---------------------------------------------------------------------------
// SHACL failure distribution
// ---------------------------------------------------------------------------

function ShaclFailureBars({
  rows,
  loading,
}: {
  rows: ShaclFailureCount[];
  loading: boolean;
}) {
  const t = useTranslations("settings.qualitySignals");

  if (loading) {
    return (
      <div className="rounded-lg border border-zinc-200 bg-white p-8 text-center dark:border-zinc-800 dark:bg-zinc-950">
        <Spinner />
      </div>
    );
  }
  if (!rows.length) {
    return (
      <div className="rounded-lg border border-zinc-200 bg-white p-6 text-center text-xs text-zinc-500 dark:border-zinc-800 dark:bg-zinc-950 dark:text-zinc-400">
        {t("shacl.empty")}
      </div>
    );
  }

  const total = rows.reduce((sum, r) => sum + r.count, 0);
  return (
    <div className="space-y-2 rounded-lg border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
      {rows.map((r) => {
        const pct = total === 0 ? 0 : Math.round((r.count / total) * 1000) / 10;
        return (
          <div
            key={r.kind}
            className="grid grid-cols-[1fr_auto] items-center gap-3"
          >
            <div>
              <div className="flex items-baseline justify-between">
                <span className="text-xs font-medium text-zinc-800 dark:text-zinc-200">
                  {t(`shacl.kind.${r.kind}`)}
                </span>
                <span className="text-[11px] tabular-nums text-zinc-500 dark:text-zinc-400">
                  {r.count} · {pct.toFixed(1)}%
                </span>
              </div>
              <div className="mt-1 h-2 overflow-hidden rounded bg-zinc-100 dark:bg-zinc-800">
                <div
                  className={cn("h-full", shaclFailureBarTone(r.kind))}
                  style={{ width: `${pct}%` }}
                />
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}

/**
 * Colour ramp per failure kind — fixed so a returning operator
 * recognises the shape of the distribution without reading labels.
 */
function shaclFailureBarTone(kind: ShaclFailureKind): string {
  switch (kind) {
    case "cardinality_violation":
      return "bg-rose-500";
    case "measure_group_by":
      return "bg-amber-500";
    case "unknown_coded_value":
      return "bg-violet-500";
    case "mandatory_property_missing":
      return "bg-sky-500";
    case "temporal_grain_mismatch":
      return "bg-emerald-500";
    case "other":
      return "bg-zinc-400";
  }
}

// ---------------------------------------------------------------------------
// Stale types table
// ---------------------------------------------------------------------------

function StaleTypesTable({
  rows,
  loading,
}: {
  rows: StaleTypeEntry[];
  loading: boolean;
}) {
  const t = useTranslations("settings.qualitySignals");
  const data = useMemo(() => rows, [rows]);

  // Φ5 #4 — Stale-jump: clicking the deprecate button on a row
  // drafts a `DeprecateNodeType` / `DeprecateEdgeType` op and
  // submits it through the standard edit pipeline, which routes
  // to the approval queue per the change_routing matrix. The
  // reviewer then sees the proposal as a queued approval with
  // payload preview (Φ5 #2). No bypass; the routing rules decide
  // whether the deprecation lands directly or requires sign-off.
  const ontologies = useOntologies({ limit: 1 });
  const ontology = ontologies.data?.items?.[0];
  const detail = useOntologyDetail(ontology?.id);
  const apply = useApplyOntologyEdits(ontology?.id);
  const confirm = useConfirm();
  const expectedVersion =
    Number(detail.data?.current_version?.version ?? "0") || 0;

  const handleDeprecate = async (row: StaleTypeEntry) => {
    if (!ontology?.id) {
      toast.error(t("stale.deprecate.noOntology"));
      return;
    }
    const ok = await confirm({
      title: t("stale.deprecate.confirmTitle"),
      description: t("stale.deprecate.confirmDescription", {
        type: row.type_id,
        days: row.days_since_last_use,
      }),
      confirmLabel: t("stale.deprecate.confirmLabel"),
      cancelLabel: t("stale.deprecate.cancel"),
      variant: "warning",
    });
    if (!ok) return;
    // Map the row's `type_kind` ("node_type" / "edge_type") to
    // the matching deprecate op variant. Anything else is a
    // workspace-data inconsistency we surface as a toast rather
    // than forge an op for.
    const op: OntologyEditOp | null =
      row.type_kind === "node_type"
        ? { op: "deprecate_node_type", id: row.type_id }
        : row.type_kind === "edge_type"
          ? { op: "deprecate_edge_type", id: row.type_id }
          : null;
    if (!op) {
      toast.error(t("stale.deprecate.unsupportedKind", { kind: row.type_kind }));
      return;
    }
    apply.mutate(
      {
        operations: [op],
        expected_version: expectedVersion,
        message: t("stale.deprecate.message", { type: row.type_id }),
      },
      {
        onSuccess: () => toast.success(t("stale.deprecate.queued")),
        onError: (err) =>
          toast.error(t("stale.deprecate.failed", { error: err.message })),
      },
    );
  };

  if (loading) {
    return (
      <div className="rounded-lg border border-zinc-200 bg-white p-8 text-center dark:border-zinc-800 dark:bg-zinc-950">
        <Spinner />
      </div>
    );
  }
  if (!data.length) {
    return (
      <div className="rounded-lg border border-zinc-200 bg-white p-6 text-center text-xs text-zinc-500 dark:border-zinc-800 dark:bg-zinc-950 dark:text-zinc-400">
        {t("stale.empty")}
      </div>
    );
  }

  return (
    <div className="overflow-hidden rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-950">
      <table className="w-full min-w-[720px] text-left text-xs">
        <thead className="border-b border-zinc-200 bg-zinc-50 dark:border-zinc-800 dark:bg-zinc-900">
          <tr>
            <th className="py-3 pl-4 pr-6 font-semibold text-zinc-600 dark:text-zinc-400">
              {t("stale.col.kind")}
            </th>
            <th className="py-3 pr-6 font-semibold text-zinc-600 dark:text-zinc-400">
              {t("stale.col.typeId")}
            </th>
            <th className="py-3 pr-6 font-semibold text-zinc-600 dark:text-zinc-400">
              {t("stale.col.lastUsed")}
            </th>
            <th className="py-3 pr-6 text-right font-semibold text-zinc-600 dark:text-zinc-400">
              {t("stale.col.daysSince")}
            </th>
            <th className="py-3 pl-2 pr-4 text-right font-semibold text-zinc-600 dark:text-zinc-400">
              {t("stale.col.action")}
            </th>
          </tr>
        </thead>
        <tbody>
          {data.map((r) => (
            <tr
              key={`${r.workspace_id}-${r.type_id}`}
              className="border-b border-zinc-100 last:border-b-0 dark:border-zinc-900"
            >
              <td className="py-2 pl-4 pr-6 font-medium text-zinc-800 dark:text-zinc-200">
                {r.type_kind}
              </td>
              <td className="py-2 pr-6 font-mono text-[11px] text-zinc-600 dark:text-zinc-400">
                {r.type_id}
              </td>
              <td className="py-2 pr-6 text-zinc-600 dark:text-zinc-400">
                {r.last_used_at
                  ? new Date(r.last_used_at).toLocaleDateString()
                  : t("stale.never")}
              </td>
              <td className="py-2 pr-6 text-right tabular-nums text-zinc-600 dark:text-zinc-400">
                {r.days_since_last_use}
              </td>
              <td className="py-2 pl-2 pr-4 text-right">
                <Button
                  variant="ghost"
                  size="xs"
                  onClick={() => handleDeprecate(r)}
                  disabled={apply.isPending || !ontology?.id}
                >
                  {t("stale.deprecate.button")}
                </Button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
