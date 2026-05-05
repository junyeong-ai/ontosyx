// Public extension surface for workbench plugins.
//
// An extension is a passive object — `install(api)` runs once at boot
// and returns an `uninstall()` cleanup. The contract is symmetric on
// purpose: tests, HMR, and feature-flag gating all rely on being able
// to remove an extension as cleanly as it was added.
//
// Authoring a new extension is three lines: define an object that
// implements `WorkbenchExtension`, drop it into the `EXTENSIONS`
// array in `index.ts`, and the boot pass picks it up. No hidden
// auto-discovery — the registration list is explicit so the
// production bundle stays static-analyzable and tree-shakeable.
//
// Surface scope:
//   * Inspector facets — fully open: any string id is registrable,
//     so plugins can add new facets ("permissions", "audit", …).
//   * Workbench modes — fully open: `WorkspaceMode` is widened to
//     accept arbitrary strings, so plugins can register entirely new
//     modes (e.g. `audit`, `incident`) and the sidebar / URL match
//     pick them up automatically. The seven default ids ship with
//     editor autocomplete; extension-supplied ids are accepted.

import type {
  RegisterFacetOptions,
  InspectorFacet,
} from "@/components/workbench/inspector/facets/registry";
import type {
  RegisterModeOptions,
  WorkbenchMode,
} from "@/lib/workbench-modes";

/**
 * The mutation surface exposed to extensions. Wrapping the registries
 * (rather than re-exporting them) gives us one funnel point for any
 * future cross-cutting concern — telemetry, error boundaries around
 * `install`, dev-warnings on duplicate ids — without each extension
 * importing those registries directly.
 */
export interface WorkbenchExtensionAPI {
  registerWorkbenchMode: (
    mode: WorkbenchMode,
    options?: RegisterModeOptions,
  ) => void;
  unregisterWorkbenchMode: (id: WorkbenchMode["id"]) => void;
  registerInspectorFacet: (
    facet: InspectorFacet,
    options?: RegisterFacetOptions,
  ) => void;
  unregisterInspectorFacet: (id: InspectorFacet["id"]) => void;
}

export interface WorkbenchExtension {
  /** Stable identity. Used for dedup, logging, and the disable list. */
  id: string;
  /** Optional one-line dev-tools label. */
  description?: string;
  /**
   * Run the extension's registrations against the provided API. The
   * returned function is invoked on uninstall — it should reverse
   * exactly what `install` did so reload / HMR / feature-flag
   * toggling stays clean.
   */
  install: (api: WorkbenchExtensionAPI) => () => void;
}
