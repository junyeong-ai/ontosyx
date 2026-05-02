import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";
import { cn } from "@/lib/cn";
import { Button } from "./button";

type EmptyStateVariant = "hero" | "compact";

interface EmptyStateProps {
  icon?: IconSvgElement;
  title: string;
  description?: string;
  hint?: string;
  action?: { label: string; onClick: () => void };
  secondaryAction?: { label: string; onClick: () => void };
  /**
   * `hero` (default) is page-level: stretches to fill its container,
   * generous padding, large icon. `compact` is inline-friendly: sizes
   * to content with tighter padding and a smaller icon. Pick
   * `compact` inside a sidebar, popover, or inspector panel where a
   * full-height card would dominate.
   */
  variant?: EmptyStateVariant;
  className?: string;
}

const containerClass: Record<EmptyStateVariant, string> = {
  hero:    "h-full gap-3 p-8",
  compact: "gap-2 p-4",
};

const iconWrapClass: Record<EmptyStateVariant, string> = {
  hero:    "h-12 w-12",
  compact: "h-9 w-9",
};

const iconClass: Record<EmptyStateVariant, string> = {
  hero:    "h-5 w-5",
  compact: "h-4 w-4",
};

const titleClass: Record<EmptyStateVariant, string> = {
  hero:    "text-sm",
  compact: "text-xs",
};

export function EmptyState({
  icon,
  title,
  description,
  hint,
  action,
  secondaryAction,
  variant = "hero",
  className,
}: EmptyStateProps) {
  return (
    <div
      role="status"
      className={cn(
        "flex flex-col items-center justify-center text-center",
        containerClass[variant],
        className,
      )}
    >
      {icon && (
        <div
          className={cn(
            "flex items-center justify-center rounded-full bg-brand-surface",
            iconWrapClass[variant],
          )}
        >
          <HugeiconsIcon
            icon={icon}
            className={cn("text-brand-foreground", iconClass[variant])}
            size="100%"
          />
        </div>
      )}
      <div className="max-w-sm">
        <p className={cn("font-medium text-foreground", titleClass[variant])}>
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
