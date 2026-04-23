"use client";

// Bootstrap wizard state persists in localStorage so a refresh or an
// accidental browser close doesn't lose the operator's progress.
// Keeping the schema tight — the wizard collects intent only; real
// writes (project creation, glossary edits, etc.) still go through
// the existing APIs once the user hits Finish on step 6.

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useSyncExternalStore,
} from "react";
import { z } from "zod";

// Storage key is versioned. When we need to change the schema in a
// backward-incompatible way, bump to `.v2` and Zod validation below
// guarantees that an old-shape `.v1` payload can't leak through —
// the parse fails and we fall back to EMPTY cleanly.
const STORAGE_KEY = "ontosyx.bootstrap.v1";

// Single source of truth — Zod schema doubles as the type and the
// runtime validator. Any field added here has to declare a default
// too, so partial payloads (older writers, aborted writes, manual
// tampering) merge cleanly into the full shape without `undefined`
// reaching React state.
const BootstrapStateSchema = z.object({
  pilotName: z.string().default(""),
  pilotScope: z.string().default(""),
  sourceKind: z.string().default(""),
  sourceConnection: z.string().default(""),
  glossaryDraft: z.string().default(""),
  rulesDraft: z.string().default(""),
  mappingNotes: z.string().default(""),
  completedSteps: z.array(z.string()).default([]),
});

export type BootstrapState = z.infer<typeof BootstrapStateSchema>;

// EMPTY is derived from the schema defaults so the two stay aligned
// by construction — no parallel maintenance of a hand-rolled object.
const EMPTY: BootstrapState = BootstrapStateSchema.parse({});

interface Ctx {
  state: BootstrapState;
  update: (patch: Partial<BootstrapState>) => void;
  markComplete: (stepKey: string) => void;
  reset: () => void;
}

const BootstrapCtx = createContext<Ctx | null>(null);

// Module-scope store (singleton). The snapshot is initialized at
// module load on the client — before any component renders — so
// `useSyncExternalStore`'s `getClientSnapshot` never needs to mutate
// module state during render. SSR returns EMPTY via getServerSnapshot,
// and React reconciles post-hydration if client snapshot differs.

function readFromStorage(): BootstrapState {
  if (typeof window === "undefined") return EMPTY;
  let raw: string | null;
  try {
    raw = window.localStorage.getItem(STORAGE_KEY);
  } catch {
    // Private mode / disabled storage — in-memory EMPTY is fine.
    return EMPTY;
  }
  if (!raw) return EMPTY;

  // Parse JSON first, then Zod-validate. We split the two steps so
  // a malformed-JSON payload and a valid-JSON-wrong-shape payload
  // take the same "discard + reset" branch — both are unrecoverable
  // from a user-progress standpoint, and we never want a corrupt
  // snapshot to reach React state.
  let json: unknown;
  try {
    json = JSON.parse(raw);
  } catch {
    return EMPTY;
  }
  const result = BootstrapStateSchema.safeParse(json);
  return result.success ? result.data : EMPTY;
}

function writeToStorage(next: BootstrapState): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Quota or disabled — in-memory is still functional.
  }
}

// Snapshot lives in module scope so provider instances share it.
// Hydration from localStorage is deferred to the first `subscribe`
// call — that way tests can seed `localStorage` before the provider
// mounts, and React doesn't observe a render-phase mutation (the
// first subscribe fires *after* `getClientSnapshot` returns EMPTY,
// and the subsequent notify schedules a re-render that picks up the
// restored snapshot).
let snapshot: BootstrapState = EMPTY;
let hydrated = false;
const listeners = new Set<() => void>();

function setSnapshot(next: BootstrapState): void {
  snapshot = next;
  writeToStorage(next);
  for (const fn of listeners) fn();
}

function subscribe(onChange: () => void): () => void {
  if (!hydrated) {
    hydrated = true;
    const stored = readFromStorage();
    if (stored !== EMPTY) {
      snapshot = stored;
      // Notify on a microtask so we don't recurse through the just-
      // registered listener before React finishes wiring it up.
      queueMicrotask(() => {
        for (const fn of listeners) fn();
      });
    }
  }
  listeners.add(onChange);
  return () => {
    listeners.delete(onChange);
  };
}

// Test hook — lets integration tests simulate a fresh page load
// after manipulating localStorage. No-op in production code paths.
export function __resetBootstrapStore(): void {
  snapshot = EMPTY;
  hydrated = false;
  for (const fn of listeners) fn();
}

function getClientSnapshot(): BootstrapState {
  return snapshot;
}

function getServerSnapshot(): BootstrapState {
  return EMPTY;
}

export function BootstrapProvider({ children }: { children: React.ReactNode }) {
  const state = useSyncExternalStore(
    subscribe,
    getClientSnapshot,
    getServerSnapshot,
  );

  const update = useCallback((patch: Partial<BootstrapState>) => {
    setSnapshot({ ...snapshot, ...patch });
  }, []);

  const markComplete = useCallback((stepKey: string) => {
    if (snapshot.completedSteps.includes(stepKey)) return;
    setSnapshot({
      ...snapshot,
      completedSteps: [...snapshot.completedSteps, stepKey],
    });
  }, []);

  const reset = useCallback(() => {
    setSnapshot(EMPTY);
    if (typeof window !== "undefined") {
      try {
        window.localStorage.removeItem(STORAGE_KEY);
      } catch {
        // no-op
      }
    }
  }, []);

  const value = useMemo(
    () => ({ state, update, markComplete, reset }),
    [state, update, markComplete, reset],
  );

  return <BootstrapCtx.Provider value={value}>{children}</BootstrapCtx.Provider>;
}

export function useBootstrap() {
  const ctx = useContext(BootstrapCtx);
  if (!ctx) {
    throw new Error("useBootstrap must be called inside <BootstrapProvider>");
  }
  return ctx;
}
