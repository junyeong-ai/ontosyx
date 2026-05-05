"use client";

import type { ComponentProps } from "react";
import type { LucideIcon } from "lucide-react";

/**
 * Render a Lucide icon component held in a runtime variable.
 *
 * `<Foo />` JSX requires a capital identifier; a state-derived
 * variable (`leadingIcon`, `mode.icon`, `isOpen ? ArrowUp :
 * ArrowDown`) doesn't satisfy that. Instead of forcing every
 * call site to invent a local capital alias, this component
 * accepts the icon as `as` and forwards every other prop
 * verbatim.
 *
 * Type-wise it's the same shape Lucide ships — `LucideIcon`
 * accepts `className`, `size`, `strokeWidth`, every standard
 * SVG prop — so consumers get full prop autocomplete.
 */
export function DynamicIcon({
  as: As,
  ...props
}: { as: LucideIcon } & ComponentProps<LucideIcon>) {
  return <As {...props} />;
}
