"use client";

/**
 * URL-synchronized UI state for client components.
 *
 * Lets a component persist UI state (search text, focus node, breadcrumb,
 * active filter, selected widget id, etc.) to the URL query string so that:
 *  - Navigations and reloads preserve the state.
 *  - Users can share deep-links (e.g. "Explore a specific node", "Reports
 *    page 3 filtered by ontology X").
 *  - The back/forward buttons do NOT create noise while the user is typing.
 *
 * Design choices:
 * - Zod-powered parser/validation. Invalid URL values silently fall back to
 *   the default — we never throw at render time because URLs are untrusted.
 * - Writes are debounced (~200ms). A user typing "ontology" in a search
 *   field should NOT spam 8 entries into the router; instead we coalesce
 *   into one URL update after the typing settles.
 * - Writes use `router.replace` (not `push`) so back-stack only reflects
 *   meaningful navigations, not every keystroke.
 * - Server-render safe: this hook is a client hook and reads from
 *   `useSearchParams()`. Page components that want URL state must be
 *   client components (or render this via a child client component).
 *
 * Note about React 19: `react-hooks/refs` forbids reading `ref.current`
 * during render. Callers must therefore pass *stable* option references
 * (parser / default / serialize / deserialize) — i.e. define them at module
 * scope or wrap in useMemo if they depend on state. A schema like
 * `z.string()` is already stable across renders.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useRouter, useSearchParams, usePathname } from "next/navigation";
import type { ZodType } from "zod";

export interface QueryStateOptions<T> {
  /** Fallback when the URL has no value, or the parse fails. */
  default: T;
  /** Zod schema used to validate/coerce the deserialized value. */
  parser: ZodType<T>;
  /**
   * Serialize the value into the URL. Defaults to `String(value)`, which is
   * fine for strings. Arrays/objects should provide a serializer (e.g.
   * `(xs) => xs.join(",")`).
   */
  serialize?: (value: T) => string;
  /**
   * Deserialize the raw URL string. If not provided, the raw string is
   * passed directly to `parser.safeParse`. Use this for arrays/JSON payloads
   * (e.g. `(raw) => raw.split(",")`).
   */
  deserialize?: (raw: string) => unknown;
  /** Debounce window in ms. Default 200. Set to 0 to write synchronously. */
  debounceMs?: number;
}

const DEFAULT_DEBOUNCE_MS = 200;

export function useQueryState<T>(
  key: string,
  options: QueryStateOptions<T>,
): [T, (next: T) => void] {
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();

  const {
    default: defaultValue,
    parser,
    serialize,
    deserialize,
    debounceMs = DEFAULT_DEBOUNCE_MS,
  } = options;

  // Read the raw URL value. Validation happens inside the sync effect so
  // that we don't re-validate (or re-allocate) on every render when the
  // caller passes inline options like `z.string()`.
  const rawUrlValue = searchParams.get(key);

  // Local (optimistic) state so typing feels instantaneous even before the
  // debounced URL write lands. Initialize from the URL on first mount.
  const [localValue, setLocalValue] = useState<T>(() => {
    if (rawUrlValue === null) return defaultValue;
    const deserialized = deserialize ? deserialize(rawUrlValue) : rawUrlValue;
    const parsed = parser.safeParse(deserialized);
    return parsed.success ? parsed.data : defaultValue;
  });

  // Sync when the URL changes externally (e.g. browser back button).
  // Only depends on `rawUrlValue` + `key` — stable primitives — to avoid
  // an infinite loop when callers pass fresh option objects each render.
  useEffect(() => {
    let next: T;
    if (rawUrlValue === null) {
      next = defaultValue;
    } else {
      const deserialized = deserialize ? deserialize(rawUrlValue) : rawUrlValue;
      const parsed = parser.safeParse(deserialized);
      next = parsed.success ? parsed.data : defaultValue;
    }
    // Shallow-compare via JSON so structurally identical arrays/objects
    // don't trigger cascading re-renders.
    setLocalValue((prev) => (JSON.stringify(prev) === JSON.stringify(next) ? prev : next));
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rawUrlValue, key]);

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const writeToUrl = useCallback(
    (next: T) => {
      const params = new URLSearchParams(searchParams.toString());
      const isDefault =
        JSON.stringify(next) === JSON.stringify(defaultValue);
      if (isDefault) {
        params.delete(key);
      } else {
        const serialized = serialize ? serialize(next) : String(next);
        params.set(key, serialized);
      }
      const query = params.toString();
      const url = query ? `${pathname}?${query}` : pathname;
      // `replace` (not `push`) — URL state changes are UI sync, not
      // navigations. Otherwise every keystroke lands in browser history.
      router.replace(url, { scroll: false });
    },
    [key, pathname, router, searchParams, defaultValue, serialize],
  );

  // Cancel any pending debounced write on unmount OR pathname change.
  //
  // Why cancel (not flush): the effect cleanup captures the PREVIOUS
  // `writeToUrl` closure, which bakes in the *old* pathname. Flushing
  // would call `router.replace(oldPath?q=...)` after Next.js has already
  // committed to the new path, snapping the user back to the route they
  // just left. Dropping the last ~200ms of keystrokes is the lesser evil
  // — callers that need guaranteed URL persistence should pass
  // `debounceMs: 0` for synchronous writes.
  useEffect(() => {
    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
        debounceRef.current = null;
      }
    };
  }, [pathname]);

  const setValue = useCallback(
    (next: T) => {
      setLocalValue(next);
      if (debounceRef.current) clearTimeout(debounceRef.current);
      if (debounceMs <= 0) {
        writeToUrl(next);
        return;
      }
      debounceRef.current = setTimeout(() => writeToUrl(next), debounceMs);
    },
    [debounceMs, writeToUrl],
  );

  return [localValue, setValue];
}
