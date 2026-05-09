"use client";

import { useEffect, type ReactNode } from "react";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import { cn } from "@/lib/cn";

/**
 * Persistent sidebar shell with off-canvas behaviour below the `md`
 * breakpoint. Above 768px the children render in flow exactly like
 * a static sidebar; below it the wrapper anchors to the inline-start
 * edge, slides in/out via `translate-x`, and lays a backdrop button
 * over the canvas so the user can dismiss by tap or `Escape`.
 *
 * Route changes auto-close the drawer — sidebar nav links should
 * never strand the user behind a backdrop after navigation.
 */
export function MobileNavRoot({ children }: { children: ReactNode }) {
  const open = useAppStore((s) => s.isMobileNavOpen);
  const setOpen = useAppStore((s) => s.setMobileNavOpen);
  const pathname = usePathname();
  const t = useTranslations("chrome.sidebar");

  // biome-ignore lint/correctness/useExhaustiveDependencies: pathname drives auto-close on navigation
  useEffect(() => {
    setOpen(false);
  }, [pathname, setOpen]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  return (
    <>
      <div
        className={cn(
          "fixed inset-y-0 start-0 z-modal flex md:relative md:z-auto md:translate-x-0",
          "transition-transform duration-[var(--duration-base)] ease-[var(--ease-out)]",
          open ? "translate-x-0" : "-translate-x-full md:translate-x-0",
        )}
      >
        {children}
      </div>
      {open && (
        <button
          type="button"
          onClick={() => setOpen(false)}
          aria-label={t("closeMobileNav")}
          className="fixed inset-0 z-overlay bg-surface-overlay/60 backdrop-blur-sm md:hidden"
        />
      )}
    </>
  );
}
