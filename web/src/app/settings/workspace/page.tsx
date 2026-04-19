"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { useTranslations } from "next-intl";
import { Spinner } from "@/components/ui/spinner";
import { FormInput } from "@/components/ui/form-input";
import { Button } from "@/components/ui/button";
import { SettingsSection } from "@/components/settings/settings-section";
import {
  getWorkspace,
  updateWorkspace,
  updateWorkspaceLocale,
  listMembers,
} from "@/lib/api/workspaces";
import { getWorkspaceId, setWorkspaceName } from "@/lib/workspace";
import { MembersTable } from "@/components/workspace/members-table";
import type { Workspace, WorkspaceMember } from "@/types/workspace";

// BCP 47 subset matching ox-core's `LanguageTag::parse`. Validates the
// same shape the backend + DB CHECK enforce — catches typos client-side
// so the user sees the problem immediately instead of a 400 round-trip.
const BCP47_RE = /^[a-z]{2,3}(-[a-z0-9]{2,8})*$/;

export default function WorkspaceSettingsPage() {
  const t = useTranslations("settings.workspace");
  const wsId = getWorkspaceId();

  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [members, setMembers] = useState<WorkspaceMember[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [savingLocale, setSavingLocale] = useState(false);
  const [editName, setEditName] = useState("");
  const [editPrimaryLocale, setEditPrimaryLocale] = useState("ko");
  // Fallback is edited as a single comma-separated string; parsed at save time.
  const [editFallback, setEditFallback] = useState("ko,en");

  const load = useCallback(async () => {
    if (!wsId) return;
    try {
      const [ws, mems] = await Promise.all([
        getWorkspace(wsId),
        listMembers(wsId),
      ]);
      setWorkspace(ws);
      setEditName(ws.name);
      setEditPrimaryLocale(ws.primary_locale ?? "ko");
      setEditFallback((ws.locale_fallback ?? ["ko", "en"]).join(","));
      setMembers(mems);
    } catch {
      toast.error(t("loadError"));
    } finally {
      setLoading(false);
    }
  }, [wsId, t]);

  useEffect(() => {
    load();
  }, [load]);

  const handleSave = async () => {
    if (!wsId || !editName.trim()) return;
    setSaving(true);
    try {
      const updated = await updateWorkspace(wsId, { name: editName.trim() });
      setWorkspace(updated);
      setWorkspaceName(updated.name);
      toast.success(t("updatedToast"));
    } catch {
      toast.error(t("updateError"));
    } finally {
      setSaving(false);
    }
  };

  const parsedFallback = editFallback
    .split(",")
    .map((s) => s.trim().toLowerCase())
    .filter(Boolean);
  const primaryValid = BCP47_RE.test(editPrimaryLocale.trim().toLowerCase());
  const fallbackValid =
    parsedFallback.length > 0 && parsedFallback.every((t) => BCP47_RE.test(t));
  const hasLocaleChanges =
    workspace !== null &&
    (editPrimaryLocale.trim().toLowerCase() !== workspace.primary_locale ||
      parsedFallback.join(",") !==
        (workspace.locale_fallback ?? []).join(","));

  const handleSaveLocale = async () => {
    if (!wsId || !primaryValid || !fallbackValid) return;
    setSavingLocale(true);
    try {
      const updated = await updateWorkspaceLocale(wsId, {
        primary_locale: editPrimaryLocale.trim().toLowerCase(),
        locale_fallback: parsedFallback,
      });
      setWorkspace(updated);
      setEditPrimaryLocale(updated.primary_locale);
      setEditFallback((updated.locale_fallback ?? []).join(","));
      toast.success(t("locale.updatedToast"));
    } catch {
      toast.error(t("locale.updateError"));
    } finally {
      setSavingLocale(false);
    }
  };

  if (!wsId) {
    return (
      <div className="py-12 text-center text-sm text-muted-foreground">
        {t("noWorkspace")}
      </div>
    );
  }

  if (loading) return <Spinner />;

  const hasChanges = editName.trim() !== (workspace?.name ?? "");

  return (
    <SettingsSection
      title={t("title")}
      description={t("description")}
      actions={
        <Button
          variant="primary"
          size="sm"
          onClick={handleSave}
          disabled={!hasChanges || saving}
        >
          {saving ? t("saving") : t("save")}
        </Button>
      }
    >
      {/* ── General ────────────────────────────────────────────── */}
      <section className="mt-6">
        <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">
          {t("general.heading")}
        </h2>
        <div className="mt-3 space-y-3">
          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-600 dark:text-muted-foreground">
              {t("general.name")}
            </label>
            <FormInput
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-600 dark:text-muted-foreground">
              {t("general.slug")}
            </label>
            <FormInput
              value={workspace?.slug ?? ""}
              readOnly
              className="bg-zinc-50 font-mono text-muted-foreground dark:bg-zinc-800/50"
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-600 dark:text-muted-foreground">
              {t("general.created")}
            </label>
            <p className="text-sm text-muted-foreground">
              {workspace?.created_at
                ? new Date(workspace.created_at).toLocaleDateString()
                : "-"}
            </p>
          </div>
        </div>
      </section>

      {/* ── Locale ─────────────────────────────────────────────── */}
      <section className="mt-8">
        <div className="flex items-baseline justify-between">
          <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">
            {t("locale.heading")}
          </h2>
          <Button
            variant="primary"
            size="sm"
            onClick={handleSaveLocale}
            disabled={
              !hasLocaleChanges || !primaryValid || !fallbackValid || savingLocale
            }
          >
            {savingLocale ? t("locale.saving") : t("locale.save")}
          </Button>
        </div>
        <p className="mt-1 text-xs text-muted-foreground">
          {t.rich("locale.description", {
            k: (chunks) => <code>{chunks}</code>,
          })}
        </p>
        <div className="mt-3 space-y-3">
          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-600 dark:text-muted-foreground">
              {t("locale.primary")}
            </label>
            <FormInput
              value={editPrimaryLocale}
              onChange={(e) => setEditPrimaryLocale(e.target.value)}
              placeholder="ko"
            />
            {!primaryValid && (
              <p className="mt-1 text-xs text-red-600">
                {t("locale.primaryInvalid")}
              </p>
            )}
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-600 dark:text-muted-foreground">
              {t("locale.fallback")}
            </label>
            <FormInput
              value={editFallback}
              onChange={(e) => setEditFallback(e.target.value)}
              placeholder="ko,en"
            />
            {!fallbackValid && (
              <p className="mt-1 text-xs text-red-600">
                {t("locale.fallbackInvalid")}
              </p>
            )}
          </div>
        </div>
      </section>

      {/* ── Members ────────────────────────────────────────────── */}
      <MembersTable wsId={wsId} members={members} onReload={load} />
    </SettingsSection>
  );
}
