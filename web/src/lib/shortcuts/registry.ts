"use client";

import { useEffect, useRef } from "react";
import { create } from "zustand";

import { eventMatchesCombo, formatGlyph, normalizeCombo } from "./combo";

/**
 * Stable identifier for a shortcut. Used as the React key in the help
 * dialog and the deduplication key in the registry. Convention:
 * `<scope>.<action>` — e.g. `"design.save"`, `"workbench.commandPalette"`.
 */
export type ShortcutId = string;

/**
 * Canonical key-combo string. See `combo.ts` for the grammar — `mod+k`,
 * `shift+?`, `Escape`, `[`. `mod` resolves to ⌘ on macOS, Ctrl elsewhere.
 */
export type KeyCombo = string;

export interface ShortcutSpec {
  id: ShortcutId;
  /**
   * Combos that fire this shortcut. Multiple entries when the same
   * action should bind to several keystrokes (e.g. `["?", "shift+?"]`
   * for layouts that disagree about whether `?` requires shift).
   * Combos are matched in declaration order; the first match wins.
   */
  keys: readonly KeyCombo[];
  /** Invoked when any of `keys` matches. The handler decides whether to call `preventDefault`. */
  handler: (e: KeyboardEvent) => void;
  /**
   * Group bucket the help dialog renders under. Free-form i18n key
   * resolved at render time — `"chrome.global"`, `"workbench.design"`, …
   */
  group: string;
  /** Description shown in the help dialog (i18n key resolved by the dialog). */
  description: string;
  /**
   * Runtime guard. When provided and returns false, the dispatcher skips
   * this shortcut even if a combo matches. Use for state-dependent
   * shortcuts (e.g. `Escape` only when a panel is fullscreen). Combo
   * collision detection still runs statically — two specs sharing a
   * combo log a warning regardless of `enabled`.
   */
  enabled?: () => boolean;
  /**
   * Fire even when focus is in an input / textarea / contentEditable.
   * Default false — most shortcuts must yield to typing. Opt in for
   * `mod+enter`-style submit shortcuts.
   */
  fireInTypingTarget?: boolean;
  /** Higher first; default 0. Lets scoped shortcuts beat globals on the same combo. */
  priority?: number;
  /**
   * Override the displayed glyph. Defaults to `formatGlyph(keys[0])` —
   * the per-platform rendering of the first combo (⌘K on macOS, Ctrl+K
   * elsewhere).
   */
  glyph?: string;
}

interface ShortcutsState {
  shortcuts: Map<ShortcutId, ShortcutSpec>;
  register(spec: ShortcutSpec): void;
  unregister(id: ShortcutId): void;
}

function warnOnCollision(
  incoming: ShortcutSpec,
  existing: Map<ShortcutId, ShortcutSpec>,
): void {
  if (typeof process !== "undefined" && process.env?.NODE_ENV === "production") {
    return;
  }
  // Mutual-exclusion is a first-class pattern: scoped shortcuts
  // share a combo with a global one, but each carries an `enabled`
  // guard so only one is active at a time (e.g. `Escape` on
  // help.close vs design.exitFullscreen). Both specs declaring an
  // `enabled` predicate makes the overlap intentional — silence the
  // warning so the dev console stays signal-only. A real collision
  // — at least one spec runs unconditionally — still logs.
  const incomingHasGuard = typeof incoming.enabled === "function";
  const incomingCombos = new Set(incoming.keys.map(normalizeCombo));
  for (const [otherId, other] of existing) {
    if (otherId === incoming.id) continue;
    const otherHasGuard = typeof other.enabled === "function";
    if (incomingHasGuard && otherHasGuard) continue;
    for (const combo of other.keys.map(normalizeCombo)) {
      if (incomingCombos.has(combo)) {
        console.warn(
          `[shortcuts] combo "${combo}" registered by both "${otherId}" and "${incoming.id}". The higher-priority spec fires; the other is silently skipped. Reassign one of them.`,
        );
      }
    }
  }
}

const useShortcutsStore = create<ShortcutsState>((set, get) => ({
  shortcuts: new Map(),
  register: (spec) => {
    warnOnCollision(spec, get().shortcuts);
    set((state) => {
      const next = new Map(state.shortcuts);
      next.set(spec.id, spec);
      return { shortcuts: next };
    });
  },
  unregister: (id) =>
    set((state) => {
      if (!state.shortcuts.has(id)) return state;
      const next = new Map(state.shortcuts);
      next.delete(id);
      return { shortcuts: next };
    }),
}));

/** Sort by priority (descending), stable across registrations. */
function sorted(map: Map<ShortcutId, ShortcutSpec>): ShortcutSpec[] {
  return Array.from(map.values()).sort(
    (a, b) => (b.priority ?? 0) - (a.priority ?? 0),
  );
}

/**
 * Imperative access for the global dispatcher. The dispatcher reads
 * the live map on every keydown so registrations / unregistrations
 * take effect on the next event without prop wiring.
 */
export function getRegisteredShortcuts(): ShortcutSpec[] {
  return sorted(useShortcutsStore.getState().shortcuts);
}

/**
 * Subscribe to shortcut registry changes. The help dialog reads
 * through this — the snapshot updates whenever any consumer (un)mounts.
 */
export function useShortcuts(): ShortcutSpec[] {
  const map = useShortcutsStore((s) => s.shortcuts);
  return sorted(map);
}

/**
 * Returns true if any registered combo matches the event. The
 * dispatcher uses this; tests use it to assert spec → keystroke
 * mapping without going through `window.dispatchEvent`.
 */
export function specMatchesEvent(
  spec: ShortcutSpec,
  e: KeyboardEvent,
): boolean {
  if (spec.enabled && !spec.enabled()) return false;
  if (!spec.fireInTypingTarget && isTypingTarget(e.target)) return false;
  return spec.keys.some((combo) => eventMatchesCombo(e, combo));
}

/**
 * Resolve the displayed glyph for a spec. Primitives prefer this over
 * directly reading `spec.glyph` so an unset glyph still renders the
 * canonical per-platform rendering of `keys[0]`.
 */
export function specGlyph(spec: ShortcutSpec): string {
  if (spec.glyph) return spec.glyph;
  if (spec.keys.length === 0) return "";
  return formatGlyph(spec.keys[0]);
}

/**
 * Register a keyboard shortcut for the lifetime of the calling
 * component. Inline `handler` / `enabled` closures are tracked through
 * a ref so the dispatcher always reads the latest values without
 * thrashing register/unregister on every render.
 *
 * Passing `undefined` registers nothing — useful for callers that
 * conditionally want a shortcut (the hook itself stays unconditional,
 * preserving rules-of-hooks order).
 *
 * Re-registering the same id replaces the prior entry (intentional —
 * keeps closure refresh cheap). Two different ids registering the same
 * combo log a dev-time warning; pick one.
 */
export function useShortcut(spec: ShortcutSpec | undefined): void {
  const specRef = useRef(spec);
  // Sync the latest spec into the ref after every render so the
  // registered entry (mounted once below) reads the current closure
  // values without rebuilding on every keystroke.
  useEffect(() => {
    specRef.current = spec;
  });

  const register = useShortcutsStore((s) => s.register);
  const unregister = useShortcutsStore((s) => s.unregister);

  // Activation is keyed on the spec's identity at first mount. A
  // passed-in `undefined` means "no shortcut today" and the effect
  // body short-circuits — but the hook still runs unconditionally
  // so rules-of-hooks ordering is preserved across re-renders.
  const initialId = spec?.id;
  // Capture whether the caller supplied an `enabled` predicate at
  // mount. The collision warning treats "both specs guarded" as a
  // mutual-exclusion intent and stays silent — wrapping every
  // registered spec in a closure unconditionally would defeat that
  // signal, since every entry would then read as guarded.
  const hasEnabledPredicate = typeof spec?.enabled === "function";
  useEffect(() => {
    if (!initialId) return;
    register({
      id: initialId,
      get keys() {
        return specRef.current?.keys ?? [];
      },
      get group() {
        return specRef.current?.group ?? "";
      },
      get description() {
        return specRef.current?.description ?? "";
      },
      get glyph() {
        return specRef.current?.glyph;
      },
      get priority() {
        return specRef.current?.priority;
      },
      get fireInTypingTarget() {
        return specRef.current?.fireInTypingTarget;
      },
      enabled: hasEnabledPredicate
        ? () => specRef.current?.enabled?.() ?? false
        : undefined,
      handler: (e) => specRef.current?.handler(e),
    });
    return () => unregister(initialId);
  }, [register, unregister, initialId, hasEnabledPredicate]);
}

export function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}
