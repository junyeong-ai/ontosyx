"use client";

import {
  useState,
  useCallback,
  createContext,
  useContext,
  useEffect,
  useRef,
} from "react";
import { AlertDialog } from "@base-ui/react/alert-dialog";
import { useTranslations } from "next-intl";

import { cn } from "@/lib/cn";
import { Button } from "@/components/ui/button";

type ConfirmVariant = "danger" | "warning" | "default";

interface ConfirmOptions {
  title: string;
  description: string;
  confirmLabel?: string;
  cancelLabel?: string;
  variant?: ConfirmVariant;
  /**
   * Type-to-confirm gate for high-stakes destructive actions
   * (project deletion, ontology drop, workspace delete). When set,
   * the confirm button stays disabled until the user types the
   * supplied phrase verbatim — typically the resource name.
   *
   * The phrase is matched case-sensitively because Foundry-class
   * tools all do; case-insensitive match makes "delete" too easy
   * to autocomplete past. Pair with a `description` that names
   * what to type so the user knows what string the gate expects.
   */
  typeToConfirm?: {
    /** The exact phrase the user must type. */
    phrase: string;
    /** Field label (e.g. "Project name"). */
    label: string;
    /** Placeholder text in the input. */
    placeholder?: string;
  };
}

type ConfirmFn = (options: ConfirmOptions) => Promise<boolean>;

const ConfirmContext = createContext<ConfirmFn | null>(null);

export function useConfirm(): ConfirmFn {
  const fn = useContext(ConfirmContext);
  if (!fn) throw new Error("useConfirm must be used within <ConfirmProvider>");
  return fn;
}

const confirmVariant: Record<ConfirmVariant, "primary" | "danger"> = {
  default: "primary",
  warning: "primary",
  danger: "danger",
};

export function ConfirmProvider({ children }: { children: React.ReactNode }) {
  const t = useTranslations("common");
  const [open, setOpen] = useState(false);
  const [options, setOptions] = useState<ConfirmOptions>({
    title: "",
    description: "",
  });
  const [typedValue, setTypedValue] = useState("");
  const resolveRef = useRef<((value: boolean) => void) | null>(null);
  const triggerRef = useRef<HTMLElement | null>(null);

  const confirm = useCallback((opts: ConfirmOptions) => {
    resolveRef.current?.(false);
    resolveRef.current = null;
    triggerRef.current =
      typeof document !== "undefined"
        ? (document.activeElement as HTMLElement | null)
        : null;
    setOptions(opts);
    setTypedValue("");
    setOpen(true);
    return new Promise<boolean>((resolve) => {
      resolveRef.current = resolve;
    });
  }, []);

  const typeGate = options.typeToConfirm;
  const typedMatch = typeGate ? typedValue === typeGate.phrase : true;

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

  // Restore focus to the trigger element after the dialog closes.
  // The rAF lets Base UI finish its own focus dance before we move
  // focus back, so the user lands on the button that opened it.
  useEffect(() => {
    if (open) return;
    const target = triggerRef.current;
    triggerRef.current = null;
    if (!target || !document.contains(target)) return;
    const handle = requestAnimationFrame(() => target.focus({ preventScroll: true }));
    return () => cancelAnimationFrame(handle);
  }, [open]);

  return (
    <ConfirmContext value={confirm}>
      {children}
      <AlertDialog.Root open={open} onOpenChange={(next) => !next && handleCancel()}>
        <AlertDialog.Portal>
          <AlertDialog.Backdrop
            className={cn(
              "fixed inset-0 z-overlay bg-surface-overlay backdrop-blur-sm",
              "transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)]",
              "data-[starting-style]:opacity-0 data-[ending-style]:opacity-0",
            )}
          />
          <AlertDialog.Popup
            className={cn(
              "fixed left-1/2 top-1 z-modal w-full max-w-md -translate-x-1/2 -translate-y-1/2",
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
            {typeGate && (
              <label className="mt-4 block text-xs">
                <span className="font-medium text-foreground-strong">
                  {typeGate.label}
                </span>
                <span className="ms-1 font-mono text-foreground-muted">
                  ({typeGate.phrase})
                </span>
                <input
                  type="text"
                  value={typedValue}
                  onChange={(e) => setTypedValue(e.target.value)}
                  placeholder={typeGate.placeholder ?? typeGate.phrase}
                  className="mt-1.5 w-full rounded-md border border-divider bg-surface-base px-3 py-2 font-mono text-sm text-foreground outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40 placeholder:text-foreground-muted"
                  spellCheck={false}
                  autoComplete="off"
                />
              </label>
            )}
            <div className="mt-6 flex justify-end gap-2">
              <AlertDialog.Close render={<Button variant="ghost" size="md" />}>
                {options.cancelLabel ?? t("cancel")}
              </AlertDialog.Close>
              <Button
                variant={confirmVariant[options.variant ?? "default"]}
                size="md"
                onClick={handleConfirm}
                disabled={!typedMatch}
              >
                {options.confirmLabel ?? t("confirm")}
              </Button>
            </div>
          </AlertDialog.Popup>
        </AlertDialog.Portal>
      </AlertDialog.Root>
    </ConfirmContext>
  );
}
