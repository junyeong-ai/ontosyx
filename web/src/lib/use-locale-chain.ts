"use client";

/**
 * `useLocaleChain` — fetches the active workspace's
 * `locale_fallback` from `GET /api/workspaces/me` and exposes it as
 * the BCP 47 chain `localize()` walks. Re-runs whenever the
 * workspace id in `localStorage.ontosyx.workspace_id` changes
 * (storage event + manual `chainChange` dispatch from the
 * workspace switcher).
 *
 * Why a hook and not a constant:
 * - The chain is workspace-scoped. Different workspaces inside the
 *   same login session can hold different default locales.
 * - The static `DEFAULT_LOCALE_CHAIN = ["ko","en"]` lives in
 *   `lib/locale/localize` as a *boot fallback* for the brief moment
 *   before the fetch resolves; production renders use the chain
 *   this hook returns.
 *
 * The fetch goes through the BFF proxy at `/api/proxy/workspaces/me`
 * — the same path every other API call uses, so the workspace
 * header is injected server-side.
 *
 * **Multi-instance safety.** Backed by TanStack Query so N components
 * calling this hook share a single fetch per workspace. Without the
 * shared cache, an N-instance consumer (e.g., the canvas's per-edge
 * tooltip) would mount N concurrent fetches at workbench load.
 */

import { useSyncExternalStore } from "react";
import { useQuery } from "@tanstack/react-query";

import { getWorkspaceId } from "@/lib/workspace";
import { DEFAULT_LOCALE_CHAIN } from "@/lib/locale/localize";

const SUBSCRIPTION_KEY = "ontosyx.workspace_id";

function subscribeToWorkspace(onChange: () => void): () => void {
  if (typeof window === "undefined") return () => {};
  const handle = (event: StorageEvent) => {
    if (event.key === SUBSCRIPTION_KEY) onChange();
  };
  window.addEventListener("storage", handle);
  // Same-tab change: the workspace switcher dispatches this.
  const sameTab = () => onChange();
  window.addEventListener("ontosyx:workspaceChange", sameTab);
  return () => {
    window.removeEventListener("storage", handle);
    window.removeEventListener("ontosyx:workspaceChange", sameTab);
  };
}

function readWorkspaceId(): string | undefined {
  return getWorkspaceId();
}

function readWorkspaceIdServer(): string | undefined {
  // SSR has no localStorage / no workspace selection yet.
  return undefined;
}

interface WorkspaceMe {
  id: string;
  name: string;
  slug: string;
  role: string;
  primary_locale: string;
  locale_fallback: string[];
}

async function fetchWorkspaceMe(workspaceId: string): Promise<WorkspaceMe | null> {
  const response = await fetch("/api/proxy/workspaces/me", {
    headers: { "x-workspace-id": workspaceId },
  });
  if (!response.ok) return null;
  const envelope = (await response.json()) as { data?: WorkspaceMe };
  return envelope.data ?? null;
}

export const localeChainKeys = {
  all: ["localeChain"] as const,
  workspace: (workspaceId: string) =>
    [...localeChainKeys.all, workspaceId] as const,
};

/**
 * Returns the current workspace's locale fallback chain. Returns
 * `DEFAULT_LOCALE_CHAIN` while the fetch is in flight or when no
 * workspace is selected, so call-sites never receive `undefined`.
 *
 * The chain reference is stable for a given workspace — re-renders
 * of consuming components from this hook only fire on workspace
 * switch or initial fetch resolution.
 */
export function useLocaleChain(): readonly string[] {
  const workspaceId = useSyncExternalStore(
    subscribeToWorkspace,
    readWorkspaceId,
    readWorkspaceIdServer,
  );

  const { data } = useQuery({
    queryKey: workspaceId ? localeChainKeys.workspace(workspaceId) : localeChainKeys.all,
    queryFn: () => fetchWorkspaceMe(workspaceId!),
    enabled: !!workspaceId,
    // Locale fallback is admin-set workspace metadata; treat the
    // value as effectively static between explicit invalidations.
    staleTime: Infinity,
  });

  return data?.locale_fallback?.length ? data.locale_fallback : DEFAULT_LOCALE_CHAIN;
}
