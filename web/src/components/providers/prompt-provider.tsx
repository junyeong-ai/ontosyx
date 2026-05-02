"use client";

import { useState, useCallback, useEffect, useRef, createContext, useContext } from "react";
import { Dialog } from "@base-ui/react/dialog";
import { cn } from "@/lib/cn";

interface PromptOptions {
  title: string;
  description?: string;
  defaultValue?: string;
  placeholder?: string;
  confirmLabel?: string;
  cancelLabel?: string;
}

type PromptFn = (options: PromptOptions) => Promise<string | null>;

const PromptContext = createContext<PromptFn | null>(null);

export function usePrompt(): PromptFn {
  const fn = useContext(PromptContext);
  if (!fn) throw new Error("usePrompt must be used within <PromptProvider>");
  return fn;
}

export function PromptProvider({ children }: { children: React.ReactNode }) {
  const [open, setOpen] = useState(false);
  const [options, setOptions] = useState<PromptOptions>({ title: "" });
  const [value, setValue] = useState("");
  const resolveRef = useRef<((value: string | null) => void) | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const prompt = useCallback((opts: PromptOptions) => {
    resolveRef.current?.(null);
    resolveRef.current = null;
    setOptions(opts);
    setValue(opts.defaultValue ?? "");
    setOpen(true);
    return new Promise<string | null>((resolve) => {
      resolveRef.current = resolve;
    });
  }, []);

  useEffect(() => {
    if (open) {
      requestAnimationFrame(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      });
    }
  }, [open]);

  const handleConfirm = () => {
    setOpen(false);
    resolveRef.current?.(value);
    resolveRef.current = null;
  };

  const handleCancel = () => {
    setOpen(false);
    resolveRef.current?.(null);
    resolveRef.current = null;
  };

  return (
    <PromptContext value={prompt}>
      {children}
      <Dialog.Root open={open} onOpenChange={(next) => !next && handleCancel()}>
        <Dialog.Portal>
          <Dialog.Backdrop
            className={cn(
              "fixed inset-0 z-50 bg-[var(--surface-overlay)] backdrop-blur-sm",
              "transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)]",
              "data-[starting-style]:opacity-0 data-[ending-style]:opacity-0",
            )}
          />
          <Dialog.Popup
            className={cn(
              "fixed left-1/2 top-1 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2",
              "rounded-xl border border-divider bg-surface-base p-6 shadow-3",
              "transition-[opacity,transform] duration-[var(--duration-base)] ease-[var(--ease-out)]",
              "data-[starting-style]:scale-95 data-[starting-style]:opacity-0",
              "data-[ending-style]:scale-95 data-[ending-style]:opacity-0",
            )}
          >
            <Dialog.Title className="text-base font-semibold text-foreground-strong">
              {options.title}
            </Dialog.Title>
            {options.description && (
              <Dialog.Description className="mt-2 text-sm leading-relaxed text-foreground-muted">
                {options.description}
              </Dialog.Description>
            )}
            <input
              ref={inputRef}
              type="text"
              value={value}
              onChange={(e) => setValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  handleConfirm();
                }
              }}
              placeholder={options.placeholder}
              className={cn(
                "mt-4 w-full rounded-md border border-divider bg-surface-base px-3 py-2",
                "text-sm text-foreground-strong outline-none",
                "transition-colors duration-[var(--duration-quick)]",
                "focus:border-brand-foreground focus:ring-2 focus:ring-brand-foreground/40",
              )}
            />
            <div className="mt-6 flex justify-end gap-2">
              <Dialog.Close
                className="rounded-md px-4 py-2 text-sm font-medium text-foreground-muted transition-colors hover:bg-surface-inset"
                onClick={handleCancel}
              >
                {options.cancelLabel ?? "Cancel"}
              </Dialog.Close>
              <button
                onClick={handleConfirm}
                className="rounded-md bg-brand-solid px-4 py-2 text-sm font-medium text-foreground-onbrand transition-colors hover:bg-brand-solid-hover"
              >
                {options.confirmLabel ?? "OK"}
              </button>
            </div>
          </Dialog.Popup>
        </Dialog.Portal>
      </Dialog.Root>
    </PromptContext>
  );
}
