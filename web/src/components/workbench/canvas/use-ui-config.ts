"use client";

import { useSyncExternalStore } from "react";
import { getUiConfig } from "@/lib/api";
import { updateElkConfig } from "./elk-layout";
import type { UiConfig } from "@/types/api";

// ---------------------------------------------------------------------------
// Module-level store
//
// The UiConfig fetch is a singleton: we load once on first subscribe, then
// every hook instance reads the same cached value. React 19 replaces the
// `useEffect(() => fetchIfNeeded())` pattern with a `useSyncExternalStore`
// subscription to this module-level store.
// ---------------------------------------------------------------------------

let globalConfig: UiConfig | null = null;
let fetchStarted = false;
const listeners = new Set<() => void>();

function notify(): void {
  for (const listener of listeners) listener();
}

function ensureFetch(): void {
  if (fetchStarted) return;
  fetchStarted = true;
  getUiConfig()
    .then((loaded) => {
      globalConfig = loaded;
      updateElkConfig(loaded);
      notify();
    })
    .catch((err) => {
      console.warn(
        "[ui-config] Failed to load server config, using defaults:",
        err,
      );
    });
}

function subscribe(listener: () => void): () => void {
  // Subscribing triggers the fetch on first use — exactly the behaviour
  // the legacy effect body had, but now driven by the `useSyncExternalStore`
  // subscription lifecycle instead of `useEffect + setState`.
  listeners.add(listener);
  ensureFetch();
  return () => {
    listeners.delete(listener);
  };
}

function getSnapshot(): UiConfig | null {
  return globalConfig;
}

function getServerSnapshot(): UiConfig | null {
  return null;
}

/**
 * Fetch UiConfig from the server once and cache globally.
 * Updates the ELK worker timeout on load.
 * All hook instances see the same cached value (single source of truth).
 */
export function useUiConfig(): UiConfig | null {
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}
