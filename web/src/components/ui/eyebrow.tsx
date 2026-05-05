// Eyebrow — small section label that lives below the heading scale.
//
// `Heading` covers the six-tier visual hierarchy (display → 6, where
// 6 is `text-sm`). Below that sits a different shape: tightly-spaced
// section labels in `text-2xs`–`text-xs`, often uppercase + tracking-
// wider, frequently tinted to match a status colour. They were the
// dominant raw `<h2 className="text-2xs uppercase tracking-wider …">`
// pattern across the workbench, chat, recipes, and design surfaces.
//
// `Eyebrow` separates them out so the visual hierarchy stays a clean
// ladder (Heading 1–6) and the small-label vocabulary lives on a
// sibling primitive that can carry tone + caps + density without
// overloading `Heading`'s `size` enum.
//
// `level` still picks the rendered tag — outline / a11y matters as
// much for these labels as for any heading.

import type { HTMLAttributes, ReactNode, Ref } from "react";
import { cn } from "@/lib/cn";

export type EyebrowLevel = 2 | 3 | 4 | 5 | 6;
export type EyebrowTone =
  | "muted"
  | "strong"
  | "brand"
  | "warning"
  | "success"
  | "info"
  | "danger";
/** `default` = `text-2xs` (10px). `dense` = `text-xs` (12px). */
export type EyebrowSize = "default" | "dense";
/** `upper` = uppercase + wider tracking. `none` = sentence case. */
export type EyebrowCaps = "upper" | "none";

const TONE_CLASS: Record<EyebrowTone, string> = {
  muted: "text-foreground-muted",
  strong: "text-foreground-strong",
  brand: "text-brand-foreground-strong",
  warning: "text-warning-foreground",
  success: "text-success-foreground",
  info: "text-info-foreground",
  danger: "text-danger-foreground",
};

interface EyebrowProps extends HTMLAttributes<HTMLHeadingElement> {
  level: EyebrowLevel;
  tone?: EyebrowTone;
  size?: EyebrowSize;
  caps?: EyebrowCaps;
  children: ReactNode;
  ref?: Ref<HTMLHeadingElement>;
}

export function Eyebrow({
  level,
  tone = "muted",
  size = "default",
  caps = "upper",
  className,
  children,
  ref,
  ...rest
}: EyebrowProps) {
  const klass = cn(
    "font-semibold",
    size === "dense" ? "text-xs" : "text-2xs",
    caps === "upper" ? "uppercase tracking-wider" : null,
    TONE_CLASS[tone],
    className,
  );
  switch (level) {
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
