"use client";

import { useSyncExternalStore } from "react";
import { useAppStore } from "./index";

/**
 * Returns true after Zustand persist middleware has hydrated from localStorage.
 * Use this to guard rendering of components that depend on persisted state
 * (e.g., workspaceMode) to prevent a flash of default values.
 *
 * React 19 compliant: subscribes to zustand's own hydration signal via
 * `useSyncExternalStore` instead of paying a useEffect + setState cascade.
 * Hydration is an external system (localStorage), so the external-store
 * subscription pattern is the canonical fit per React 19 guidance.
 *
 * Why the arrow wrappers: `useAppStore.persist.onFinishHydration` and
 * `.hasHydrated` both dispatch through the `persist` carrier — passing
 * bare method references strips the receiver and throws
 * "Cannot read properties of undefined" the first time React invokes
 * the subscriber.
 */
export function useHydrated(): boolean {
  return useSyncExternalStore(
    (onChange) => useAppStore.persist.onFinishHydration(onChange),
    () => useAppStore.persist.hasHydrated(),
    () => false,
  );
}
