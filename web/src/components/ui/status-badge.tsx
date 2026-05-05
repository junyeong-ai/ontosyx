import type { HTMLAttributes } from "react";
import { cn } from "@/lib/cn";

export type StatusTone =
  | "neutral"
  | "brand"
  | "success"
  | "warning"
  | "danger"
  | "info"
  | "concept";

type StatusVariant = "soft" | "outline" | "solid";
type StatusSize = "sm" | "md";

interface StatusBadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: StatusTone;
  variant?: StatusVariant;
  size?: StatusSize;
}

const softClass: Record<StatusTone, string> = {
  neutral: "bg-surface-inset text-foreground-muted",
  brand:   "bg-brand-surface text-brand-foreground",
  success: "bg-success-surface text-success-foreground",
  warning: "bg-warning-surface text-warning-foreground",
  danger:  "bg-danger-surface text-danger-foreground",
  info:    "bg-info-surface text-info-foreground",
  concept: "bg-concept-surface text-concept-foreground",
};

const outlineClass: Record<StatusTone, string> = {
  neutral: "border border-divider text-foreground-muted",
  brand:   "border border-brand-border text-brand-foreground",
  success: "border border-success-border text-success-foreground",
  warning: "border border-warning-border text-warning-foreground",
  danger:  "border border-danger-border text-danger-foreground",
  info:    "border border-info-border text-info-foreground",
  concept: "border border-concept-border text-concept-foreground",
};

const solidClass: Record<StatusTone, string> = {
  neutral: "bg-foreground text-foreground-onbrand",
  brand:   "bg-brand-solid text-foreground-onbrand",
  success: "bg-brand-solid text-foreground-onbrand",
  warning: "bg-warning-foreground text-surface-base",
  danger:  "bg-danger-solid text-foreground-on-accent",
  info:    "bg-info-foreground text-surface-base",
  concept: "bg-concept-foreground text-surface-base",
};

const sizeClass: Record<StatusSize, string> = {
  sm: "px-1.5 py-0.5 text-2xs",
  md: "px-2 py-0.5 text-xs",
};

export function StatusBadge({
  tone = "neutral",
  variant = "soft",
  size = "sm",
  className,
  ...rest
}: StatusBadgeProps) {
  const toneClass =
    variant === "outline"
      ? outlineClass[tone]
      : variant === "solid"
        ? solidClass[tone]
        : softClass[tone];

  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full font-medium",
        sizeClass[size],
        toneClass,
        className,
      )}
      {...rest}
    />
  );
}
