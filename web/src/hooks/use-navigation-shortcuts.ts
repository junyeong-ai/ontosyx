"use client";

import { useEffect, useRef } from "react";
import { useRouter } from "next/navigation";
import { NAVIGATION_SHORTCUTS } from "@/lib/navigation-shortcuts";

const SEQUENCE_TIMEOUT_MS = 1000;

// `Map<string, string>` (not the narrowed key/href literal types) so a
// runtime `event.key` can be queried without casting. The membership
// check is `Map.get` returning `undefined` for unknown keys — exactly
// the runtime semantic we want.
const SHORTCUT_MAP = new Map<string, string>(
  NAVIGATION_SHORTCUTS.map((s) => [s.key, s.href]),
);

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  return false;
}

/**
 * Installs the `g + key` two-key navigation sequence (Linear / GitHub
 * pattern). Press `g`, then within ~1s press a second key matching one
 * of the entries in `NAVIGATION_SHORTCUTS`. Sequences are silently
 * dropped while the user is typing into a text field or while any
 * modifier (Cmd/Ctrl/Alt/Shift) is held — keeping the affordance from
 * stealing keys from the browser, the OS, or the user's prose.
 */
export function useNavigationShortcuts() {
  const router = useRouter();
  const armedRef = useRef<number | null>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) {
        return;
      }
      if (isTypingTarget(event.target)) return;

      const key = event.key.toLowerCase();

      if (armedRef.current !== null) {
        const href = SHORTCUT_MAP.get(key);
        window.clearTimeout(armedRef.current);
        armedRef.current = null;
        if (href) {
          event.preventDefault();
          router.push(href);
        }
        return;
      }

      if (key === "g") {
        armedRef.current = window.setTimeout(() => {
          armedRef.current = null;
        }, SEQUENCE_TIMEOUT_MS);
      }
    };

    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      if (armedRef.current !== null) {
        window.clearTimeout(armedRef.current);
        armedRef.current = null;
      }
    };
  }, [router]);
}
