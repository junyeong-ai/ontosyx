"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";

// ---------------------------------------------------------------------------
// Command Palette — VS Code-style command launcher (Cmd+Shift+P)
// ---------------------------------------------------------------------------

export interface PaletteCommand {
  id: string;
  label: string;
  shortcut?: string;
  execute: () => void;
}

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  commands: PaletteCommand[];
}

export function CommandPalette({ open, onClose, commands }: CommandPaletteProps) {
  const t = useTranslations("workbench.canvas.commandPalette");
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const filtered = useMemo(() => {
    if (!query.trim()) return commands;
    const q = query.toLowerCase();
    return commands.filter(
      (cmd) =>
        cmd.label.toLowerCase().includes(q) ||
        (cmd.shortcut && cmd.shortcut.toLowerCase().includes(q)),
    );
  }, [commands, query]);

  // Focus input on mount (component is conditionally rendered, so mount = open)
  useEffect(() => {
    requestAnimationFrame(() => inputRef.current?.focus());
  }, []);

  // Keep selected index in bounds — derive during render
  const clampedIndex = selectedIndex >= filtered.length
    ? Math.max(0, filtered.length - 1)
    : selectedIndex;
  if (clampedIndex !== selectedIndex) {
    setSelectedIndex(clampedIndex);
  }

  // Scroll selected item into view
  useEffect(() => {
    if (!listRef.current) return;
    const items = listRef.current.children;
    const item = items[selectedIndex] as HTMLElement | undefined;
    item?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const executeSelected = useCallback(() => {
    const cmd = filtered[selectedIndex];
    if (cmd) {
      onClose();
      // Defer execution so overlay closes first
      requestAnimationFrame(() => cmd.execute());
    }
  }, [filtered, selectedIndex, onClose]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setSelectedIndex((i) => (i + 1) % Math.max(1, filtered.length));
          break;
        case "ArrowUp":
          e.preventDefault();
          setSelectedIndex(
            (i) => (i - 1 + filtered.length) % Math.max(1, filtered.length),
          );
          break;
        case "Enter":
          e.preventDefault();
          executeSelected();
          break;
        case "Escape":
          e.preventDefault();
          onClose();
          break;
      }
    },
    [filtered.length, executeSelected, onClose],
  );

  if (!open) return null;

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 z-50 bg-black/20 backdrop-blur-[2px]"
        onClick={onClose}
      />

      {/* Palette */}
      <div className="fixed left-1/2 top-[15%] z-50 w-full max-w-lg -translate-x-1/2">
        <div className="overflow-hidden rounded-xl border border-divider bg-surface-base shadow-2xl">
          {/* Search input */}
          <div className="flex items-center border-b border-divider px-4 py-3">
            <span className="mr-2 text-xs text-muted-foreground">&gt;</span>
            <input
              ref={inputRef}
              type="text"
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setSelectedIndex(0);
              }}
              onKeyDown={handleKeyDown}
              placeholder={t("placeholder")}
              className="flex-1 bg-transparent text-sm text-foreground-strong outline-none placeholder:text-foreground-muted-strong dark:placeholder:text-foreground-muted"
            />
          </div>

          {/* Command list */}
          <div ref={listRef} className="max-h-[320px] overflow-auto py-1">
            {filtered.length === 0 ? (
              <div className="px-4 py-6 text-center text-xs text-muted-foreground">
                {t("empty")}
              </div>
            ) : (
              filtered.map((cmd, i) => (
                <button
                  key={cmd.id}
                  onClick={() => {
                    setSelectedIndex(i);
                    onClose();
                    requestAnimationFrame(() => cmd.execute());
                  }}
                  onMouseEnter={() => setSelectedIndex(i)}
                  className={cn(
                    "flex w-full items-center justify-between px-4 py-2 text-left text-sm transition-colors",
                    i === selectedIndex
                      ? "bg-brand-surface text-brand-foreground-strong-strong"
                      : "text-foreground hover:bg-surface-raised-muted dark:hover:bg-surface-base/50",
                  )}
                >
                  <span>{cmd.label}</span>
                  {cmd.shortcut && (
                    <kbd className="ml-3 rounded bg-surface-inset px-1.5 py-0.5 text-2xs font-mono text-muted-foreground">
                      {cmd.shortcut}
                    </kbd>
                  )}
                </button>
              ))
            )}
          </div>

          {/* Footer hint */}
          <div className="border-t border-divider-soft px-4 py-1.5 text-2xs text-muted-foreground">
            <span className="mr-3">{t("hintNavigate")}</span>
            <span className="mr-3">{t("hintExecute")}</span>
            <span>{t("hintClose")}</span>
          </div>
        </div>
      </div>
    </>
  );
}
