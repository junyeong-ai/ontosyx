"use client";

import { useSyncExternalStore } from "react";

/**
 * Returns `false` during SSR and on the first client render; switches
 * to `true` once the component has mounted on the client.
 *
 * Why this exists: several surfaces need to defer client-only content
 * (localStorage reads, `useAuth`, persisted Zustand state) without
 * hydration-mismatch errors. The idiomatic React 19 replacement for
 * `const [m, setM] = useState(false); useEffect(() => setM(true), [])`
 * is `useSyncExternalStore` pointed at the SSR/client split — no setState
 * in an effect body.
 *
 * Not the same as `useHydrated` (which waits for Zustand persist to
 * finish). Use `useHydrated` when you specifically depend on persisted
 * UI state; use `useIsClient` for "any client-only render gate".
 */
const subscribeNoop = (): (() => void) => () => {};

export function useIsClient(): boolean {
  return useSyncExternalStore(
    subscribeNoop,
    () => true,
    () => false,
  );
}
