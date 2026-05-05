// `installExtensions()` — boot pass that wires every entry in
// `EXTENSIONS` into the runtime registries.
//
// The boot is idempotent: every invocation uninstalls extensions
// from the previous round that no longer appear in the list, and
// re-runs `install` for entries whose id was already installed.
// Layout re-renders, HMR reloads, and double-mounts therefore can't
// accumulate ghost facets / modes regardless of how many times the
// `<ExtensionsBoot>` effect fires.

import {
  registerInspectorFacet,
  unregisterInspectorFacet,
} from "@/components/workbench/inspector/facets/registry";
import {
  registerWorkbenchMode,
  unregisterWorkbenchMode,
} from "@/lib/workbench-modes";

import type { WorkbenchExtension, WorkbenchExtensionAPI } from "./types";

const api: WorkbenchExtensionAPI = {
  registerWorkbenchMode,
  unregisterWorkbenchMode,
  registerInspectorFacet,
  unregisterInspectorFacet,
};

interface InstalledRecord {
  uninstall: () => void;
}

const installed = new Map<string, InstalledRecord>();

function safeUninstall(id: string, run: () => void, context: "live" | "stale") {
  try {
    run();
  } catch (err) {
    const label = context === "stale" ? "stale " : "";
    console.warn(
      `[extensions] uninstall threw for ${label}"${id}":`,
      err instanceof Error ? err.message : err,
    );
  }
}

/**
 * Install every extension in declared order, returning the seen ids.
 * Re-installing an extension with the same `id` first runs its prior
 * `uninstall` so the next round starts from a clean baseline.
 */
export function installExtensions(extensions: WorkbenchExtension[]): string[] {
  const seenIds = new Set<string>();
  for (const ext of extensions) {
    if (seenIds.has(ext.id)) {
      // Two extensions sharing an id is almost always a copy-paste
      // mistake. Skip the duplicate so the first registration wins.
      console.warn(
        `[extensions] duplicate extension id "${ext.id}" — keeping first`,
      );
      continue;
    }
    seenIds.add(ext.id);

    const prior = installed.get(ext.id);
    if (prior) safeUninstall(ext.id, prior.uninstall, "live");

    const uninstall = ext.install(api);
    installed.set(ext.id, { uninstall });
  }

  for (const [id, record] of installed) {
    if (!seenIds.has(id)) {
      safeUninstall(id, record.uninstall, "stale");
      installed.delete(id);
    }
  }

  return [...seenIds];
}

/**
 * Uninstall every currently-installed extension. Test-only escape
 * hatch — production never calls this.
 */
export function _uninstallAllExtensionsForTests(): void {
  for (const [, record] of installed) {
    try {
      record.uninstall();
    } catch {
      /* tests already asserted the desired post-state */
    }
  }
  installed.clear();
}

/** Snapshot of currently-installed extension ids in registration order. */
export function listInstalledExtensionIds(): string[] {
  return [...installed.keys()];
}
