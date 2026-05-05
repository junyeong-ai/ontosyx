"use client";

// CommandPalette — unified ⌘K modal that renders every registered
// command source. Industry pattern (Linear, VS Code Quick Open,
// Slack quick switcher, Stripe Dashboard search): one chord, one
// surface, sources stay pluggable.
//
// The component itself owns NO catalogue — it reads from the
// `command-registry` singleton through `useSyncExternalStore`. Add a
// new contributing surface by calling `registerCommandSource()` (or
// using `useCommandSource` from a React subtree). The palette
// re-evaluates the visible commands every time the registry
// notifies, every time the query changes, and every time it
// re-opens.

import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { useTranslations } from "next-intl";
import { useRouter } from "next/navigation";
import { HugeiconsIcon } from "@hugeicons/react";
import { Cancel01Icon, Search01Icon } from "@hugeicons/core-free-icons";

import { FocusTrap } from "@/components/ui/focus-trap";
import { SearchInput } from "@/components/ui/form-input";
import { KeyboardShortcut } from "@/components/ui/keyboard-shortcut";
import { toast } from "@/components/ui/toast";
import {
  type Command,
  type CommandContext,
  commandRegistry,
  filterCommands,
  shortcutGlyph,
} from "@/lib/command-registry";
import { useAppStore } from "@/lib/store";
import { cn } from "@/lib/cn";

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
}

interface FlatRow {
  kind: "header" | "command";
  id: string;
  groupLabel?: string;
  command?: Command;
  /** Index within the navigable rows (commands only). Undefined for headers. */
  navIndex?: number;
}

export function CommandPalette({ open, onClose }: CommandPaletteProps) {
  const t = useTranslations("commandPalette");
  const router = useRouter();
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const isMac =
    typeof navigator !== "undefined" && /Mac/i.test(navigator.userAgent);

  // Subscribe to the registry so external mutations (a settings
  // mount, a route change, a plugin loader) trigger re-render.
  const sources = useSyncExternalStore(
    (listener) => commandRegistry.subscribe(listener),
    () => commandRegistry.list(),
    () => commandRegistry.list(),
  );

  // Reset state on open. Focus the input on the next tick so the
  // portal has a chance to attach the ref.
  useEffect(() => {
    if (open) {
      setQuery("");
      setActiveIndex(0);
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  // Build the flat row list — `[{header, group}, {command}, …]` —
  // honouring the registry's source order. Headers are filtered out
  // when their group has zero matches against the query.
  const { rows, navigableCount, navigableByIndex } = useMemo(() => {
    const flat: FlatRow[] = [];
    let nav = 0;
    const navList: Command[] = [];
    for (const source of sources) {
      const matched = filterCommands(source.commands(), query);
      if (matched.length === 0) continue;
      flat.push({
        kind: "header",
        id: `header:${source.id}`,
        groupLabel: source.groupLabel,
      });
      for (const cmd of matched) {
        flat.push({
          kind: "command",
          id: `${source.id}:${cmd.id}`,
          command: cmd,
          navIndex: nav,
        });
        navList.push(cmd);
        nav += 1;
      }
    }
    return { rows: flat, navigableCount: nav, navigableByIndex: navList };
  }, [sources, query]);

  // Re-clamp the active index when filtering shrinks the list.
  useEffect(() => {
    if (activeIndex >= navigableCount) {
      setActiveIndex(Math.max(0, navigableCount - 1));
    }
  }, [navigableCount, activeIndex]);

  const runCommand = useCallback(
    async (cmd: Command) => {
      onClose();
      const ctx: CommandContext = {
        router,
        store: {
          getState: useAppStore.getState,
          setState: useAppStore.setState,
        },
      };
      try {
        await cmd.execute(ctx);
      } catch (error) {
        toast.error(t("runFailed", { command: cmd.label }), {
          description: error instanceof Error ? error.message : String(error),
        });
      }
    },
    [onClose, router, t],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setActiveIndex((i) => Math.min(navigableCount - 1, i + 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setActiveIndex((i) => Math.max(0, i - 1));
      } else if (e.key === "Enter") {
        e.preventDefault();
        const cmd = navigableByIndex[activeIndex];
        if (cmd) void runCommand(cmd);
      } else if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    },
    [navigableCount, navigableByIndex, activeIndex, onClose, runCommand],
  );

  if (!open) return null;

  return (
    <FocusTrap>
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("title")}
        className="fixed inset-0 z-modal flex items-start justify-center bg-surface-scrim-strong px-4 pt-24 backdrop-blur-sm"
        onClick={onClose}
      >
        <div
          onClick={(e) => e.stopPropagation()}
          className="w-full max-w-xl overflow-hidden rounded-xl border border-divider bg-surface-base shadow-4"
        >
          <div className="border-b border-divider p-2">
            <SearchInput
              ref={inputRef}
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={t("placeholder")}
              aria-label={t("placeholder")}
              density="settings"
              leadingIcon={Search01Icon}
              trailing={
                <button
                  type="button"
                  onClick={onClose}
                  aria-label={t("closeAria")}
                  className="rounded p-0.5 text-foreground-muted hover:bg-surface-inset hover:text-foreground"
                >
                  <HugeiconsIcon
                    icon={Cancel01Icon}
                    className="h-3.5 w-3.5"
                    size="100%"
                  />
                </button>
              }
            />
          </div>
          <div
            role="listbox"
            aria-label={t("listAria")}
            className="max-h-80 overflow-y-auto py-1"
          >
            {rows.length === 0 ? (
              <p className="px-3 py-4 text-center text-xs text-foreground-muted">
                {t("noResults", { query })}
              </p>
            ) : (
              rows.map((row) => {
                if (row.kind === "header") {
                  return (
                    <Header key={row.id} label={row.groupLabel ?? ""} />
                  );
                }
                const cmd = row.command;
                if (!cmd) return null;
                const navIdx = row.navIndex ?? 0;
                const active = navIdx === activeIndex;
                const shortcut = shortcutGlyph(cmd, isMac);
                return (
                  <Row
                    key={row.id}
                    command={cmd}
                    shortcut={shortcut}
                    active={active}
                    onMouseEnter={() => setActiveIndex(navIdx)}
                    onClick={() => void runCommand(cmd)}
                  />
                );
              })
            )}
          </div>
          <div className="flex items-center justify-between border-t border-divider bg-surface-raised px-3 py-1.5 text-2xs text-foreground-muted">
            <span>{t("footerNav")}</span>
            <span>{t("footerCount", { count: navigableCount })}</span>
          </div>
        </div>
      </div>
    </FocusTrap>
  );
}

function Header({ label }: { label: string }) {
  return (
    <div className="px-3 pb-1 pt-2 text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
      {label}
    </div>
  );
}

interface RowProps {
  command: Command;
  shortcut: string | null;
  active: boolean;
  onMouseEnter: () => void;
  onClick: () => void;
}

function Row({ command, shortcut, active, onMouseEnter, onClick }: RowProps) {
  const icon: ReactNode = command.icon ? (
    <HugeiconsIcon
      icon={command.icon}
      className="h-3.5 w-3.5 shrink-0 text-foreground-muted"
      size="100%"
    />
  ) : (
    <span className="h-3.5 w-3.5 shrink-0" aria-hidden />
  );
  return (
    <button
      type="button"
      role="option"
      aria-selected={active}
      onMouseEnter={onMouseEnter}
      onClick={onClick}
      className={cn(
        "flex w-full items-center gap-2 px-3 py-1.5 text-start text-xs transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
        active
          ? "bg-brand-surface text-brand-foreground-strong"
          : "text-foreground hover:bg-surface-raised-muted",
      )}
    >
      {icon}
      <span className="min-w-0 flex-1">
        <span className="block truncate">{command.label}</span>
        {command.description && (
          <span className="block truncate text-2xs text-foreground-muted">
            {command.description}
          </span>
        )}
      </span>
      {shortcut && <KeyboardShortcut glyph={shortcut} variant="outline" />}
    </button>
  );
}
