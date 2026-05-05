"use client";

// `<WorkspaceDangerZone>` — admin-only deletion surface for the
// active workspace. Self-contained state machine: admin gate +
// type-to-confirm + delete mutation + post-delete reset + redirect.
//
// Lives next to `<MembersTable>` so the workspace settings page
// composes from focused subcomponents rather than carrying every
// concern inline. The page passes the `Workspace` snapshot in; the
// danger zone owns nothing else.

import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/toast";
import { useConfirm } from "@/components/providers/confirm-provider";
import { useDeleteWorkspace } from "@/hooks/api/use-workspaces";
import { setWorkspaceId } from "@/lib/workspace";
import type { Workspace } from "@/types/workspace";

interface WorkspaceDangerZoneProps {
  workspace: Workspace;
}

export function WorkspaceDangerZone({ workspace }: WorkspaceDangerZoneProps) {
  const t = useTranslations("settings.workspace.danger");
  const confirm = useConfirm();
  const deleteMutation = useDeleteWorkspace();

  async function handleDelete() {
    const ok = await confirm({
      title: t("confirmTitle"),
      description: t("confirmDescription", { name: workspace.name }),
      confirmLabel: t("confirmAction"),
      variant: "danger",
      typeToConfirm: {
        phrase: workspace.slug,
        label: t("confirmTypeLabel"),
      },
    });
    if (!ok) return;
    try {
      await deleteMutation.mutateAsync(workspace.id);
      toast.success(t("deleted"));
      // The workspace the user was scoped into is gone — `setWorkspaceId`
      // with `undefined` cascades to clear name + role from localStorage
      // (see `lib/workspace.ts`), so a hard reload to root fully
      // re-bootstraps the session.
      setWorkspaceId(undefined);
      window.location.assign("/");
    } catch {
      toast.error(t("deleteFailed"));
    }
  }

  return (
    <section
      className="mt-10 rounded-lg border border-danger-border bg-danger-surface p-4"
      aria-labelledby="workspace-danger-heading"
    >
      <h2
        id="workspace-danger-heading"
        className="text-sm font-semibold text-danger-foreground"
      >
        {t("heading")}
      </h2>
      <p className="mt-1 text-xs text-foreground-muted">{t("description")}</p>
      <div className="mt-4 flex flex-col gap-2 rounded-md border border-divider bg-surface-base p-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex flex-col">
          <span className="text-xs font-medium text-foreground-strong">
            {t("deleteTitle")}
          </span>
          <span className="text-2xs text-foreground-muted">
            {t("deleteHint")}
          </span>
        </div>
        <Button
          variant="danger"
          size="sm"
          onClick={handleDelete}
          disabled={deleteMutation.isPending}
        >
          {t("deleteAction")}
        </Button>
      </div>
    </section>
  );
}
