"use client";

/**
 * `useFormatters` — locale-aware number / date / relative-time formatters
 * bound to the active workspace's admin chain.
 *
 * Returns a stable `Formatters` object (re-memoised only when the
 * underlying chain changes) so consumers can pass it to children
 * without triggering unrelated re-renders. Reach for this from React
 * surfaces; non-React callers import the raw functions from
 * `@/lib/locale/format`.
 */

import { useMemo } from "react";

import { useLocaleChain } from "@/hooks/use-locale-chain";
import {
  formatDate,
  formatNumber,
  formatRelativeTime,
} from "@/lib/locale/format";

export interface Formatters {
  number(value: number | bigint, options?: Intl.NumberFormatOptions): string;
  date(
    value: Date | string | number,
    options?: Intl.DateTimeFormatOptions,
  ): string;
  relativeTime(value: Date | string | number, now?: Date): string;
}

export function useFormatters(): Formatters {
  const chain = useLocaleChain();
  return useMemo<Formatters>(
    () => ({
      number: (value, options) => formatNumber(value, chain, options),
      date: (value, options) => formatDate(value, chain, options),
      relativeTime: (value, now) => formatRelativeTime(value, chain, now),
    }),
    [chain],
  );
}
