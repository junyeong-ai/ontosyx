"use client";

// Dialog surfaced on Step 6 Finish when the wizard's pilot name
// already matches an ontology in the workspace. The legacy path
// relied on a 409 from POST /bootstrap/seed-glossary and a toast,
// leaving a returning user at a wall. This dialog gives three
// explicit choices before any mutating request fires.
//
// The race-condition fallback (dialog cleared, second user commits
// in the meantime, 409 arrives anyway) remains on the seed-glossary
// catch path — the dialog narrows the window, it does not replace
// conflict handling.

import { useTranslations } from "next-intl";
import { AlertDialog } from "@base-ui/react/alert-dialog";

import type { OntologyListItem } from "@/types/api";

export type ExistingPilotChoice = "continue" | "rename" | "cancel";

export interface ExistingPilotDialogProps {
  open: boolean;
  /** The existing ontology the wizard collided with. */
  existing: OntologyListItem | null;
  /** Suggested alternative name the Rename button applies. */
  renameSuggestion: string;
  onChoose: (choice: ExistingPilotChoice) => void;
}

export function ExistingPilotDialog({
  open,
  existing,
  renameSuggestion,
  onChoose,
}: ExistingPilotDialogProps) {
  const t = useTranslations("bootstrap.step6.existingPilot");
  const name = existing?.name ?? "";

  return (
    <AlertDialog.Root
      open={open}
      onOpenChange={(isOpen) => !isOpen && onChoose("cancel")}
    >
      <AlertDialog.Portal>
        <AlertDialog.Backdrop className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm data-[starting-style]:opacity-0 data-[ending-style]:opacity-0 transition-opacity" />
        <AlertDialog.Popup
          className="fixed left-1/2 top-1/2 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2 rounded-xl border border-zinc-200 bg-white p-6 shadow-xl data-[starting-style]:scale-95 data-[starting-style]:opacity-0 data-[ending-style]:scale-95 data-[ending-style]:opacity-0 transition-all dark:border-zinc-700 dark:bg-zinc-900"
          data-testid="existing-pilot-dialog"
        >
          <AlertDialog.Title className="text-base font-semibold text-zinc-900 dark:text-zinc-100">
            {t("title", { name })}
          </AlertDialog.Title>
          <AlertDialog.Description className="mt-2 text-sm leading-relaxed text-zinc-600 dark:text-muted-foreground">
            {t("description", { name, suggestion: renameSuggestion })}
          </AlertDialog.Description>

          <div className="mt-6 flex flex-col gap-2">
            <button
              type="button"
              onClick={() => onChoose("continue")}
              className="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-emerald-700"
              data-testid="existing-pilot-continue"
            >
              {t("continue")}
            </button>
            <button
              type="button"
              onClick={() => onChoose("rename")}
              className="rounded-lg border border-zinc-300 bg-white px-4 py-2 text-sm font-medium text-zinc-900 transition-colors hover:bg-zinc-50 dark:border-zinc-600 dark:bg-zinc-900 dark:text-zinc-100 dark:hover:bg-zinc-800"
              data-testid="existing-pilot-rename"
            >
              {t("rename", { suggestion: renameSuggestion })}
            </button>
            <AlertDialog.Close
              className="rounded-lg px-4 py-2 text-sm font-medium text-zinc-600 transition-colors hover:bg-zinc-100 dark:text-muted-foreground dark:hover:bg-zinc-800"
              onClick={() => onChoose("cancel")}
              data-testid="existing-pilot-cancel"
            >
              {t("cancel")}
            </AlertDialog.Close>
          </div>
        </AlertDialog.Popup>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}

/**
 * Derive a rename suggestion from the conflicting pilot name. Adds
 * a numeric suffix — `Pilot` → `Pilot 2`, `Pilot 2` → `Pilot 3` —
 * so repeated retries walk forward instead of looping on the same
 * suffix. Whitespace-only input yields an empty suggestion; the
 * caller decides whether to forbid Rename in that state.
 */
export function suggestRename(name: string): string {
  const base = name.trim();
  if (!base) return "";
  // Match a trailing integer (with or without whitespace before it).
  const suffixMatch = /^(.*?)(\s+)(\d+)$/.exec(base);
  if (suffixMatch) {
    const [, stem, gap, nStr] = suffixMatch;
    const next = Number.parseInt(nStr, 10) + 1;
    return `${stem}${gap}${next}`;
  }
  return `${base} 2`;
}
