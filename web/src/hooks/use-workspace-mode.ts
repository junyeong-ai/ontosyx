"use client";

import { usePathname } from "next/navigation";

import type { WorkspaceMode } from "@/lib/store";

/**
 * URL-derived workspace mode. The pathname is the single source of
 * truth — reloads and share links land on the same surface. The
 * `"design"` fallback is defensive; every `(workbench)` page matches
 * one of the prefixes.
 */
const MODE_PREFIXES: ReadonlyArray<[string, WorkspaceMode]> = [
  ["/design", "design"],
  ["/analyze", "analyze"],
  ["/explore", "explore"],
  ["/dashboard", "dashboard"],
  ["/glossary", "glossary"],
  ["/vocabulary", "vocabulary"],
  ["/recipes", "recipes"],
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
