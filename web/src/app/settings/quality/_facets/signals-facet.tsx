"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";
import { Heading } from "@/components/ui/heading";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton, SkeletonTable } from "@/components/ui/skeleton";
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
import { useWorkspaceOntology } from "@/hooks/api/use-workspace-ontology";
import { useApplyOntologyEdits } from "@/hooks/api/use-ontology-edits";
import type { OntologyEditOp } from "@/lib/api/edit-ops";

import { Eyebrow } from "@/components/ui/eyebrow";
/**
 * Ontology-quality signal dashboard — the patent's "6 창" surface.
 *
 * Lives at `/settings/quality?tab=signals` so the existing
 * `/settings/quality` (data-quality rule CRUD) stays untouched. This
 * page reads from `QualitySignalStore`: 6 tiles + SHACL failure
 * distribution + stale-type candidates.
 */
export function SignalsFacet() {
  const t = useTranslations("settings.quality.signals");
  const [windowChoice, setWindowChoice] = useState<MetricWindow>("last7d");
  const [staleDays, setStaleDays] = useState(180);

  const metrics = useQualityMetrics(windowChoice);
  const failures = useShaclFailures(windowChoice);
  const stale = useStaleTypes(staleDays);

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center justify-end">
        <SettingsSelect
          label={t("window.label")}
          hideLabel
          value={windowChoice}
          onChange={(e) => setWindowChoice(e.target.value as MetricWindow)}
        >
          <option value="last7d">{t("window.last7d")}</option>
          <option value="last30d">{t("window.last30d")}</option>
          <option value="last90d">{t("window.last90d")}</option>
        </SettingsSelect>
      </div>

      <MetricsGrid
        report={metrics.data}
        loading={metrics.isLoading}
        failed={metrics.isError}
        onRetry={() => metrics.refetch()}
      />

      <section>
        <Heading level={2} size={6} className="mb-3">
          {t("shacl.title")}
        </Heading>
        <ShaclFailureBars
          rows={failures.data ?? []}
          loading={failures.isLoading}
          failed={failures.isError}
          onRetry={() => failures.refetch()}
        />
      </section>

      <section>
        <div className="mb-3 flex items-end justify-between gap-4">
          <div>
            <Heading level={2} size={6}>
              {t("stale.title")}
            </Heading>
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
        <StaleTypesTable
          rows={stale.data ?? []}
          loading={stale.isLoading}
          failed={stale.isError}
          onRetry={() => stale.refetch()}
        />
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
  failed,
  onRetry,
}: {
  report: QualityMetricsReport | undefined;
  loading: boolean;
  failed: boolean;
  onRetry: () => void;
}) {
  const t = useTranslations("settings.quality.signals");
  const tCommon = useTranslations("common");

  if (failed) {
    return (
      <Card padding="lg">
        <ErrorState
          title={tCommon("loadError.title")}
          description={tCommon("loadError.description")}
          onRetry={onRetry}
          retryLabel={tCommon("retry")}
        />
      </Card>
    );
  }

  if (loading) {
    return (
      <section>
        <div className="mb-3 flex items-baseline justify-between">
          <Heading level={2} size={6}>
            {t("tiles.title")}
          </Heading>
        </div>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 lg:grid-cols-3">
          {Array.from({ length: 6 }, (_, i) => (
            <Card key={i} padding="md">
              <Skeleton className="mb-3 h-3 w-1/2" />
              <Skeleton className="h-7 w-1/3" />
              <Skeleton className="mt-3 h-2 w-full" />
            </Card>
          ))}
        </div>
      </section>
    );
  }

  const tiles = [
    { key: "anchor_match_rate", value: report?.anchor_match_rate },
    { key: "concept_hit_rate", value: report?.concept_hit_rate },
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
        <Heading level={2} size={6}>
          {t("tiles.title")}
        </Heading>
        {report && (
          <span className="text-2xs text-foreground-muted">
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
  const t = useTranslations("settings.quality.signals");
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
        <Eyebrow level={3} tone="muted" size="dense" caps="upper">
          {label}
        </Eyebrow>
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
          <span className="text-2xs text-foreground-muted">
            {`[${(metric!.lower_bound_95 * 100).toFixed(1)}%, ${(
              metric!.upper_bound_95 * 100
            ).toFixed(1)}%]`}
          </span>
        )}
      </div>
      {bandWide && (
        <p className="mt-1 text-2xs italic text-warning-foreground">
          {t("metric.wideConfidenceBand")}
        </p>
      )}
      <p className="mt-2 text-2xs leading-snug text-foreground-muted">
        {description}
      </p>
    </Card>
  );
}

function TrendBadge({ delta }: { delta: number }) {
  const t = useTranslations("settings.quality.signals");
  // Dead-band below ±0.5pp renders flat so noise doesn't flip arrows.
  if (Math.abs(delta) < 0.005) {
    return (
      <span className="rounded px-1.5 py-0.5 text-2xs font-medium text-foreground-muted">
        {t("trend.flat")}
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
          : "bg-danger-surface text-danger-foreground",
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
  failed,
  onRetry,
}: {
  rows: ShaclFailureCount[];
  loading: boolean;
  failed: boolean;
  onRetry: () => void;
}) {
  const t = useTranslations("settings.quality.signals");
  const tCommon = useTranslations("common");

  if (failed) {
    return (
      <Card padding="lg">
        <ErrorState
          title={tCommon("loadError.title")}
          description={tCommon("loadError.description")}
          onRetry={onRetry}
          retryLabel={tCommon("retry")}
        />
      </Card>
    );
  }

  if (loading) {
    return (
      <Card padding="md" className="space-y-3">
        {Array.from({ length: 5 }, (_, i) => (
          <div key={i}>
            <div className="mb-1 flex items-baseline justify-between">
              <Skeleton className="h-3 w-1/3" />
              <Skeleton className="h-2 w-12" />
            </div>
            <Skeleton className="h-2 w-full" />
          </div>
        ))}
      </Card>
    );
  }
  if (!rows.length) {
    return (
      <Card padding="none">
        <EmptyState variant="compact" title={t("shacl.empty")} />
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
                <span className="text-2xs tabular-nums text-foreground-muted">
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
      return "bg-foreground-muted";
  }
}

// ---------------------------------------------------------------------------
// Stale types table
// ---------------------------------------------------------------------------

function StaleTypesTable({
  rows,
  loading,
  failed,
  onRetry,
}: {
  rows: StaleTypeEntry[];
  loading: boolean;
  failed: boolean;
  onRetry: () => void;
}) {
  const t = useTranslations("settings.quality.signals");
  const tCommon = useTranslations("common");
  const data = useMemo(() => rows, [rows]);

  const detail = useWorkspaceOntology();
  const ontology = detail.data ?? null;
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

  if (failed) {
    return (
      <Card padding="lg">
        <ErrorState
          title={tCommon("loadError.title")}
          description={tCommon("loadError.description")}
          onRetry={onRetry}
          retryLabel={tCommon("retry")}
        />
      </Card>
    );
  }
  if (loading) {
    return (
      <Card padding="md">
        <SkeletonTable rows={5} cols={5} />
      </Card>
    );
  }
  if (!data.length) {
    return (
      <Card padding="none">
        <EmptyState variant="compact" title={t("stale.empty")} />
      </Card>
    );
  }

  return (
    <Card padding="none" className="overflow-hidden">
      <table className="w-full min-w-[720px] text-start text-xs">
        <thead className="border-b border-divider bg-surface-raised">
          <tr>
            <th className="py-3 ps-4 pe-6 font-semibold text-foreground-muted">
              {t("stale.col.kind")}
            </th>
            <th className="py-3 pe-6 font-semibold text-foreground-muted">
              {t("stale.col.typeId")}
            </th>
            <th className="py-3 pe-6 font-semibold text-foreground-muted">
              {t("stale.col.lastUsed")}
            </th>
            <th className="py-3 pe-6 text-end font-semibold text-foreground-muted">
              {t("stale.col.daysSince")}
            </th>
            <th className="py-3 ps-2 pe-4 text-end font-semibold text-foreground-muted">
              {t("stale.col.action")}
            </th>
          </tr>
        </thead>
        <tbody>
          {data.map((r) => (
            <tr
              key={`${r.workspace_id}-${r.type_id}`}
              className="border-b border-divider-soft last:border-b-0"
            >
              <td className="py-2 ps-4 pe-6 font-medium text-foreground-strong">
                {r.type_kind}
              </td>
              <td className="py-2 pe-6 font-mono text-2xs text-foreground-muted">
                {r.type_id}
              </td>
              <td className="py-2 pe-6 text-foreground-muted">
                {r.last_used_at
                  ? new Date(r.last_used_at).toLocaleDateString()
                  : t("stale.never")}
              </td>
              <td className="py-2 pe-6 text-end tabular-nums text-foreground-muted">
                {r.days_since_last_use}
              </td>
              <td className="py-2 ps-2 pe-4 text-end">
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
