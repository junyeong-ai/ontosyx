"use client";

import { Dialog } from "@base-ui/react/dialog";
import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

type ModalSize = "sm" | "md" | "lg" | "xl";

interface ModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title?: string;
  description?: string;
  size?: ModalSize;
  children: ReactNode;
  footer?: ReactNode;
}

const sizeClass: Record<ModalSize, string> = {
  sm: "max-w-sm",
  md: "max-w-md",
  lg: "max-w-2xl",
  xl: "max-w-4xl",
};

export function Modal({
  open,
  onOpenChange,
  title,
  description,
  size = "md",
  children,
  footer,
}: ModalProps) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Backdrop
          className={cn(
            "fixed inset-0 z-overlay bg-[var(--color-surface-overlay)] backdrop-blur-sm",
            "transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)]",
            "data-[ending-style]:opacity-0 data-[starting-style]:opacity-0",
          )}
        />
        <Dialog.Popup
          className={cn(
            "fixed left-1/2 top-1/2 z-modal w-full -translate-x-1/2 -translate-y-1/2",
            "overflow-y-auto rounded-xl border border-divider bg-surface-base shadow-3",
            "transition-[opacity,transform] duration-[var(--duration-base)] ease-[var(--ease-out)]",
            "data-[starting-style]:scale-[0.96] data-[starting-style]:opacity-0",
            "data-[ending-style]:scale-[0.96] data-[ending-style]:opacity-0",
            sizeClass[size],
          )}
          style={{ maxHeight: "calc(100vh - 4rem)" }}
        >
          {(title || description) && (
            <header className="border-b border-divider px-5 py-4">
              {title && (
                <Dialog.Title className="text-base font-semibold text-foreground-strong">
                  {title}
                </Dialog.Title>
              )}
              {description && (
                <Dialog.Description className="mt-1 text-xs text-foreground-muted">
                  {description}
                </Dialog.Description>
              )}
            </header>
          )}
          <div className="px-5 py-4">{children}</div>
          {footer && (
            <footer className="flex items-center justify-end gap-2 border-t border-divider px-5 py-3">
              {footer}
            </footer>
          )}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
