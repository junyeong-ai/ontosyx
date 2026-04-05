"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { request } from "@/lib/api/client";
import { Spinner } from "@/components/ui/spinner";
import { SettingsSelect } from "@/components/ui/form-input";
import { useConfirm } from "@/components/ui/confirm-dialog";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface QualityRule {
  id: string;
  name: string;
  rule_type: string;
  target_label: string;
  target_property: string | null;
  threshold: number;
  severity: string;
  cypher_check: string | null;
  is_active: boolean;
  created_at: string;
}

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

const SEVERITIES = ["critical", "warning", "info"] as const;

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
      toast.error("Failed to load quality dashboard");
    } finally {
      setLoading(false);
    }
  }, []);

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
    if (errors[field]) setErrors((prev) => { const { [field]: _, ...rest } = prev; return rest; });
  };

  // ---- Validate ----
  const validate = (): boolean => {
    const e: Record<string, string> = {};
    if (!form.name.trim()) e.name = "Required";
    if (!form.target_label.trim()) e.target_label = "Required";
    if (form.threshold < 0 || form.threshold > 100) e.threshold = "Must be 0-100";
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
        toast.success("Rule updated");
      } else {
        await request("/quality/rules", {
          method: "POST",
          body: JSON.stringify(body),
        });
        toast.success("Rule created");
      }
      cancelForm();
      await load();
    } catch {
      toast.error(editingId ? "Failed to update rule" : "Failed to create rule");
    } finally {
      setSaving(false);
    }
  };

  // ---- Delete ----
  const handleDelete = async (ruleId: string) => {
    const rule = dashboard.find((d) => d.rule_id === ruleId);
    const ok = await confirm({
      title: `Delete quality rule '${rule?.name ?? ruleId}'?`,
      description: "This action cannot be undone. The rule and its evaluation history will be permanently removed.",
      variant: "danger",
    });
    if (!ok) return;
    setDeletingId(ruleId);
    try {
      await request(`/quality/rules/${ruleId}`, { method: "DELETE" });
      toast.success("Rule deleted");
      await load();
    } catch {
      toast.error("Failed to delete rule");
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
      toast.success(
        result.passed
          ? `Rule passed (${result.actual_value?.toFixed(1) ?? "-"}%)`
          : `Rule failed (${result.actual_value?.toFixed(1) ?? "-"}%)`,
      );
      await load();
    } catch {
      toast.error("Failed to execute rule");
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
        `Executed ${results.length} rules: ${passedCount} passed, ${results.length - passedCount} failed`,
      );
      await load();
    } catch {
      toast.error("Failed to execute rules");
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
            Quality Rules
          </h1>
          <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
            Declarative data quality rules evaluated automatically against graph
            data.
          </p>
        </div>
        {!formOpen && (
          <div className="flex items-center gap-2">
            <button
              onClick={handleExecuteAll}
              disabled={executingAll || dashboard.length === 0}
              className="rounded-md border border-zinc-300 bg-white px-3 py-1.5 text-xs font-medium text-zinc-700 hover:bg-zinc-50 disabled:opacity-50 dark:border-zinc-600 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-zinc-700"
            >
              {executingAll ? "Executing..." : "Execute All"}
            </button>
            <button
              onClick={openCreate}
              className="rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700"
            >
              Create Rule
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
          <div className="text-xs text-emerald-600 dark:text-emerald-500">
            Passing
          </div>
        </div>
        <div className="rounded-lg border border-red-200 bg-red-50 p-4 dark:border-red-900 dark:bg-red-950">
          <div className="text-2xl font-bold text-red-700 dark:text-red-400">
            {failed}
          </div>
          <div className="text-xs text-red-600 dark:text-red-500">Failing</div>
        </div>
        <div className="rounded-lg border border-zinc-200 bg-zinc-50 p-4 dark:border-zinc-700 dark:bg-zinc-900">
          <div className="text-2xl font-bold text-zinc-700 dark:text-zinc-300">
            {pending}
          </div>
          <div className="text-xs text-zinc-500">Not Yet Evaluated</div>
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
      <div className="mt-6 overflow-x-auto -mx-6 px-6">
        <table className="w-full min-w-[900px] text-sm">
          <thead>
            <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-zinc-500 dark:border-zinc-700">
              <th className="py-3 pr-6">Rule</th>
              <th className="py-3 pr-6">Type</th>
              <th className="py-3 pr-6">Target</th>
              <th className="py-3 pr-6">Threshold</th>
              <th className="py-3 pr-6">Severity</th>
              <th className="py-3 pr-6">Status</th>
              <th className="py-3 pr-6">Value</th>
              <th className="py-3 pr-6 text-right">Actions</th>
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
                <td className="py-3 pr-6 text-zinc-500">{d.rule_type}</td>
                <td className="py-3 pr-6 text-zinc-500">
                  {d.target_label}
                  {d.target_property ? `.${d.target_property}` : ""}
                </td>
                <td className="py-3 pr-6 text-zinc-500">{d.threshold}%</td>
                <td className="py-3 pr-6">
                  <SeverityBadge severity={d.severity} />
                </td>
                <td className="py-3 pr-6">
                  {d.latest_passed === null ? (
                    <span className="text-zinc-400">-</span>
                  ) : d.latest_passed ? (
                    <span className="text-emerald-600 dark:text-emerald-400">
                      Pass
                    </span>
                  ) : (
                    <span className="text-red-600 dark:text-red-400">Fail</span>
                  )}
                </td>
                <td className="py-3 pr-6 text-zinc-500">
                  {d.latest_value !== null
                    ? `${d.latest_value.toFixed(1)}%`
                    : "-"}
                </td>
                <td className="py-3 pr-6 text-right">
                  <div className="flex items-center justify-end gap-1">
                    <button
                      onClick={() => handleExecute(d.rule_id)}
                      disabled={executingId === d.rule_id || executingAll}
                      className="rounded px-2 py-1 text-xs text-emerald-600 hover:bg-emerald-50 hover:text-emerald-700 disabled:opacity-50 dark:text-emerald-400 dark:hover:bg-emerald-950"
                    >
                      {executingId === d.rule_id ? "..." : "Run"}
                    </button>
                    <button
                      onClick={() => openEdit(d)}
                      className="rounded px-2 py-1 text-xs text-zinc-500 hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
                    >
                      Edit
                    </button>
                    <button
                      onClick={() => handleDelete(d.rule_id)}
                      disabled={deletingId === d.rule_id}
                      className="rounded px-2 py-1 text-xs text-red-500 hover:bg-red-50 hover:text-red-700 disabled:opacity-50 dark:hover:bg-red-950"
                    >
                      {deletingId === d.rule_id ? "..." : "Delete"}
                    </button>
                  </div>
                </td>
              </tr>
            ))}
            {dashboard.length === 0 && (
              <tr>
                <td colSpan={8} className="py-8 text-center text-zinc-400">
                  No quality rules configured
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
  const color =
    severity === "critical"
      ? "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400"
      : severity === "warning"
        ? "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400"
        : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400";

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ${color}`}
    >
      {severity}
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
          {isEditing ? "Edit Rule" : "New Rule"}
        </span>
        <button
          type="button"
          onClick={onCancel}
          className="text-xs text-zinc-400 hover:text-zinc-600"
        >
          Cancel
        </button>
      </div>

      <div className="grid grid-cols-2 gap-3">
        {/* Name */}
        <div className="col-span-2">
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Name
          </label>
          <input
            value={form.name}
            onChange={(e) => update("name", { name: e.target.value })}
            placeholder="e.g. Brand completeness"
            required
            className={`mt-0.5 w-full rounded-md border bg-white px-3 py-1.5 text-xs dark:bg-zinc-900 ${errors.name ? "border-red-400 dark:border-red-600" : "border-zinc-200 dark:border-zinc-700"}`}
          />
          {errors.name && <p className="mt-0.5 text-[10px] text-red-500">{errors.name}</p>}
        </div>

        {/* Rule type */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Rule Type
          </label>
          <SettingsSelect
            value={form.rule_type}
            onChange={(e) => update("rule_type", { rule_type: e.target.value })}
          >
            {RULE_TYPES.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </SettingsSelect>
        </div>

        {/* Severity */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Severity
          </label>
          <SettingsSelect
            value={form.severity}
            onChange={(e) => update("severity", { severity: e.target.value })}
          >
            {SEVERITIES.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </SettingsSelect>
        </div>

        {/* Target label */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Target Label
          </label>
          <input
            value={form.target_label}
            onChange={(e) => update("target_label", { target_label: e.target.value })}
            placeholder="e.g. Brand, Product"
            required
            className={`mt-0.5 w-full rounded-md border bg-white px-3 py-1.5 text-xs dark:bg-zinc-900 ${errors.target_label ? "border-red-400 dark:border-red-600" : "border-zinc-200 dark:border-zinc-700"}`}
          />
          {errors.target_label && <p className="mt-0.5 text-[10px] text-red-500">{errors.target_label}</p>}
        </div>

        {/* Target property */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Target Property{" "}
            <span className="normal-case text-zinc-400">(optional)</span>
          </label>
          <input
            value={form.target_property}
            onChange={(e) => update("target_property", { target_property: e.target.value })}
            placeholder="e.g. email"
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Threshold */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Threshold (%)
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
            <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Cypher Check
            </label>
            <textarea
              value={form.cypher_check}
              onChange={(e) => update("cypher_check", { cypher_check: e.target.value })}
              placeholder="MATCH (n:Label) WHERE ... RETURN count(n) as value"
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
              ? "Updating..."
              : "Creating..."
            : isEditing
              ? "Update Rule"
              : "Create Rule"}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md px-3 py-1.5 text-xs text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
        >
          Cancel
        </button>
      </div>
    </form>
  );
}
