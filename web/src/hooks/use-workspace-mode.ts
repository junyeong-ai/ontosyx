"use client";

import { usePathname } from "next/navigation";

import { listWorkbenchModes } from "@/lib/workbench-modes";
import type { WorkspaceMode } from "@/lib/store";

/**
 * URL-derived workspace mode. The pathname is the single source of
 * truth — reloads and share links land on the same surface.
 *
 * The mode list is read from the workbench-mode registry on every
 * call, so plugin-registered modes are recognised here automatically
 * without needing a parallel switch statement. The `"design"` fallback
 * fires only when the pathname doesn't match any registered mode —
 * defensive against routes outside the workbench shell.
 */
export function useWorkspaceMode(): WorkspaceMode {
  const pathname = usePathname() ?? "";
  for (const mode of listWorkbenchModes()) {
    if (pathname === mode.href || pathname.startsWith(`${mode.href}/`)) {
      return mode.id;
    }
  }
  return "design";
}

/** Path for a given mode id. Resolves through the registry so
 *  plugin-supplied hrefs round-trip; falls back to `/<id>` for
 *  unknown modes (matches the historical default-mode convention). */
export function workspaceModeHref(mode: WorkspaceMode): string {
  return listWorkbenchModes().find((m) => m.id === mode)?.href ?? `/${mode}`;
}
