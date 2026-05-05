"use client";

import {
  useState,
  useCallback,
  useEffect,
  useRef,
  createContext,
  useContext,
} from "react";
import { useTranslations } from "next-intl";
import { Dialog } from "@base-ui/react/dialog";

import { Button } from "@/components/ui/button";
import { FormInput } from "@/components/ui/form-input";

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
  const tCommon = useTranslations("common");
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
      <Dialog.Root
        open={open}
        onOpenChange={(isOpen) => !isOpen && handleCancel()}
      >
        <Dialog.Portal>
          <Dialog.Backdrop className="fixed inset-0 z-overlay bg-surface-scrim-strong backdrop-blur-sm transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)] data-[starting-style]:opacity-0 data-[ending-style]:opacity-0" />
          <Dialog.Popup className="fixed left-1/2 top-1/2 z-modal w-full max-w-md -translate-x-1/2 -translate-y-1/2 rounded-xl border border-divider bg-surface-base p-6 shadow-4 transition-all duration-[var(--duration-base)] ease-[var(--ease-out)] data-[starting-style]:scale-95 data-[starting-style]:opacity-0 data-[ending-style]:scale-95 data-[ending-style]:opacity-0">
            <Dialog.Title className="text-base font-semibold text-foreground-strong">
              {options.title}
            </Dialog.Title>
            {options.description && (
              <Dialog.Description className="mt-2 text-sm leading-relaxed text-foreground-muted">
                {options.description}
              </Dialog.Description>
            )}
            <FormInput
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
              className="mt-4"
            />
            <div className="mt-6 flex justify-end gap-2">
              <Button variant="ghost" size="sm" onClick={handleCancel}>
                {options.cancelLabel ?? tCommon("cancel")}
              </Button>
              <Button variant="primary" size="sm" onClick={handleConfirm}>
                {options.confirmLabel ?? tCommon("confirm")}
              </Button>
            </div>
          </Dialog.Popup>
        </Dialog.Portal>
      </Dialog.Root>
    </PromptContext>
  );
}
