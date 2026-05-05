// Heading — decouples document outline from visual size.
//
// `level` controls the rendered tag (`h1`…`h6`) and therefore the
// accessibility / outline ordering. `size` controls appearance and is
// optional — when omitted it tracks `level`, which keeps the simple
// case ergonomic. Decoupling matters for nested chrome: a card inside
// a section that already has an `h2` should render `h3` to keep the
// outline correct, but the card title may visually want to look like a
// fourth-tier label. Combining them in one prop is what causes WCAG
// 2.4.6 violations across most design systems.
//
// `display` is a separate visual tier reserved for hero / landing
// surfaces; it never tracks an h-level by default. Pass `size="display"`
// explicitly when you want it.

import type { HTMLAttributes, ReactNode, Ref } from "react";
import { cn } from "@/lib/cn";

export type HeadingLevel = 1 | 2 | 3 | 4 | 5 | 6;
export type HeadingSize = "display" | 1 | 2 | 3 | 4 | 5 | 6;

interface HeadingProps extends HTMLAttributes<HTMLHeadingElement> {
  /** Outline / a11y level — picks the rendered `h1`…`h6` tag. */
  level: HeadingLevel;
  /** Visual tier. Defaults to `level` (h6 → 6, the smallest tier). */
  size?: HeadingSize;
  children: ReactNode;
  ref?: Ref<HTMLHeadingElement>;
}

const SIZE_CLASS: Record<HeadingSize, string> = {
  display: "heading-display",
  1: "heading-1",
  2: "heading-2",
  3: "heading-3",
  4: "heading-4",
  5: "heading-5",
  6: "heading-6",
};

export function Heading({
  level,
  size,
  className,
  children,
  ref,
  ...rest
}: HeadingProps) {
  const visualSize: HeadingSize = size ?? (level as HeadingSize);
  const klass = cn(
    "text-foreground-strong",
    SIZE_CLASS[visualSize],
    className,
  );
  switch (level) {
    case 1:
      return (
        <h1 ref={ref} className={klass} {...rest}>
          {children}
        </h1>
      );
    case 2:
      return (
        <h2 ref={ref} className={klass} {...rest}>
          {children}
        </h2>
      );
    case 3:
      return (
        <h3 ref={ref} className={klass} {...rest}>
          {children}
        </h3>
      );
    case 4:
      return (
        <h4 ref={ref} className={klass} {...rest}>
          {children}
        </h4>
      );
    case 5:
      return (
        <h5 ref={ref} className={klass} {...rest}>
          {children}
        </h5>
      );
    case 6:
      return (
        <h6 ref={ref} className={klass} {...rest}>
          {children}
        </h6>
      );
  }
}
