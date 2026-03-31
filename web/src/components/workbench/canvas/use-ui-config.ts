"use client";

import { useEffect, useState } from "react";
import { getUiConfig } from "@/lib/api";
import { updateElkConfig } from "./elk-layout";
import type { UiConfig } from "@/types/api";

let globalConfig: UiConfig | null = null;
let fetching = false;
const listeners = new Set<(config: UiConfig) => void>();

/**
 * Fetch UiConfig from the server once and cache globally.
 * Updates the ELK worker timeout on load.
 * All hook instances are notified when config arrives.
 */
export function useUiConfig(): UiConfig | null {
  const [config, setConfig] = useState<UiConfig | null>(globalConfig);

  useEffect(() => {
    // Already loaded — sync immediately
    if (globalConfig) {
      setConfig(globalConfig);
      return;
    }

    // Subscribe to future load
    listeners.add(setConfig);

    // Trigger fetch if not already in progress
    if (!fetching) {
      fetching = true;
      getUiConfig()
        .then((loaded) => {
          globalConfig = loaded;
          updateElkConfig(loaded);
          for (const fn of listeners) fn(loaded);
        })
        .catch((err) => {
          console.warn("[ui-config] Failed to load server config, using defaults:", err);
        })
        .finally(() => {
          fetching = false;
        });
    }

    return () => {
      listeners.delete(setConfig);
    };
  }, []);

  return config;
}
