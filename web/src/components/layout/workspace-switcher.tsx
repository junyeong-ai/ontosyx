"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useQueryClient } from "@tanstack/react-query";
import { useAppStore } from "@/lib/store";
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";
import { Spinner } from "@/components/ui/spinner";
import { ArrowDown, Plus, Settings } from "lucide-react";
import { toast } from "@/components/ui/toast";
import {
  getWorkspaceId,
  getWorkspaceName,
  setWorkspaceName,
  setWorkspaceRole,
} from "@/lib/workspace";
import { CreateWorkspaceDialog } from "@/components/workspace/create-workspace-dialog";
import { useWorkspaces } from "@/hooks/api/use-workspaces";

// ---------------------------------------------------------------------------
// Shared trigger styling — matches context-selector exactly
// ---------------------------------------------------------------------------

const TRIGGER_CLASS =
  "flex min-w-0 items-center gap-1.5 rounded-md border border-divider bg-surface-raised px-2.5 py-1.5 text-xs font-medium text-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset";

const POPOVER_CLASS =
  "popup-pop z-popover w-72 rounded-lg border border-divider bg-surface-base shadow-3";

// ---------------------------------------------------------------------------

const ROLE_COLORS: Record<string, string> = {
  owner:
    "bg-warning-surface text-warning-foreground",
  admin:
    "bg-concept-surface text-concept-foreground",
  member:
    "bg-surface-inset text-foreground",
  viewer:
    "bg-surface-inset text-foreground-muted",
};

export function WorkspaceSwitcher() {
  const t = useTranslations("chrome.workspaceSwitcher");
  const [open, setOpen] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  // Read workspace from Zustand store (synced with localStorage by initWorkspace)
  const storeWorkspaceId = useAppStore((s) => s.workspaceId);
  const storeWorkspaceName = useAppStore((s) => s.workspaceName);
  // Fallback to localStorage for SSR hydration edge case
  const currentId = storeWorkspaceId ?? getWorkspaceId();
  const cachedName = storeWorkspaceName ?? getWorkspaceName();

  // Why `enabled: open` — don't burn bandwidth while the popover is closed.
  // Tanstack Query caches for 30s after close, so a quick close/reopen is free.
  const { data: workspaces = [], isFetching, isError } = useWorkspaces({
    enabled: open,
  });

  useEffect(() => {
    if (isError) toast.error(t("toast.loadFailed"));
  }, [isError, t]);

  // Hydrate cached name/role for current workspace when the list arrives.
  // This writes to localStorage (external system) — a legitimate useEffect
  // side-effect, not a React setState. Safe under the React 19 linter.
  useEffect(() => {
    if (!currentId || cachedName || workspaces.length === 0) return;
    const current = workspaces.find((w) => w.id === currentId);
    if (current) {
      setWorkspaceName(current.name);
      setWorkspaceRole(current.role);
    }
  }, [workspaces, currentId, cachedName]);

  const setActiveWorkspace = useAppStore((s) => s.setActiveWorkspace);
  const queryClient = useQueryClient();
  const router = useRouter();

  const handleSwitch = (ws: (typeof workspaces)[number]) => {
    setOpen(false);
    // 1. In-memory store flips to the new workspace + resets every
    //    workspace-scoped slice (ontology / draft / chat / selection
    //    / dashboard / verifications / mode badges).
    setActiveWorkspace(ws.id, ws.name, ws.role);
    // 2. Drop every cached server response — RLS scopes data per
    //    workspace, so prior caches are stale by definition. The
    //    next mount triggers fresh fetches.
    queryClient.clear();
    // 3. `router.refresh()` re-runs RSC for the current segment so
    //    server-rendered chrome (locale + bootstrap) re-derives
    //    against the new workspace cookie. No full reload, so
    //    chunks stay in memory and the React tree shape is
    //    preserved across the swap.
    router.refresh();
  };

  const label = cachedName || t("defaultLabel");

  return (
    <>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger className={TRIGGER_CLASS}>
          <span className="max-w-[140px] truncate">{label}</span>
          <ArrowDown className="h-3 w-3 text-foreground-muted" />
        </PopoverTrigger>
        <PopoverContent className={POPOVER_CLASS}>
          <div className="max-h-60 overflow-auto p-1">
            {isFetching ? (
              <div className="flex items-center justify-center py-4">
                <Spinner size="sm" className="text-foreground-muted" />
              </div>
            ) : workspaces.length === 0 ? (
              <p className="px-3 py-4 text-center text-xs text-foreground-muted">
                {t("noWorkspaces")}
              </p>
            ) : (
              workspaces.map((ws) => (
                <button
                  type="button"
                  key={ws.id}
                  onClick={() => handleSwitch(ws)}
                  className={`flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-start text-xs hover:bg-surface-raised ${
                    ws.id === currentId
                      ? "bg-brand-surface text-brand-foreground"
                      : "text-foreground"
                  }`}
                >
                  <span className="flex-1 truncate">{ws.name}</span>
                  <span
                    className={`rounded px-1 text-2xs font-medium ${ROLE_COLORS[ws.role] ?? ROLE_COLORS.member}`}
                  >
                    {ws.role}
                  </span>
                </button>
              ))
            )}
            <div className="my-1 h-px bg-surface-inset" />
            <button
              type="button"
              onClick={() => {
                setOpen(false);
                setDialogOpen(true);
              }}
              className="flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-start text-xs font-medium text-concept-foreground hover:bg-concept-surface"
            >
              <Plus className="h-3 w-3" />
              {t("newWorkspace")}
            </button>
            <Link
              href="/settings/workspace/general"
              onClick={() => setOpen(false)}
              className="flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-start text-xs text-foreground-muted hover:bg-surface-raised"
            >
              <Settings className="h-3 w-3" />
              {t("workspaceSettings")}
            </Link>
          </div>
        </PopoverContent>
      </Popover>
      <CreateWorkspaceDialog open={dialogOpen} onOpenChange={setDialogOpen} />
    </>
  );
}
