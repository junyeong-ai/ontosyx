"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { Spinner } from "@/components/ui/spinner";
import { SettingsSelect, SettingsSwitch } from "@/components/ui/form-input";
import { useConfirm } from "@/components/ui/confirm-dialog";
import {
  listChannels,
  createChannel,
  updateChannel,
  deleteChannel,
  testChannel,
  listLogs,
  type NotificationChannel,
  type NotificationLog,
} from "@/lib/api/notifications";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const CHANNEL_TYPES = [
  { value: "slack_webhook", label: "Slack Webhook" },
  { value: "generic_webhook", label: "Generic Webhook" },
] as const;

const EVENT_TYPES = [
  { value: "quality_rule_failed", label: "Quality Rule Failed" },
  { value: "quality_rule_passed", label: "Quality Rule Passed" },
] as const;

type ChannelFormValues = {
  name: string;
  channel_type: string;
  url: string;
  events: string[];
};

const EMPTY_FORM: ChannelFormValues = {
  name: "",
  channel_type: "slack_webhook",
  url: "",
  events: ["quality_rule_failed"],
};

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function NotificationsSettingsPage() {
  const [channels, setChannels] = useState<NotificationChannel[]>([]);
  const [logs, setLogs] = useState<NotificationLog[]>([]);
  const [loading, setLoading] = useState(true);

  // Form state
  const [formOpen, setFormOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<ChannelFormValues>(EMPTY_FORM);
  const [saving, setSaving] = useState(false);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [testingId, setTestingId] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const confirm = useConfirm();

  const load = useCallback(async () => {
    try {
      const [ch, lg] = await Promise.all([listChannels(), listLogs(50)]);
      setChannels(ch);
      setLogs(lg);
    } catch {
      toast.error("Failed to load notification settings");
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
  const openEdit = (ch: NotificationChannel) => {
    setEditingId(ch.id);
    setForm({
      name: ch.name,
      channel_type: ch.channel_type,
      url: (ch.config.url as string) ?? "",
      events: [...ch.events],
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
    if (errors[field])
      setErrors((prev) => {
        const { [field]: _, ...rest } = prev;
        return rest;
      });
  };

  // ---- Validate ----
  const validate = (): boolean => {
    const e: Record<string, string> = {};
    if (!form.name.trim()) e.name = "Required";
    if (!form.url.trim()) e.url = "Required";
    try {
      new URL(form.url.trim());
    } catch {
      if (form.url.trim()) e.url = "Invalid URL";
    }
    if (form.events.length === 0) e.events = "Select at least one event";
    setErrors(e);
    return Object.keys(e).length === 0;
  };

  // ---- Submit (create or update) ----
  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;

    setSaving(true);
    try {
      if (editingId) {
        await updateChannel(editingId, {
          name: form.name.trim(),
          config: { url: form.url.trim() },
          events: form.events,
        });
        toast.success("Channel updated");
      } else {
        await createChannel({
          name: form.name.trim(),
          channel_type: form.channel_type,
          config: { url: form.url.trim() },
          events: form.events,
        });
        toast.success("Channel created");
      }
      cancelForm();
      await load();
    } catch {
      toast.error(
        editingId ? "Failed to update channel" : "Failed to create channel",
      );
    } finally {
      setSaving(false);
    }
  };

  // ---- Delete ----
  const handleDelete = async (id: string) => {
    const ch = channels.find((c) => c.id === id);
    const ok = await confirm({
      title: `Delete notification channel '${ch?.name ?? id}'?`,
      description:
        "This action cannot be undone. The channel and its delivery log will be permanently removed.",
      variant: "danger",
    });
    if (!ok) return;
    setDeletingId(id);
    try {
      await deleteChannel(id);
      toast.success("Channel deleted");
      await load();
    } catch {
      toast.error("Failed to delete channel");
    } finally {
      setDeletingId(null);
    }
  };

  // ---- Toggle enabled ----
  const handleToggle = async (ch: NotificationChannel) => {
    try {
      await updateChannel(ch.id, { enabled: !ch.enabled });
      toast.success(ch.enabled ? "Channel disabled" : "Channel enabled");
      await load();
    } catch {
      toast.error("Failed to toggle channel");
    }
  };

  // ---- Test ----
  const handleTest = async (id: string) => {
    setTestingId(id);
    try {
      const result = await testChannel(id);
      if (result.success) {
        toast.success("Test notification sent successfully");
      } else {
        toast.error(`Test failed: ${result.error ?? "Unknown error"}`);
      }
      await load();
    } catch {
      toast.error("Failed to send test notification");
    } finally {
      setTestingId(null);
    }
  };

  // ---- Event checkbox toggle ----
  const toggleEvent = (eventValue: string) => {
    setForm((prev) => ({
      ...prev,
      events: prev.events.includes(eventValue)
        ? prev.events.filter((e) => e !== eventValue)
        : [...prev.events, eventValue],
    }));
    clearError("events");
  };

  if (loading) return <Spinner />;

  return (
    <div>
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            Notifications
          </h1>
          <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
            Configure webhook channels to receive notifications when quality
            rules are evaluated.
          </p>
        </div>
        {!formOpen && (
          <button
            onClick={openCreate}
            className="rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700"
          >
            Add Channel
          </button>
        )}
      </div>

      {/* Inline form */}
      {formOpen && (
        <ChannelForm
          form={form}
          setForm={setForm}
          errors={errors}
          clearError={clearError}
          toggleEvent={toggleEvent}
          isEditing={!!editingId}
          saving={saving}
          onSubmit={handleSubmit}
          onCancel={cancelForm}
        />
      )}

      {/* Channels table */}
      <div className="mt-6">
        <h2 className="mb-3 text-xs font-semibold uppercase tracking-wider text-zinc-500">
          Channels
        </h2>
        <div className="overflow-x-auto -mx-6 px-6">
          <table className="w-full min-w-[640px] text-sm">
            <thead>
              <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-zinc-500 dark:border-zinc-700">
                <th className="py-3 pr-6">Name</th>
                <th className="py-3 pr-6">Type</th>
                <th className="py-3 pr-6">Events</th>
                <th className="py-3 pr-6">Enabled</th>
                <th className="py-3 pr-6 text-right">Actions</th>
              </tr>
            </thead>
            <tbody>
              {channels.map((ch) => (
                <tr
                  key={ch.id}
                  className="border-b border-zinc-100 dark:border-zinc-800"
                >
                  <td className="py-3 pr-6 font-medium text-zinc-900 dark:text-zinc-100">
                    {ch.name}
                  </td>
                  <td className="py-3 pr-6 text-zinc-500">
                    <ChannelTypeBadge type={ch.channel_type} />
                  </td>
                  <td className="py-3 pr-6 text-zinc-500">
                    <div className="flex flex-wrap gap-1">
                      {ch.events.map((ev) => (
                        <span
                          key={ev}
                          className="inline-flex rounded-full bg-zinc-100 px-2 py-0.5 text-[10px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400"
                        >
                          {ev}
                        </span>
                      ))}
                    </div>
                  </td>
                  <td className="py-3 pr-6">
                    <SettingsSwitch
                      checked={ch.enabled}
                      onChange={() => handleToggle(ch)}
                    />
                  </td>
                  <td className="py-3 pr-6 text-right">
                    <div className="flex items-center justify-end gap-1">
                      <button
                        onClick={() => handleTest(ch.id)}
                        disabled={testingId === ch.id}
                        className="rounded px-2 py-1 text-xs text-blue-600 hover:bg-blue-50 hover:text-blue-700 disabled:opacity-50 dark:text-blue-400 dark:hover:bg-blue-950"
                      >
                        {testingId === ch.id ? "..." : "Test"}
                      </button>
                      <button
                        onClick={() => openEdit(ch)}
                        className="rounded px-2 py-1 text-xs text-zinc-500 hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
                      >
                        Edit
                      </button>
                      <button
                        onClick={() => handleDelete(ch.id)}
                        disabled={deletingId === ch.id}
                        className="rounded px-2 py-1 text-xs text-red-500 hover:bg-red-50 hover:text-red-700 disabled:opacity-50 dark:hover:bg-red-950"
                      >
                        {deletingId === ch.id ? "..." : "Delete"}
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
              {channels.length === 0 && (
                <tr>
                  <td colSpan={5} className="py-8 text-center text-zinc-400">
                    No notification channels configured
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* Recent notifications log */}
      <div className="mt-8">
        <h2 className="mb-3 text-xs font-semibold uppercase tracking-wider text-zinc-500">
          Recent Notifications
        </h2>
        {logs.length === 0 ? (
          <p className="text-sm text-zinc-400">No notifications sent yet.</p>
        ) : (
          <div className="overflow-x-auto -mx-6 px-6">
            <table className="w-full min-w-[640px] text-sm">
              <thead>
                <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-zinc-500 dark:border-zinc-700">
                  <th className="py-3 pr-6">Time</th>
                  <th className="py-3 pr-6">Event</th>
                  <th className="py-3 pr-6">Subject</th>
                  <th className="py-3 pr-6">Status</th>
                  <th className="py-3 pr-6">Error</th>
                </tr>
              </thead>
              <tbody>
                {logs.map((log) => (
                  <tr
                    key={log.id}
                    className="border-b border-zinc-100 dark:border-zinc-800"
                  >
                    <td className="py-3 pr-6 text-zinc-500 whitespace-nowrap">
                      {new Date(log.created_at).toLocaleString()}
                    </td>
                    <td className="py-3 pr-6 text-zinc-500">
                      <span className="inline-flex rounded-full bg-zinc-100 px-2 py-0.5 text-[10px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400">
                        {log.event_type}
                      </span>
                    </td>
                    <td className="py-3 pr-6 font-medium text-zinc-900 dark:text-zinc-100">
                      {log.subject}
                    </td>
                    <td className="py-3 pr-6">
                      <StatusBadge status={log.status} />
                    </td>
                    <td className="py-3 pr-6 text-zinc-400 text-xs max-w-48 truncate">
                      {log.error ?? "-"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Channel type badge
// ---------------------------------------------------------------------------

function ChannelTypeBadge({ type }: { type: string }) {
  const label =
    type === "slack_webhook"
      ? "Slack"
      : type === "generic_webhook"
        ? "Webhook"
        : type;
  const color =
    type === "slack_webhook"
      ? "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400"
      : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400";

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ${color}`}
    >
      {label}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Status badge
// ---------------------------------------------------------------------------

function StatusBadge({ status }: { status: string }) {
  const color =
    status === "sent"
      ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400"
      : status === "failed"
        ? "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400"
        : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400";

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ${color}`}
    >
      {status}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Channel form (create / edit)
// ---------------------------------------------------------------------------

function ChannelForm({
  form,
  setForm,
  errors,
  clearError,
  toggleEvent,
  isEditing,
  saving,
  onSubmit,
  onCancel,
}: {
  form: ChannelFormValues;
  setForm: React.Dispatch<React.SetStateAction<ChannelFormValues>>;
  errors: Record<string, string>;
  clearError: (field: string) => void;
  toggleEvent: (eventValue: string) => void;
  isEditing: boolean;
  saving: boolean;
  onSubmit: (e: React.FormEvent) => void;
  onCancel: () => void;
}) {
  const update = (field: string, patch: Partial<ChannelFormValues>) => {
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
          {isEditing ? "Edit Channel" : "New Channel"}
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
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Name
          </label>
          <input
            value={form.name}
            onChange={(e) => update("name", { name: e.target.value })}
            placeholder="e.g. Slack #alerts"
            required
            className={`mt-0.5 w-full rounded-md border bg-white px-3 py-1.5 text-xs dark:bg-zinc-900 ${errors.name ? "border-red-400 dark:border-red-600" : "border-zinc-200 dark:border-zinc-700"}`}
          />
          {errors.name && (
            <p className="mt-0.5 text-[10px] text-red-500">{errors.name}</p>
          )}
        </div>

        {/* Channel type */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Channel Type
          </label>
          <SettingsSelect
            value={form.channel_type}
            onChange={(e) =>
              update("channel_type", { channel_type: e.target.value })
            }
            disabled={isEditing}
          >
            {CHANNEL_TYPES.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
          </SettingsSelect>
        </div>

        {/* Webhook URL */}
        <div className="col-span-2">
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Webhook URL
          </label>
          <input
            value={form.url}
            onChange={(e) => update("url", { url: e.target.value })}
            placeholder={
              form.channel_type === "slack_webhook"
                ? "https://hooks.slack.com/services/..."
                : "https://example.com/webhook"
            }
            required
            className={`mt-0.5 w-full rounded-md border bg-white px-3 py-1.5 text-xs font-mono dark:bg-zinc-900 ${errors.url ? "border-red-400 dark:border-red-600" : "border-zinc-200 dark:border-zinc-700"}`}
          />
          {errors.url && (
            <p className="mt-0.5 text-[10px] text-red-500">{errors.url}</p>
          )}
        </div>

        {/* Events */}
        <div className="col-span-2">
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Events
          </label>
          <div className="mt-1 flex flex-wrap gap-3">
            {EVENT_TYPES.map((ev) => (
              <label
                key={ev.value}
                className="flex items-center gap-1.5 text-xs text-zinc-700 dark:text-zinc-300"
              >
                <input
                  type="checkbox"
                  checked={form.events.includes(ev.value)}
                  onChange={() => toggleEvent(ev.value)}
                  className="rounded border-zinc-300 text-emerald-600 focus:ring-emerald-500 dark:border-zinc-600"
                />
                {ev.label}
              </label>
            ))}
          </div>
          {errors.events && (
            <p className="mt-0.5 text-[10px] text-red-500">{errors.events}</p>
          )}
        </div>
      </div>

      <div className="mt-3 flex items-center gap-2">
        <button
          type="submit"
          disabled={!form.name.trim() || !form.url.trim() || saving}
          className="rounded-md bg-emerald-600 px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50 hover:bg-emerald-700"
        >
          {saving
            ? isEditing
              ? "Updating..."
              : "Creating..."
            : isEditing
              ? "Update Channel"
              : "Create Channel"}
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
