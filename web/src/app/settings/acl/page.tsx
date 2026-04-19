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

interface AclPolicy {
  id: string;
  name: string;
  subject_type: string;
  subject_value: string;
  resource_type: string;
  resource_value: string | null;
  action: string;
  properties: string[] | null;
  mask_pattern: string | null;
  priority: number;
  is_active: boolean;
}

const SUBJECT_TYPES = ["role", "workspace_role", "user"] as const;
const RESOURCE_TYPES = ["node_label", "edge_label", "all"] as const;
const ACTIONS = ["mask", "deny", "allow"] as const;

type KnownSubjectType = (typeof SUBJECT_TYPES)[number];
type KnownResourceType = (typeof RESOURCE_TYPES)[number];
type KnownAction = (typeof ACTIONS)[number];

function isKnownSubjectType(s: string): s is KnownSubjectType {
  return (SUBJECT_TYPES as readonly string[]).includes(s);
}

function isKnownResourceType(s: string): s is KnownResourceType {
  return (RESOURCE_TYPES as readonly string[]).includes(s);
}

function isKnownAction(s: string): s is KnownAction {
  return (ACTIONS as readonly string[]).includes(s);
}

type PolicyFormValues = {
  name: string;
  subject_type: string;
  subject_value: string;
  resource_type: string;
  resource_value: string;
  action: string;
  properties: string;
  mask_pattern: string;
  priority: number;
};

const EMPTY_FORM: PolicyFormValues = {
  name: "",
  subject_type: "role",
  subject_value: "",
  resource_type: "node_label",
  resource_value: "",
  action: "deny",
  properties: "",
  mask_pattern: "",
  priority: 0,
};

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function AclSettingsPage() {
  const t = useTranslations("settings.acl");
  const [policies, setPolicies] = useState<AclPolicy[]>([]);
  const [loading, setLoading] = useState(true);

  // Form state
  const [formOpen, setFormOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<PolicyFormValues>(EMPTY_FORM);
  const [saving, setSaving] = useState(false);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const confirm = useConfirm();

  const load = useCallback(async () => {
    try {
      const data = await request<AclPolicy[]>("/acl/policies");
      setPolicies(data);
    } catch {
      toast.error(t("loadError"));
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
  const openEdit = (p: AclPolicy) => {
    setEditingId(p.id);
    setForm({
      name: p.name,
      subject_type: p.subject_type,
      subject_value: p.subject_value,
      resource_type: p.resource_type,
      resource_value: p.resource_value ?? "",
      action: p.action,
      properties: p.properties?.join(", ") ?? "",
      mask_pattern: p.mask_pattern ?? "",
      priority: p.priority,
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
    if (!form.name.trim()) e.name = t("required");
    if (!form.subject_value.trim()) e.subject_value = t("required");
    setErrors(e);
    return Object.keys(e).length === 0;
  };

  // ---- Submit ----
  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;

    const propsArray = form.properties
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);

    const body: Record<string, unknown> = {
      name: form.name.trim(),
      subject_type: form.subject_type,
      subject_value: form.subject_value.trim(),
      resource_type: form.resource_type,
      resource_value: form.resource_value.trim() || null,
      action: form.action,
      properties: propsArray.length > 0 ? propsArray : null,
      mask_pattern:
        form.action === "mask" && form.mask_pattern.trim()
          ? form.mask_pattern.trim()
          : null,
      priority: form.priority,
    };

    setSaving(true);
    try {
      if (editingId) {
        await request(`/acl/policies/${editingId}`, {
          method: "PATCH",
          body: JSON.stringify(body),
        });
        toast.success(t("updatedToast"));
      } else {
        await request("/acl/policies", {
          method: "POST",
          body: JSON.stringify(body),
        });
        toast.success(t("createdToast"));
      }
      cancelForm();
      await load();
    } catch {
      toast.error(editingId ? t("updateError") : t("createError"));
    } finally {
      setSaving(false);
    }
  };

  // ---- Delete ----
  const handleDelete = async (id: string) => {
    const policy = policies.find((p) => p.id === id);
    const ok = await confirm({
      title: t("deleteConfirmTitle", { name: policy?.name ?? id }),
      description: t("deleteConfirmDescription"),
      variant: "danger",
    });
    if (!ok) return;
    setDeletingId(id);
    try {
      await request(`/acl/policies/${id}`, { method: "DELETE" });
      toast.success(t("deletedToast"));
      await load();
    } catch {
      toast.error(t("deleteError"));
    } finally {
      setDeletingId(null);
    }
  };

  if (loading) return <Spinner />;

  const actionColor = (action: string) => {
    switch (action) {
      case "deny":
        return "text-red-600 dark:text-red-400";
      case "mask":
        return "text-amber-600 dark:text-amber-400";
      case "allow":
        return "text-emerald-600 dark:text-emerald-400";
      default:
        return "text-muted-foreground";
    }
  };

  const subjectTypeLabel = (value: string): string =>
    isKnownSubjectType(value) ? t(`subjectType.${value}`) : value;

  const resourceTypeLabel = (value: string): string =>
    isKnownResourceType(value) ? t(`resourceType.${value}`) : value;

  const actionLabel = (value: string): string =>
    isKnownAction(value) ? t(`actionLabel.${value}`) : value;

  const actionBadge = (value: string): string =>
    isKnownAction(value) ? t(`actionBadge.${value}`) : value.toUpperCase();

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
          <button
            onClick={openCreate}
            className="rounded-md bg-emerald-700 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-800"
          >
            {t("createPolicy")}
          </button>
        )}
      </div>

      {/* Inline form */}
      {formOpen && (
        <PolicyForm
          form={form}
          setForm={setForm}
          errors={errors}
          clearError={clearError}
          isEditing={!!editingId}
          saving={saving}
          onSubmit={handleSubmit}
          onCancel={cancelForm}
          subjectTypeLabel={subjectTypeLabel}
          resourceTypeLabel={resourceTypeLabel}
          actionLabel={actionLabel}
        />
      )}

      {/* Policies table */}
      <div className="mt-6 overflow-x-auto -mx-6 px-6" tabIndex={0} role="region" aria-label="Table data — scroll horizontally">
        <table className="w-full min-w-[960px] text-sm">
          <thead>
            <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-muted-foreground dark:border-zinc-700">
              <th className="py-3 pr-6">{t("column.policy")}</th>
              <th className="py-3 pr-6">{t("column.subject")}</th>
              <th className="py-3 pr-6">{t("column.resource")}</th>
              <th className="py-3 pr-6">{t("column.action")}</th>
              <th className="py-3 pr-6">{t("column.properties")}</th>
              <th className="py-3 pr-6">{t("column.priority")}</th>
              <th className="py-3 pr-6 text-right">{t("column.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {policies.map((p) => (
              <tr
                key={p.id}
                className="border-b border-zinc-100 dark:border-zinc-800"
              >
                <td className="py-3 pr-6 font-medium text-zinc-900 dark:text-zinc-100">
                  {p.name}
                </td>
                <td className="py-3 pr-6 text-muted-foreground">
                  {subjectTypeLabel(p.subject_type)}:{p.subject_value}
                </td>
                <td className="py-3 pr-6 text-muted-foreground">
                  {p.resource_value || resourceTypeLabel(p.resource_type)}
                </td>
                <td className={`py-2 font-medium ${actionColor(p.action)}`}>
                  {actionBadge(p.action)}
                  {p.action === "mask" && p.mask_pattern && (
                    <span className="ml-1 text-xs font-normal text-muted-foreground">
                      ({p.mask_pattern})
                    </span>
                  )}
                </td>
                <td className="py-3 pr-6 text-muted-foreground">
                  {p.properties?.join(", ") || t("allProperties")}
                </td>
                <td className="py-3 pr-6 text-muted-foreground">{p.priority}</td>
                <td className="py-3 pr-6 text-right">
                  <div className="flex items-center justify-end gap-1">
                    <button
                      onClick={() => openEdit(p)}
                      className="rounded px-2 py-1 text-xs text-muted-foreground hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
                    >
                      {t("edit")}
                    </button>
                    <button
                      onClick={() => handleDelete(p.id)}
                      disabled={deletingId === p.id}
                      className="rounded px-2 py-1 text-xs text-red-500 hover:bg-red-50 hover:text-red-700 disabled:opacity-50 dark:hover:bg-red-950"
                    >
                      {deletingId === p.id ? t("deleting") : t("delete")}
                    </button>
                  </div>
                </td>
              </tr>
            ))}
            {policies.length === 0 && (
              <tr>
                <td colSpan={7} className="py-8 text-center text-muted-foreground">
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
// Policy form (create / edit)
// ---------------------------------------------------------------------------

function PolicyForm({
  form,
  setForm,
  errors,
  clearError,
  isEditing,
  saving,
  onSubmit,
  onCancel,
  subjectTypeLabel,
  resourceTypeLabel,
  actionLabel,
}: {
  form: PolicyFormValues;
  setForm: React.Dispatch<React.SetStateAction<PolicyFormValues>>;
  errors: Record<string, string>;
  clearError: (field: string) => void;
  isEditing: boolean;
  saving: boolean;
  onSubmit: (e: React.FormEvent) => void;
  onCancel: () => void;
  subjectTypeLabel: (value: string) => string;
  resourceTypeLabel: (value: string) => string;
  actionLabel: (value: string) => string;
}) {
  const t = useTranslations("settings.acl");

  const update = (field: string, patch: Partial<PolicyFormValues>) => {
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
          {isEditing ? t("editPolicyHeading") : t("newPolicyHeading")}
        </span>
        <button
          type="button"
          onClick={onCancel}
          className="text-xs text-muted-foreground hover:text-zinc-600"
        >
          {t("cancel")}
        </button>
      </div>

      <div className="grid grid-cols-2 gap-3">
        {/* Name */}
        <div className="col-span-2">
          <label htmlFor="acl-rule-name" className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("field.name")}
          </label>
          <input
            id="acl-rule-name"
            value={form.name}
            onChange={(e) => update("name", { name: e.target.value })}
            placeholder={t("placeholder.name")}
            required
            className={`mt-0.5 w-full rounded-md border bg-white px-3 py-1.5 text-xs dark:bg-zinc-900 ${errors.name ? "border-red-400 dark:border-red-600" : "border-zinc-200 dark:border-zinc-700"}`}
          />
          {errors.name && <p className="mt-0.5 text-[10px] text-red-500">{errors.name}</p>}
        </div>

        {/* Subject type */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("field.subjectType")}
          </label>
          <SettingsSelect
            label={t("field.subjectType")}
            hideLabel
            value={form.subject_type}
            onChange={(e) => update("subject_type", { subject_type: e.target.value })}
          >
            {SUBJECT_TYPES.map((st) => (
              <option key={st} value={st}>
                {subjectTypeLabel(st)}
              </option>
            ))}
          </SettingsSelect>
        </div>

        {/* Subject value */}
        <div>
          <label htmlFor="acl-rule-subject-value" className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("field.subjectValue")}
          </label>
          <input
            id="acl-rule-subject-value"
            value={form.subject_value}
            onChange={(e) => update("subject_value", { subject_value: e.target.value })}
            placeholder={t("placeholder.subjectValue")}
            required
            className={`mt-0.5 w-full rounded-md border bg-white px-3 py-1.5 text-xs dark:bg-zinc-900 ${errors.subject_value ? "border-red-400 dark:border-red-600" : "border-zinc-200 dark:border-zinc-700"}`}
          />
          {errors.subject_value && <p className="mt-0.5 text-[10px] text-red-500">{errors.subject_value}</p>}
        </div>

        {/* Resource type */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("field.resourceType")}
          </label>
          <SettingsSelect
            label={t("field.resourceType")}
            hideLabel
            value={form.resource_type}
            onChange={(e) => update("resource_type", { resource_type: e.target.value })}
          >
            {RESOURCE_TYPES.map((rt) => (
              <option key={rt} value={rt}>
                {resourceTypeLabel(rt)}
              </option>
            ))}
          </SettingsSelect>
        </div>

        {/* Resource value */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t.rich("field.resourceValueHint", {
              hint: (chunks) => (
                <span className="normal-case text-muted-foreground">{chunks}</span>
              ),
            })}
          </label>
          <input
            value={form.resource_value}
            onChange={(e) => update("resource_value", { resource_value: e.target.value })}
            placeholder={t("placeholder.resourceValue")}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Action */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("field.action")}
          </label>
          <SettingsSelect
            label={t("field.action")}
            hideLabel
            value={form.action}
            onChange={(e) => update("action", { action: e.target.value })}
          >
            {ACTIONS.map((a) => (
              <option key={a} value={a}>
                {actionLabel(a)}
              </option>
            ))}
          </SettingsSelect>
        </div>

        {/* Priority */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("field.priority")}
          </label>
          <input
            type="number"
            min={0}
            value={form.priority}
            onChange={(e) => update("priority", { priority: Number(e.target.value) })}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Properties */}
        <div className="col-span-2">
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t.rich("field.propertiesHint", {
              hint: (chunks) => (
                <span className="normal-case text-muted-foreground">{chunks}</span>
              ),
            })}
          </label>
          <input
            value={form.properties}
            onChange={(e) => update("properties", { properties: e.target.value })}
            placeholder={t("placeholder.properties")}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Mask pattern — only for mask action */}
        {form.action === "mask" && (
          <div className="col-span-2">
            <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
              {t("field.maskPattern")}
            </label>
            <input
              value={form.mask_pattern}
              onChange={(e) => update("mask_pattern", { mask_pattern: e.target.value })}
              placeholder={t("placeholder.maskPattern")}
              className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
            />
          </div>
        )}
      </div>

      <div className="mt-3 flex items-center gap-2">
        <button
          type="submit"
          disabled={!form.name.trim() || !form.subject_value.trim() || saving}
          className="rounded-md bg-emerald-600 px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50 hover:bg-emerald-700"
        >
          {saving
            ? isEditing
              ? t("updating")
              : t("creating")
            : isEditing
              ? t("updatePolicy")
              : t("createPolicy")}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md px-3 py-1.5 text-xs text-muted-foreground hover:bg-zinc-100 dark:hover:bg-zinc-800"
        >
          {t("cancel")}
        </button>
      </div>
    </form>
  );
}
