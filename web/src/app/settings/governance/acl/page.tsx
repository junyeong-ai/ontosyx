"use client";

import { useState } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";

import { request } from "@/lib/api/client";
import { SkeletonTable } from "@/components/ui/skeleton";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { FormInput, SettingsSelect } from "@/components/ui/form-input";
import { Button } from "@/components/ui/button";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { useConfirm } from "@/components/providers/confirm-provider";

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

const aclKeys = {
  all: ["acl"] as const,
  policies: () => [...aclKeys.all, "policies"] as const,
};

export default function AclSettingsPage() {
  const t = useTranslations("settings.governance.acl");
  const tCommon = useTranslations("common");
  const qc = useQueryClient();

  // Form state
  const [formOpen, setFormOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<PolicyFormValues>(EMPTY_FORM);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const confirm = useConfirm();

  const policiesQuery = useQuery({
    queryKey: aclKeys.policies(),
    queryFn: () => request<AclPolicy[]>("/acl/policies"),
  });
  const policies = policiesQuery.data ?? [];
  const reload = () =>
    qc.invalidateQueries({ queryKey: aclKeys.policies() });

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

  const submitMutation = useMutation({
    mutationFn: async (body: Record<string, unknown>) => {
      if (editingId) {
        await request(`/acl/policies/${editingId}`, {
          method: "PATCH",
          body: JSON.stringify(body),
        });
      } else {
        await request("/acl/policies", {
          method: "POST",
          body: JSON.stringify(body),
        });
      }
    },
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
    mutationFn: (id: string) =>
      request(`/acl/policies/${id}`, { method: "DELETE" }),
    onSuccess: () => {
      toast.success(t("toast.deleted"));
      reload();
    },
    onError: () => toast.error(t("toast.deleteFailed")),
  });

  // ---- Submit ----
  const handleSubmit = (e: React.FormEvent) => {
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

    submitMutation.mutate(body);
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
    deleteMutation.mutate(id);
  };

  const saving = submitMutation.isPending;
  const deletingId = deleteMutation.isPending ? deleteMutation.variables : null;

  if (policiesQuery.isLoading || policiesQuery.isError) {
    const pageState: PageState = policiesQuery.isLoading
      ? { kind: "loading" }
      : { kind: "error", onRetry: () => void policiesQuery.refetch() };
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <PageStateView
          state={pageState}
          skeleton={<SkeletonTable rows={5} cols={5} />}
          error={{
            title: tCommon("loadError.title"),
            description: tCommon("loadError.description"),
            retryLabel: tCommon("retry"),
          }}
        >
          <></>
        </PageStateView>
      </SettingsPageShell>
    );
  }

  const actionColor = (action: string) => {
    switch (action) {
      case "deny":
        return "text-danger-foreground";
      case "mask":
        return "text-warning-foreground";
      case "allow":
        return "text-brand-foreground";
      default:
        return "text-foreground-muted";
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
    <SettingsPageShell
      title={t("title")}
      subtitle={t("description")}
      actions={
        !formOpen && (
          <Button variant="primary" size="sm" onClick={openCreate}>
            {t("createPolicy")}
          </Button>
        )
      }
    >
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
      <div className="mt-6 overflow-x-auto -mx-6 px-6" tabIndex={0} role="region" aria-label={tCommon("scrollableTableAria")}>
        <table className="w-full min-w-[960px] text-sm">
          <thead>
            <tr className="border-b border-divider text-start text-xs font-medium uppercase text-foreground-muted">
              <th className="py-3 pe-6">{t("column.policy")}</th>
              <th className="py-3 pe-6">{t("column.subject")}</th>
              <th className="py-3 pe-6">{t("column.resource")}</th>
              <th className="py-3 pe-6">{t("column.action")}</th>
              <th className="py-3 pe-6">{t("column.properties")}</th>
              <th className="py-3 pe-6">{t("column.priority")}</th>
              <th className="py-3 pe-6 text-end">{t("column.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {policies.map((p) => (
              <tr
                key={p.id}
                className="border-b border-divider-soft"
              >
                <td className="py-3 pe-6 font-medium text-foreground-strong">
                  {p.name}
                </td>
                <td className="py-3 pe-6 text-foreground-muted">
                  {subjectTypeLabel(p.subject_type)}:{p.subject_value}
                </td>
                <td className="py-3 pe-6 text-foreground-muted">
                  {p.resource_value || resourceTypeLabel(p.resource_type)}
                </td>
                <td className={`py-2 font-medium ${actionColor(p.action)}`}>
                  {actionBadge(p.action)}
                  {p.action === "mask" && p.mask_pattern && (
                    <span className="ms-1 text-xs font-normal text-foreground-muted">
                      ({p.mask_pattern})
                    </span>
                  )}
                </td>
                <td className="py-3 pe-6 text-foreground-muted">
                  {p.properties?.join(", ") || t("allProperties")}
                </td>
                <td className="py-3 pe-6 text-foreground-muted">{p.priority}</td>
                <td className="py-3 pe-6 text-end">
                  <div className="flex items-center justify-end gap-1">
                    <button
                      type="button"
                      onClick={() => openEdit(p)}
                      className="rounded px-2 py-1 text-xs text-foreground-muted hover:bg-surface-inset hover:text-foreground-muted"
                    >
                      {tCommon("edit")}
                    </button>
                    <button
                      type="button"
                      onClick={() => handleDelete(p.id)}
                      disabled={deletingId === p.id}
                      className="rounded px-2 py-1 text-xs text-danger-foreground hover:bg-danger-surface hover:text-danger-foreground disabled:opacity-50"
                    >
                      {deletingId === p.id ? tCommon("deleting") : tCommon("delete")}
                    </button>
                  </div>
                </td>
              </tr>
            ))}
            {policies.length === 0 && (
              <tr>
                <td colSpan={7} className="py-8 text-center text-foreground-muted">
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
  const t = useTranslations("settings.governance.acl");
  const tCommon = useTranslations("common");

  const update = (field: string, patch: Partial<PolicyFormValues>) => {
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
          {isEditing ? t("editPolicyHeading") : t("newPolicyHeading")}
        </span>
        <button
          type="button"
          onClick={onCancel}
          className="text-xs text-foreground-muted hover:text-foreground"
        >
          {tCommon("cancel")}
        </button>
      </div>

      <div className="grid grid-cols-2 gap-3">
        {/* Name */}
        <div className="col-span-2">
          <label htmlFor="acl-rule-name" className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("field.name")}
          </label>
          <FormInput
            id="acl-rule-name"
            value={form.name}
            onChange={(e) => update("name", { name: e.target.value })}
            placeholder={t("placeholder.name")}
            required
            error={!!errors.name}
            className="mt-0.5 text-xs"
          />
          {errors.name && <p className="mt-0.5 text-2xs text-danger-foreground">{errors.name}</p>}
        </div>

        {/* Subject type */}
        <label className="block">
          <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("field.subjectType")}
          </span>
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
        </label>

        {/* Subject value */}
        <div>
          <label htmlFor="acl-rule-subject-value" className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("field.subjectValue")}
          </label>
          <FormInput
            id="acl-rule-subject-value"
            value={form.subject_value}
            onChange={(e) => update("subject_value", { subject_value: e.target.value })}
            placeholder={t("placeholder.subjectValue")}
            required
            error={!!errors.subject_value}
            className="mt-0.5 text-xs"
          />
          {errors.subject_value && <p className="mt-0.5 text-2xs text-danger-foreground">{errors.subject_value}</p>}
        </div>

        {/* Resource type */}
        <label className="block">
          <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("field.resourceType")}
          </span>
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
        </label>

        {/* Resource value */}
        <label className="block">
          <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t.rich("field.resourceValueHint", {
              hint: (chunks) => (
                <span className="normal-case text-foreground-muted">{chunks}</span>
              ),
            })}
          </span>
          <FormInput
            value={form.resource_value}
            onChange={(e) => update("resource_value", { resource_value: e.target.value })}
            placeholder={t("placeholder.resourceValue")}
            className="mt-0.5 text-xs"
          />
        </label>

        {/* Action */}
        <label className="block">
          <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("field.action")}
          </span>
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
        </label>

        {/* Priority */}
        <label className="block">
          <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("field.priority")}
          </span>
          <FormInput
            type="number"
            min={0}
            value={form.priority}
            onChange={(e) => update("priority", { priority: Number(e.target.value) })}
            className="mt-0.5 text-xs"
          />
        </label>

        {/* Properties */}
        <label className="block col-span-2">
          <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t.rich("field.propertiesHint", {
              hint: (chunks) => (
                <span className="normal-case text-foreground-muted">{chunks}</span>
              ),
            })}
          </span>
          <FormInput
            value={form.properties}
            onChange={(e) => update("properties", { properties: e.target.value })}
            placeholder={t("placeholder.properties")}
            className="mt-0.5 text-xs"
          />
        </label>

        {/* Mask pattern — only for mask action */}
        {form.action === "mask" && (
          <label className="block col-span-2">
            <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {t("field.maskPattern")}
            </span>
            <FormInput
              value={form.mask_pattern}
              onChange={(e) => update("mask_pattern", { mask_pattern: e.target.value })}
              placeholder={t("placeholder.maskPattern")}
              className="mt-0.5 text-xs"
            />
          </label>
        )}
      </div>

      <div className="mt-3 flex items-center gap-2">
        <Button
          type="submit"
          variant="primary"
          size="sm"
          disabled={!form.name.trim() || !form.subject_value.trim()}
          loading={saving}
        >
          {isEditing ? t("updatePolicy") : t("createPolicy")}
        </Button>
        <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
          {tCommon("cancel")}
        </Button>
      </div>
    </form>
  );
}
