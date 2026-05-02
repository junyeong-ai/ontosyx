"use client";

import { useState } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import { request } from "@/lib/api/client";
import { ErrorState } from "@/components/ui/error-state";
import { SkeletonTable } from "@/components/ui/skeleton";
import { SettingsSelect } from "@/components/ui/form-input";
import { KpiCard } from "@/components/ui/kpi-card";
import { Button } from "@/components/ui/button";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { useConfirm } from "@/components/providers/confirm-provider";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface QualityResult {
  id: string;
  workspace_id: string;
  rule_id: string;
  passed: boolean;
  actual_value: number | null;
  details: Record<string, unknown>;
  evaluated_at: string;
}

interface DashboardEntry {
  rule_id: string;
  name: string;
  rule_type: string;
  target_label: string;
  target_property: string | null;
  severity: string;
  threshold: number;
  cypher_check: string | null;
  latest_passed: boolean | null;
  latest_value: number | null;
  latest_evaluated_at: string | null;
}

const RULE_TYPES = [
  "completeness",
  "uniqueness",
  "freshness",
  "consistency",
  "custom",
] as const;
type KnownRuleType = (typeof RULE_TYPES)[number];

function isKnownRuleType(s: string): s is KnownRuleType {
  return (
    s === "completeness" ||
    s === "uniqueness" ||
    s === "freshness" ||
    s === "consistency" ||
    s === "custom"
  );
}

const SEVERITIES = ["critical", "warning", "info"] as const;
type KnownSeverity = (typeof SEVERITIES)[number];

function isKnownSeverity(s: string): s is KnownSeverity {
  return s === "critical" || s === "warning" || s === "info";
}

type RuleFormValues = {
  name: string;
  rule_type: string;
  target_label: string;
  target_property: string;
  threshold: number;
  severity: string;
  cypher_check: string;
};

const EMPTY_FORM: RuleFormValues = {
  name: "",
  rule_type: "completeness",
  target_label: "",
  target_property: "",
  threshold: 95,
  severity: "warning",
  cypher_check: "",
};

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

const qualityKeys = {
  all: ["quality"] as const,
  dashboard: () => [...qualityKeys.all, "dashboard"] as const,
};

export default function QualitySettingsPage() {
  const t = useTranslations("settings.quality");
  const tCommon = useTranslations("common");
  const qc = useQueryClient();

  // Form state
  const [formOpen, setFormOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<RuleFormValues>(EMPTY_FORM);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [executingId, setExecutingId] = useState<string | null>(null);
  const confirm = useConfirm();

  const dashboardQuery = useQuery({
    queryKey: qualityKeys.dashboard(),
    queryFn: () => request<DashboardEntry[]>("/quality/dashboard"),
  });
  const dashboard = dashboardQuery.data ?? [];
  const reload = () =>
    qc.invalidateQueries({ queryKey: qualityKeys.dashboard() });

  // ---- Open create form ----
  const openCreate = () => {
    setEditingId(null);
    setForm(EMPTY_FORM);
    setErrors({});
    setFormOpen(true);
  };

  // ---- Open edit form ----
  const openEdit = (d: DashboardEntry) => {
    setEditingId(d.rule_id);
    setForm({
      name: d.name,
      rule_type: d.rule_type,
      target_label: d.target_label,
      target_property: d.target_property ?? "",
      threshold: d.threshold,
      severity: d.severity,
      cypher_check: d.cypher_check ?? "",
    });
    setErrors({});
    setFormOpen(true);
  };

  // ---- Cancel ----
  const cancelForm = () => {
    setFormOpen(false);
    setEditingId(null);
    setForm(EMPTY_FORM);
    setErrors({});
  };

  // ---- Clear single field error on change ----
  const clearError = (field: string) => {
    if (errors[field]) setErrors((prev) => { const next = { ...prev }; delete next[field]; return next; });
  };

  // ---- Validate ----
  const validate = (): boolean => {
    const e: Record<string, string> = {};
    if (!form.name.trim()) e.name = t("validation.required");
    if (!form.target_label.trim()) e.target_label = t("validation.required");
    if (form.threshold < 0 || form.threshold > 100) e.threshold = t("validation.thresholdRange");
    setErrors(e);
    return Object.keys(e).length === 0;
  };

  const submitMutation = useMutation({
    mutationFn: (body: Record<string, unknown>) =>
      editingId
        ? request(`/quality/rules/${editingId}`, {
            method: "PATCH",
            body: JSON.stringify(body),
          })
        : request("/quality/rules", {
            method: "POST",
            body: JSON.stringify(body),
          }),
    onSuccess: () => {
      toast.success(editingId ? t("toast.updated") : t("toast.created"));
      cancelForm();
      reload();
    },
    onError: () =>
      toast.error(
        editingId ? t("toast.updateFailed") : t("toast.createFailed"),
      ),
  });

  const deleteMutation = useMutation({
    mutationFn: (ruleId: string) =>
      request(`/quality/rules/${ruleId}`, { method: "DELETE" }),
    onSuccess: () => {
      toast.success(t("toast.deleted"));
      reload();
    },
    onError: () => toast.error(t("toast.deleteFailed")),
  });

  const executeAllMutation = useMutation({
    mutationFn: () =>
      request<QualityResult[]>("/quality/execute-all", { method: "POST" }),
    onSuccess: (results) => {
      const passedCount = results.filter((r) => r.passed).length;
      toast.success(
        t("toast.executed", {
          total: results.length,
          passed: passedCount,
          failed: results.length - passedCount,
        }),
      );
      reload();
    },
    onError: () => toast.error(t("toast.executeAllError")),
  });

  // ---- Submit (create or update) ----
  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;

    const body: Record<string, unknown> = {
      name: form.name.trim(),
      rule_type: form.rule_type,
      target_label: form.target_label.trim(),
      target_property: form.target_property.trim() || null,
      threshold: form.threshold,
      severity: form.severity,
      cypher_check:
        form.rule_type === "custom" && form.cypher_check.trim()
          ? form.cypher_check.trim()
          : null,
    };
    submitMutation.mutate(body);
  };

  // ---- Delete ----
  const handleDelete = async (ruleId: string) => {
    const rule = dashboard.find((d) => d.rule_id === ruleId);
    const ok = await confirm({
      title: t("deleteConfirm.title", { name: rule?.name ?? ruleId }),
      description: t("deleteConfirm.description"),
      variant: "danger",
    });
    if (!ok) return;
    deleteMutation.mutate(ruleId);
  };

  // ---- Execute single rule ----
  const handleExecute = async (ruleId: string) => {
    setExecutingId(ruleId);
    try {
      const result = await request<QualityResult>(
        `/quality/rules/${ruleId}/execute`,
        { method: "POST" },
      );
      const valueStr =
        result.actual_value !== null ? result.actual_value.toFixed(1) : "-";
      toast.success(
        result.passed
          ? t("toast.rulePassed", { value: valueStr })
          : t("toast.ruleFailed", { value: valueStr }),
      );
      reload();
    } catch {
      toast.error(t("toast.executeError"));
    } finally {
      setExecutingId(null);
    }
  };

  if (dashboardQuery.isLoading) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <SkeletonTable rows={6} cols={6} />
      </SettingsPageShell>
    );
  }

  if (dashboardQuery.isError) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <ErrorState
          title={tCommon("loadError.title")}
          description={tCommon("loadError.description")}
          onRetry={() => dashboardQuery.refetch()}
          retryLabel={tCommon("retry")}
        />
      </SettingsPageShell>
    );
  }

  const saving = submitMutation.isPending;
  const deletingId = deleteMutation.isPending ? deleteMutation.variables : null;
  const executingAll = executeAllMutation.isPending;
  const handleExecuteAll = () => executeAllMutation.mutate();

  const passed = dashboard.filter((d) => d.latest_passed === true).length;
  const failed = dashboard.filter((d) => d.latest_passed === false).length;
  const pending = dashboard.filter((d) => d.latest_passed === null).length;

  return (
    <SettingsPageShell
      title={t("title")}
      subtitle={t("description")}
      actions={
        !formOpen && (
          <>
            <Button
              variant="outline"
              size="sm"
              onClick={handleExecuteAll}
              disabled={executingAll || dashboard.length === 0}
            >
              {executingAll ? t("executingAll") : t("executeAll")}
            </Button>
            <Button variant="primary" size="sm" onClick={openCreate}>
              {t("createRule")}
            </Button>
          </>
        )
      }
    >
      <div className="grid grid-cols-3 gap-4">
        <KpiCard tone="success" label={t("summary.passing")} value={passed} />
        <KpiCard tone="danger"  label={t("summary.failing")} value={failed} />
        <KpiCard tone="neutral" label={t("summary.pending")} value={pending} />
      </div>

      {/* Inline form */}
      {formOpen && (
        <RuleForm
          form={form}
          setForm={setForm}
          errors={errors}
          clearError={clearError}
          isEditing={!!editingId}
          saving={saving}
          onSubmit={handleSubmit}
          onCancel={cancelForm}
        />
      )}

      {/* Rules table */}
      <div className="mt-6 overflow-x-auto -mx-6 px-6" tabIndex={0} role="region" aria-label="Table data — scroll horizontally">
        <table className="w-full min-w-[900px] text-sm">
          <thead>
            <tr className="border-b border-divider text-left text-xs font-medium uppercase text-muted-foreground">
              <th className="py-3 pr-6">{t("column.rule")}</th>
              <th className="py-3 pr-6">{t("column.type")}</th>
              <th className="py-3 pr-6">{t("column.target")}</th>
              <th className="py-3 pr-6">{t("column.threshold")}</th>
              <th className="py-3 pr-6">{t("column.severity")}</th>
              <th className="py-3 pr-6">{t("column.status")}</th>
              <th className="py-3 pr-6">{t("column.value")}</th>
              <th className="py-3 pr-6 text-right">{t("column.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {dashboard.map((d) => (
              <tr
                key={d.rule_id}
                className="border-b border-divider-soft"
              >
                <td className="py-3 pr-6 font-medium text-foreground-strong">
                  {d.name}
                </td>
                <td className="py-3 pr-6 text-muted-foreground">
                  {isKnownRuleType(d.rule_type) ? t(`ruleType.${d.rule_type}`) : d.rule_type}
                </td>
                <td className="py-3 pr-6 text-muted-foreground">
                  {d.target_label}
                  {d.target_property ? `.${d.target_property}` : ""}
                </td>
                <td className="py-3 pr-6 text-muted-foreground">{d.threshold}%</td>
                <td className="py-3 pr-6">
                  <SeverityBadge severity={d.severity} />
                </td>
                <td className="py-3 pr-6">
                  {d.latest_passed === null ? (
                    <span className="text-muted-foreground">{t("status.none")}</span>
                  ) : d.latest_passed ? (
                    <span className="text-brand-foreground">
                      {t("status.pass")}
                    </span>
                  ) : (
                    <span className="text-danger-foreground">{t("status.fail")}</span>
                  )}
                </td>
                <td className="py-3 pr-6 text-muted-foreground">
                  {d.latest_value !== null
                    ? `${d.latest_value.toFixed(1)}%`
                    : "-"}
                </td>
                <td className="py-3 pr-6 text-right">
                  <div className="flex items-center justify-end gap-1">
                    <button
                      onClick={() => handleExecute(d.rule_id)}
                      disabled={executingId === d.rule_id || executingAll}
                      className="rounded px-2 py-1 text-xs text-brand-foreground hover:bg-brand-surface hover:text-brand-foreground-strong disabled:opacity-50 dark:hover:bg-brand-surface"
                    >
                      {executingId === d.rule_id ? t("action.running") : t("action.run")}
                    </button>
                    <button
                      onClick={() => openEdit(d)}
                      className="rounded px-2 py-1 text-xs text-muted-foreground hover:bg-surface-inset hover:text-foreground dark:hover:text-foreground-muted"
                    >
                      {t("action.edit")}
                    </button>
                    <button
                      onClick={() => handleDelete(d.rule_id)}
                      disabled={deletingId === d.rule_id}
                      className="rounded px-2 py-1 text-xs text-danger-foreground hover:bg-danger-surface hover:text-danger-foreground disabled:opacity-50 dark:hover:bg-danger-surface"
                    >
                      {deletingId === d.rule_id ? t("action.deleting") : t("action.delete")}
                    </button>
                  </div>
                </td>
              </tr>
            ))}
            {dashboard.length === 0 && (
              <tr>
                <td colSpan={8} className="py-8 text-center text-muted-foreground">
                  {t("empty")}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </SettingsPageShell>
  );
}

// ---------------------------------------------------------------------------
// Severity badge
// ---------------------------------------------------------------------------

function SeverityBadge({ severity }: { severity: string }) {
  const t = useTranslations("settings.quality");
  const color =
    severity === "critical"
      ? "bg-danger-surface text-danger-foreground"
      : severity === "warning"
        ? "bg-warning-surface text-warning-foreground"
        : "bg-surface-inset text-foreground dark:text-muted-foreground";

  const label = isKnownSeverity(severity) ? t(`severityLevel.${severity}`) : severity;

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-2xs font-semibold uppercase tracking-wider ${color}`}
    >
      {label}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Rule form (create / edit)
// ---------------------------------------------------------------------------

function RuleForm({
  form,
  setForm,
  errors,
  clearError,
  isEditing,
  saving,
  onSubmit,
  onCancel,
}: {
  form: RuleFormValues;
  setForm: React.Dispatch<React.SetStateAction<RuleFormValues>>;
  errors: Record<string, string>;
  clearError: (field: string) => void;
  isEditing: boolean;
  saving: boolean;
  onSubmit: (e: React.FormEvent) => void;
  onCancel: () => void;
}) {
  const t = useTranslations("settings.quality");
  const update = (field: string, patch: Partial<RuleFormValues>) => {
    setForm((prev) => ({ ...prev, ...patch }));
    clearError(field);
  };

  return (
    <form
      onSubmit={onSubmit}
      className="mt-4 rounded-lg border border-brand-border bg-brand-surface p-4"
    >
      <div className="mb-3 flex items-center justify-between">
        <span className="text-xs font-semibold text-brand-foreground">
          {isEditing ? t("form.editTitle") : t("form.newTitle")}
        </span>
        <button
          type="button"
          onClick={onCancel}
          className="text-xs text-muted-foreground hover:text-foreground"
        >
          {t("form.cancel")}
        </button>
      </div>

      <div className="grid grid-cols-2 gap-3">
        {/* Name */}
        <div className="col-span-2">
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.name")}
          </label>
          <input
            value={form.name}
            onChange={(e) => update("name", { name: e.target.value })}
            placeholder={t("form.namePlaceholder")}
            required
            className={`mt-0.5 w-full rounded-md border bg-surface-base px-3 py-1.5 text-xs ${errors.name ? "border-danger-border" : "border-divider"}`}
          />
          {errors.name && <p className="mt-0.5 text-2xs text-danger-foreground">{errors.name}</p>}
        </div>

        {/* Rule type */}
        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.ruleType")}
          </label>
          <SettingsSelect
            label={t("form.ruleType")}
            hideLabel
            value={form.rule_type}
            onChange={(e) => update("rule_type", { rule_type: e.target.value })}
          >
            {RULE_TYPES.map((value) => (
              <option key={value} value={value}>
                {t(`ruleType.${value}`)}
              </option>
            ))}
          </SettingsSelect>
        </div>

        {/* Severity */}
        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.severity")}
          </label>
          <SettingsSelect
            label={t("form.severity")}
            hideLabel
            value={form.severity}
            onChange={(e) => update("severity", { severity: e.target.value })}
          >
            {SEVERITIES.map((value) => (
              <option key={value} value={value}>
                {t(`severityLevel.${value}`)}
              </option>
            ))}
          </SettingsSelect>
        </div>

        {/* Target label */}
        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.targetLabel")}
          </label>
          <input
            value={form.target_label}
            onChange={(e) => update("target_label", { target_label: e.target.value })}
            placeholder={t("form.targetLabelPlaceholder")}
            required
            className={`mt-0.5 w-full rounded-md border bg-surface-base px-3 py-1.5 text-xs ${errors.target_label ? "border-danger-border" : "border-divider"}`}
          />
          {errors.target_label && <p className="mt-0.5 text-2xs text-danger-foreground">{errors.target_label}</p>}
        </div>

        {/* Target property */}
        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.targetProperty")}{" "}
            <span className="normal-case text-muted-foreground">{t("form.targetPropertyHint")}</span>
          </label>
          <input
            value={form.target_property}
            onChange={(e) => update("target_property", { target_property: e.target.value })}
            placeholder={t("form.targetPropertyPlaceholder")}
            className="mt-0.5 w-full rounded-md border border-divider bg-surface-base px-3 py-1.5 text-xs"
          />
        </div>

        {/* Threshold */}
        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.threshold")}
          </label>
          <input
            type="number"
            min={0}
            max={100}
            step={1}
            value={form.threshold}
            onChange={(e) => update("threshold", { threshold: Number(e.target.value) })}
            className={`mt-0.5 w-full rounded-md border bg-surface-base px-3 py-1.5 text-xs ${errors.threshold ? "border-danger-border" : "border-divider"}`}
          />
          {errors.threshold && <p className="mt-0.5 text-2xs text-danger-foreground">{errors.threshold}</p>}
        </div>

        {/* Cypher check — only for custom type */}
        {form.rule_type === "custom" && (
          <div className="col-span-2">
            <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
              {t("form.cypherCheck")}
            </label>
            <textarea
              value={form.cypher_check}
              onChange={(e) => update("cypher_check", { cypher_check: e.target.value })}
              placeholder={t("form.cypherCheckPlaceholder")}
              rows={3}
              className="mt-0.5 w-full rounded-md border border-divider bg-surface-base px-3 py-1.5 font-mono text-xs"
            />
          </div>
        )}
      </div>

      <div className="mt-3 flex items-center gap-2">
        <button
          type="submit"
          disabled={!form.name.trim() || !form.target_label.trim() || saving}
          className="rounded-md bg-brand-solid px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50 hover:bg-brand-solid"
        >
          {saving
            ? isEditing
              ? t("form.updating")
              : t("form.creating")
            : isEditing
              ? t("form.update")
              : t("form.create")}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md px-3 py-1.5 text-xs text-muted-foreground hover:bg-surface-inset"
        >
          {t("form.cancel")}
        </button>
      </div>
    </form>
  );
}
