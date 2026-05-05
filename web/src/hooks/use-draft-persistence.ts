"use client";

// `useDraftPersistence` — localStorage-backed draft for long-form
// editors so a closed tab, accidental navigation, or transient
// network failure doesn't lose unsaved work.
//
// Long-form editors (rule SHACL editor, glossary term editor,
// binding wizard) gather a lot of structured input over many
// minutes. The save action is downstream — they're not
// continuously persisted. Without a draft layer the user pays for
// the whole session if anything interrupts them. This hook keeps
// the in-progress value sticky for `ttlMs` (7 days by default) and
// gives the calling form an idiomatic three-step flow:
//
//   1. On mount, `hasDraft === true` if a recent draft is on disk.
//      The form chrome surfaces a "Restore draft" affordance.
//   2. On every change, the form calls `save(value)` — the hook
//      debounces writes so a stream of keystrokes doesn't burn
//      localStorage IO. Default debounce 500ms.
//   3. On successful submit, the form calls `clear()` so the
//      next mount starts fresh.
//
// The store shape is `{ value, savedAt }` so an old draft past TTL
// is silently ignored — the user doesn't have to opt into a
// cleanup, and stale drafts don't accumulate forever in
// localStorage.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

const DEFAULT_TTL_MS = 7 * 24 * 60 * 60 * 1000; // 7 days
const DEFAULT_DEBOUNCE_MS = 500;

interface DraftRecord<T> {
  value: T;
  savedAt: number;
}

export interface UseDraftPersistenceOptions {
  /**
   * Stable key for the draft. Include the workspace / user / resource
   * id when the draft is scoped — `draft:rule:{id}` is a typical
   * shape. Two forms sharing one key clobber each other's drafts;
   * scope deliberately to avoid that.
   */
  key: string;
  /** TTL in milliseconds. Default: 7 days. */
  ttlMs?: number;
  /** Debounce window for save writes. Default: 500ms. */
  debounceMs?: number;
}

export interface DraftPersistence<T> {
  /** The persisted value loaded at mount, or `null` if no recent draft exists. */
  draft: T | null;
  /** True when `draft` is non-null at mount time — surface a restore CTA. */
  hasDraft: boolean;
  /** Persist the value. Debounced — flushes after `debounceMs` of inactivity. */
  save: (value: T) => void;
  /** Drop the persisted draft. Call on successful submit. */
  clear: () => void;
}

function readDraft<T>(key: string, ttlMs: number): T | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return null;
    const record = JSON.parse(raw) as DraftRecord<T>;
    if (typeof record !== "object" || record === null) return null;
    if (typeof record.savedAt !== "number") return null;
    if (Date.now() - record.savedAt > ttlMs) {
      // Stale — drop it eagerly so the next read is cheap.
      window.localStorage.removeItem(key);
      return null;
    }
    return record.value;
  } catch {
    // JSON parse failure or quota error — treat as "no draft".
    return null;
  }
}

function writeDraft<T>(key: string, value: T): void {
  if (typeof window === "undefined") return;
  try {
    const record: DraftRecord<T> = { value, savedAt: Date.now() };
    window.localStorage.setItem(key, JSON.stringify(record));
  } catch {
    // QuotaExceeded or serialisation error — silently drop the write
    // rather than crashing the editor; the user can still submit
    // normally, they just lose the draft safety net for this session.
  }
}

function deleteDraft(key: string): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.removeItem(key);
  } catch {
    // ignore — see writeDraft.
  }
}

export function useDraftPersistence<T>(
  options: UseDraftPersistenceOptions,
): DraftPersistence<T> {
  const ttlMs = options.ttlMs ?? DEFAULT_TTL_MS;
  const debounceMs = options.debounceMs ?? DEFAULT_DEBOUNCE_MS;

  // Read once, on mount only — subsequent re-renders pull from the
  // closure, never re-read localStorage. The `key` change case is
  // handled by re-running the effect below.
  const [draft, setDraft] = useState<T | null>(() =>
    readDraft<T>(options.key, ttlMs),
  );
  const [hasDraft, setHasDraft] = useState<boolean>(() => draft !== null);

  // When `key` changes (e.g. user opens a different rule), refresh
  // the cached value. `ttlMs` is read from a ref so it doesn't need
  // to be in the dep array.
  const ttlRef = useRef(ttlMs);
  useEffect(() => {
    ttlRef.current = ttlMs;
  }, [ttlMs]);
  useEffect(() => {
    const v = readDraft<T>(options.key, ttlRef.current);
    setDraft(v);
    setHasDraft(v !== null);
  }, [options.key]);

  // Debounced writer. We keep the last-pending value in a ref so a
  // burst of `save(...)` calls coalesces into one localStorage write
  // after the debounce window closes — the user sees no lag and the
  // disk sees minimal writes.
  const pendingValueRef = useRef<T | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const keyRef = useRef(options.key);
  const debounceRef = useRef(debounceMs);
  useEffect(() => {
    keyRef.current = options.key;
  }, [options.key]);
  useEffect(() => {
    debounceRef.current = debounceMs;
  }, [debounceMs]);

  const save = useCallback((value: T) => {
    pendingValueRef.current = value;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      if (pendingValueRef.current !== null) {
        writeDraft(keyRef.current, pendingValueRef.current);
      }
    }, debounceRef.current);
  }, []);

  const clear = useCallback(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    pendingValueRef.current = null;
    deleteDraft(keyRef.current);
    setDraft(null);
    setHasDraft(false);
  }, []);

  // Flush the pending write on unmount so a quick close-tab / navigate
  // doesn't lose a few hundred milliseconds of recent edits.
  useEffect(() => {
    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        if (pendingValueRef.current !== null) {
          writeDraft(keyRef.current, pendingValueRef.current);
        }
      }
    };
  }, []);

  return useMemo(
    () => ({ draft, hasDraft, save, clear }),
    [draft, hasDraft, save, clear],
  );
}
