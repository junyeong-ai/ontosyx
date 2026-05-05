/**
 * Locale-aware number, date, and relative-time formatters.
 *
 * Pure functions that wrap the standard `Intl` API and accept the
 * workspace's BCP 47 locale chain (the same array `localize()` walks).
 * The first locale `Intl` recognises wins — non-existent tags are
 * skipped, falling through to the chain's tail.
 *
 * Performance: `Intl.{Number,DateTime,RelativeTime}Format` constructors
 * are non-trivial (CLDR table walk). Constructed instances are cached
 * by `(chain, options)` and reused across calls so a 1k-row table with
 * a single chain rebuilds the formatter once, not per-cell. Cache keyed
 * by the chain reference + a structural options key — `useLocaleChain`
 * keeps its return reference stable per workspace, so the WeakMap entry
 * survives across renders.
 *
 * Components reach these through `useFormatters()`; raw imports are
 * for non-React surfaces (server actions, scripts).
 */

/**
 * Standard datetime presets. Composed from `Intl.DateTimeFormatOptions`
 * directly so callers can spread + override (e.g. `{ ...DATE_PRESETS.dateTime, second: "2-digit" }`).
 */
export const DATE_PRESETS = {
  /** Date only, written-out month: "May 3, 2026" / "2026년 5월 3일". */
  date: { dateStyle: "medium" },
  /** Date + 24h-style time: "May 3, 2026, 7:42 PM" / "2026년 5월 3일 오후 7:42". */
  dateTime: { dateStyle: "medium", timeStyle: "short" },
  /** Time only, locale-aware AM/PM. */
  time: { timeStyle: "short" },
  /** Year-month-day numeric, sortable when concatenated. */
  isoDate: { year: "numeric", month: "2-digit", day: "2-digit" },
} as const satisfies Record<string, Intl.DateTimeFormatOptions>;

const RELATIVE_UNITS: ReadonlyArray<[Intl.RelativeTimeFormatUnit, number]> = [
  ["year", 31_536_000],
  ["month", 2_592_000],
  ["week", 604_800],
  ["day", 86_400],
  ["hour", 3_600],
  ["minute", 60],
  ["second", 1],
];

function toDate(value: Date | string | number): Date {
  return value instanceof Date ? value : new Date(value);
}

// ---------------------------------------------------------------------------
// Intl instance cache
// ---------------------------------------------------------------------------
//
// Two-level lookup: `WeakMap<chain, Map<optionsKey, Intl.*Format>>`. Chains
// are array references coming from React state / TanStack cache — they are
// stable for the lifetime of a workspace, so the WeakMap entry survives.
// `optionsKey` is the JSON of the options object (or "" when undefined),
// which is sufficient because `Intl.*FormatOptions` are flat, plain values.

type FormatterKind = "number" | "date" | "relativeTime";

type FormatterFor<K extends FormatterKind> =
  K extends "number" ? Intl.NumberFormat :
  K extends "date" ? Intl.DateTimeFormat :
  K extends "relativeTime" ? Intl.RelativeTimeFormat :
  never;

interface FormatterPool {
  number: Map<string, Intl.NumberFormat>;
  date: Map<string, Intl.DateTimeFormat>;
  relativeTime: Map<string, Intl.RelativeTimeFormat>;
}

const POOLS = new WeakMap<readonly string[], FormatterPool>();

function poolFor(chain: readonly string[]): FormatterPool {
  let pool = POOLS.get(chain);
  if (!pool) {
    pool = { number: new Map(), date: new Map(), relativeTime: new Map() };
    POOLS.set(chain, pool);
  }
  return pool;
}

function optionsKey(options: object | undefined): string {
  return options ? JSON.stringify(options) : "";
}

function getFormatter<K extends FormatterKind>(
  kind: K,
  chain: readonly string[],
  options: object | undefined,
  build: () => FormatterFor<K>,
): FormatterFor<K> {
  const bucket = poolFor(chain)[kind] as Map<string, FormatterFor<K>>;
  const key = optionsKey(options);
  let formatter = bucket.get(key);
  if (!formatter) {
    formatter = build();
    bucket.set(key, formatter);
  }
  return formatter;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Format a number against the workspace locale chain.
 *
 *   formatNumber(1234567, chain) // "1,234,567" / "1.234.567"
 *   formatNumber(0.42, chain, { style: "percent" })
 *   formatNumber(1500, chain, { style: "currency", currency: "KRW" })
 *   formatNumber(12345, chain, { notation: "compact" }) // "12K" / "1.2만"
 */
export function formatNumber(
  value: number | bigint,
  chain: readonly string[],
  options?: Intl.NumberFormatOptions,
): string {
  return getFormatter(
    "number",
    chain,
    options,
    () => new Intl.NumberFormat(chain as string[], options),
  ).format(value);
}

/**
 * Format a date / ISO string / epoch ms against the workspace locale
 * chain. Defaults to {@link DATE_PRESETS.dateTime} so call-sites that
 * just say "format this timestamp" get a reasonable default.
 */
export function formatDate(
  value: Date | string | number,
  chain: readonly string[],
  options: Intl.DateTimeFormatOptions = DATE_PRESETS.dateTime,
): string {
  return getFormatter(
    "date",
    chain,
    options,
    () => new Intl.DateTimeFormat(chain as string[], options),
  ).format(toDate(value));
}

/**
 * Format a date as a locale-aware relative phrase against `now`
 * (defaults to "now" at call time): "3 minutes ago" / "in 2 days" /
 * "어제" / "2시간 후". Picks the largest unit whose magnitude is ≥1;
 * the second-unit row anchors the loop so very small deltas still
 * resolve.
 *
 * `now` is injectable so tests don't need to freeze the system clock
 * just to assert "30 minutes ago".
 */
export function formatRelativeTime(
  value: Date | string | number,
  chain: readonly string[],
  now: Date = new Date(),
): string {
  const diffSeconds = (toDate(value).getTime() - now.getTime()) / 1000;
  const formatter = getFormatter(
    "relativeTime",
    chain,
    { numeric: "auto" },
    () => new Intl.RelativeTimeFormat(chain as string[], { numeric: "auto" }),
  );
  for (const [unit, secondsPerUnit] of RELATIVE_UNITS) {
    if (Math.abs(diffSeconds) >= secondsPerUnit || unit === "second") {
      return formatter.format(Math.round(diffSeconds / secondsPerUnit), unit);
    }
  }
  // Unreachable — RELATIVE_UNITS ends with "second" and the guard above
  // matches that row unconditionally. Throw so a future edit that reorders
  // the table fails loudly instead of silently returning the wrong unit.
  throw new Error("formatRelativeTime: RELATIVE_UNITS missing terminal second entry");
}
