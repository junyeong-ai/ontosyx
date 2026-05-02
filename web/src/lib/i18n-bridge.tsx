// i18n bridge — module-level translator handle for non-component
// callers (Zustand stores, lib utilities). React-context-free
// modules can't call `useTranslations`; the workbench shell
// registers the active translator on mount and the modules read
// from this module-level slot.
//
// The active workspace's locale already lives in `messages` via
// next-intl's server-resolved cookie. The `<I18nBridge>` provider
// runs once at the workbench shell, snapshots the relevant
// namespaces into raw JSON, and stashes them here so a Zustand
// reducer can format a toast string at command-application time
// without round-tripping through React.

"use client";

import { useEffect } from "react";
import { useTranslations } from "next-intl";

interface BridgeNamespaces {
  inspectorToast: {
    undoLimit: string;
  };
}

let snapshot: BridgeNamespaces | null = null;

export function getI18nBridge(): BridgeNamespaces {
  if (!snapshot) {
    // Pre-mount fallback. Only relevant during SSR or before
    // `<I18nBridgeProvider>` runs; the keys are stable enough that
    // the placeholder never reaches a user in the live app.
    return { inspectorToast: { undoLimit: "" } };
  }
  return snapshot;
}

export function I18nBridgeProvider({ children }: { children: React.ReactNode }) {
  const tInspector = useTranslations("inspector.toast");
  useEffect(() => {
    snapshot = {
      inspectorToast: {
        undoLimit: tInspector("undoLimit"),
      },
    };
  }, [tInspector]);
  return <>{children}</>;
}
