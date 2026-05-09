// Workbench mode registry — runtime-mutable, single source of truth
// for which top-level surfaces live in the workbench shell.
//
// Each entry combines the navigation metadata (label, icon, href)
// with optional UI hints (panel toggles, keyboard shortcut). The
// seven default modes ship pre-registered below; plugin code adds
// another with
//
//     import { registerWorkbenchMode } from "@/lib/workbench-modes";
//     registerWorkbenchMode({ id: "audit", labelKey: "modeAudit",
//       icon: ShieldIcon, href: "/audit" });
//
// Sidebar / panel-toggle gate / `useWorkspaceMode` URL match all
// read through the public API rather than a static array, so a new
// mode is uniformly discoverable without forking this file.
//
// `settings` is intentionally absent — it has a navigation shortcut
// (`g ,`) but is not a peer of the workbench modes; it lives in the
// chrome footer and follows its own sub-navigation tree.

import { Book, BookOpen, CircleCheck, FlaskConical, GitBranch, GitFork, History, LayoutDashboard, LineChart, Link, List, MessageCircle, Search, ShieldCheck, Wand2 } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import {
  shortcutForRoute,
  type NavigationRoute,
  type NavigationShortcut,
} from "./navigation-shortcuts";
import type { WorkspaceMode } from "./store";

/**
 * Sidebar navigation category — workbench surfaces are ontology-
 * authoring tools (design / explore / vocabulary / …), operations
 * surfaces are workspace-level monitoring + action queues
 * (evaluation / approvals / audit / …). The sidebar groups modes
 * by this discriminator with a section heading between, mirroring
 * the Linear / Datadog / Stripe Dashboard pattern of separating
 * "build" from "operate". Settings (cog) sits below both as
 * system configuration.
 */
export type ModeCategory = "workbench" | "operations";

export interface WorkbenchMode {
  /** Stable identity. Matches a `WorkspaceMode` literal for default
   *  modes; plugin-registered modes use their own id string. */
  id: WorkspaceMode;
  /** Whether this mode lives under the Workbench or Operations
   *  group in the sidebar. Defaults to `"workbench"` so legacy
   *  registrations stay where they are. */
  category?: ModeCategory;
  /** Sidebar label key (resolved through the `chrome.sidebar` namespace). */
  labelKey: string;
  /** Sidebar icon glyph. */
  icon: LucideIcon;
  /** Top-level URL — drives the sidebar Link target and
   *  `useWorkspaceMode` URL match. */
  href: string;
  /** Optional `g`-prefix shortcut for the help dialog and tooltip.
   *  Default modes ship with one; plugin modes may omit it. */
  shortcut?: NavigationShortcut;
  /**
   * Render the explorer / inspector panel toggles in the sidebar
   * footer when this mode is active. Only `design` opts in today;
   * any future mode that builds an inspector-style sidebar should
   * flip this flag and inherit the toggles for free.
   */
  hasPanelToggles?: boolean;
  /**
   * Mode is meaningful only AFTER a canonical ontology has been
   * committed to the workspace (post-completion artifacts: physical-
   * to-graph mappings, source-load lineage). On a greenfield
   * workspace the surface only renders an empty-state pointing at
   * Design mode, so the sidebar hides the entry entirely until the
   * canonical exists. The shortcut + URL stay valid (deep-links and
   * help dialog still work) — visibility is the only thing the gate
   * controls.
   */
  requiresCanonical?: boolean;
}

// Build a default mode by deriving `href` + `shortcut` from the
// matching navigation-shortcut record. Plugin modes don't go through
// this helper — they construct `WorkbenchMode` directly so they can
// supply their own href and skip the shortcut.
function defaultMode(
  id: NavigationRoute & WorkspaceMode,
  labelKey: string,
  icon: LucideIcon,
  options: { hasPanelToggles?: boolean; requiresCanonical?: boolean } = {},
): WorkbenchMode {
  const shortcut = shortcutForRoute(id);
  return {
    id,
    labelKey,
    icon,
    href: shortcut.href,
    shortcut,
    hasPanelToggles: options.hasPanelToggles,
    requiresCanonical: options.requiresCanonical,
  };
}

/**
 * Order matters — the sidebar renders modes top-to-bottom in
 * registration order, grouped by `category`. Workbench defaults
 * cluster the editing trio (Design / Analyze / Explore) ahead of
 * the artifact surfaces (Dashboard / Glossary / Vocabulary /
 * Recipes); Operations modes are the "monitor + act" surfaces
 * that operators visit daily. Operations URLs live under
 * `/settings/*` today — the chrome groups them as Operations so
 * the navigation reads as "Workbench vs Operations vs Settings"
 * regardless of route nesting.
 */
const DEFAULT_MODES: readonly WorkbenchMode[] = [
  defaultMode("design", "modeDesign", Wand2, { hasPanelToggles: true }),
  defaultMode("analyze", "modeAnalyze", MessageCircle),
  defaultMode("explore", "modeExplore", Search),
  defaultMode("dashboard", "modeDashboard", LayoutDashboard),
  defaultMode("glossary", "modeGlossary", Book),
  defaultMode("vocabulary", "modeVocabulary", List),
  defaultMode("mappings", "modeMappings", Link, { requiresCanonical: true }),
  defaultMode("lineage", "modeLineage", GitBranch, { requiresCanonical: true }),
  defaultMode("branches", "modeBranches", GitFork),
  defaultMode("recipes", "modeRecipes", LineChart),
  // --- Operations ---------------------------------------------------------
  // Workspace-level monitoring + action queues. Daily-visit surfaces
  // get top-level navigation parity with workbench tools; the underlying
  // route stays under `/settings/*` until a follow-up URL migration.
  {
    id: "evaluation",
    category: "operations",
    labelKey: "modeEvaluation",
    icon: FlaskConical,
    href: "/settings/evaluation",
  },
  {
    id: "approvals",
    category: "operations",
    labelKey: "modeApprovals",
    icon: ShieldCheck,
    href: "/settings/governance/approvals",
  },
  {
    id: "audit",
    category: "operations",
    labelKey: "modeAudit",
    icon: History,
    href: "/settings/governance/audit",
  },
  {
    id: "quality",
    category: "operations",
    labelKey: "modeQuality",
    icon: CircleCheck,
    href: "/settings/quality",
  },
  {
    id: "knowledgeBase",
    category: "operations",
    labelKey: "modeKnowledgeBase",
    icon: BookOpen,
    href: "/settings/knowledge/base",
  },
];

const modeOrder: WorkspaceMode[] = DEFAULT_MODES.map((m) => m.id);
const modeById = new Map<WorkspaceMode, WorkbenchMode>(
  DEFAULT_MODES.map((m) => [m.id, m] as const),
);

export interface RegisterModeOptions {
  /** Insert before the named mode id; ignored if not present. */
  before?: WorkspaceMode;
  /** Insert after the named mode id; ignored if not present. */
  after?: WorkspaceMode;
}

/**
 * Register or replace a workbench mode. Re-registering an existing
 * id replaces the entry while preserving its position; a fresh id
 * is appended unless `before` / `after` is supplied.
 */
export function registerWorkbenchMode(
  m: WorkbenchMode,
  options: RegisterModeOptions = {},
): void {
  const isReplacement = modeById.has(m.id);
  modeById.set(m.id, m);
  if (isReplacement) return;

  if (options.before && modeOrder.includes(options.before)) {
    const idx = modeOrder.indexOf(options.before);
    modeOrder.splice(idx, 0, m.id);
    return;
  }
  if (options.after && modeOrder.includes(options.after)) {
    const idx = modeOrder.indexOf(options.after);
    modeOrder.splice(idx + 1, 0, m.id);
    return;
  }
  modeOrder.push(m.id);
}

/** Remove a previously registered mode. Idempotent on unknown ids. */
export function unregisterWorkbenchMode(id: WorkspaceMode): void {
  if (!modeById.has(id)) return;
  modeById.delete(id);
  const idx = modeOrder.indexOf(id);
  if (idx !== -1) modeOrder.splice(idx, 1);
}

/** Snapshot of currently-registered modes in declared order. */
export function listWorkbenchModes(): WorkbenchMode[] {
  return modeOrder.map((id) => modeById.get(id)!).filter(Boolean);
}

/**
 * Filter `listWorkbenchModes()` by sidebar category so the chrome
 * can render Workbench / Operations sections separately. Modes
 * without an explicit `category` resolve to `"workbench"`.
 */
export function listModesByCategory(category: ModeCategory): WorkbenchMode[] {
  return listWorkbenchModes().filter(
    (m) => (m.category ?? "workbench") === category,
  );
}

/** Resolve a registered mode by id, or `undefined` if not registered. */
export function workbenchModeById(id: WorkspaceMode): WorkbenchMode | undefined {
  return modeById.get(id);
}

/** Test-only escape hatch: reset the registry to the default-shipping
 *  set so registration tests stay hermetic. Production never calls this. */
export function _resetWorkbenchModeRegistryForTests(): void {
  modeOrder.length = 0;
  modeById.clear();
  for (const m of DEFAULT_MODES) {
    modeOrder.push(m.id);
    modeById.set(m.id, m);
  }
}
