"use client";

import { useCallback } from "react";
import { useAppStore } from "@/lib/store";
import { useConfirm } from "@/components/ui/confirm-dialog";

// ---------------------------------------------------------------------------
// Shared guard: warn before actions that would discard unsaved local edits
// ---------------------------------------------------------------------------

/**
 * Returns a guard function that checks for unsaved command-stack edits.
 * If pending edits exist, shows a confirmation dialog.
 * Returns `true` if the action should proceed, `false` to abort.
 */
export function useGuardPendingEdits() {
  const commandStack = useAppStore((s) => s.commandStack);
  const confirmDialog = useConfirm();

  return useCallback(
    async (actionName: string): Promise<boolean> => {
      if (commandStack.length === 0) return true;
      return confirmDialog({
        title: "Unsaved Changes",
        description: `You have ${commandStack.length} unsaved edit(s). ${actionName} will use the server-saved version and discard your local changes.\n\nSave your edits first (\u2318S) or continue to discard them.`,
        confirmLabel: "Discard & Continue",
        variant: "warning",
      });
    },
    [commandStack.length, confirmDialog],
  );
}
