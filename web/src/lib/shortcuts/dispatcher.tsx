"use client";

import { useEffect } from "react";
import { getRegisteredShortcuts, specMatchesEvent } from "./registry";

/**
 * Single global keydown listener that drives the shortcut registry.
 * Mounted once at the workbench shell. Iterates registered specs in
 * priority order (highest first) and stops after the first match —
 * scoped shortcuts (modal, canvas) override globals.
 */
export function ShortcutDispatcher() {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const list = getRegisteredShortcuts();
      for (const spec of list) {
        if (specMatchesEvent(spec, e)) {
          spec.handler(e);
          return;
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);
  return null;
}
