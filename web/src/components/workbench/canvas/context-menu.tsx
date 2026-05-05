"use client";

import { useEffect, useRef } from "react";
import { cn } from "@/lib/cn";
import { useClickOutside } from "@/hooks/use-click-outside";

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
      className="fixed z-popover min-w-[180px] rounded-lg border border-divider bg-surface-base py-1 shadow-4"
      style={{ left: state.x, top: state.y }}
    >
      {items.map((item) =>
        item.submenu ? (
          <SubmenuItem key={item.label} item={item} onClose={onClose} />
        ) : (
          <button type="button"
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
              "flex w-full items-center justify-between px-3 py-1.5 text-start text-xs transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
              item.disabled
                ? "cursor-not-allowed text-foreground-muted"
                : item.danger
                  ? "text-danger-foreground hover:bg-danger-surface"
                  : "text-foreground hover:bg-surface-inset",
            )}
          >
            <span>{item.label}</span>
            {item.shortcut && (
              <span className="ms-4 text-2xs text-foreground-muted">{item.shortcut}</span>
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
      <button type="button"
        role="menuitem"
        aria-haspopup="true"
        className={cn(
          "flex w-full items-center justify-between px-3 py-1.5 text-start text-xs text-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset",
        )}
      >
        <span>{item.label}</span>
        <span className="text-foreground-muted">&#9656;</span>
      </button>
      <div role="menu" className="absolute left-full top-0 hidden min-w-[160px] rounded-lg border border-divider bg-surface-base py-1 shadow-3 group-hover:block">
        {item.submenu?.map((sub, j) => (
          <button type="button"
            role="menuitem"
            key={j}
            onClick={() => {
              sub.onClick?.();
              onClose();
            }}
            className={cn(
              "flex w-full items-center px-3 py-1.5 text-start text-xs text-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset",
            )}
          >
            {sub.label}
          </button>
        ))}
      </div>
    </div>
  );
}
