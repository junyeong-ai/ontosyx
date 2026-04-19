"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";
import { Spinner } from "@/components/ui/spinner";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowDown01Icon,
  PlusSignIcon,
  Settings01Icon,
} from "@hugeicons/core-free-icons";
import { toast } from "sonner";
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
  "flex min-w-0 items-center gap-1.5 rounded-md border border-zinc-200 bg-zinc-50 px-2.5 py-1.5 text-xs font-medium text-zinc-700 transition-colors hover:bg-zinc-100 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800";

const POPOVER_CLASS =
  "z-50 w-72 rounded-lg border border-zinc-200 bg-white shadow-lg data-[starting-style]:scale-95 data-[starting-style]:opacity-0 data-[ending-style]:scale-95 data-[ending-style]:opacity-0 transition-all dark:border-zinc-700 dark:bg-zinc-900";

// ---------------------------------------------------------------------------

const ROLE_COLORS: Record<string, string> = {
  owner:
    "bg-amber-100 text-amber-700 dark:bg-amber-900/50 dark:text-amber-400",
  admin:
    "bg-indigo-100 text-indigo-700 dark:bg-indigo-900/50 dark:text-indigo-400",
  member:
    "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground",
  viewer:
    "bg-zinc-100 text-muted-foreground dark:bg-zinc-800",
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
    if (isError) toast.error(t("loadError"));
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

  const handleSwitch = (ws: (typeof workspaces)[number]) => {
    setOpen(false);
    setActiveWorkspace(ws.id, ws.name, ws.role);
    window.location.reload();
  };

  const label = cachedName || t("defaultLabel");

  return (
    <>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger className={TRIGGER_CLASS}>
          <span className="max-w-[140px] truncate">{label}</span>
          <HugeiconsIcon
            icon={ArrowDown01Icon}
            className="h-3 w-3 text-muted-foreground"
            size="100%"
          />
        </PopoverTrigger>
        <PopoverContent className={POPOVER_CLASS}>
          <div className="max-h-60 overflow-auto p-1">
            {isFetching ? (
              <div className="flex items-center justify-center py-4">
                <Spinner size="sm" className="text-muted-foreground" />
              </div>
            ) : workspaces.length === 0 ? (
              <p className="px-3 py-4 text-center text-xs text-muted-foreground">
                {t("noWorkspaces")}
              </p>
            ) : (
              workspaces.map((ws) => (
                <button
                  key={ws.id}
                  onClick={() => handleSwitch(ws)}
                  className={`flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-left text-xs hover:bg-zinc-50 dark:hover:bg-zinc-800 ${
                    ws.id === currentId
                      ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-400"
                      : "text-zinc-700 dark:text-zinc-300"
                  }`}
                >
                  <span className="flex-1 truncate">{ws.name}</span>
                  <span
                    className={`rounded px-1 text-[9px] font-medium ${ROLE_COLORS[ws.role] ?? ROLE_COLORS.member}`}
                  >
                    {ws.role}
                  </span>
                </button>
              ))
            )}
            <div className="my-1 h-px bg-zinc-200 dark:bg-zinc-700" />
            <button
              onClick={() => {
                setOpen(false);
                setDialogOpen(true);
              }}
              className="flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-left text-xs font-medium text-indigo-600 hover:bg-indigo-50 dark:text-indigo-400 dark:hover:bg-indigo-950"
            >
              <HugeiconsIcon icon={PlusSignIcon} className="h-3 w-3" size="100%" />
              {t("newWorkspace")}
            </button>
            <Link
              href="/settings/workspace"
              onClick={() => setOpen(false)}
              className="flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-left text-xs text-zinc-500 hover:bg-zinc-50 dark:text-muted-foreground dark:hover:bg-zinc-800"
            >
              <HugeiconsIcon icon={Settings01Icon} className="h-3 w-3" size="100%" />
              {t("workspaceSettings")}
            </Link>
          </div>
        </PopoverContent>
      </Popover>
      <CreateWorkspaceDialog open={dialogOpen} onOpenChange={setDialogOpen} />
    </>
  );
}
