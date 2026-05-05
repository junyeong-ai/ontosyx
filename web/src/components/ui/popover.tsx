"use client";

import { Popover as BasePopover } from "@base-ui/react/popover";

// ---------------------------------------------------------------------------
// Popover — Base UI Popover wrapper replacing manual dropdown state
// ---------------------------------------------------------------------------

interface PopoverProps {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  children: React.ReactNode;
}

export function Popover({ open, onOpenChange, children }: PopoverProps) {
  return (
    <BasePopover.Root open={open} onOpenChange={onOpenChange}>
      {children}
    </BasePopover.Root>
  );
}

export function PopoverTrigger({
  children,
  className,
  "aria-label": ariaLabel,
}: {
  children: React.ReactNode;
  className?: string;
  "aria-label"?: string;
}) {
  return (
    <BasePopover.Trigger className={className} aria-label={ariaLabel}>
      {children}
    </BasePopover.Trigger>
  );
}

export function PopoverContent({
  children,
  className,
  side = "bottom",
  align = "start",
  sideOffset = 4,
}: {
  children: React.ReactNode;
  className?: string;
  side?: "top" | "bottom" | "left" | "right";
  align?: "start" | "center" | "end";
  sideOffset?: number;
}) {
  return (
    <BasePopover.Portal>
      <BasePopover.Positioner side={side} align={align} sideOffset={sideOffset}>
        <BasePopover.Popup
          className={
            className ??
            "z-popover rounded-lg border border-divider bg-surface-base shadow-3 outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40 transition-all duration-[var(--duration-base)] ease-[var(--ease-out)] data-[starting-style]:scale-95 data-[starting-style]:opacity-0 data-[ending-style]:scale-95 data-[ending-style]:opacity-0"
          }
        >
          {children}
        </BasePopover.Popup>
      </BasePopover.Positioner>
    </BasePopover.Portal>
  );
}
