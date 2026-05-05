"use client";

// `useMasterDetailSelection` — round-trip a master-detail page's
// selected entity through the URL search-param, model the
// "creating new" draft state via the `__new__` sentinel, and
// auto-select the first item on initial load so the detail pane
// is never blank.
//
// The pattern was duplicated across `MasterDetailEntityPage`
// (vocabulary tabs: code-systems / value-sets / concept-maps /
// notation-patterns / rules) and the `/settings/knowledge/mappings`
// page; both are now thin call sites of this hook.
//
// Selection state lives in the URL — `?id=<entityId>` for an
// existing item, `?id=__new__` for the draft-create state, no
// param for "no selection" (which only persists when the list is
// empty; once it has items the auto-select effect picks one).
//
// `router.replace` is used so back/forward navigation hops
// between meaningful pages rather than every list-row click.

import { useCallback, useEffect, useMemo } from "react";
import { useRouter, useSearchParams } from "next/navigation";

/// URL sentinel for "the user clicked New, the editor pane shows a
/// blank draft". Exported so detail panes that render their own
/// "create" affordance can call `setSelection(DRAFT_ID)` directly.
export const DRAFT_ID = "__new__";

export interface UseMasterDetailSelectionOptions<T> {
  /** Items the master pane lists. */
  items: readonly T[];
  /** Stable string id for `T` — usually `(t) => t.id`. */
  itemId: (item: T) => string;
  /**
   * URL search-param key. Defaults to `"id"`. Override only when
   * the surface needs distinct selection slots (e.g. a tabbed
   * page sharing a route with sibling content).
   */
  selectionParam?: string;
}

export interface MasterDetailSelection<T> {
  /** Raw URL value — `null` / `string` / `__new__`. */
  selectedId: string | null;
  /** Resolved item, or `null` when nothing is selected or in draft. */
  selected: T | null;
  /** True iff the URL carries the `__new__` sentinel. */
  isDraft: boolean;
  /**
   * Update selection. Pass `null` to clear, `DRAFT_ID` to enter the
   * draft-create state, or an item id to select an existing entity.
   */
  setSelection: (id: string | null) => void;
}

export function useMasterDetailSelection<T>(
  options: UseMasterDetailSelectionOptions<T>,
): MasterDetailSelection<T> {
  const { items, itemId, selectionParam = "id" } = options;
  const router = useRouter();
  const searchParams = useSearchParams();

  const selectedId = searchParams.get(selectionParam);
  const isDraft = selectedId === DRAFT_ID;

  const selected = useMemo(() => {
    if (!selectedId || isDraft) return null;
    return items.find((item) => itemId(item) === selectedId) ?? null;
  }, [items, selectedId, isDraft, itemId]);

  const setSelection = useCallback(
    (id: string | null) => {
      const next = new URLSearchParams(searchParams);
      if (id === null) next.delete(selectionParam);
      else next.set(selectionParam, id);
      const qs = next.toString();
      router.replace(qs ? `?${qs}` : "?");
    },
    [router, searchParams, selectionParam],
  );

  // Auto-select first item when no selection and items are
  // available. Skip while in draft state — the user explicitly
  // requested a blank editor, don't override that.
  useEffect(() => {
    if (selectedId === null && items.length > 0 && !isDraft) {
      setSelection(itemId(items[0]));
    }
  }, [selectedId, items, isDraft, itemId, setSelection]);

  return { selectedId, selected, isDraft, setSelection };
}
