"use client";

import { useState, useCallback, createContext, useContext, useRef } from "react";
import { AlertDialog } from "@base-ui/react/alert-dialog";
import { cn } from "@/lib/cn";

interface ConfirmOptions {
  title: string;
  description: string;
  confirmLabel?: string;
  cancelLabel?: string;
  variant?: "danger" | "warning" | "default";
}

type ConfirmFn = (options: ConfirmOptions) => Promise<boolean>;

const ConfirmContext = createContext<ConfirmFn | null>(null);

export function useConfirm(): ConfirmFn {
  const fn = useContext(ConfirmContext);
  if (!fn) throw new Error("useConfirm must be used within <ConfirmProvider>");
  return fn;
}

export function ConfirmProvider({ children }: { children: React.ReactNode }) {
  const [open, setOpen] = useState(false);
  const [options, setOptions] = useState<ConfirmOptions>({
    title: "",
    description: "",
  });
  const resolveRef = useRef<((value: boolean) => void) | null>(null);

  const confirm = useCallback((opts: ConfirmOptions) => {
    resolveRef.current?.(false);
    resolveRef.current = null;
    setOptions(opts);
    setOpen(true);
    return new Promise<boolean>((resolve) => {
      resolveRef.current = resolve;
    });
  }, []);

  const handleConfirm = () => {
    setOpen(false);
    resolveRef.current?.(true);
    resolveRef.current = null;
  };

  const handleCancel = () => {
    setOpen(false);
    resolveRef.current?.(false);
    resolveRef.current = null;
  };

  const isDanger = options.variant === "danger";

  return (
    <ConfirmContext value={confirm}>
      {children}
      <AlertDialog.Root open={open} onOpenChange={(next) => !next && handleCancel()}>
        <AlertDialog.Portal>
          <AlertDialog.Backdrop
            className={cn(
              "fixed inset-0 z-50 bg-[var(--surface-overlay)] backdrop-blur-sm",
              "transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)]",
              "data-[starting-style]:opacity-0 data-[ending-style]:opacity-0",
            )}
          />
          <AlertDialog.Popup
            className={cn(
              "fixed left-1/2 top-1 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2",
              "rounded-xl border border-divider bg-surface-base p-6 shadow-3",
              "transition-[opacity,transform] duration-[var(--duration-base)] ease-[var(--ease-out)]",
              "data-[starting-style]:scale-95 data-[starting-style]:opacity-0",
              "data-[ending-style]:scale-95 data-[ending-style]:opacity-0",
            )}
          >
            <AlertDialog.Title className="text-base font-semibold text-foreground-strong">
              {options.title}
            </AlertDialog.Title>
            <AlertDialog.Description className="mt-2 text-sm leading-relaxed text-foreground-muted">
              {options.description}
            </AlertDialog.Description>
            <div className="mt-6 flex justify-end gap-2">
              <AlertDialog.Close
                className="rounded-md px-4 py-2 text-sm font-medium text-foreground-muted transition-colors hover:bg-surface-inset"
                onClick={handleCancel}
              >
                {options.cancelLabel ?? "Cancel"}
              </AlertDialog.Close>
              <button
                onClick={handleConfirm}
                className={cn(
                  "rounded-md px-4 py-2 text-sm font-medium text-foreground-onbrand transition-colors",
                  isDanger
                    ? "bg-danger-solid hover:bg-danger-solid-hover"
                    : "bg-brand-solid hover:bg-brand-solid-hover",
                )}
              >
                {options.confirmLabel ?? "Confirm"}
              </button>
            </div>
          </AlertDialog.Popup>
        </AlertDialog.Portal>
      </AlertDialog.Root>
    </ConfirmContext>
  );
}
