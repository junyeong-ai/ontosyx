"use client";

import { useEffect } from "react";

/**
 * Vim-style keyboard navigation for the analysis review surface.
 *
 * `J` advances to the next visible review-section anchor (warnings →
 * relationships → exclusions → pii → clarifications); `K` reverses.
 * The hook honours the standard input-focus guard — keystrokes inside
 * `<input>`, `<textarea>`, `[contenteditable]` are passed through so
 * typing never collides with navigation.
 *
 * `anchors` is the ordered list the parent renders into the DOM.
 * Anchors that have no matching element at the moment of dispatch are
 * skipped, so a section that's hidden because its underlying data is
 * empty does not strand the cursor.
 */
export function useReviewKeyboardNav(anchors: ReadonlyArray<string>) {
  useEffect(() => {
    if (anchors.length === 0) return;

    const handler = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key !== "j" && e.key !== "k") return;

      const target = e.target as HTMLElement | null;
      if (target && isEditableTarget(target)) return;

      e.preventDefault();
      const direction = e.key === "j" ? 1 : -1;
      const visible = anchors.filter((id) =>
        typeof document !== "undefined" ? document.getElementById(id) : null,
      );
      if (visible.length === 0) return;
      const currentIndex = activeAnchorIndex(visible);
      const nextIndex =
        (currentIndex + direction + visible.length) % visible.length;
      focusAnchor(visible[nextIndex]);
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [anchors]);
}

function isEditableTarget(el: HTMLElement): boolean {
  if (el.isContentEditable) return true;
  const tag = el.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

function activeAnchorIndex(anchors: ReadonlyArray<string>): number {
  if (typeof document === "undefined") return -1;
  // The "active" section is the topmost anchor whose element is at or
  // above the viewport's mid-line. Falls back to -1 (so `J` lands on
  // index 0) when the user is above every section.
  const mid = window.innerHeight / 2;
  let bestIdx = -1;
  let bestTop = Number.NEGATIVE_INFINITY;
  for (let i = 0; i < anchors.length; i++) {
    const el = document.getElementById(anchors[i]);
    if (!el) continue;
    const top = el.getBoundingClientRect().top;
    if (top <= mid && top > bestTop) {
      bestTop = top;
      bestIdx = i;
    }
  }
  return bestIdx;
}

function focusAnchor(anchor: string): void {
  if (typeof document === "undefined") return;
  const el = document.getElementById(anchor);
  if (!el) return;
  el.scrollIntoView({ behavior: "smooth", block: "start" });
  el.classList.add("ring-2", "ring-emerald-300", "ring-offset-2");
  window.setTimeout(() => {
    el.classList.remove("ring-2", "ring-emerald-300", "ring-offset-2");
  }, 800);
}
