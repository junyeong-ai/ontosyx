// Key-combo parser, matcher, and pretty-printer.
//
// The single canonical format is `[modifier+]*key`, lowercase modifiers
// joined by `+`, then the target key. The key is case-insensitive for
// letters (`a` matches `A`, `Shift` is the explicit modifier) and exact
// for special keys (`Escape`, `ArrowDown`, `?`, `[`, `\`).
//
// `mod` resolves to `meta` on macOS and `ctrl` everywhere else. Every
// platform-aware shortcut uses `mod+...` so spec authors don't have to
// branch on `navigator.platform`. The dispatcher and the help-dialog
// glyph render the resolved modifier so users see what their OS expects.

export type Modifier = "ctrl" | "meta" | "alt" | "shift";

export interface ParsedCombo {
  modifiers: Set<Modifier>;
  /** The non-modifier key, lowercased for letter keys. */
  key: string;
}

const MODIFIER_NAMES = new Set(["mod", "ctrl", "meta", "alt", "shift"]);

interface NavigatorUAData {
  platform?: string;
}

function isMacPlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  // `userAgentData.platform` is the modern read; fall back to the
  // legacy `platform` string for older Safari / WebKit.
  const ua = (navigator as Navigator & { userAgentData?: NavigatorUAData })
    .userAgentData;
  if (typeof ua?.platform === "string") {
    return ua.platform.toLowerCase().includes("mac");
  }
  return navigator.platform.toLowerCase().includes("mac");
}

export function parseCombo(combo: string): ParsedCombo {
  const parts = combo.split("+");
  if (parts.length === 0 || parts.some((p) => p.length === 0)) {
    throw new Error(`shortcuts: invalid key combo "${combo}"`);
  }
  const key = parts[parts.length - 1];
  const modTokens = parts.slice(0, -1).map((m) => m.toLowerCase());
  for (const token of modTokens) {
    if (!MODIFIER_NAMES.has(token)) {
      throw new Error(
        `shortcuts: invalid combo "${combo}" — "${token}" is not a modifier`,
      );
    }
  }
  const modifiers = new Set<Modifier>();
  const isMac = isMacPlatform();
  for (const token of modTokens) {
    if (token === "mod") {
      modifiers.add(isMac ? "meta" : "ctrl");
    } else {
      modifiers.add(token as Modifier);
    }
  }
  return { modifiers, key: key.length === 1 ? key.toLowerCase() : key };
}

/** Canonical lowercase string for collision-detection set membership. */
export function normalizeCombo(combo: string): string {
  const { modifiers, key } = parseCombo(combo);
  const ordered = (["ctrl", "meta", "alt", "shift"] as const).filter((m) =>
    modifiers.has(m),
  );
  return [...ordered, key].join("+");
}

export function eventMatchesCombo(
  e: KeyboardEvent,
  combo: string,
): boolean {
  const { modifiers, key } = parseCombo(combo);
  if (e.ctrlKey !== modifiers.has("ctrl")) return false;
  if (e.metaKey !== modifiers.has("meta")) return false;
  if (e.altKey !== modifiers.has("alt")) return false;
  if (e.shiftKey !== modifiers.has("shift")) return false;
  // Letter keys match case-insensitively; special keys exactly.
  if (key.length === 1) return e.key.toLowerCase() === key;
  return e.key === key;
}

const GLYPH_BY_KEY: Record<string, string> = {
  Escape: "Esc",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Enter: "↵",
  Backspace: "⌫",
  Delete: "⌦",
  Tab: "⇥",
  " ": "Space",
  Space: "Space",
};

const MAC_MODIFIER_GLYPH: Record<Modifier, string> = {
  ctrl: "⌃",
  meta: "⌘",
  alt: "⌥",
  shift: "⇧",
};

const NON_MAC_MODIFIER_GLYPH: Record<Modifier, string> = {
  ctrl: "Ctrl+",
  meta: "Win+",
  alt: "Alt+",
  shift: "Shift+",
};

/** Render a combo into a per-platform glyph string for help dialogs and tooltips. */
export function formatGlyph(combo: string): string {
  const { modifiers, key } = parseCombo(combo);
  const isMac = isMacPlatform();
  const order: Modifier[] = isMac
    ? ["ctrl", "alt", "shift", "meta"] // matches macOS HIG modifier order
    : ["ctrl", "alt", "shift", "meta"];
  const mods = order
    .filter((m) => modifiers.has(m))
    .map((m) => (isMac ? MAC_MODIFIER_GLYPH[m] : NON_MAC_MODIFIER_GLYPH[m]))
    .join("");
  const visibleKey =
    GLYPH_BY_KEY[key] ?? (key.length === 1 ? key.toUpperCase() : key);
  return `${mods}${visibleKey}`;
}
