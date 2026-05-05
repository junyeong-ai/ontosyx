"use client";

// CommandPaletteHost — single mount point for the unified ⌘K
// palette. Lives at the root layout so the chord works on every
// surface (workbench / settings / canvas). The store flag
// `isCommandPaletteOpen` drives visibility — same flag that
// `useShortcut("global.commandPalette")` toggles below.

import { useCallback } from "react";

import { CommandPalette } from "@/components/ui/command-palette";
import { useShortcut } from "@/lib/shortcuts";
import { useAppStore } from "@/lib/store";

export function CommandPaletteHost() {
  const open = useAppStore((s) => s.isCommandPaletteOpen);
  const setOpen = useAppStore((s) => s.setCommandPaletteOpen);

  const onClose = useCallback(() => setOpen(false), [setOpen]);

  useShortcut({
    id: "global.commandPalette",
    keys: ["mod+k"],
    group: "keyboardShortcuts.sections.global",
    description: "keyboardShortcuts.shortcuts.commandPalette",
    handler: (e) => {
      e.preventDefault();
      const store = useAppStore.getState();
      store.setCommandPaletteOpen(!store.isCommandPaletteOpen);
    },
  });

  return <CommandPalette open={open} onClose={onClose} />;
}
