"use client";

import { useEffect, useRef } from "react";
import { cn } from "@/lib/cn";
import { useClickOutside } from "@/lib/use-click-outside";

// ---------------------------------------------------------------------------
// Context menu — floating right-click menu for nodes and edges
// ---------------------------------------------------------------------------

export interface ContextMenuItem {
  label: string;
  shortcut?: string;
  onClick?: () => void;
  danger?: boolean;
  disabled?: boolean;
  /** Submenu items (one level only) */
  submenu?: ContextMenuItem[];
}

export interface ContextMenuState {
  type: "node" | "edge";
  id: string;
  x: number;
  y: number;
}

interface ContextMenuProps {
  state: ContextMenuState;
  items: ContextMenuItem[];
  onClose: () => void;
}

export function ContextMenu({ state, items, onClose }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);

  // Close on click outside
  useClickOutside(menuRef, onClose);

  // Close on Escape
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener("keydown", handleKey);
    return () => document.removeEventListener("keydown", handleKey);
  }, [onClose]);

  // Position menu within viewport bounds
  useEffect(() => {
    if (!menuRef.current) return;
    const el = menuRef.current;
    const rect = el.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;

    if (rect.right > vw) {
      el.style.left = `${state.x - rect.width}px`;
    }
    if (rect.bottom > vh) {
      el.style.top = `${state.y - rect.height}px`;
    }
  }, [state.x, state.y]);

  return (
    <div
      ref={menuRef}
      role="menu"
      className="fixed z-50 min-w-[180px] rounded-lg border border-zinc-200 bg-white py-1 shadow-xl dark:border-zinc-700 dark:bg-zinc-900"
      style={{ left: state.x, top: state.y }}
    >
      {items.map((item) =>
        item.submenu ? (
          <SubmenuItem key={item.label} item={item} onClose={onClose} />
        ) : (
          <button
            role="menuitem"
            key={item.label}
            onClick={() => {
              if (!item.disabled && item.onClick) {
                item.onClick();
                onClose();
              }
            }}
            disabled={item.disabled}
            className={cn(
              "flex w-full items-center justify-between px-3 py-1.5 text-left text-xs transition-colors",
              item.disabled
                ? "cursor-not-allowed text-zinc-300 dark:text-zinc-600"
                : item.danger
                  ? "text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950/30"
                  : "text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800",
            )}
          >
            <span>{item.label}</span>
            {item.shortcut && (
              <span className="ml-4 text-[10px] text-zinc-400">{item.shortcut}</span>
            )}
          </button>
        ),
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Submenu item (hover to reveal)
// ---------------------------------------------------------------------------

function SubmenuItem({
  item,
  onClose,
}: {
  item: ContextMenuItem;
  onClose: () => void;
}) {
  return (
    <div className="group relative">
      <button
        role="menuitem"
        aria-haspopup="true"
        className={cn(
          "flex w-full items-center justify-between px-3 py-1.5 text-left text-xs text-zinc-700 transition-colors hover:bg-zinc-100",
          "dark:text-zinc-300 dark:hover:bg-zinc-800",
        )}
      >
        <span>{item.label}</span>
        <span className="text-zinc-400">&#9656;</span>
      </button>
      <div role="menu" className="absolute left-full top-0 hidden min-w-[160px] rounded-lg border border-zinc-200 bg-white py-1 shadow-lg group-hover:block dark:border-zinc-700 dark:bg-zinc-900">
        {item.submenu?.map((sub, j) => (
          <button
            role="menuitem"
            key={j}
            onClick={() => {
              sub.onClick?.();
              onClose();
            }}
            className={cn(
              "flex w-full items-center px-3 py-1.5 text-left text-xs text-zinc-700 transition-colors hover:bg-zinc-100",
              "dark:text-zinc-300 dark:hover:bg-zinc-800",
            )}
          >
            {sub.label}
          </button>
        ))}
      </div>
    </div>
  );
}
