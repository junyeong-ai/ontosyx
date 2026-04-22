"use client";

import { useSyncExternalStore } from "react";

/**
 * Detect whether the user prefers dark mode.
 *
 * React 19 compliant: `matchMedia` + `documentElement.classList` are external
 * systems, so the React-idiomatic shape is `useSyncExternalStore` — subscribe
 * once, read via getSnapshot. No useEffect/setState cascade.
 *
 * Tracks two sources of truth:
 * 1. OS preference (`prefers-color-scheme: dark`) — via MediaQueryList.
 * 2. Explicit `.dark` class on `<html>` — via MutationObserver, for when the
 *    user has toggled the theme manually.
 */
function subscribe(onChange: () => void): () => void {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  mq.addEventListener("change", onChange);
  const observer = new MutationObserver(onChange);
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["class"],
  });
  return () => {
    mq.removeEventListener("change", onChange);
    observer.disconnect();
  };
}

function getSnapshot(): boolean {
  return (
    window.matchMedia("(prefers-color-scheme: dark)").matches ||
    document.documentElement.classList.contains("dark")
  );
}

function getServerSnapshot(): boolean {
  // SSR: assume light. Client hydration will immediately resync on first paint
  // via the subscription above.
  return false;
}

export function useIsDarkMode(): boolean {
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}
