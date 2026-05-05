"use client";

// `useUnsavedChangesGuard` — block navigation while a workspace has
// pending edits.
//
// Next.js App Router has no `routeChangeStart` event (the legacy
// pages-router escape hatch is gone), so the guard runs in two
// stages:
//
// 1. **`beforeunload`** — covers tab close, window close, manual URL
//    edit. The browser-native dialog is the only surface that can
//    actually abort navigation at this layer; we can't replace it
//    with a styled modal there.
//
// 2. **document-level click capture** — intercepts every internal
//    anchor before the router sees it, runs the styled confirm modal
//    (`useConfirm`), and replays the navigation if the user confirms.
//    External links (`http(s)://`, `mailto:`, `target="_blank"`,
//    download anchors) skip the guard because the SPA isn't going
//    to lose state from those.
//
// The hook is a no-op when `dirty` is false. Toggling it off cleans
// up listeners synchronously so a save → navigate sequence doesn't
// leave a stale handler behind.

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useConfirm } from "@/components/providers/confirm-provider";

export function useUnsavedChangesGuard(dirty: boolean): void {
  const router = useRouter();
  const confirm = useConfirm();
  const t = useTranslations("workbench.unsavedChanges");

  useEffect(() => {
    if (!dirty) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      // Modern browsers ignore the returnValue copy for security;
      // setting it is still required to trigger the dialog at all.
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [dirty]);

  useEffect(() => {
    if (!dirty) return;
    const onClick = async (e: MouseEvent) => {
      // Modifier-clicks (cmd / shift / middle button) open in a new
      // tab and don't lose state — let them through.
      if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.button !== 0) {
        return;
      }
      const target = e.target;
      if (!(target instanceof Element)) return;
      const anchor = target.closest("a[href]") as HTMLAnchorElement | null;
      if (!anchor) return;
      if (anchor.target === "_blank") return;
      if (anchor.hasAttribute("download")) return;
      const href = anchor.getAttribute("href");
      if (!href) return;
      if (href.startsWith("http://") || href.startsWith("https://")) {
        // External — `beforeunload` fires when the browser navigates
        // away, no need to double-prompt here.
        return;
      }
      if (href.startsWith("#") || href.startsWith("mailto:") || href.startsWith("tel:")) {
        return;
      }
      // Internal SPA navigation — pause the browser, prompt, then
      // either replay through `router.push` or cancel.
      e.preventDefault();
      e.stopPropagation();
      const confirmed = await confirm({
        title: t("title"),
        description: t("description"),
        confirmLabel: t("leave"),
        cancelLabel: t("stay"),
        variant: "warning",
      });
      if (confirmed) router.push(href);
    };
    // Capture phase so we run before `<Link>`'s own click handler.
    document.addEventListener("click", onClick, true);
    return () => document.removeEventListener("click", onClick, true);
  }, [dirty, confirm, router, t]);
}
