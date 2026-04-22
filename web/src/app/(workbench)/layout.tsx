"use client";

import { useEffect } from "react";
import { Sidebar } from "@/components/layout/sidebar";
import { Header } from "@/components/layout/header";
import { KeyboardShortcutsDialog } from "@/components/ui/keyboard-shortcuts";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import { PromptProvider } from "@/components/ui/prompt-dialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useHydrated } from "@/lib/store/use-hydrated";
import { useAppStore } from "@/lib/store";

/**
 * Shared shell for every workspace mode (design / analyze / explore /
 * dashboard). Each mode lives in its own route segment under this
 * group — the shell renders the Sidebar + Header + the mode-specific
 * page chrome, and each segment's `page.tsx` fills the `<main>` slot.
 *
 * Moved out of `src/app/page.tsx` so the URL, not Zustand, is the
 * source of truth for the active mode (Phase 2-4).
 */
export default function WorkbenchLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const hydrated = useHydrated();
  const workspaceReady = useAppStore((s) => s.workspaceReady);
  const initWorkspace = useAppStore((s) => s.initWorkspace);

  // Initialize workspace after Zustand hydration — same bootstrap the
  // old `page.tsx` performed.
  useEffect(() => {
    if (hydrated && !workspaceReady) {
      initWorkspace();
    }
  }, [hydrated, workspaceReady, initWorkspace]);

  if (!hydrated) {
    // Skeleton matches the live layout's structural chrome so the
    // hydration swap doesn't reflow.
    return (
      <div className="flex h-dvh overflow-hidden bg-white dark:bg-zinc-950">
        <div className="w-12 shrink-0 border-r border-zinc-200 dark:border-zinc-800" />
        <div className="flex flex-1 flex-col overflow-hidden">
          <div className="h-10 shrink-0 border-b border-zinc-200 dark:border-zinc-800" />
          <main className="flex-1" />
        </div>
      </div>
    );
  }

  return (
    <ErrorBoundary>
      <TooltipProvider>
        <PromptProvider>
          <div className="flex h-dvh overflow-hidden">
            <Sidebar />
            <div className="flex flex-1 flex-col overflow-hidden">
              <Header />
              <main className="flex-1 overflow-hidden">
                <div className="h-full overflow-hidden">{children}</div>
              </main>
            </div>
          </div>
          <KeyboardShortcutsDialog />
        </PromptProvider>
      </TooltipProvider>
    </ErrorBoundary>
  );
}
