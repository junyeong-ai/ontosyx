// `<KeyboardShortcut>` — single-source display element for key
// chords.
//
// Without a primitive, every surface that mentions a shortcut
// hand-rolls `<kbd className="rounded ... font-mono ...">` and the
// styling drifts: some chips get borders, some get filled
// backgrounds, some bake in `text-foreground-muted` while others
// inherit. The chord glyph itself also ends up encoded twice — once
// by the registry's `formatGlyph()` and once by hand at the
// surface (`⌘K` typed literally).
//
// `<KeyboardShortcut>` collapses the matrix to one component:
//
//   * Style: `surface` (default — filled chip on inset) or
//     `outline` (border-only, used in tooltips where the chip sits
//     atop a coloured background).
//   * Size: `compact` (default — 11px) or `default` (12px).
//   * Source: `glyph` (verbatim string the caller already has) or
//     `keys` (combo string passed to `formatGlyph`, useful when
//     the caller is not consuming a registered shortcut). The
//     latter pulls per-platform rendering for free — `mod+k`
//     renders ⌘K on macOS and Ctrl+K elsewhere.
//
// `<kbd>` is the right HTML element here — assistive tech announces
// "key" and the visual register matches.

import { cn } from "@/lib/cn";
import { formatGlyph, type KeyCombo } from "@/lib/shortcuts";

type ShortcutVariant = "surface" | "outline";
type ShortcutSize = "compact" | "default";

interface BaseProps {
  variant?: ShortcutVariant;
  size?: ShortcutSize;
  className?: string;
}

type KeyboardShortcutProps = BaseProps &
  ({ glyph: string; keys?: never } | { keys: KeyCombo; glyph?: never });

const VARIANT_CLASS: Record<ShortcutVariant, string> = {
  surface: "bg-surface-inset text-foreground-muted",
  outline: "border border-divider bg-surface-base text-foreground-muted",
};

const SIZE_CLASS: Record<ShortcutSize, string> = {
  compact: "px-1 py-0.5 text-2xs",
  default: "px-1.5 py-0.5 text-xs",
};

export function KeyboardShortcut({
  glyph,
  keys,
  variant = "surface",
  size = "compact",
  className,
}: KeyboardShortcutProps) {
  const display = glyph ?? formatGlyph(keys!);
  return (
    <kbd
      className={cn(
        "inline-flex items-center rounded font-mono leading-none",
        VARIANT_CLASS[variant],
        SIZE_CLASS[size],
        className,
      )}
    >
      {display}
    </kbd>
  );
}
