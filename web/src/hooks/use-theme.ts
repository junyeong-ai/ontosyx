"use client";

import { useCallback, useSyncExternalStore } from "react";

// ---------------------------------------------------------------------------
// Theme preference — persistent user choice across `system | light | dark`.
// ---------------------------------------------------------------------------
//
// `system`  follows `prefers-color-scheme` and re-resolves whenever the OS
//           media query flips (mid-session if the OS auto-darkens at sunset).
// `light` / `dark`  pin the resolved theme regardless of OS preference.
//
// Persistence: a single `localStorage` key (`ontosyx_theme`). When the user
// picks `system`, the key is *removed* — absence means "follow the OS",
// presence means "user has opted in to a manual override". This is the
// pattern Linear / Vercel / Stripe ship; the inline boot script in
// `app/layout.tsx` reads the same key + `prefers-color-scheme` to apply the
// `.dark` class before first paint, so a manually-pinned dark workspace
// does not flash light during hydration.
// ---------------------------------------------------------------------------

export type ThemePreference = "system" | "light" | "dark";

export const THEME_STORAGE_KEY = "ontosyx_theme";

function readPreference(): ThemePreference {
  if (typeof window === "undefined") return "system";
  try {
    const raw = window.localStorage.getItem(THEME_STORAGE_KEY);
    return raw === "light" || raw === "dark" ? raw : "system";
  } catch {
    return "system";
  }
}

function applyResolvedTheme(pref: ThemePreference): void {
  if (typeof window === "undefined") return;
  const dark =
    pref === "dark" ||
    (pref === "system" &&
      window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
}

// In-process subscriber set — every `useThemePreference` mount hooks in here
// so a setPref call from one surface notifies every observer (sidebar
// chrome, theme switcher menu item, mobile drawer, etc.) in the same tick.
const subscribers = new Set<() => void>();

function notify(): void {
  for (const fn of subscribers) fn();
}

function subscribe(onChange: () => void): () => void {
  subscribers.add(onChange);
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  const handleSystemChange = () => {
    // Only re-apply when the user's preference is `system`. A pinned
    // light / dark workspace ignores OS preference flips.
    if (readPreference() === "system") {
      applyResolvedTheme("system");
    }
    onChange();
  };
  mq.addEventListener("change", handleSystemChange);
  return () => {
    subscribers.delete(onChange);
    mq.removeEventListener("change", handleSystemChange);
  };
}

function getSnapshot(): ThemePreference {
  return readPreference();
}

function getServerSnapshot(): ThemePreference {
  return "system";
}

export function useThemePreference(): {
  preference: ThemePreference;
  setPreference: (next: ThemePreference) => void;
} {
  const preference = useSyncExternalStore(
    subscribe,
    getSnapshot,
    getServerSnapshot,
  );
  const setPreference = useCallback((next: ThemePreference) => {
    try {
      if (next === "system") {
        window.localStorage.removeItem(THEME_STORAGE_KEY);
      } else {
        window.localStorage.setItem(THEME_STORAGE_KEY, next);
      }
    } catch {
      // localStorage can be unavailable (privacy mode, ITP) — apply the
      // visual change anyway so the in-tab session reflects the choice.
    }
    applyResolvedTheme(next);
    notify();
  }, []);
  return { preference, setPreference };
}
