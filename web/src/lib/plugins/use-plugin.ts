"use client";

// Generic React hook for plugin registration. Mirrors the
// non-React `register(item)` API while owning the unmount cleanup.
// Use a stable item identity (memoised at the call site) — the
// effect re-runs whenever the item identity changes, so re-rendering
// with a fresh literal each render would thrash the registry.

import { useEffect } from "react";

import type { PluginItem, PluginRegistry } from "./registry";

export function usePlugin<T extends PluginItem>(
  registry: PluginRegistry<T>,
  item: T,
): void {
  useEffect(() => {
    return registry.register(item);
  }, [registry, item]);
}
