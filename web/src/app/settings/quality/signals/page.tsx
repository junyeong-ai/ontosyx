"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Spinner } from "@/components/ui/spinner";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { SettingsSelect } from "@/components/ui/form-input";
import { useConfirm } from "@/components/providers/confirm-provider";
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
    <SettingsPageShell
      title={t("title")}
      subtitle={t("subtitle")}
      actions={
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
      }
    >
      <div className="flex flex-col gap-6">

      <MetricsGrid report={metrics.data} loading={metrics.isLoading} />

      <section>
        <h2 className="mb-3 text-sm font-semibold text-foreground-strong">
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
            <h2 className="text-sm font-semibold text-foreground-strong">
              {t("stale.title")}
            </h2>
            <p className="mt-0.5 text-xs text-foreground-muted">
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
    </SettingsPageShell>
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
      <Card padding="lg" className="text-center">
        <Spinner />
      </Card>
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
        <h2 className="text-sm font-semibold text-foreground-strong">
          {t("tiles.title")}
        </h2>
        {report && (
          <span className="text-[11px] text-foreground-muted">
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
    <Card padding="md">
      <div className="mb-2 flex items-start justify-between gap-2">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-foreground-muted">
          {label}
        </h3>
        <TrendBadge delta={trend} />
      </div>
      <div className="flex items-baseline gap-2">
        <span
          className={cn(
            "text-2xl font-semibold tabular-nums",
            hasData
              ? "text-foreground-strong"
              : "text-foreground-subtle",
          )}
        >
          {pct !== null ? `${pct.toFixed(1)}%` : "—"}
        </span>
        {hasData && (
          <span className="text-[11px] text-foreground-muted">
            {`[${(metric!.lower_bound_95 * 100).toFixed(1)}%, ${(
              metric!.upper_bound_95 * 100
            ).toFixed(1)}%]`}
          </span>
        )}
      </div>
      {bandWide && (
        <p className="mt-1 text-2xs italic text-warning-foreground">
          wide confidence band — limited samples
        </p>
      )}
      <p className="mt-2 text-[11px] leading-snug text-foreground-muted">
        {description}
      </p>
    </Card>
  );
}

function TrendBadge({ delta }: { delta: number }) {
  // Dead-band below ±0.5pp renders flat so noise doesn't flip arrows.
  if (Math.abs(delta) < 0.005) {
    return (
      <span className="rounded px-1.5 py-0.5 text-2xs font-medium text-foreground-muted">
        = flat
      </span>
    );
  }
  const up = delta > 0;
  return (
    <span
      className={cn(
        "rounded px-1.5 py-0.5 text-2xs font-medium tabular-nums",
        up
          ? "bg-brand-surface text-brand-foreground"
          : "bg-danger-surface text-danger-foreground dark:text-danger-foreground",
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
      <Card padding="lg" className="text-center">
        <Spinner />
      </Card>
    );
  }
  if (!rows.length) {
    return (
      <Card padding="none">
        <EmptyState size="sm" title={t("shacl.empty")} />
      </Card>
    );
  }

  const total = rows.reduce((sum, r) => sum + r.count, 0);
  return (
    <Card padding="md" className="space-y-2">
      {rows.map((r) => {
        const pct = total === 0 ? 0 : Math.round((r.count / total) * 1000) / 10;
        return (
          <div
            key={r.kind}
            className="grid grid-cols-[1fr_auto] items-center gap-3"
          >
            <div>
              <div className="flex items-baseline justify-between">
                <span className="text-xs font-medium text-foreground-strong">
                  {t(`shacl.kind.${r.kind}`)}
                </span>
                <span className="text-[11px] tabular-nums text-foreground-muted">
                  {r.count} · {pct.toFixed(1)}%
                </span>
              </div>
              <div className="mt-1 h-2 overflow-hidden rounded bg-surface-inset">
                <div
                  className={cn("h-full", shaclFailureBarTone(r.kind))}
                  style={{ width: `${pct}%` }}
                />
              </div>
            </div>
          </div>
        );
      })}
    </Card>
  );
}

/**
 * Colour ramp per failure kind — fixed so a returning operator
 * recognises the shape of the distribution without reading labels.
 */
function shaclFailureBarTone(kind: ShaclFailureKind): string {
  switch (kind) {
    case "cardinality_violation":
      return "bg-danger-foreground";
    case "measure_group_by":
      return "bg-warning-foreground";
    case "unknown_coded_value":
      return "bg-concept-foreground";
    case "mandatory_property_missing":
      return "bg-info-surface";
    case "temporal_grain_mismatch":
      return "bg-brand-solid";
    case "other":
      return "bg-muted-foreground";
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
      <Card padding="lg" className="text-center">
        <Spinner />
      </Card>
    );
  }
  if (!data.length) {
    return (
      <Card padding="none">
        <EmptyState size="sm" title={t("stale.empty")} />
      </Card>
    );
  }

  return (
    <Card padding="none" className="overflow-hidden">
      <table className="w-full min-w-[720px] text-left text-xs">
        <thead className="border-b border-divider bg-surface-raised">
          <tr>
            <th className="py-3 pl-4 pr-6 font-semibold text-foreground-muted">
              {t("stale.col.kind")}
            </th>
            <th className="py-3 pr-6 font-semibold text-foreground-muted">
              {t("stale.col.typeId")}
            </th>
            <th className="py-3 pr-6 font-semibold text-foreground-muted">
              {t("stale.col.lastUsed")}
            </th>
            <th className="py-3 pr-6 text-right font-semibold text-foreground-muted">
              {t("stale.col.daysSince")}
            </th>
            <th className="py-3 pl-2 pr-4 text-right font-semibold text-foreground-muted">
              {t("stale.col.action")}
            </th>
          </tr>
        </thead>
        <tbody>
          {data.map((r) => (
            <tr
              key={`${r.workspace_id}-${r.type_id}`}
              className="border-b border-divider-soft last:border-b-0 dark:border-divider"
            >
              <td className="py-2 pl-4 pr-6 font-medium text-foreground-strong">
                {r.type_kind}
              </td>
              <td className="py-2 pr-6 font-mono text-[11px] text-foreground-muted">
                {r.type_id}
              </td>
              <td className="py-2 pr-6 text-foreground-muted">
                {r.last_used_at
                  ? new Date(r.last_used_at).toLocaleDateString()
                  : t("stale.never")}
              </td>
              <td className="py-2 pr-6 text-right tabular-nums text-foreground-muted">
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
    </Card>
  );
}
