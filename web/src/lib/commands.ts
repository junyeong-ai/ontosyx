/**
 * Single source of truth for the global command palette.
 *
 * Each command is one entry — when the operator presses
 * Cmd/Ctrl+Shift+P, the palette renders these as actionable rows.
 * Adding a new global action means appending one entry; the palette
 * automatically picks up label, group, keybinding, and visibility
 * predicate.
 *
 * Labels reference i18n keys under `commandPalette.commands.<id>` so
 * the palette is locale-aware. Keep the keys flat — nesting buys
 * nothing here and makes the catalogue harder to scan.
 */

import type { useRouter } from "next/navigation";
import type { AppStore } from "@/lib/store";

/** Stable command identifier — also the i18n catalogue key. */
export type CommandId =
  | "search-entities"
  | "navigate-design"
  | "navigate-analyze"
  | "navigate-explore"
  | "navigate-dashboard"
  | "navigate-settings"
  | "toggle-explorer"
  | "toggle-inspector"
  | "toggle-bottom-panel"
  | "cycle-bottom-panel-mode"
  | "panel-mode-fullscreen"
  | "panel-mode-default";

/**
 * Group label for visual clustering in the palette UI. Free-form
 * ordered list — `commands.ts` declaration order picks the visible
 * order; the palette stable-sorts groups by first-occurrence.
 */
export type CommandGroup = "navigate" | "view" | "search" | "settings";

/**
 * Context passed to every command's `action` thunk. Provides every
 * dependency commands realistically need without forcing each one
 * to import its own router / store hook (impossible in a static
 * catalogue — these utilities are React-coupled).
 */
export interface CommandContext {
  /** Next.js router for navigation commands. */
  router: ReturnType<typeof useRouter>;
  /** Mutable Zustand store for state-toggling commands. */
  store: {
    getState: () => AppStore;
    setState: (partial: Partial<AppStore> | ((state: AppStore) => Partial<AppStore>)) => void;
  };
}

export interface CommandDef {
  id: CommandId;
  group: CommandGroup;
  /** i18n key under `commandPalette.commands.<id>.label`. */
  labelKey: CommandId;
  /**
   * Cross-platform shortcut hint shown in the palette row (rendered
   * as `⌘K` on macOS, `Ctrl+K` elsewhere). The actual keybinding
   * lives in the layout's keyboard handler — this field is purely
   * informational so the palette can advertise discoverable
   * shortcuts. `null` when no keybinding exists.
   */
  shortcut: { mac: string; other: string } | null;
  /**
   * `true` when the command should appear in the palette in the
   * current store state. Visibility predicates keep the palette
   * focused — e.g. "Toggle Inspector" hides outside design mode.
   */
  visible: (store: AppStore) => boolean;
  /** Imperative effect when the operator selects the command. */
  action: (ctx: CommandContext) => void | Promise<void>;
}

export const COMMANDS: CommandDef[] = [
  {
    id: "search-entities",
    group: "search",
    labelKey: "search-entities",
    shortcut: { mac: "⌘K", other: "Ctrl+K" },
    visible: () => true,
    action: ({ store }) => {
      store.setState({ isSearchOpen: true });
    },
  },
  {
    id: "navigate-design",
    group: "navigate",
    labelKey: "navigate-design",
    shortcut: null,
    visible: () => true,
    action: ({ router }) => router.push("/design"),
  },
  {
    id: "navigate-analyze",
    group: "navigate",
    labelKey: "navigate-analyze",
    shortcut: null,
    visible: () => true,
    action: ({ router }) => router.push("/analyze"),
  },
  {
    id: "navigate-explore",
    group: "navigate",
    labelKey: "navigate-explore",
    shortcut: null,
    visible: () => true,
    action: ({ router }) => router.push("/explore"),
  },
  {
    id: "navigate-dashboard",
    group: "navigate",
    labelKey: "navigate-dashboard",
    shortcut: null,
    visible: () => true,
    action: ({ router }) => router.push("/dashboard"),
  },
  {
    id: "navigate-settings",
    group: "settings",
    labelKey: "navigate-settings",
    shortcut: null,
    visible: () => true,
    action: ({ router }) => router.push("/settings"),
  },
  {
    id: "toggle-explorer",
    group: "view",
    labelKey: "toggle-explorer",
    shortcut: null,
    visible: () => true,
    action: ({ store }) => {
      store.getState().toggleExplorer();
    },
  },
  {
    id: "toggle-inspector",
    group: "view",
    labelKey: "toggle-inspector",
    shortcut: null,
    visible: () => true,
    action: ({ store }) => {
      store.getState().toggleInspector();
    },
  },
  {
    id: "toggle-bottom-panel",
    group: "view",
    labelKey: "toggle-bottom-panel",
    shortcut: null,
    visible: () => true,
    action: ({ store }) => {
      store.getState().toggleBottomPanel();
    },
  },
  {
    id: "cycle-bottom-panel-mode",
    group: "view",
    labelKey: "cycle-bottom-panel-mode",
    shortcut: { mac: "⌘\\", other: "Ctrl+\\" },
    visible: () => true,
    action: ({ store }) => {
      store.getState().cycleBottomPanelMode();
    },
  },
  {
    id: "panel-mode-fullscreen",
    group: "view",
    labelKey: "panel-mode-fullscreen",
    shortcut: { mac: "⌘⇧\\", other: "Ctrl+Shift+\\" },
    visible: (s) => s.bottomPanelMode !== "fullscreen",
    action: ({ store }) => {
      store.getState().setBottomPanelMode("fullscreen");
    },
  },
  {
    id: "panel-mode-default",
    group: "view",
    labelKey: "panel-mode-default",
    shortcut: { mac: "Esc", other: "Esc" },
    visible: (s) => s.bottomPanelMode !== "default",
    action: ({ store }) => {
      store.getState().setBottomPanelMode("default");
    },
  },
];

/**
 * Filter the static catalogue against the current store state and a
 * lower-cased query. The filter is lenient: any token in the query
 * that appears in the localised label, group, or id passes. Empty
 * query returns every visible command.
 */
export function filterCommands(
  commands: ReadonlyArray<CommandDef>,
  store: AppStore,
  query: string,
  resolveLabel: (id: CommandId) => string,
): CommandDef[] {
  const visible = commands.filter((c) => c.visible(store));
  const q = query.trim().toLowerCase();
  if (!q) return visible;
  const tokens = q.split(/\s+/);
  return visible.filter((c) => {
    const haystack = [resolveLabel(c.id), c.group, c.id]
      .join(" ")
      .toLowerCase();
    return tokens.every((t) => haystack.includes(t));
  });
}

/** Pick the platform-appropriate shortcut string. */
export function shortcutFor(
  cmd: CommandDef,
  isMac: boolean,
): string | null {
  if (!cmd.shortcut) return null;
  return isMac ? cmd.shortcut.mac : cmd.shortcut.other;
}
