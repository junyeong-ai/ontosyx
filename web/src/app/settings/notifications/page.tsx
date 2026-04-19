"use client";

import { useState, useEffect, useCallback } from "react";
import { useTranslations } from "next-intl";
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
// Constants & type guards
// ---------------------------------------------------------------------------

const CHANNEL_TYPE_VALUES = ["slack_webhook", "generic_webhook"] as const;
type KnownChannelType = (typeof CHANNEL_TYPE_VALUES)[number];

function isKnownChannelType(s: string): s is KnownChannelType {
  return s === "slack_webhook" || s === "generic_webhook";
}

const EVENT_TYPE_VALUES = [
  "quality_rule_failed",
  "quality_rule_passed",
] as const;
type KnownEventType = (typeof EVENT_TYPE_VALUES)[number];

function isKnownEventType(s: string): s is KnownEventType {
  return s === "quality_rule_failed" || s === "quality_rule_passed";
}

type KnownStatus = "sent" | "failed";

function isKnownStatus(s: string): s is KnownStatus {
  return s === "sent" || s === "failed";
}

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
  const t = useTranslations("settings.notifications");
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
        const next = { ...prev };
        delete next[field];
        return next;
      });
  };

  // ---- Validate ----
  const validate = (): boolean => {
    const e: Record<string, string> = {};
    if (!form.name.trim()) e.name = t("validation.required");
    if (!form.url.trim()) e.url = t("validation.required");
    try {
      new URL(form.url.trim());
    } catch {
      if (form.url.trim()) e.url = t("validation.invalidUrl");
    }
    if (form.events.length === 0) e.events = t("validation.selectEvent");
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
        toast.success(t("toast.updated"));
      } else {
        await createChannel({
          name: form.name.trim(),
          channel_type: form.channel_type,
          config: { url: form.url.trim() },
          events: form.events,
        });
        toast.success(t("toast.created"));
      }
      cancelForm();
      await load();
    } catch {
      toast.error(editingId ? t("toast.updateError") : t("toast.createError"));
    } finally {
      setSaving(false);
    }
  };

  // ---- Delete ----
  const handleDelete = async (id: string) => {
    const ch = channels.find((c) => c.id === id);
    const ok = await confirm({
      title: t("deleteConfirm.title", { name: ch?.name ?? id }),
      description: t("deleteConfirm.description"),
      variant: "danger",
    });
    if (!ok) return;
    setDeletingId(id);
    try {
      await deleteChannel(id);
      toast.success(t("toast.deleted"));
      await load();
    } catch {
      toast.error(t("toast.deleteError"));
    } finally {
      setDeletingId(null);
    }
  };

  // ---- Toggle enabled ----
  const handleToggle = async (ch: NotificationChannel) => {
    try {
      await updateChannel(ch.id, { enabled: !ch.enabled });
      toast.success(ch.enabled ? t("toast.disabled") : t("toast.enabled"));
      await load();
    } catch {
      toast.error(t("toast.toggleError"));
    }
  };

  // ---- Test ----
  const handleTest = async (id: string) => {
    setTestingId(id);
    try {
      const result = await testChannel(id);
      if (result.success) {
        toast.success(t("toast.testSuccess"));
      } else {
        toast.error(
          t("toast.testFailed", {
            error: result.error ?? t("toast.unknownError"),
          }),
        );
      }
      await load();
    } catch {
      toast.error(t("toast.testError"));
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

  const eventLabel = (ev: string) =>
    isKnownEventType(ev) ? t(`event.${ev}`) : ev;

  if (loading) return <Spinner />;

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
            {t("addChannel")}
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
        <h2 className="mb-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          {t("channelsHeading")}
        </h2>
        <div className="overflow-x-auto -mx-6 px-6">
          <table className="w-full min-w-[640px] text-sm">
            <thead>
              <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-muted-foreground dark:border-zinc-700">
                <th className="py-3 pr-6">{t("column.name")}</th>
                <th className="py-3 pr-6">{t("column.type")}</th>
                <th className="py-3 pr-6">{t("column.events")}</th>
                <th className="py-3 pr-6">{t("column.enabled")}</th>
                <th className="py-3 pr-6 text-right">{t("column.actions")}</th>
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
                  <td className="py-3 pr-6 text-muted-foreground">
                    <ChannelTypeBadge type={ch.channel_type} />
                  </td>
                  <td className="py-3 pr-6 text-muted-foreground">
                    <div className="flex flex-wrap gap-1">
                      {ch.events.map((ev) => (
                        <span
                          key={ev}
                          className="inline-flex rounded-full bg-zinc-100 px-2 py-0.5 text-[10px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground"
                        >
                          {eventLabel(ev)}
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
                        {testingId === ch.id
                          ? t("action.testing")
                          : t("action.test")}
                      </button>
                      <button
                        onClick={() => openEdit(ch)}
                        className="rounded px-2 py-1 text-xs text-muted-foreground hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
                      >
                        {t("action.edit")}
                      </button>
                      <button
                        onClick={() => handleDelete(ch.id)}
                        disabled={deletingId === ch.id}
                        className="rounded px-2 py-1 text-xs text-red-500 hover:bg-red-50 hover:text-red-700 disabled:opacity-50 dark:hover:bg-red-950"
                      >
                        {deletingId === ch.id
                          ? t("action.deleting")
                          : t("action.delete")}
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
              {channels.length === 0 && (
                <tr>
                  <td colSpan={5} className="py-8 text-center text-muted-foreground">
                    {t("emptyChannels")}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* Recent notifications log */}
      <div className="mt-8">
        <h2 className="mb-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          {t("recentHeading")}
        </h2>
        {logs.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("emptyLogs")}</p>
        ) : (
          <div className="overflow-x-auto -mx-6 px-6">
            <table className="w-full min-w-[640px] text-sm">
              <thead>
                <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-muted-foreground dark:border-zinc-700">
                  <th className="py-3 pr-6">{t("column.time")}</th>
                  <th className="py-3 pr-6">{t("column.event")}</th>
                  <th className="py-3 pr-6">{t("column.subject")}</th>
                  <th className="py-3 pr-6">{t("column.status")}</th>
                  <th className="py-3 pr-6">{t("column.error")}</th>
                </tr>
              </thead>
              <tbody>
                {logs.map((log) => (
                  <tr
                    key={log.id}
                    className="border-b border-zinc-100 dark:border-zinc-800"
                  >
                    <td className="py-3 pr-6 text-muted-foreground whitespace-nowrap">
                      {new Date(log.created_at).toLocaleString()}
                    </td>
                    <td className="py-3 pr-6 text-muted-foreground">
                      <span className="inline-flex rounded-full bg-zinc-100 px-2 py-0.5 text-[10px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground">
                        {eventLabel(log.event_type)}
                      </span>
                    </td>
                    <td className="py-3 pr-6 font-medium text-zinc-900 dark:text-zinc-100">
                      {log.subject}
                    </td>
                    <td className="py-3 pr-6">
                      <StatusBadge status={log.status} />
                    </td>
                    <td className="py-3 pr-6 text-muted-foreground text-xs max-w-48 truncate">
                      {log.error ?? t("emptyErrorCell")}
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
  const t = useTranslations("settings.notifications");
  const label = isKnownChannelType(type)
    ? t(`channelType.${type}Short`)
    : type;
  const color =
    type === "slack_webhook"
      ? "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400"
      : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground";

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
  const t = useTranslations("settings.notifications");
  const label = isKnownStatus(status) ? t(`status.${status}`) : status;
  const color =
    status === "sent"
      ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400"
      : status === "failed"
        ? "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400"
        : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground";

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ${color}`}
    >
      {label}
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
  const t = useTranslations("settings.notifications");
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
        <div>
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
          {errors.name && (
            <p className="mt-0.5 text-[10px] text-red-500">{errors.name}</p>
          )}
        </div>

        {/* Channel type */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.channelType")}
          </label>
          <SettingsSelect
            label={t("form.channelType")}
            hideLabel
            value={form.channel_type}
            onChange={(e) =>
              update("channel_type", { channel_type: e.target.value })
            }
            disabled={isEditing}
          >
            {CHANNEL_TYPE_VALUES.map((value) => (
              <option key={value} value={value}>
                {t(`channelType.${value}`)}
              </option>
            ))}
          </SettingsSelect>
        </div>

        {/* Webhook URL */}
        <div className="col-span-2">
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.webhookUrl")}
          </label>
          <input
            value={form.url}
            onChange={(e) => update("url", { url: e.target.value })}
            placeholder={
              form.channel_type === "slack_webhook"
                ? t("form.urlPlaceholderSlack")
                : t("form.urlPlaceholderGeneric")
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
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.events")}
          </label>
          <div className="mt-1 flex flex-wrap gap-3">
            {EVENT_TYPE_VALUES.map((value) => (
              <label
                key={value}
                className="flex items-center gap-1.5 text-xs text-zinc-700 dark:text-zinc-300"
              >
                <input
                  type="checkbox"
                  checked={form.events.includes(value)}
                  onChange={() => toggleEvent(value)}
                  className="rounded border-zinc-300 text-emerald-600 focus:ring-emerald-500 dark:border-zinc-600"
                />
                {t(`event.${value}`)}
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
