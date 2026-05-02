"use client";

// NarrowViewportBanner — explains the desktop-first stance to users
// who land on Ontosyx from a phone or tablet. The workbench (canvas
// + multi-pane inspector) is built for ≥1024px viewports; below that
// the layout still renders but feature density makes touch input
// frustrating. Rather than fight responsive on every panel, we
// surface a single non-blocking banner so the user knows what they
// are seeing and can dismiss it for the session.
//
// Persistence note: dismissal lives in `sessionStorage`, not local —
// re-showing on a fresh tab/visit is the right balance between
// "don't nag" and "don't strand a user who forgot the limitation".

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";

const STORAGE_KEY = "ontosyx.narrow-viewport-banner.dismissed";
const NARROW_BREAKPOINT_PX = 1024;

export function NarrowViewportBanner() {
  const t = useTranslations("narrowViewport");
  const [show, setShow] = useState(false);

  useEffect(() => {
    if (typeof window === "undefined") return;
    if (sessionStorage.getItem(STORAGE_KEY)) return;
    const evaluate = () => setShow(window.innerWidth < NARROW_BREAKPOINT_PX);
    evaluate();
    window.addEventListener("resize", evaluate);
    return () => window.removeEventListener("resize", evaluate);
  }, []);

  if (!show) return null;

  const dismiss = () => {
    sessionStorage.setItem(STORAGE_KEY, "1");
    setShow(false);
  };

  return (
    <div
      role="status"
      className="fixed inset-x-3 bottom-3 z-40 flex items-start gap-3 rounded-lg border border-warning-border bg-warning-surface px-4 py-3 text-xs shadow-lg lg:hidden"
    >
      <div className="flex-1">
        <p className="font-medium text-warning-foreground">{t("title")}</p>
        <p className="mt-0.5 text-warning-foreground/80">{t("description")}</p>
      </div>
      <button
        type="button"
        onClick={dismiss}
        className="rounded-md px-2 py-1 text-warning-foreground hover:bg-warning-surface-strong"
        aria-label={t("dismissLabel")}
      >
        ✕
      </button>
    </div>
  );
}
