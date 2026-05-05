"use client";

import { useState } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";

import { SkeletonTable } from "@/components/ui/skeleton";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { FormInput, SettingsSelect, SettingsSwitch } from "@/components/ui/form-input";
import { Checkbox } from "@/components/ui/checkbox";
import { StatusBadge } from "@/components/ui/status-badge";
import { useFormatters } from "@/hooks/use-formatters";
import { Button } from "@/components/ui/button";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { useConfirm } from "@/components/providers/confirm-provider";
import { Eyebrow } from "@/components/ui/eyebrow";
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
  const t = useTranslations("account.notifications");
  const tCommon = useTranslations("common");
  const fmt = useFormatters();
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

  if (channelsQuery.isLoading || channelsQuery.isError) {
    const pageState: PageState = channelsQuery.isLoading
      ? { kind: "loading" }
      : { kind: "error", onRetry: () => void channelsQuery.refetch() };
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <PageStateView
          state={pageState}
          skeleton={<SkeletonTable rows={4} cols={5} />}
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
        <Eyebrow level={2} tone="muted" size="dense" caps="upper" className="mb-3">
          {t("channelsHeading")}
        </Eyebrow>
        <div className="overflow-x-auto -mx-6 px-6">
          <table className="w-full min-w-[640px] text-sm">
            <thead>
              <tr className="border-b border-divider text-start text-xs font-medium uppercase text-foreground-muted">
                <th className="py-3 pe-6">{t("column.name")}</th>
                <th className="py-3 pe-6">{t("column.type")}</th>
                <th className="py-3 pe-6">{t("column.events")}</th>
                <th className="py-3 pe-6">{t("column.enabled")}</th>
                <th className="py-3 pe-6 text-end">{t("column.actions")}</th>
              </tr>
            </thead>
            <tbody>
              {channels.map((ch) => (
                <tr
                  key={ch.id}
                  className="border-b border-divider-soft"
                >
                  <td className="py-3 pe-6 font-medium text-foreground-strong">
                    {ch.name}
                  </td>
                  <td className="py-3 pe-6 text-foreground-muted">
                    <ChannelTypeBadge type={ch.channel_type} />
                  </td>
                  <td className="py-3 pe-6 text-foreground-muted">
                    <div className="flex flex-wrap gap-1">
                      {ch.events.map((ev) => (
                        <span
                          key={ev}
                          className="inline-flex rounded-full bg-surface-inset px-2 py-0.5 text-2xs font-medium text-foreground"
                        >
                          {eventLabel(ev)}
                        </span>
                      ))}
                    </div>
                  </td>
                  <td className="py-3 pe-6">
                    <SettingsSwitch
                      checked={ch.enabled}
                      onChange={() => handleToggle(ch)}
                    />
                  </td>
                  <td className="py-3 pe-6 text-end">
                    <div className="flex items-center justify-end gap-1">
                      <button
                        type="button"
                        onClick={() => handleTest(ch.id)}
                        disabled={testingId === ch.id}
                        className="rounded px-2 py-1 text-xs text-info-foreground hover:bg-info-surface hover:text-info-foreground disabled:opacity-50"
                      >
                        {testingId === ch.id
                          ? t("action.testing")
                          : t("action.test")}
                      </button>
                      <button
                        type="button"
                        onClick={() => openEdit(ch)}
                        className="rounded px-2 py-1 text-xs text-foreground-muted hover:bg-surface-inset hover:text-foreground-muted"
                      >
                        {t("action.edit")}
                      </button>
                      <button
                        type="button"
                        onClick={() => handleDelete(ch.id)}
                        disabled={deletingId === ch.id}
                        className="rounded px-2 py-1 text-xs text-danger-foreground hover:bg-danger-surface hover:text-danger-foreground disabled:opacity-50"
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
                  <td colSpan={5} className="py-8 text-center text-foreground-muted">
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
        <Eyebrow level={2} tone="muted" size="dense" caps="upper" className="mb-3">
          {t("recentHeading")}
        </Eyebrow>
        {logs.length === 0 ? (
          <p className="text-sm text-foreground-muted">{t("emptyLogs")}</p>
        ) : (
          <div className="overflow-x-auto -mx-6 px-6">
            <table className="w-full min-w-[640px] text-sm">
              <thead>
                <tr className="border-b border-divider text-start text-xs font-medium uppercase text-foreground-muted">
                  <th className="py-3 pe-6">{t("column.time")}</th>
                  <th className="py-3 pe-6">{t("column.event")}</th>
                  <th className="py-3 pe-6">{t("column.subject")}</th>
                  <th className="py-3 pe-6">{t("column.status")}</th>
                  <th className="py-3 pe-6">{t("column.error")}</th>
                </tr>
              </thead>
              <tbody>
                {logs.map((log) => (
                  <tr
                    key={log.id}
                    className="border-b border-divider-soft"
                  >
                    <td className="py-3 pe-6 text-foreground-muted whitespace-nowrap">
                      {fmt.date(log.created_at)}
                    </td>
                    <td className="py-3 pe-6 text-foreground-muted">
                      <span className="inline-flex rounded-full bg-surface-inset px-2 py-0.5 text-2xs font-medium text-foreground">
                        {eventLabel(log.event_type)}
                      </span>
                    </td>
                    <td className="py-3 pe-6 font-medium text-foreground-strong">
                      {log.subject}
                    </td>
                    <td className="py-3 pe-6">
                      <DeliveryStatusBadge status={log.status} />
                    </td>
                    <td className="py-3 pe-6 text-foreground-muted text-xs max-w-48 truncate">
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
  const t = useTranslations("account.notifications");
  const label = isKnownChannelType(type)
    ? t(`channelType.${type}Short`)
    : type;
  const color =
    type === "slack_webhook"
      ? "bg-concept-surface text-concept-foreground"
      : "bg-surface-inset text-foreground";

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-2xs font-semibold uppercase tracking-wider ${color}`}
    >
      {label}
    </span>
  );
}

function DeliveryStatusBadge({ status }: { status: string }) {
  const t = useTranslations("account.notifications");
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
  const t = useTranslations("account.notifications");
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
          className="text-xs text-foreground-muted hover:text-foreground"
        >
          {t("form.cancel")}
        </button>
      </div>

      <div className="grid grid-cols-2 gap-3">
        {/* Name */}
        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("form.name")}
          </label>
          <FormInput
            value={form.name}
            onChange={(e) => update("name", { name: e.target.value })}
            placeholder={t("form.namePlaceholder")}
            required
            error={!!errors.name}
            className="mt-0.5 text-xs"
          />
          {errors.name && (
            <p className="mt-0.5 text-2xs text-danger-foreground">{errors.name}</p>
          )}
        </div>

        {/* Channel type */}
        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
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
          <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("form.webhookUrl")}
          </label>
          <FormInput
            value={form.url}
            onChange={(e) => update("url", { url: e.target.value })}
            placeholder={
              form.channel_type === "slack_webhook"
                ? t("form.urlPlaceholderSlack")
                : t("form.urlPlaceholderGeneric")
            }
            required
            error={!!errors.url}
            className="mt-0.5 font-mono text-xs"
          />
          {errors.url && (
            <p className="mt-0.5 text-2xs text-danger-foreground">{errors.url}</p>
          )}
        </div>

        {/* Events */}
        <div className="col-span-2">
          <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("form.events")}
          </label>
          <div className="mt-1 flex flex-wrap gap-3">
            {EVENT_TYPE_VALUES.map((value) => (
              <Checkbox
                key={value}
                checked={form.events.includes(value)}
                onChange={() => toggleEvent(value)}
                label={t(`event.${value}`)}
              />
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
          className="rounded-md bg-brand-solid px-4 py-1.5 text-xs font-medium text-foreground-onbrand disabled:opacity-50 hover:bg-brand-solid"
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
          className="rounded-md px-3 py-1.5 text-xs text-foreground-muted hover:bg-surface-inset"
        >
          {t("form.cancel")}
        </button>
      </div>
    </form>
  );
}
