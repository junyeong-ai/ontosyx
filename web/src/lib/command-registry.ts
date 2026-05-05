// Command registry — typed plugin surface for the unified command
// palette (⌘K). Each surface (workbench-global, settings, canvas,
// future plugins) registers a `CommandSource` describing the
// commands it contributes; the palette reads through `listSources()`
// and renders them grouped + searchable.
//
// Backed by the generic `PluginRegistry<CommandSource>` from
// `lib/plugins/registry.ts` — snapshot caching + subscriber
// notification + idempotent re-registration come for free, so this
// module exposes only the call-site sugar (typed names, sort by
// `order`, `Command` shape).
//
// Keep the contract narrow: nothing in here imports React or
// Next.js — every command's `execute` receives a `CommandContext`
// at call time, so the registry survives SSR and module evaluation
// in any environment.

import type { useRouter } from "next/navigation";
import type { IconSvgElement } from "@hugeicons/react";

import type { AppStore } from "@/lib/store";
import { PluginRegistry, type PluginItem } from "@/lib/plugins/registry";

/**
 * Context passed to every command's `execute` thunk. The host
 * (palette) populates this just-in-time so the registry stays
 * decoupled from the React tree.
 */
export interface CommandContext {
  router: ReturnType<typeof useRouter>;
  store: {
    getState: () => AppStore;
    setState: (
      partial: Partial<AppStore> | ((state: AppStore) => Partial<AppStore>),
    ) => void;
  };
}

export interface Command {
  /** Globally unique within its source. */
  id: string;
  /** Localised display label (already-resolved string). */
  label: string;
  /**
   * Cross-platform shortcut hint — `{ mac: "⌘K", other: "Ctrl+K" }`.
   * Shown to the right of the row. The palette does not bind
   * shortcuts itself; binding lives in the host (e.g. layout-level
   * `useShortcut` registrations).
   */
  shortcut?: { mac: string; other: string };
  /** Optional leading icon. */
  icon?: IconSvgElement;
  /**
   * Optional subtitle / hint shown below the label. Use sparingly —
   * dense palettes are easier to scan.
   */
  description?: string;
  /**
   * Additional search keywords. Useful when the visible label
   * differs from common search terms (e.g. label "Open Inbox",
   * keywords ["notifications", "messages"]).
   */
  keywords?: string[];
  /**
   * Imperative effect when the operator selects the command. The
   * palette closes itself before invoking; use `requestAnimationFrame`
   * if the action depends on the palette being unmounted.
   */
  execute: (ctx: CommandContext) => void | Promise<void>;
}

export interface CommandSource extends PluginItem {
  /** Stable source id — `"global" | "settings" | "canvas" | …`. */
  id: string;
  /** Localised group label rendered above each source's commands. */
  groupLabel: string;
  /**
   * Order weight — sources sort by this ascending. Default 0 places
   * a source between negative-weight (top) and positive-weight
   * (bottom) sources.
   */
  order?: number;
  /**
   * Static command list, OR a thunk that returns the current list.
   * Thunks are re-evaluated on every palette open + on every
   * `notify()` call from the source — use it when underlying state
   * changes (selection, store toggles, route changes) so the palette
   * filter sees the new list.
   */
  commands: () => Command[];
}

/**
 * The command registry singleton. Exported so React subtrees can
 * register sources via the generic `usePlugin(commandRegistry, …)`
 * hook instead of a domain-specific wrapper. Non-React code (route
 * guards, plugin loaders, ad-hoc subscriptions) reads this same
 * instance.
 */
export const commandRegistry = new PluginRegistry<CommandSource>({
  compare: (a, b) => (a.order ?? 0) - (b.order ?? 0),
});

/**
 * Filter helper used by the palette UI. Splits the query into
 * tokens; each token must appear in the command's label, id, or
 * keywords (case-insensitive). Empty query returns the input
 * verbatim. The filter is lenient — partial matches pass — to
 * favour discovery over precision.
 */
export function filterCommands(
  commands: ReadonlyArray<Command>,
  query: string,
): Command[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...commands];
  const tokens = q.split(/\s+/).filter(Boolean);
  return commands.filter((cmd) => {
    const haystack = [cmd.label, cmd.id, ...(cmd.keywords ?? [])]
      .join(" ")
      .toLowerCase();
    return tokens.every((t) => haystack.includes(t));
  });
}

/** Pick the platform-appropriate shortcut glyph. */
export function shortcutGlyph(
  cmd: Command,
  isMac: boolean,
): string | null {
  if (!cmd.shortcut) return null;
  return isMac ? cmd.shortcut.mac : cmd.shortcut.other;
}
