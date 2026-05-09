"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { useRouter, useSearchParams, usePathname } from "next/navigation";
import type { ColumnSort } from "@tanstack/react-table";

interface TableUrlState {
  /** Currently active filter values keyed by URL parameter name. */
  filters: Record<string, string>;
  /** Active sort columns. */
  sort: ColumnSort[];
  setFilter: (key: string, value: string | null) => void;
  setSort: (next: ColumnSort[]) => void;
  /** Reset every tracked param. */
  reset: () => void;
}

interface TableUrlStateConfig {
  /**
   * Filter params tracked in the URL — `["status", "kind"]` maps to
   * `?status=...&kind=...`. Empty / `null` values are dropped from
   * the URL so the surface URL stays minimal.
   */
  filters?: readonly string[];
  /**
   * URL parameter that carries the sort state — defaults to `sort`.
   * Encoded as `<col>:<asc|desc>`, comma-separated for multi-sort.
   */
  sortKey?: string;
}

/**
 * Two-way URL ↔ table state binding. Filters and sort columns
 * persist into the query string so reload, share-link, and back-
 * button all land on the same view. Use with the `DataTable`
 * primitive; pass `state.sort` / `state.filters` straight into the
 * table props and `setFilter` / `setSort` into the chrome filter
 * controls.
 */
function parseFilters(
  params: URLSearchParams,
  tracked: readonly string[],
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const key of tracked) {
    const v = params.get(key);
    if (v) out[key] = v;
  }
  return out;
}

function parseSort(
  params: URLSearchParams,
  sortKey: string,
): ColumnSort[] {
  const raw = params.get(sortKey);
  if (!raw) return [];
  return raw
    .split(",")
    .map((token) => {
      const [id, dir] = token.split(":");
      if (!id) return null;
      return { id, desc: dir === "desc" };
    })
    .filter((v): v is ColumnSort => v !== null);
}

export function useTableUrlState({
  filters: tracked = [],
  sortKey = "sort",
}: TableUrlStateConfig = {}): TableUrlState {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();

  const trackedKey = tracked.join(",");
  const paramsKey = params.toString();

  const [filters, setFilters] = useState<Record<string, string>>(() =>
    parseFilters(params, tracked),
  );
  const [sort, setSortState] = useState<ColumnSort[]>(() =>
    parseSort(params, sortKey),
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: derived from URLSearchParams snapshot
  useEffect(() => {
    setFilters(parseFilters(params, tracked));
    setSortState(parseSort(params, sortKey));
  }, [paramsKey, trackedKey, sortKey]);

  const writeParams = useCallback(
    (mutate: (next: URLSearchParams) => void) => {
      const next = new URLSearchParams(paramsKey);
      mutate(next);
      const qs = next.toString();
      router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false });
    },
    [paramsKey, pathname, router],
  );

  const setFilter = useCallback(
    (key: string, value: string | null) => {
      // Optimistic local update so consumers re-render immediately —
      // URL replace lands asynchronously, and tests that don't run a
      // real Next router still see the new value.
      setFilters((prev) => {
        if (value && value.length > 0) return { ...prev, [key]: value };
        const { [key]: _drop, ...rest } = prev;
        return rest;
      });
      writeParams((next) => {
        if (value && value.length > 0) {
          next.set(key, value);
        } else {
          next.delete(key);
        }
      });
    },
    [writeParams],
  );

  const setSort = useCallback(
    (nextSort: ColumnSort[]) => {
      setSortState(nextSort);
      writeParams((next) => {
        if (nextSort.length === 0) {
          next.delete(sortKey);
        } else {
          next.set(
            sortKey,
            nextSort
              .map((s) => `${s.id}:${s.desc ? "desc" : "asc"}`)
              .join(","),
          );
        }
      });
    },
    [sortKey, writeParams],
  );

  const reset = useCallback(() => {
    setFilters({});
    setSortState([]);
    writeParams((next) => {
      for (const key of tracked) next.delete(key);
      next.delete(sortKey);
    });
  }, [tracked, sortKey, writeParams]);

  return useMemo(
    () => ({ filters, sort, setFilter, setSort, reset }),
    [filters, sort, setFilter, setSort, reset],
  );
}
