// Single source of truth for global navigation shortcuts.
//
// The array is `as const`, so the literal types of `route` flow into
// `NavigationRoute` without a hand-written union. Adding a new
// `{ ..., route: "newPlace" }` entry expands `NavigationRoute`
// automatically; every consumer (sidebar tooltip, `g`-prefix handler,
// KeyboardShortcutsDialog navigation section, command-palette
// `navigate-*` derive, `NAV_COMMAND_ID` mapping) recomputes against
// the wider union and the TypeScript compiler flags every site that
// must absorb the new variant.
//
// `g`-prefix sequences are the Linear / GitHub idiom: more memorable
// than `Cmd+1..N` and never conflict with browser tab shortcuts.

export const NAVIGATION_SHORTCUTS = [
  { key: "d", href: "/design", route: "design", glyph: "G D" },
  { key: "a", href: "/analyze", route: "analyze", glyph: "G A" },
  { key: "e", href: "/explore", route: "explore", glyph: "G E" },
  { key: "b", href: "/dashboard", route: "dashboard", glyph: "G B" },
  { key: "g", href: "/glossary", route: "glossary", glyph: "G G" },
  { key: "v", href: "/vocabulary", route: "vocabulary", glyph: "G V" },
  { key: "r", href: "/recipes", route: "recipes", glyph: "G R" },
  { key: ",", href: "/settings", route: "settings", glyph: "G ," },
] as const satisfies readonly {
  /** Second key pressed after `g` (lowercase). */
  key: string;
  /** Route to navigate to. */
  href: string;
  /** Sidebar route id used for active-state matching. */
  route: string;
  /** Display glyph rendered in tooltips and the shortcut dialog. */
  glyph: string;
}[];

/** All declared routes — derived from `NAVIGATION_SHORTCUTS` so a new
 *  entry expands the union and a removed entry contracts it. */
export type NavigationRoute = (typeof NAVIGATION_SHORTCUTS)[number]["route"];

/** Concrete shortcut record — derived from the array so consumers can
 *  refer to the shape without re-stating it. */
export type NavigationShortcut = (typeof NAVIGATION_SHORTCUTS)[number];

// Pre-computed route → shortcut lookup. The `reduce` walks every
// entry, so once `NAVIGATION_SHORTCUTS` includes every route in the
// derived union, `BY_ROUTE` is exhaustive by construction. The cast
// at the seed object is the only `as` in this module — it lets the
// compiler treat the in-progress accumulator as a complete record
// during the build phase.
const BY_ROUTE = NAVIGATION_SHORTCUTS.reduce<
  Record<NavigationRoute, NavigationShortcut>
>(
  (acc, shortcut) => {
    acc[shortcut.route] = shortcut;
    return acc;
  },
  {} as Record<NavigationRoute, NavigationShortcut>,
);

export function shortcutForRoute(route: NavigationRoute): NavigationShortcut {
  return BY_ROUTE[route];
}
