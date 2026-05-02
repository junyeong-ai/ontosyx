"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { useRouter } from "next/navigation";
import FocusTrap from "focus-trap-react";
import { HugeiconsIcon } from "@hugeicons/react";
import { Cancel01Icon, Search01Icon } from "@hugeicons/core-free-icons";
import { toast } from "sonner";

import { useAppStore } from "@/lib/store";
import { cn } from "@/lib/cn";
import {
  COMMANDS,
  type CommandDef,
  type CommandId,
  filterCommands,
  shortcutFor,
} from "@/lib/commands";

/**
 * Cross-app command palette.
 *
 * Distinct from `workbench/canvas/command-palette.tsx` which scopes
 * to schema operations during ontology design — this one navigates
 * the app, toggles layout, and surfaces the keyboard-shortcut
 * catalogue. Cmd/Ctrl+Shift+P opens it (matching VS Code's
 * "Command Palette" binding); Cmd/Ctrl+K stays bound to the
 * graph-entity search dialog.
 *
 * The catalogue lives in `@/lib/commands` as a single static array
 * — the palette is pure presentation. Adding a new global command
 * means appending one entry there; this component picks it up
 * automatically (label, shortcut, visibility predicate, action).
 */
export function GlobalCommandPalette({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const t = useTranslations("commandPalette");
  const tCommands = useTranslations("commandPalette.commands");
  const router = useRouter();
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const isMac =
    typeof navigator !== "undefined" && /Mac/i.test(navigator.userAgent);

  const resolveLabel = useCallback(
    (id: CommandId) => tCommands(`${id}.label`),
    [tCommands],
  );

  // Reset state on open/close so the palette never opens with a
  // stale query or an out-of-bounds active row.
  useEffect(() => {
    if (open) {
      setQuery("");
      setActiveIndex(0);
      // Focus the search input on the next tick — the palette
      // mounts inside a portal, so the input ref is only attached
      // after layout.
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  // The visibility predicates on each command read fields off the
  // store (`bottomPanelMode`, etc.); subscribe to those slices so
  // the filter re-runs when they change. Picking the slice via a
  // selector is cheaper than `useAppStore()` (which would re-render
  // on every store mutation) and survives Zustand's shallow-equal
  // optimisation: any string value in this tuple changing wakes up
  // the memo.
  const visibilityFingerprint = useAppStore(
    (s) => `${s.bottomPanelMode}|${s.isExplorerOpen ? 1 : 0}|${s.isInspectorOpen ? 1 : 0}|${s.isBottomPanelOpen ? 1 : 0}`,
  );
  const filtered = useMemo(() => {
    const store = useAppStore.getState();
    return filterCommands(COMMANDS, store, query, resolveLabel);
    // The fingerprint participates in the dep list so any state a
    // command's `visible()` reads invalidates the memo.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, resolveLabel, visibilityFingerprint]);

  // Re-clamp the active row when the filtered list shrinks below it.
  useEffect(() => {
    if (activeIndex >= filtered.length) {
      setActiveIndex(Math.max(0, filtered.length - 1));
    }
  }, [filtered.length, activeIndex]);

  const runCommand = useCallback(
    async (cmd: CommandDef) => {
      onClose();
      try {
        await cmd.action({
          router,
          store: {
            getState: useAppStore.getState,
            setState: useAppStore.setState,
          },
        });
      } catch (error) {
        // Surface command failures so the operator gets feedback
        // instead of the palette closing silently. The label is
        // already localised via the catalogue; the error message
        // (typically a router rejection) ships verbatim under
        // `description` for triage.
        toast.error(t("runFailed", { command: tCommands(`${cmd.id}.label`) }), {
          description:
            error instanceof Error ? error.message : String(error),
        });
      }
    },
    [onClose, router, t, tCommands],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setActiveIndex((i) => Math.min(filtered.length - 1, i + 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setActiveIndex((i) => Math.max(0, i - 1));
      } else if (e.key === "Enter") {
        e.preventDefault();
        const cmd = filtered[activeIndex];
        if (cmd) void runCommand(cmd);
      } else if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    },
    [filtered, activeIndex, onClose, runCommand],
  );

  if (!open) return null;

  return (
    <FocusTrap>
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("title")}
        className="fixed inset-0 z-[60] flex items-start justify-center bg-black/40 px-4 pt-24 backdrop-blur-sm"
        onClick={onClose}
      >
        <div
          onClick={(e) => e.stopPropagation()}
          className="w-full max-w-xl overflow-hidden rounded-xl border border-divider bg-surface-base shadow-2xl"
        >
          <div className="flex items-center gap-2 border-b border-divider px-3 py-2">
            <HugeiconsIcon
              icon={Search01Icon}
              className="h-3.5 w-3.5 text-muted-foreground"
              size="100%"
            />
            <input
              ref={inputRef}
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={t("placeholder")}
              className="flex-1 bg-transparent text-sm text-foreground-strong outline-none placeholder:text-muted-foreground-strong"
            />
            <button
              type="button"
              onClick={onClose}
              aria-label={t("closeAria")}
              className="rounded p-0.5 text-muted-foreground hover:bg-surface-inset hover:text-foreground dark:hover:bg-surface-base dark:hover:text-foreground-muted"
            >
              <HugeiconsIcon
                icon={Cancel01Icon}
                className="h-3.5 w-3.5"
                size="100%"
              />
            </button>
          </div>
          <div
            role="listbox"
            aria-label={t("listAria")}
            className="max-h-80 overflow-y-auto py-1"
          >
            {filtered.length === 0 ? (
              <p className="px-3 py-4 text-center text-xs text-muted-foreground">
                {t("noResults", { query })}
              </p>
            ) : (
              filtered.map((cmd, i) => {
                const shortcut = shortcutFor(cmd, isMac);
                const active = i === activeIndex;
                return (
                  <button
                    key={cmd.id}
                    role="option"
                    aria-selected={active}
                    onMouseEnter={() => setActiveIndex(i)}
                    onClick={() => void runCommand(cmd)}
                    className={cn(
                      "flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors",
                      active
                        ? "bg-brand-surface text-brand-foreground-strong"
                        : "text-foreground hover:bg-surface-raised-muted dark:hover:bg-surface-base/60",
                    )}
                  >
                    <span className="w-16 shrink-0 text-2xs uppercase tracking-wider text-muted-foreground">
                      {t(`groups.${cmd.group}`)}
                    </span>
                    <span className="flex-1 truncate">{resolveLabel(cmd.id)}</span>
                    {shortcut && (
                      <kbd className="rounded border border-divider px-1 text-2xs text-muted-foreground dark:border-divider">
                        {shortcut}
                      </kbd>
                    )}
                  </button>
                );
              })
            )}
          </div>
          <div className="flex items-center justify-between border-t border-divider bg-surface-raised px-3 py-1.5 text-2xs text-foreground-muted">
            <span>{t("footerNav")}</span>
            <span>{t("footerCount", { count: filtered.length })}</span>
          </div>
        </div>
      </div>
    </FocusTrap>
  );
}
