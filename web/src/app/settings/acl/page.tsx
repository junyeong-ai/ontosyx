"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { request } from "@/lib/api/client";
import { Spinner } from "@/components/ui/spinner";

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
  const [policies, setPolicies] = useState<AclPolicy[]>([]);
  const [loading, setLoading] = useState(true);

  // Form state
  const [formOpen, setFormOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<PolicyFormValues>(EMPTY_FORM);
  const [saving, setSaving] = useState(false);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const data = await request<AclPolicy[]>("/acl/policies");
      setPolicies(data);
    } catch {
      toast.error("Failed to load ACL policies");
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
    setFormOpen(true);
  };

  // ---- Cancel ----
  const cancelForm = () => {
    setFormOpen(false);
    setEditingId(null);
    setForm(EMPTY_FORM);
  };

  // ---- Submit ----
  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!form.name.trim() || !form.subject_value.trim()) return;

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
        toast.success("Policy updated");
      } else {
        await request("/acl/policies", {
          method: "POST",
          body: JSON.stringify(body),
        });
        toast.success("Policy created");
      }
      cancelForm();
      await load();
    } catch {
      toast.error(
        editingId ? "Failed to update policy" : "Failed to create policy",
      );
    } finally {
      setSaving(false);
    }
  };

  // ---- Delete ----
  const handleDelete = async (id: string) => {
    setDeletingId(id);
    try {
      await request(`/acl/policies/${id}`, { method: "DELETE" });
      toast.success("Policy deleted");
      await load();
    } catch {
      toast.error("Failed to delete policy");
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
        return "text-zinc-500";
    }
  };

  return (
    <div>
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            Access Control
          </h1>
          <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
            Fine-grained ABAC policies for column-level masking and
            property-level deny on graph data.
          </p>
        </div>
        {!formOpen && (
          <button
            onClick={openCreate}
            className="rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700"
          >
            Create Policy
          </button>
        )}
      </div>

      {/* Inline form */}
      {formOpen && (
        <PolicyForm
          form={form}
          setForm={setForm}
          isEditing={!!editingId}
          saving={saving}
          onSubmit={handleSubmit}
          onCancel={cancelForm}
        />
      )}

      {/* Policies table */}
      <div className="mt-6">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-zinc-500 dark:border-zinc-700">
              <th className="py-2">Policy</th>
              <th className="py-2">Subject</th>
              <th className="py-2">Resource</th>
              <th className="py-2">Action</th>
              <th className="py-2">Properties</th>
              <th className="py-2">Priority</th>
              <th className="py-2 text-right">Actions</th>
            </tr>
          </thead>
          <tbody>
            {policies.map((p) => (
              <tr
                key={p.id}
                className="border-b border-zinc-100 dark:border-zinc-800"
              >
                <td className="py-2 font-medium text-zinc-900 dark:text-zinc-100">
                  {p.name}
                </td>
                <td className="py-2 text-zinc-500">
                  {p.subject_type}:{p.subject_value}
                </td>
                <td className="py-2 text-zinc-500">
                  {p.resource_value || p.resource_type}
                </td>
                <td className={`py-2 font-medium ${actionColor(p.action)}`}>
                  {p.action.toUpperCase()}
                  {p.action === "mask" && p.mask_pattern && (
                    <span className="ml-1 text-xs font-normal text-zinc-400">
                      ({p.mask_pattern})
                    </span>
                  )}
                </td>
                <td className="py-2 text-zinc-500">
                  {p.properties?.join(", ") || "all"}
                </td>
                <td className="py-2 text-zinc-500">{p.priority}</td>
                <td className="py-2 text-right">
                  <div className="flex items-center justify-end gap-1">
                    <button
                      onClick={() => openEdit(p)}
                      className="rounded px-2 py-1 text-xs text-zinc-500 hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
                    >
                      Edit
                    </button>
                    <button
                      onClick={() => handleDelete(p.id)}
                      disabled={deletingId === p.id}
                      className="rounded px-2 py-1 text-xs text-red-500 hover:bg-red-50 hover:text-red-700 disabled:opacity-50 dark:hover:bg-red-950"
                    >
                      {deletingId === p.id ? "..." : "Delete"}
                    </button>
                  </div>
                </td>
              </tr>
            ))}
            {policies.length === 0 && (
              <tr>
                <td colSpan={7} className="py-8 text-center text-zinc-400">
                  No ACL policies configured
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
  isEditing,
  saving,
  onSubmit,
  onCancel,
}: {
  form: PolicyFormValues;
  setForm: React.Dispatch<React.SetStateAction<PolicyFormValues>>;
  isEditing: boolean;
  saving: boolean;
  onSubmit: (e: React.FormEvent) => void;
  onCancel: () => void;
}) {
  const update = (patch: Partial<PolicyFormValues>) =>
    setForm((prev) => ({ ...prev, ...patch }));

  return (
    <form
      onSubmit={onSubmit}
      className="mt-4 rounded-lg border border-emerald-200 bg-emerald-50/50 p-4 dark:border-emerald-800 dark:bg-emerald-950/20"
    >
      <div className="mb-3 flex items-center justify-between">
        <span className="text-xs font-semibold text-emerald-700 dark:text-emerald-400">
          {isEditing ? "Edit Policy" : "New Policy"}
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
            onChange={(e) => update({ name: e.target.value })}
            placeholder="e.g. Mask PII for viewers"
            required
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Subject type */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Subject Type
          </label>
          <select
            value={form.subject_type}
            onChange={(e) => update({ subject_type: e.target.value })}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          >
            {SUBJECT_TYPES.map((t) => (
              <option key={t} value={t}>
                {t.replace(/_/g, " ")}
              </option>
            ))}
          </select>
        </div>

        {/* Subject value */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Subject Value
          </label>
          <input
            value={form.subject_value}
            onChange={(e) => update({ subject_value: e.target.value })}
            placeholder="e.g. viewer, admin"
            required
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Resource type */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Resource Type
          </label>
          <select
            value={form.resource_type}
            onChange={(e) => update({ resource_type: e.target.value })}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          >
            {RESOURCE_TYPES.map((t) => (
              <option key={t} value={t}>
                {t.replace(/_/g, " ")}
              </option>
            ))}
          </select>
        </div>

        {/* Resource value */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Resource Value{" "}
            <span className="normal-case text-zinc-400">(optional)</span>
          </label>
          <input
            value={form.resource_value}
            onChange={(e) => update({ resource_value: e.target.value })}
            placeholder="e.g. Customer"
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Action */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Action
          </label>
          <select
            value={form.action}
            onChange={(e) => update({ action: e.target.value })}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          >
            {ACTIONS.map((a) => (
              <option key={a} value={a}>
                {a}
              </option>
            ))}
          </select>
        </div>

        {/* Priority */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Priority
          </label>
          <input
            type="number"
            min={0}
            value={form.priority}
            onChange={(e) => update({ priority: Number(e.target.value) })}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Properties */}
        <div className="col-span-2">
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Properties{" "}
            <span className="normal-case text-zinc-400">
              (comma-separated, leave empty for all)
            </span>
          </label>
          <input
            value={form.properties}
            onChange={(e) => update({ properties: e.target.value })}
            placeholder="e.g. email, phone"
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Mask pattern — only for mask action */}
        {form.action === "mask" && (
          <div className="col-span-2">
            <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Mask Pattern
            </label>
            <input
              value={form.mask_pattern}
              onChange={(e) => update({ mask_pattern: e.target.value })}
              placeholder="e.g. ***"
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
              ? "Updating..."
              : "Creating..."
            : isEditing
              ? "Update Policy"
              : "Create Policy"}
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
