"use client";

import { useState, useCallback, useEffect, useRef, createContext, useContext } from "react";
import { Dialog } from "@base-ui/react/dialog";
import { useTranslations } from "next-intl";

import { cn } from "@/lib/cn";
import { Button } from "@/components/ui/button";

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
  const t = useTranslations("common");
  const [open, setOpen] = useState(false);
  const [options, setOptions] = useState<PromptOptions>({ title: "" });
  const [value, setValue] = useState("");
  const resolveRef = useRef<((value: string | null) => void) | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const triggerRef = useRef<HTMLElement | null>(null);

  const prompt = useCallback((opts: PromptOptions) => {
    resolveRef.current?.(null);
    resolveRef.current = null;
    triggerRef.current =
      typeof document !== "undefined"
        ? (document.activeElement as HTMLElement | null)
        : null;
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

  // Restore focus to the trigger element after the dialog closes.
  useEffect(() => {
    if (open) return;
    const target = triggerRef.current;
    triggerRef.current = null;
    if (!target || !document.contains(target)) return;
    const handle = requestAnimationFrame(() => target.focus({ preventScroll: true }));
    return () => cancelAnimationFrame(handle);
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
              "fixed inset-0 z-overlay bg-surface-overlay backdrop-blur-sm",
              "transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)]",
              "data-[starting-style]:opacity-0 data-[ending-style]:opacity-0",
            )}
          />
          <Dialog.Popup
            className={cn(
              "popup-pop fixed left-1/2 top-1 z-modal w-full max-w-md -translate-x-1/2 -translate-y-1/2",
              "rounded-xl border border-divider bg-surface-base p-6 shadow-3",
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
              aria-label={options.title}
              className={cn(
                "mt-4 w-full rounded-md border border-divider bg-surface-base px-3 py-2",
                "text-sm text-foreground-strong outline-none",
                "transition-colors duration-[var(--duration-quick)]",
                "focus:border-brand-foreground focus:ring-2 focus:ring-brand-foreground/40",
              )}
            />
            <div className="mt-6 flex justify-end gap-2">
              <Dialog.Close render={<Button variant="ghost" size="md" />}>
                {options.cancelLabel ?? t("cancel")}
              </Dialog.Close>
              <Button variant="primary" size="md" onClick={handleConfirm}>
                {options.confirmLabel ?? t("confirm")}
              </Button>
            </div>
          </Dialog.Popup>
        </Dialog.Portal>
      </Dialog.Root>
    </PromptContext>
  );
}
