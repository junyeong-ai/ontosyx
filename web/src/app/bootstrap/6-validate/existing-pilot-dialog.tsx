"use client";

// Dialog surfaced on Step 6 Finish when the wizard's pilot name
// already matches an ontology in the workspace. Without this pre-
// flight, a returning user would hit a generic 409 toast from the
// create POST with no recovery affordance. This dialog gives three
// explicit choices before any mutating request fires.
//
// The race-condition fallback (dialog cleared, second user commits
// in the meantime, 409 arrives anyway) remains on the create
// catch path — the dialog narrows the window, it does not replace
// conflict handling.

import { useTranslations } from "next-intl";

import { Modal } from "@/components/ui/modal";
import { Button } from "@/components/ui/button";
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
    <Modal
      open={open}
      onOpenChange={(next) => {
        if (!next) onChoose("cancel");
      }}
      title={t("title", { name })}
      description={t("description", { name, suggestion: renameSuggestion })}
      size="md"
    >
      <div
        className="flex flex-col gap-2"
        data-testid="existing-pilot-dialog"
      >
        <Button
          variant="primary"
          size="md"
          onClick={() => onChoose("continue")}
          data-testid="existing-pilot-continue"
        >
          {t("continue")}
        </Button>
        <Button
          variant="outline"
          size="md"
          onClick={() => onChoose("rename")}
          data-testid="existing-pilot-rename"
        >
          {t("rename", { suggestion: renameSuggestion })}
        </Button>
        <Button
          variant="ghost"
          size="md"
          onClick={() => onChoose("cancel")}
          data-testid="existing-pilot-cancel"
        >
          {t("cancel")}
        </Button>
      </div>
    </Modal>
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
