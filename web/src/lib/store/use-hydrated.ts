import { useEffect, useState } from "react";

/**
 * Returns true after Zustand persist middleware has hydrated from localStorage.
 * Use this to guard rendering of components that depend on persisted state
 * (e.g., workspaceMode) to prevent a flash of default values.
 *
 * Without this guard, the initial render uses Zustand defaults (workspaceMode: "design"),
 * then hydration updates to the actual persisted value (e.g., "explore"),
 * causing a visible mode flash during workspace switching or page refresh.
 */
export function useHydrated(): boolean {
  const [hydrated, setHydrated] = useState(false);

  useEffect(() => {
    // Zustand persist hydrates synchronously during the first render cycle.
    // By the time useEffect runs, hydration is complete.
    setHydrated(true);
  }, []);

  return hydrated;
}
