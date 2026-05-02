"use client";

import { useState, useEffect, useCallback } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";
import { request } from "@/lib/api/client";
import { Spinner } from "@/components/ui/spinner";
import { SettingsSelect } from "@/components/ui/form-input";
import { useConfirm } from "@/components/ui/confirm-dialog";

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

export default function QualitySettingsPage() {
  const t = useTranslations("settings.quality");
  const [dashboard, setDashboard] = useState<DashboardEntry[]>([]);
  const [loading, setLoading] = useState(true);

  // Form state
  const [formOpen, setFormOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<RuleFormValues>(EMPTY_FORM);
  const [saving, setSaving] = useState(false);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [executingId, setExecutingId] = useState<string | null>(null);
  const [executingAll, setExecutingAll] = useState(false);
  const confirm = useConfirm();

  const load = useCallback(async () => {
    try {
      const data = await request<DashboardEntry[]>("/quality/dashboard");
      setDashboard(data);
    } catch {
      toast.error(t("toast.loadFailed"));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    load();
  }, [load]);

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

  // ---- Submit (create or update) ----
  const handleSubmit = async (e: React.FormEvent) => {
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

    setSaving(true);
    try {
      if (editingId) {
        await request(`/quality/rules/${editingId}`, {
          method: "PATCH",
          body: JSON.stringify(body),
        });
        toast.success(t("toast.updated"));
      } else {
        await request("/quality/rules", {
          method: "POST",
          body: JSON.stringify(body),
        });
        toast.success(t("toast.created"));
      }
      cancelForm();
      await load();
    } catch {
      toast.error(editingId ? t("toast.updateFailed") : t("toast.createFailed"));
    } finally {
      setSaving(false);
    }
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
    setDeletingId(ruleId);
    try {
      await request(`/quality/rules/${ruleId}`, { method: "DELETE" });
      toast.success(t("toast.deleted"));
      await load();
    } catch {
      toast.error(t("toast.deleteFailed"));
    } finally {
      setDeletingId(null);
    }
  };

  // ---- Execute single rule ----
  const handleExecute = async (ruleId: string) => {
    setExecutingId(ruleId);
    try {
      const result = await request<QualityResult>(`/quality/rules/${ruleId}/execute`, {
        method: "POST",
      });
      const valueStr = result.actual_value !== null ? result.actual_value.toFixed(1) : "-";
      toast.success(
        result.passed
          ? t("toast.rulePassed", { value: valueStr })
          : t("toast.ruleFailed", { value: valueStr }),
      );
      await load();
    } catch {
      toast.error(t("toast.executeError"));
    } finally {
      setExecutingId(null);
    }
  };

  // ---- Execute all rules ----
  const handleExecuteAll = async () => {
    setExecutingAll(true);
    try {
      const results = await request<QualityResult[]>("/quality/execute-all", {
        method: "POST",
      });
      const passedCount = results.filter((r) => r.passed).length;
      toast.success(
        t("toast.executed", {
          total: results.length,
          passed: passedCount,
          failed: results.length - passedCount,
        }),
      );
      await load();
    } catch {
      toast.error(t("toast.executeAllError"));
    } finally {
      setExecutingAll(false);
    }
  };

  if (loading) return <Spinner />;

  const passed = dashboard.filter((d) => d.latest_passed === true).length;
  const failed = dashboard.filter((d) => d.latest_passed === false).length;
  const pending = dashboard.filter((d) => d.latest_passed === null).length;

  return (
    <div>
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            {t("title")}
          </h1>
          <p className="mt-1 text-sm text-zinc-500 dark:text-muted-foreground">
            {t("description")}
          </p>
        </div>
        {!formOpen && (
          <div className="flex items-center gap-2">
            <button
              onClick={handleExecuteAll}
              disabled={executingAll || dashboard.length === 0}
              className="rounded-md border border-zinc-300 bg-white px-3 py-1.5 text-xs font-medium text-zinc-700 hover:bg-zinc-50 disabled:opacity-50 dark:border-zinc-600 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-zinc-700"
            >
              {executingAll ? t("executingAll") : t("executeAll")}
            </button>
            <button
              onClick={openCreate}
              className="rounded-md bg-emerald-700 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-800"
            >
              {t("createRule")}
            </button>
          </div>
        )}
      </div>

      {/* Summary */}
      <div className="mt-6 grid grid-cols-3 gap-4">
        <div className="rounded-lg border border-emerald-200 bg-emerald-50 p-4 dark:border-emerald-900 dark:bg-emerald-950">
          <div className="text-2xl font-bold text-emerald-700 dark:text-emerald-400">
            {passed}
          </div>
          <div className="text-xs text-emerald-700 dark:text-emerald-500">
            {t("summary.passing")}
          </div>
        </div>
        <div className="rounded-lg border border-red-200 bg-red-50 p-4 dark:border-red-900 dark:bg-red-950">
          <div className="text-2xl font-bold text-red-700 dark:text-red-400">
            {failed}
          </div>
          <div className="text-xs text-red-700 dark:text-red-300">{t("summary.failing")}</div>
        </div>
        <div className="rounded-lg border border-zinc-200 bg-zinc-50 p-4 dark:border-zinc-700 dark:bg-zinc-900">
          <div className="text-2xl font-bold text-zinc-700 dark:text-zinc-300">
            {pending}
          </div>
          <div className="text-xs text-muted-foreground">{t("summary.pending")}</div>
        </div>
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
            <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-muted-foreground dark:border-zinc-700">
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
                className="border-b border-zinc-100 dark:border-zinc-800"
              >
                <td className="py-3 pr-6 font-medium text-zinc-900 dark:text-zinc-100">
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
                    <span className="text-emerald-700 dark:text-emerald-400">
                      {t("status.pass")}
                    </span>
                  ) : (
                    <span className="text-red-600 dark:text-red-400">{t("status.fail")}</span>
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
                      className="rounded px-2 py-1 text-xs text-emerald-700 hover:bg-emerald-50 hover:text-emerald-800 disabled:opacity-50 dark:text-emerald-400 dark:hover:bg-emerald-950"
                    >
                      {executingId === d.rule_id ? t("action.running") : t("action.run")}
                    </button>
                    <button
                      onClick={() => openEdit(d)}
                      className="rounded px-2 py-1 text-xs text-muted-foreground hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
                    >
                      {t("action.edit")}
                    </button>
                    <button
                      onClick={() => handleDelete(d.rule_id)}
                      disabled={deletingId === d.rule_id}
                      className="rounded px-2 py-1 text-xs text-red-500 hover:bg-red-50 hover:text-red-700 disabled:opacity-50 dark:hover:bg-red-950"
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
    </div>
  );
}

// ---------------------------------------------------------------------------
// Severity badge
// ---------------------------------------------------------------------------

function SeverityBadge({ severity }: { severity: string }) {
  const t = useTranslations("settings.quality");
  const color =
    severity === "critical"
      ? "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400"
      : severity === "warning"
        ? "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400"
        : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground";

  const label = isKnownSeverity(severity) ? t(`severityLevel.${severity}`) : severity;

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ${color}`}
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
      className="mt-4 rounded-lg border border-emerald-200 bg-emerald-50/50 p-4 dark:border-emerald-800 dark:bg-emerald-950/20"
    >
      <div className="mb-3 flex items-center justify-between">
        <span className="text-xs font-semibold text-emerald-700 dark:text-emerald-400">
          {isEditing ? t("form.editTitle") : t("form.newTitle")}
        </span>
        <button
          type="button"
          onClick={onCancel}
          className="text-xs text-muted-foreground hover:text-zinc-600"
        >
          {t("form.cancel")}
        </button>
      </div>

      <div className="grid grid-cols-2 gap-3">
        {/* Name */}
        <div className="col-span-2">
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.name")}
          </label>
          <input
            value={form.name}
            onChange={(e) => update("name", { name: e.target.value })}
            placeholder={t("form.namePlaceholder")}
            required
            className={`mt-0.5 w-full rounded-md border bg-white px-3 py-1.5 text-xs dark:bg-zinc-900 ${errors.name ? "border-red-400 dark:border-red-600" : "border-zinc-200 dark:border-zinc-700"}`}
          />
          {errors.name && <p className="mt-0.5 text-[10px] text-red-500">{errors.name}</p>}
        </div>

        {/* Rule type */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
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
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
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
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.targetLabel")}
          </label>
          <input
            value={form.target_label}
            onChange={(e) => update("target_label", { target_label: e.target.value })}
            placeholder={t("form.targetLabelPlaceholder")}
            required
            className={`mt-0.5 w-full rounded-md border bg-white px-3 py-1.5 text-xs dark:bg-zinc-900 ${errors.target_label ? "border-red-400 dark:border-red-600" : "border-zinc-200 dark:border-zinc-700"}`}
          />
          {errors.target_label && <p className="mt-0.5 text-[10px] text-red-500">{errors.target_label}</p>}
        </div>

        {/* Target property */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.targetProperty")}{" "}
            <span className="normal-case text-muted-foreground">{t("form.targetPropertyHint")}</span>
          </label>
          <input
            value={form.target_property}
            onChange={(e) => update("target_property", { target_property: e.target.value })}
            placeholder={t("form.targetPropertyPlaceholder")}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Threshold */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.threshold")}
          </label>
          <input
            type="number"
            min={0}
            max={100}
            step={1}
            value={form.threshold}
            onChange={(e) => update("threshold", { threshold: Number(e.target.value) })}
            className={`mt-0.5 w-full rounded-md border bg-white px-3 py-1.5 text-xs dark:bg-zinc-900 ${errors.threshold ? "border-red-400 dark:border-red-600" : "border-zinc-200 dark:border-zinc-700"}`}
          />
          {errors.threshold && <p className="mt-0.5 text-[10px] text-red-500">{errors.threshold}</p>}
        </div>

        {/* Cypher check — only for custom type */}
        {form.rule_type === "custom" && (
          <div className="col-span-2">
            <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
              {t("form.cypherCheck")}
            </label>
            <textarea
              value={form.cypher_check}
              onChange={(e) => update("cypher_check", { cypher_check: e.target.value })}
              placeholder={t("form.cypherCheckPlaceholder")}
              rows={3}
              className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 font-mono text-xs dark:border-zinc-700 dark:bg-zinc-900"
            />
          </div>
        )}
      </div>

      <div className="mt-3 flex items-center gap-2">
        <button
          type="submit"
          disabled={!form.name.trim() || !form.target_label.trim() || saving}
          className="rounded-md bg-emerald-600 px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50 hover:bg-emerald-700"
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
          className="rounded-md px-3 py-1.5 text-xs text-muted-foreground hover:bg-zinc-100 dark:hover:bg-zinc-800"
        >
          {t("form.cancel")}
        </button>
      </div>
    </form>
  );
}
