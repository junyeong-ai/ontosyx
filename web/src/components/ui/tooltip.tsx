"use client";

import { Tooltip as BaseTooltip } from "@base-ui/react/tooltip";

// ---------------------------------------------------------------------------
// Tooltip — Base UI Tooltip wrapper replacing title attributes
// ---------------------------------------------------------------------------

interface TooltipProps {
  content: React.ReactNode;
  children: React.ReactElement;
  side?: "top" | "bottom" | "left" | "right";
  sideOffset?: number;
}

export function Tooltip({
  content,
  children,
  side = "top",
  sideOffset = 6,
}: TooltipProps) {
  return (
    <BaseTooltip.Root>
      <BaseTooltip.Trigger render={children} />
      <BaseTooltip.Portal>
        <BaseTooltip.Positioner side={side} sideOffset={sideOffset}>
          <BaseTooltip.Popup className="popup-pop rounded-md bg-foreground px-2.5 py-1.5 text-xs text-foreground-onbrand shadow-3">
            {content}
          </BaseTooltip.Popup>
        </BaseTooltip.Positioner>
      </BaseTooltip.Portal>
    </BaseTooltip.Root>
  );
}

export function TooltipProvider({ children }: { children: React.ReactNode }) {
  return <BaseTooltip.Provider>{children}</BaseTooltip.Provider>;
}
