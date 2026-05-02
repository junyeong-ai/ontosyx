"use client";

import { type ReactNode } from "react";
import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";
import { Tooltip } from "./tooltip";
import { Button } from "./button";
import { cn } from "@/lib/cn";

type IconButtonTone = "neutral" | "brand" | "danger";
type IconButtonSize = "sm" | "md";

interface IconButtonProps {
  /**
   * Tooltip text + a11y label. Required — icon-only buttons without
   * a textual label are inaccessible to screen readers.
   */
  label: string;
  onClick: () => void;
  /** Either pass a Hugeicons icon, or arbitrary children for inline SVGs. */
  icon?: IconSvgElement;
  children?: ReactNode;
  /**
   * `neutral` (default) — muted on rest, surface-inset on hover.
   * `brand` — muted on rest, brand-surface on hover (primary actions).
   * `danger` — muted on rest, danger-surface on hover (destructive).
   */
  tone?: IconButtonTone;
  /** When `true`, renders the brand-active state regardless of hover. */
  active?: boolean;
  size?: IconButtonSize;
  disabled?: boolean;
  className?: string;
}

const toneClass: Record<IconButtonTone, string> = {
  neutral: "hover:bg-surface-inset hover:text-foreground",
  brand:   "hover:bg-brand-surface hover:text-brand-foreground",
  danger:  "hover:bg-danger-surface hover:text-danger-foreground",
};

const iconSizeClass: Record<IconButtonSize, string> = {
  sm: "h-3.5 w-3.5",
  md: "h-4 w-4",
};

export function IconButton({
  label,
  onClick,
  icon,
  children,
  tone = "neutral",
  active,
  size = "sm",
  disabled,
  className,
}: IconButtonProps) {
  const stateClass = active
    ? "text-brand-foreground bg-brand-surface"
    : "text-foreground-muted";
  return (
    <Tooltip content={label}>
      <Button
        variant="ghost"
        size={size === "md" ? "icon" : "icon-sm"}
        aria-label={label}
        onClick={onClick}
        disabled={disabled}
        className={cn(toneClass[tone], stateClass, className)}
      >
        {icon ? (
          <HugeiconsIcon icon={icon} className={iconSizeClass[size]} size="100%" />
        ) : (
          children
        )}
      </Button>
    </Tooltip>
  );
}
