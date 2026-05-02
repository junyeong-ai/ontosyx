import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";
import { cn } from "@/lib/cn";
import { Button } from "./button";

type EmptyStateSize = "sm" | "md";

interface EmptyStateProps {
  icon?: IconSvgElement;
  title: string;
  description?: string;
  hint?: string;
  action?: { label: string; onClick: () => void };
  secondaryAction?: { label: string; onClick: () => void };
  /**
   * `md` (default) stretches to fill its container — for page-level
   * empty states (settings page, dashboard). `sm` is inline-friendly:
   * no height stretch, tighter padding, smaller icon. Pick `sm` when
   * the empty state lives inside a sidebar, popover, or inspector
   * panel where a tall card would dominate.
   */
  size?: EmptyStateSize;
  className?: string;
}

const containerSize: Record<EmptyStateSize, string> = {
  sm: "gap-2 p-4",
  md: "h-full gap-3 p-8",
};

const iconWrapSize: Record<EmptyStateSize, string> = {
  sm: "h-9 w-9",
  md: "h-12 w-12",
};

const iconSize: Record<EmptyStateSize, string> = {
  sm: "h-4 w-4",
  md: "h-5 w-5",
};

const titleSize: Record<EmptyStateSize, string> = {
  sm: "text-xs",
  md: "text-sm",
};

export function EmptyState({
  icon,
  title,
  description,
  hint,
  action,
  secondaryAction,
  size = "md",
  className,
}: EmptyStateProps) {
  return (
    <div
      role="status"
      className={cn(
        "flex flex-col items-center justify-center text-center",
        containerSize[size],
        className,
      )}
    >
      {icon && (
        <div
          className={cn(
            "flex items-center justify-center rounded-full bg-brand-surface",
            iconWrapSize[size],
          )}
        >
          <HugeiconsIcon
            icon={icon}
            className={cn("text-brand-foreground", iconSize[size])}
            size="100%"
          />
        </div>
      )}
      <div className="max-w-sm">
        <p className={cn("font-medium text-foreground", titleSize[size])}>
          {title}
        </p>
        {description && (
          <p className="mt-1 text-xs text-foreground-muted">{description}</p>
        )}
      </div>
      {(action || secondaryAction) && (
        <div className="flex items-center gap-2">
          {action && (
            <Button variant="primary" size="sm" onClick={action.onClick}>
              {action.label}
            </Button>
          )}
          {secondaryAction && (
            <Button variant="ghost" size="sm" onClick={secondaryAction.onClick}>
              {secondaryAction.label}
            </Button>
          )}
        </div>
      )}
      {hint && <p className="text-2xs text-foreground-muted">{hint}</p>}
    </div>
  );
}
