"use client";

import { type ReactNode, useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowDown01Icon,
  ArrowRight01Icon,
} from "@hugeicons/core-free-icons";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";

// ListBuilder — generic add / edit / remove / reorder editor for
// `Vec<Item>` fields on an entity. Each row collapses to a compact
// preview (rendered by `rowPreview`) and expands inline to the
// full row form (rendered by `renderRow`). Reorder uses up/down
// affordances rather than drag-and-drop because the lists this
// powers are short (codes, composition rules, related terms) and
// keyboard-only access is the contract.

interface ListBuilderProps<Item> {
  items: ReadonlyArray<Item>;
  onChange: (next: Item[]) => void;
  itemKey: (item: Item, index: number) => string;
  /** Compact preview shown when the row is collapsed. */
  rowPreview: (item: Item, index: number) => ReactNode;
  /** Full row editor shown when the row is expanded. */
  renderRow: (props: {
    item: Item;
    index: number;
    onChange: (next: Item) => void;
  }) => ReactNode;
  newItem: () => Item;
  /** i18n label for the "add" affordance. */
  addLabel: string;
  /** Empty state copy. */
  emptyTitle: string;
  emptyDescription?: string;
  disabled?: boolean;
}

export function ListBuilder<Item>({
  items,
  onChange,
  itemKey,
  rowPreview,
  renderRow,
  newItem,
  addLabel,
  emptyTitle,
  emptyDescription,
  disabled,
}: ListBuilderProps<Item>) {
  const t = useTranslations("forms.listBuilder");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const update = (index: number, next: Item) => {
    const list = [...items];
    list[index] = next;
    onChange(list);
  };

  const remove = (index: number) => {
    onChange(items.filter((_, i) => i !== index));
  };

  const move = (from: number, to: number) => {
    if (to < 0 || to >= items.length) return;
    const list = [...items];
    const [moved] = list.splice(from, 1);
    list.splice(to, 0, moved);
    onChange(list);
  };

  const add = () => {
    const next = [...items, newItem()];
    onChange(next);
    setExpanded((current) => {
      const out = new Set(current);
      out.add(itemKey(next[next.length - 1], next.length - 1));
      return out;
    });
  };

  const toggle = (key: string) => {
    setExpanded((current) => {
      const out = new Set(current);
      if (out.has(key)) out.delete(key);
      else out.add(key);
      return out;
    });
  };

  return (
    <div className="flex flex-col gap-2">
      {items.length === 0 ? (
        <EmptyState
          title={emptyTitle}
          description={emptyDescription}
          variant="compact"
        />
      ) : (
        <ul className="flex flex-col gap-1">
          {items.map((item, idx) => {
            const key = itemKey(item, idx);
            const open = expanded.has(key);
            return (
              <li
                key={key}
                className="rounded-md border border-divider bg-surface-base"
              >
                <div className="flex items-center gap-2 px-2 py-1.5">
                  <button
                    type="button"
                    onClick={() => toggle(key)}
                    aria-expanded={open}
                    className="flex flex-1 items-center gap-2 text-start"
                  >
                    <HugeiconsIcon
                      icon={open ? ArrowDown01Icon : ArrowRight01Icon}
                      className="h-3 w-3 text-foreground-muted"
                      size="100%"
                    />
                    <span className="min-w-0 flex-1 text-xs">
                      {rowPreview(item, idx)}
                    </span>
                  </button>
                  <div className="flex items-center gap-1">
                    <Button
                      type="button"
                      variant="ghost"
                      size="xs"
                      onClick={() => move(idx, idx - 1)}
                      disabled={disabled || idx === 0}
                      aria-label={t("moveUp")}
                    >
                      ↑
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="xs"
                      onClick={() => move(idx, idx + 1)}
                      disabled={disabled || idx === items.length - 1}
                      aria-label={t("moveDown")}
                    >
                      ↓
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="xs"
                      onClick={() => remove(idx)}
                      disabled={disabled}
                      aria-label={t("remove")}
                    >
                      {t("remove")}
                    </Button>
                  </div>
                </div>
                {open && (
                  <div className="border-t border-divider-soft px-3 py-2">
                    {renderRow({
                      item,
                      index: idx,
                      onChange: (next) => update(idx, next),
                    })}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}

      <Button
        type="button"
        variant="ghost"
        size="xs"
        onClick={add}
        disabled={disabled}
        className="self-start"
      >
        {addLabel}
      </Button>
    </div>
  );
}
