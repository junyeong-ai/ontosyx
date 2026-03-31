"use client";

import { useState, useCallback, useEffect, useRef, createContext, useContext } from "react";
import { Dialog } from "@base-ui/react/dialog";

// ---------------------------------------------------------------------------
// PromptDialog — Base UI Dialog wrapper replacing window.prompt()
// ---------------------------------------------------------------------------

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

  // Focus input when dialog opens
  useEffect(() => {
    if (open) {
      // Use requestAnimationFrame to ensure the dialog is rendered
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
      <Dialog.Root open={open} onOpenChange={(isOpen) => !isOpen && handleCancel()}>
        <Dialog.Portal>
          <Dialog.Backdrop className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm data-[starting-style]:opacity-0 data-[ending-style]:opacity-0 transition-opacity" />
          <Dialog.Popup className="fixed left-1/2 top-1/2 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2 rounded-xl border border-zinc-200 bg-white p-6 shadow-xl data-[starting-style]:scale-95 data-[starting-style]:opacity-0 data-[ending-style]:scale-95 data-[ending-style]:opacity-0 transition-all dark:border-zinc-700 dark:bg-zinc-900">
            <Dialog.Title className="text-base font-semibold text-zinc-900 dark:text-zinc-100">
              {options.title}
            </Dialog.Title>
            {options.description && (
              <Dialog.Description className="mt-2 text-sm leading-relaxed text-zinc-600 dark:text-zinc-400">
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
              className="mt-4 w-full rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm text-zinc-900 outline-none transition-colors focus:border-emerald-500 focus:ring-2 focus:ring-emerald-500/50 dark:border-zinc-600 dark:bg-zinc-800 dark:text-zinc-100 dark:focus:border-emerald-500"
            />
            <div className="mt-6 flex justify-end gap-2">
              <Dialog.Close
                className="rounded-lg px-4 py-2 text-sm font-medium text-zinc-600 transition-colors hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
                onClick={handleCancel}
              >
                {options.cancelLabel ?? "Cancel"}
              </Dialog.Close>
              <button
                onClick={handleConfirm}
                className="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-emerald-700"
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
