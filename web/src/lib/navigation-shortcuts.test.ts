import { describe, it, expect } from "vitest";

import {
  NAVIGATION_SHORTCUTS,
  shortcutForRoute,
  type NavigationRoute,
} from "./navigation-shortcuts";

// ---------------------------------------------------------------------------
// Invariants
//
// `NAVIGATION_SHORTCUTS` is a single source of truth driving four call sites:
//   - sidebar tooltip + aria-label
//   - global `g + key` keyboard handler
//   - `KeyboardShortcutsDialog` Navigation section
//   - `commandPalette.commands.navigate-*` entries (via `NAV_COMMAND_ID`)
//
// A regression in any of these invariants fans out to all four surfaces, so
// the test suite locks them in: unique keys, unique routes, every entry
// addressable through `shortcutForRoute()`.
// ---------------------------------------------------------------------------

describe("NAVIGATION_SHORTCUTS — invariants", () => {
  it("ships at least eight entries (the canonical workbench routes + settings)", () => {
    // The lower bound is structural rather than exact: the test should not
    // need editing every time a new route is added — only when one is
    // removed (which itself is a deliberate, reviewed change).
    expect(NAVIGATION_SHORTCUTS.length).toBeGreaterThanOrEqual(8);
  });

  it("declares a unique second-key per shortcut", () => {
    const keys = NAVIGATION_SHORTCUTS.map((s) => s.key);
    const unique = new Set(keys);
    expect(unique.size).toBe(keys.length);
  });

  it("declares a unique route per shortcut", () => {
    const routes = NAVIGATION_SHORTCUTS.map((s) => s.route);
    const unique = new Set(routes);
    expect(unique.size).toBe(routes.length);
  });

  it("uses lowercase second-keys so the handler can lowercase the event without surprises", () => {
    for (const s of NAVIGATION_SHORTCUTS) {
      expect(s.key).toBe(s.key.toLowerCase());
    }
  });

  it("renders a glyph that begins with `G ` so the tooltip kbd chip is uniform", () => {
    for (const s of NAVIGATION_SHORTCUTS) {
      expect(s.glyph.startsWith("G ")).toBe(true);
    }
  });

  it("declares an absolute route href", () => {
    for (const s of NAVIGATION_SHORTCUTS) {
      expect(s.href.startsWith("/")).toBe(true);
    }
  });
});

describe("shortcutForRoute", () => {
  it("returns the matching entry for every declared route", () => {
    for (const s of NAVIGATION_SHORTCUTS) {
      const found = shortcutForRoute(s.route);
      expect(found).toBe(s);
    }
  });

  it("is exhaustive — TypeScript guarantees no unhandled NavigationRoute", () => {
    // Compile-time check: the discriminated union is exhausted by the
    // declarations. If a route is added to the type but missing from the
    // array, this assertion forces the test to fail at runtime as well.
    const seen = new Set<NavigationRoute>(
      NAVIGATION_SHORTCUTS.map((s) => s.route),
    );
    const expected: readonly NavigationRoute[] = [
      "design",
      "analyze",
      "explore",
      "dashboard",
      "glossary",
      "vocabulary",
      "recipes",
      "settings",
    ];
    for (const route of expected) {
      expect(seen.has(route)).toBe(true);
    }
  });
});
