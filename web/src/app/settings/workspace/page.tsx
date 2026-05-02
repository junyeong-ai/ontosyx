"use client";

import { useState } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { toast } from "sonner";
import { useTranslations } from "next-intl";

import { FormInput } from "@/components/ui/form-input";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { SkeletonCard } from "@/components/ui/skeleton";
import { SettingsSection } from "@/components/settings/settings-section";
import {
  getWorkspace,
  updateWorkspace,
  updateWorkspaceLocale,
  listMembers,
} from "@/lib/api/workspaces";
import { getWorkspaceId, setWorkspaceName } from "@/lib/workspace";
import { workspacesKeys } from "@/hooks/api/use-workspaces";
import { localeChainKeys } from "@/hooks/use-locale-chain";
import { MembersTable } from "@/components/workspace/members-table";
import type { Workspace } from "@/types/workspace";

// BCP 47 subset matching ox-core's `LanguageTag::parse`. Validates the
// same shape the backend + DB CHECK enforce — catches typos client-side
// so the user sees the problem immediately instead of a 400 round-trip.
const BCP47_RE = /^[a-z]{2,3}(-[a-z0-9]{2,8})*$/;

const workspaceKeys = {
  all: ["workspace-settings"] as const,
  detail: (id: string) => [...workspaceKeys.all, "detail", id] as const,
  members: (id: string) => [...workspaceKeys.all, "members", id] as const,
};

export default function WorkspaceSettingsPage() {
  const t = useTranslations("settings.workspace");
  const tCommon = useTranslations("common");
  const wsId = getWorkspaceId();
  const qc = useQueryClient();

  const [editName, setEditName] = useState<string | null>(null);
  const [editPrimaryLocale, setEditPrimaryLocale] = useState<string | null>(
    null,
  );
  const [editAdminFallback, setEditAdminFallback] = useState<string | null>(
    null,
  );
  const [editLlmFallback, setEditLlmFallback] = useState<string | null>(null);

  const workspaceQuery = useQuery({
    queryKey: wsId ? workspaceKeys.detail(wsId) : workspaceKeys.all,
    queryFn: () => getWorkspace(wsId!),
    enabled: !!wsId,
  });
  const membersQuery = useQuery({
    queryKey: wsId ? workspaceKeys.members(wsId) : workspaceKeys.all,
    queryFn: () => listMembers(wsId!),
    enabled: !!wsId,
  });

  const workspace = workspaceQuery.data;
  const members = membersQuery.data ?? [];

  // First render after fetch: hydrate the edit state from the server
  // snapshot. We don't sync on every workspace change because the user
  // may have unsaved edits in flight; the explicit save action calls
  // `setQueryData` to refresh both.
  if (workspace && editName === null) {
    setEditName(workspace.name);
    setEditPrimaryLocale(workspace.primary_locale ?? "ko");
    setEditAdminFallback((workspace.admin_locale_fallback ?? ["ko", "en"]).join(","));
    setEditLlmFallback((workspace.llm_locale_fallback ?? ["en", "ko"]).join(","));
  }

  const saveWorkspace = useMutation({
    mutationFn: () =>
      updateWorkspace(wsId!, { name: (editName ?? "").trim() }),
    onSuccess: (updated: Workspace) => {
      qc.setQueryData(workspaceKeys.detail(wsId!), updated);
      setWorkspaceName(updated.name);
      qc.invalidateQueries({ queryKey: workspacesKeys.all });
      toast.success(t("toast.updated"));
    },
    onError: () => toast.error(t("toast.updateFailed")),
  });

  const saveLocale = useMutation({
    mutationFn: () =>
      updateWorkspaceLocale(wsId!, {
        primary_locale: (editPrimaryLocale ?? "").trim().toLowerCase(),
        admin_locale_fallback: parseChain(editAdminFallback ?? ""),
        llm_locale_fallback: parseChain(editLlmFallback ?? ""),
      }),
    onSuccess: (updated: Workspace) => {
      qc.setQueryData(workspaceKeys.detail(wsId!), updated);
      setEditPrimaryLocale(updated.primary_locale);
      setEditAdminFallback((updated.admin_locale_fallback ?? []).join(","));
      setEditLlmFallback((updated.llm_locale_fallback ?? []).join(","));
      qc.invalidateQueries({ queryKey: localeChainKeys.all });
      qc.invalidateQueries({ queryKey: workspacesKeys.all });
      toast.success(t("locale.toast.updated"));
    },
    onError: () => toast.error(t("locale.toast.updateFailed")),
  });

  if (!wsId) {
    return <EmptyState title={t("noWorkspace")} />;
  }

  if (workspaceQuery.isLoading || membersQuery.isLoading) {
    return (
      <div className="space-y-4">
        <SkeletonCard />
        <SkeletonCard />
      </div>
    );
  }

  if (workspaceQuery.isError || !workspace) {
    return (
      <ErrorState
        title={tCommon("loadError.title")}
        description={tCommon("loadError.description")}
        onRetry={() => {
          workspaceQuery.refetch();
          membersQuery.refetch();
        }}
        retryLabel={tCommon("retry")}
      />
    );
  }

  const parsedAdminFallback = parseChain(editAdminFallback ?? "");
  const parsedLlmFallback = parseChain(editLlmFallback ?? "");
  const primaryValid = BCP47_RE.test(
    (editPrimaryLocale ?? "").trim().toLowerCase(),
  );
  const adminFallbackValid =
    parsedAdminFallback.length > 0 &&
    parsedAdminFallback.every((tag) => BCP47_RE.test(tag));
  const llmFallbackValid =
    parsedLlmFallback.length > 0 &&
    parsedLlmFallback.every((tag) => BCP47_RE.test(tag));

  const hasLocaleChanges =
    (editPrimaryLocale ?? "").trim().toLowerCase() !== workspace.primary_locale ||
    parsedAdminFallback.join(",") !==
      (workspace.admin_locale_fallback ?? []).join(",") ||
    parsedLlmFallback.join(",") !==
      (workspace.llm_locale_fallback ?? []).join(",");

  const hasChanges = (editName ?? "").trim() !== workspace.name;

  return (
    <SettingsSection
      title={t("title")}
      description={t("description")}
      actions={
        <Button
          variant="primary"
          size="sm"
          onClick={() => saveWorkspace.mutate()}
          disabled={!hasChanges || saveWorkspace.isPending}
        >
          {saveWorkspace.isPending ? tCommon("saving") : tCommon("save")}
        </Button>
      }
    >
      {/* ── General ────────────────────────────────────────────── */}
      <section className="mt-6">
        <h2 className="text-sm font-semibold text-foreground-strong">
          {t("general.heading")}
        </h2>
        <div className="mt-3 space-y-3">
          <label className="block">
            <span className="mb-1 block text-xs font-medium text-foreground">
              {t("general.name")}
            </span>
            <FormInput
              value={editName ?? ""}
              onChange={(e) => setEditName(e.target.value)}
            />
          </label>
          <label className="block">
            <span className="mb-1 block text-xs font-medium text-foreground">
              {t("general.slug")}
            </span>
            <FormInput
              value={workspace.slug ?? ""}
              readOnly
              className="bg-surface-raised font-mono text-foreground-muted"
            />
          </label>
          <div>
            <label className="mb-1 block text-xs font-medium text-muted-foreground">
              {t("general.created")}
            </label>
            <p className="text-sm text-muted-foreground">
              {workspace.created_at
                ? new Date(workspace.created_at).toLocaleDateString()
                : "—"}
            </p>
          </div>
        </div>
      </section>

      {/* ── Locale ─────────────────────────────────────────────── */}
      <section className="mt-8">
        <div className="flex items-baseline justify-between">
          <h2 className="text-sm font-semibold text-foreground-strong">
            {t("locale.heading")}
          </h2>
          <Button
            variant="primary"
            size="sm"
            onClick={() => saveLocale.mutate()}
            disabled={
              !hasLocaleChanges ||
              !primaryValid ||
              !adminFallbackValid ||
              !llmFallbackValid ||
              saveLocale.isPending
            }
          >
            {saveLocale.isPending ? t("locale.saving") : t("locale.save")}
          </Button>
        </div>
        <p className="mt-1 text-xs text-muted-foreground">
          {t.rich("locale.description", {
            k: (chunks) => <code>{chunks}</code>,
          })}
        </p>
        <div className="mt-3 space-y-3">
          <div>
            <label className="mb-1 block text-xs font-medium text-muted-foreground">
              {t("locale.primary")}
            </label>
            <FormInput
              value={editPrimaryLocale ?? ""}
              onChange={(e) => setEditPrimaryLocale(e.target.value)}
              placeholder="ko"
            />
            {!primaryValid && (
              <p className="mt-1 text-xs text-danger-foreground">
                {t("locale.primaryInvalid")}
              </p>
            )}
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-muted-foreground">
              {t("locale.adminFallback")}
            </label>
            <FormInput
              value={editAdminFallback ?? ""}
              onChange={(e) => setEditAdminFallback(e.target.value)}
              placeholder="ko,en"
            />
            {!adminFallbackValid && (
              <p className="mt-1 text-xs text-danger-foreground">
                {t("locale.fallbackInvalid")}
              </p>
            )}
            <p className="mt-1 text-xs text-muted-foreground">
              {t("locale.adminFallbackHint")}
            </p>
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-muted-foreground">
              {t("locale.llmFallback")}
            </label>
            <FormInput
              value={editLlmFallback ?? ""}
              onChange={(e) => setEditLlmFallback(e.target.value)}
              placeholder="en,ko"
            />
            {!llmFallbackValid && (
              <p className="mt-1 text-xs text-danger-foreground">
                {t("locale.fallbackInvalid")}
              </p>
            )}
            <p className="mt-1 text-xs text-muted-foreground">
              {t("locale.llmFallbackHint")}
            </p>
          </div>
        </div>
      </section>

      {/* ── Members ────────────────────────────────────────────── */}
      <MembersTable
        wsId={wsId}
        members={members}
        onReload={() => membersQuery.refetch()}
      />
    </SettingsSection>
  );
}

function parseChain(raw: string): string[] {
  return raw
    .split(",")
    .map((s) => s.trim().toLowerCase())
    .filter(Boolean);
}
