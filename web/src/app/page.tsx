"use client";

import { Sidebar } from "@/components/layout/sidebar";
import { Header } from "@/components/layout/header";
import { WorkbenchLayout } from "@/components/workbench/workbench-layout";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import { ConfirmProvider } from "@/components/ui/confirm-dialog";
import { PromptProvider } from "@/components/ui/prompt-dialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useHydrated } from "@/lib/store/use-hydrated";

export default function Home() {
  // Wait for Zustand persist hydration before rendering mode-dependent UI.
  // Without this, the initial render uses default state (workspaceMode: "design")
  // which causes a visible flash before hydrating to the actual persisted mode.
  const hydrated = useHydrated();

  if (!hydrated) {
    // Minimal skeleton — matches the layout structure to prevent layout shift
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
        <ConfirmProvider>
        <PromptProvider>
          <div className="flex h-dvh overflow-hidden">
            <Sidebar />
            <div className="flex flex-1 flex-col overflow-hidden">
              <Header />
              <main className="flex-1 overflow-hidden">
                <WorkbenchLayout />
              </main>
            </div>
          </div>
        </PromptProvider>
        </ConfirmProvider>
      </TooltipProvider>
    </ErrorBoundary>
  );
}
