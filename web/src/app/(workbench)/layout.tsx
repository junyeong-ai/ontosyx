"use client";

import { useCallback, useEffect } from "react";
import { Sidebar } from "@/components/layout/sidebar";
import { Header } from "@/components/layout/header";
import { GlobalCommandPalette } from "@/components/layout/global-command-palette";
import { KeyboardShortcutsDialog } from "@/components/ui/keyboard-shortcuts";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import { PromptProvider } from "@/components/ui/prompt-dialog";
import { QualityBanner } from "@/components/quality/quality-banner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useHydrated } from "@/lib/store/use-hydrated";
import { useAppStore } from "@/lib/store";

/**
 * Shared shell for every workspace mode. Each mode owns its own
 * route segment under this group; the shell renders Sidebar +
 * Header + chrome and each segment's `page.tsx` fills the `<main>`.
 *
 * SSR / hydration contract: the provider tree and outer flex shell
 * render unconditionally so the React tree shape is identical across
 * server render, first client render, and post-hydration re-render.
 * Only the inner chrome differs between skeleton and live state, and
 * that swap happens inside the stable wrapper — returning two
 * structurally different roots would put `loading.tsx`'s implicit
 * Suspense at a different tree position on server vs. client.
 */
export default function WorkbenchLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const hydrated = useHydrated();
  const workspaceReady = useAppStore((s) => s.workspaceReady);
  const initWorkspace = useAppStore((s) => s.initWorkspace);
  const isCommandPaletteOpen = useAppStore((s) => s.isCommandPaletteOpen);
  const setCommandPaletteOpen = useAppStore((s) => s.setCommandPaletteOpen);
  const closePalette = useCallback(
    () => setCommandPaletteOpen(false),
    [setCommandPaletteOpen],
  );

  // Initialize workspace after Zustand hydration — same bootstrap the
  // old `page.tsx` performed.
  useEffect(() => {
    if (hydrated && !workspaceReady) {
      initWorkspace();
    }
  }, [hydrated, workspaceReady, initWorkspace]);

  // Cmd/Ctrl+Shift+P opens the global command palette. Bound at the
  // workbench shell so every mode (design / analyze / explore /
  // dashboard) shares the same shortcut. The dialog itself owns
  // ESC handling internally.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      if (meta && e.shiftKey && (e.key === "p" || e.key === "P")) {
        e.preventDefault();
        const store = useAppStore.getState();
        store.setCommandPaletteOpen(!store.isCommandPaletteOpen);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  return (
    <ErrorBoundary>
      <TooltipProvider>
        <PromptProvider>
          <div className="flex h-dvh overflow-hidden" aria-busy={!hydrated}>
            {hydrated ? (
              <>
                <Sidebar />
                <div className="flex flex-1 flex-col overflow-hidden">
                  <Header />
                  {workspaceReady && <QualityBanner />}
                  <main className="flex-1 overflow-hidden">
                    <div className="h-full overflow-hidden">{children}</div>
                  </main>
                </div>
              </>
            ) : (
              <>
                <div
                  className="w-12 shrink-0 border-r border-zinc-200 dark:border-zinc-800"
                  aria-hidden
                />
                <div className="flex flex-1 flex-col overflow-hidden">
                  <div
                    className="h-10 shrink-0 border-b border-zinc-200 dark:border-zinc-800"
                    aria-hidden
                  />
                  <main className="flex-1 overflow-hidden" aria-hidden />
                </div>
              </>
            )}
          </div>
          <KeyboardShortcutsDialog />
          <GlobalCommandPalette
            open={isCommandPaletteOpen}
            onClose={closePalette}
          />
        </PromptProvider>
      </TooltipProvider>
    </ErrorBoundary>
  );
}
