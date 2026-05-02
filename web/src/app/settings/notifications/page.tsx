"use client";

import { useState } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import { ErrorState } from "@/components/ui/error-state";
import { SkeletonTable } from "@/components/ui/skeleton";
import { SettingsSelect, SettingsSwitch } from "@/components/ui/form-input";
import { StatusBadge } from "@/components/ui/status-badge";
import { Button } from "@/components/ui/button";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { useConfirm } from "@/components/providers/confirm-provider";
import {
  listChannels,
  createChannel,
  updateChannel,
  deleteChannel,
  testChannel,
  listLogs,
  type NotificationChannel,
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

const notificationKeys = {
  all: ["notifications"] as const,
  channels: () => [...notificationKeys.all, "channels"] as const,
  logs: () => [...notificationKeys.all, "logs"] as const,
};

export default function NotificationsSettingsPage() {
  const t = useTranslations("settings.notifications");
  const tCommon = useTranslations("common");
  const qc = useQueryClient();

  // Form state
  const [formOpen, setFormOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<ChannelFormValues>(EMPTY_FORM);
  const [testingId, setTestingId] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const confirm = useConfirm();

  const channelsQuery = useQuery({
    queryKey: notificationKeys.channels(),
    queryFn: () => listChannels(),
  });
  const logsQuery = useQuery({
    queryKey: notificationKeys.logs(),
    queryFn: () => listLogs(50),
  });

  const channels = channelsQuery.data ?? [];
  const logs = logsQuery.data ?? [];
  const reload = () => {
    qc.invalidateQueries({ queryKey: notificationKeys.channels() });
    qc.invalidateQueries({ queryKey: notificationKeys.logs() });
  };

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

  const submitMutation = useMutation({
    mutationFn: async () => {
      if (editingId) {
        await updateChannel(editingId, {
          name: form.name.trim(),
          config: { url: form.url.trim() },
          events: form.events,
        });
      } else {
        await createChannel({
          name: form.name.trim(),
          channel_type: form.channel_type,
          config: { url: form.url.trim() },
          events: form.events,
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
    mutationFn: (id: string) => deleteChannel(id),
    onSuccess: () => {
      toast.success(t("toast.deleted"));
      reload();
    },
    onError: () => toast.error(t("toast.deleteFailed")),
  });

  const toggleMutation = useMutation({
    mutationFn: (ch: NotificationChannel) =>
      updateChannel(ch.id, { enabled: !ch.enabled }),
    onSuccess: (_data, ch) => {
      toast.success(ch.enabled ? t("toast.disabled") : t("toast.enabled"));
      reload();
    },
    onError: () => toast.error(t("toast.toggleError")),
  });

  // ---- Submit (create or update) ----
  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;
    submitMutation.mutate();
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
    deleteMutation.mutate(id);
  };

  // ---- Toggle enabled ----
  const handleToggle = (ch: NotificationChannel) => toggleMutation.mutate(ch);

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
      reload();
    } catch {
      toast.error(t("toast.testError"));
    } finally {
      setTestingId(null);
    }
  };

  const saving = submitMutation.isPending;
  const deletingId = deleteMutation.isPending ? deleteMutation.variables : null;

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

  if (channelsQuery.isLoading) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <SkeletonTable rows={4} cols={5} />
      </SettingsPageShell>
    );
  }

  if (channelsQuery.isError) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <ErrorState
          title={tCommon("loadError.title")}
          description={tCommon("loadError.description")}
          onRetry={() => channelsQuery.refetch()}
          retryLabel={tCommon("retry")}
        />
      </SettingsPageShell>
    );
  }

  return (
    <SettingsPageShell
      title={t("title")}
      subtitle={t("description")}
      actions={
        !formOpen && (
          <Button variant="primary" size="sm" onClick={openCreate}>
            {t("addChannel")}
          </Button>
        )
      }
    >
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
              <tr className="border-b border-divider text-left text-xs font-medium uppercase text-muted-foreground">
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
                  className="border-b border-divider-soft"
                >
                  <td className="py-3 pr-6 font-medium text-foreground-strong">
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
                          className="inline-flex rounded-full bg-surface-inset px-2 py-0.5 text-2xs font-medium text-foreground dark:text-muted-foreground"
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
                        className="rounded px-2 py-1 text-xs text-info-foreground hover:bg-info-surface hover:text-info-foreground disabled:opacity-50 dark:text-info-foreground"
                      >
                        {testingId === ch.id
                          ? t("action.testing")
                          : t("action.test")}
                      </button>
                      <button
                        onClick={() => openEdit(ch)}
                        className="rounded px-2 py-1 text-xs text-muted-foreground hover:bg-surface-inset hover:text-foreground dark:hover:bg-surface-base dark:hover:text-foreground-muted"
                      >
                        {t("action.edit")}
                      </button>
                      <button
                        onClick={() => handleDelete(ch.id)}
                        disabled={deletingId === ch.id}
                        className="rounded px-2 py-1 text-xs text-danger-foreground hover:bg-danger-surface hover:text-danger-foreground disabled:opacity-50 dark:hover:bg-danger-surface"
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
                <tr className="border-b border-divider text-left text-xs font-medium uppercase text-muted-foreground">
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
                    className="border-b border-divider-soft"
                  >
                    <td className="py-3 pr-6 text-muted-foreground whitespace-nowrap">
                      {new Date(log.created_at).toLocaleString()}
                    </td>
                    <td className="py-3 pr-6 text-muted-foreground">
                      <span className="inline-flex rounded-full bg-surface-inset px-2 py-0.5 text-2xs font-medium text-foreground dark:text-muted-foreground">
                        {eventLabel(log.event_type)}
                      </span>
                    </td>
                    <td className="py-3 pr-6 font-medium text-foreground-strong">
                      {log.subject}
                    </td>
                    <td className="py-3 pr-6">
                      <DeliveryStatusBadge status={log.status} />
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
    </SettingsPageShell>
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
      ? "bg-concept-surface text-concept-foreground dark:bg-concept-foreground/30 dark:text-concept-foreground"
      : "bg-surface-inset text-foreground dark:text-muted-foreground";

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-2xs font-semibold uppercase tracking-wider ${color}`}
    >
      {label}
    </span>
  );
}

function DeliveryStatusBadge({ status }: { status: string }) {
  const t = useTranslations("settings.notifications");
  const label = isKnownStatus(status) ? t(`status.${status}`) : status;
  const tone =
    status === "sent" ? "success" : status === "failed" ? "danger" : "neutral";
  return (
    <StatusBadge
      tone={tone}
      className="font-semibold uppercase tracking-wider"
    >
      {label}
    </StatusBadge>
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
        <div>
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
          {errors.name && (
            <p className="mt-0.5 text-2xs text-danger-foreground">{errors.name}</p>
          )}
        </div>

        {/* Channel type */}
        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
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
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
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
            className={`mt-0.5 w-full rounded-md border bg-surface-base px-3 py-1.5 text-xs font-mono ${errors.url ? "border-danger-border" : "border-divider"}`}
          />
          {errors.url && (
            <p className="mt-0.5 text-2xs text-danger-foreground">{errors.url}</p>
          )}
        </div>

        {/* Events */}
        <div className="col-span-2">
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.events")}
          </label>
          <div className="mt-1 flex flex-wrap gap-3">
            {EVENT_TYPE_VALUES.map((value) => (
              <label
                key={value}
                className="flex items-center gap-1.5 text-xs text-foreground"
              >
                <input
                  type="checkbox"
                  checked={form.events.includes(value)}
                  onChange={() => toggleEvent(value)}
                  className="rounded border-divider text-brand-foreground focus:ring-brand-foreground dark:border-divider"
                />
                {t(`event.${value}`)}
              </label>
            ))}
          </div>
          {errors.events && (
            <p className="mt-0.5 text-2xs text-danger-foreground">{errors.events}</p>
          )}
        </div>
      </div>

      <div className="mt-3 flex items-center gap-2">
        <button
          type="submit"
          disabled={!form.name.trim() || !form.url.trim() || saving}
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
          className="rounded-md px-3 py-1.5 text-xs text-muted-foreground hover:bg-surface-inset dark:hover:bg-surface-base"
        >
          {t("form.cancel")}
        </button>
      </div>
    </form>
  );
}
