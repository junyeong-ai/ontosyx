"use client";

import { useState, useEffect } from "react";
import FocusTrap from "focus-trap-react";

// NOTE: focus-trap-react pattern — wrap the modal popup in <FocusTrap> and
// enable it only while the dialog is open. Tab/Shift+Tab stay inside the
// dialog; Escape is handled via a global keydown listener (below) so the
// backdrop <button> can still act as the close affordance for mouse users.
// Future modals should follow this same pattern (FocusTrap + aria-modal +
// labelled title + backdrop button).

const SHORTCUTS = [
  { keys: "\u2318K", description: "Open AI command bar" },
  { keys: "\u2318S", description: "Save ontology to server" },
  { keys: "\u2318Z", description: "Undo last change" },
  { keys: "\u21e7\u2318Z", description: "Redo last change" },
  { keys: "\u2318A", description: "Select all nodes" },
  { keys: "Delete", description: "Delete selected element" },
  { keys: "Escape", description: "Close dialogs / deselect" },
  { keys: "!", description: "Raw Cypher mode (in chat)" },
];

export function KeyboardShortcutsDialog() {
  const [isOpen, setIsOpen] = useState(false);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "/" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setIsOpen((v) => !v);
      }
      if (e.key === "Escape" && isOpen) {
        setIsOpen(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [isOpen]);

  if (!isOpen) return null;

  return (
    <FocusTrap
      focusTrapOptions={{
        initialFocus: false,
        allowOutsideClick: true,
        escapeDeactivates: false,
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="kb-shortcuts-title"
        className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm"
      >
        {/* Backdrop — a real <button> collapsed to full-screen: click OR keyboard
            closes the dialog. Sits behind the popup via z-index stacking. */}
        <button
          type="button"
          aria-label="Close keyboard shortcuts"
          className="absolute inset-0 cursor-default"
          onClick={() => setIsOpen(false)}
        />
        <div
          className="relative w-80 rounded-xl border border-zinc-200 bg-white p-4 shadow-2xl dark:border-zinc-700 dark:bg-zinc-900"
        >
          <h2
            id="kb-shortcuts-title"
            className="mb-3 text-sm font-semibold text-zinc-800 dark:text-zinc-200"
          >
            Keyboard Shortcuts
          </h2>
          <div className="space-y-1.5">
            {SHORTCUTS.map((s) => (
              <div key={s.keys} className="flex items-center justify-between">
                <span className="text-xs text-zinc-600 dark:text-zinc-400">{s.description}</span>
                <kbd className="rounded bg-zinc-100 px-1.5 py-0.5 font-mono text-[10px] text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400">{s.keys}</kbd>
              </div>
            ))}
          </div>
          <p className="mt-3 text-center text-[10px] text-zinc-400">Press ⌘/ to toggle this dialog</p>
        </div>
      </div>
    </FocusTrap>
  );
}
