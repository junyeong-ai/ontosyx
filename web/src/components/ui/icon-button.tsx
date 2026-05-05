"use client";

import type { ReactNode } from "react";
import type { LucideIcon as IconSvgElement } from "lucide-react";
import { Tooltip } from "./tooltip";
import { Button } from "./button";
import { cn } from "@/lib/cn";
import { DynamicIcon } from "@/components/ui/dynamic-icon";

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

const toneHoverClass: Record<IconButtonTone, string> = {
  neutral: "hover:bg-surface-inset hover:text-foreground",
  brand:   "hover:bg-brand-surface hover:text-brand-foreground",
  danger:  "hover:bg-danger-surface hover:text-danger-foreground",
};

// Active state inherits the tone — a "brand active" toggle reads
// brand-tinted, a "danger active" pin reads danger-tinted. The previous
// brand-only active assumed every active toggle was a brand action.
const toneActiveClass: Record<IconButtonTone, string> = {
  neutral: "bg-surface-inset text-foreground",
  brand:   "bg-brand-surface text-brand-foreground",
  danger:  "bg-danger-surface text-danger-foreground",
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
  const stateClass = active ? toneActiveClass[tone] : "text-foreground-muted";
  return (
    <Tooltip content={label}>
      <Button
        variant="ghost"
        size={size === "md" ? "icon" : "icon-sm"}
        aria-label={label}
        onClick={onClick}
        disabled={disabled}
        className={cn(toneHoverClass[tone], stateClass, className)}
      >
        {icon ? (
          <DynamicIcon as={icon} className={iconSizeClass[size]} />
        ) : (
          children
        )}
      </Button>
    </Tooltip>
  );
}
