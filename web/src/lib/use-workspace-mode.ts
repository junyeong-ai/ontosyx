"use client";

import { usePathname } from "next/navigation";

import type { WorkspaceMode } from "@/lib/store";

/**
 * URL-derived replacement for `useAppStore(s => s.workspaceMode)`.
 *
 * After Phase 2-4 the active workspace mode lives in the pathname
 * (`/design`, `/analyze`, `/explore`, `/dashboard`). Components that
 * previously read `workspaceMode` from Zustand call this hook instead,
 * so the URL is the single source of truth and reloads / share links
 * land on the same surface.
 *
 * `"design"` is the default when the path doesn't match a known mode —
 * callers that render inside `(workbench)` always match one of the
 * four prefixes, so the fallback is purely defensive.
 */
const MODE_PREFIXES: ReadonlyArray<[string, WorkspaceMode]> = [
  ["/design", "design"],
  ["/analyze", "analyze"],
  ["/explore", "explore"],
  ["/dashboard", "dashboard"],
  ["/glossary", "glossary"],
];

export function useWorkspaceMode(): WorkspaceMode {
  const pathname = usePathname() ?? "";
  for (const [prefix, mode] of MODE_PREFIXES) {
    if (pathname === prefix || pathname.startsWith(`${prefix}/`)) {
      return mode;
    }
  }
  return "design";
}

/** Path for a given mode, used by navigation helpers. */
export function workspaceModeHref(mode: WorkspaceMode): string {
  return `/${mode}`;
}
