"use client";

import { useCallback, useEffect } from "react";
import { Sidebar } from "@/components/layout/sidebar";
import { Header } from "@/components/layout/header";
import { GlobalCommandPalette } from "@/components/layout/global-command-palette";
import { KeyboardShortcutsDialog } from "@/components/ui/keyboard-shortcuts-dialog";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import { PromptProvider } from "@/components/providers/prompt-provider";
import { QualityBanner } from "@/components/quality/quality-banner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useHydrated } from "@/lib/store/use-hydrated";
import { useAppStore } from "@/lib/store";
import { fetchWsToken, useCollab } from "@/lib/collab";

// `NEXT_PUBLIC_WS_URL` lets ops point the workbench at a different
// host than the HTTP API (e.g. a dedicated WS-fanout pod). The dev
// default mirrors `dev.sh`'s backend port.
const COLLAB_WS_URL =
  process.env.NEXT_PUBLIC_WS_URL ?? "ws://localhost:3101/ws/collab";

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
  const workspaceId = useAppStore((s) => s.workspaceId);
  const initWorkspace = useAppStore((s) => s.initWorkspace);
  const isCommandPaletteOpen = useAppStore((s) => s.isCommandPaletteOpen);
  const setCommandPaletteOpen = useAppStore((s) => s.setCommandPaletteOpen);

  // Collaboration WebSocket — single socket per workspace, shared
  // across every workbench mode. The hook tears the socket down
  // automatically when `workspaceId` clears or switches.
  useCollab({
    url: COLLAB_WS_URL,
    workspaceId: workspaceId ?? "",
    getToken: fetchWsToken,
  });
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
                  <main id="main" className="flex-1 overflow-hidden">
                    <div className="h-full overflow-hidden">{children}</div>
                  </main>
                </div>
              </>
            ) : (
              <>
                <div
                  className="w-12 shrink-0 border-r border-divider"
                  aria-hidden
                />
                <div className="flex flex-1 flex-col overflow-hidden">
                  <div
                    className="h-10 shrink-0 border-b border-divider"
                    aria-hidden
                  />
                  <main
                    id="main"
                    className="flex-1 overflow-hidden"
                    aria-hidden
                  />
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
