"use client";

import { useState, useEffect } from "react";

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
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm" onClick={() => setIsOpen(false)}>
      <div className="w-80 rounded-xl border border-zinc-200 bg-white p-4 shadow-2xl dark:border-zinc-700 dark:bg-zinc-900" onClick={(e) => e.stopPropagation()}>
        <h2 className="mb-3 text-sm font-semibold text-zinc-800 dark:text-zinc-200">Keyboard Shortcuts</h2>
        <div className="space-y-1.5">
          {SHORTCUTS.map((s) => (
            <div key={s.keys} className="flex items-center justify-between">
              <span className="text-xs text-zinc-600 dark:text-zinc-400">{s.description}</span>
              <kbd className="rounded bg-zinc-100 px-1.5 py-0.5 font-mono text-[10px] text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400">{s.keys}</kbd>
            </div>
          ))}
        </div>
        <p className="mt-3 text-center text-[10px] text-zinc-400">Press \u2318/ to toggle this dialog</p>
      </div>
    </div>
  );
}
