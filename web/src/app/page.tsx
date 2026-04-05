"use client";

import { Sidebar } from "@/components/layout/sidebar";
import { Header } from "@/components/layout/header";
import { WorkbenchLayout } from "@/components/workbench/workbench-layout";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import { PromptProvider } from "@/components/ui/prompt-dialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useHydrated } from "@/lib/store/use-hydrated";
import { useAppStore } from "@/lib/store";
import { useEffect } from "react";

export default function Home() {
  const hydrated = useHydrated();
  const workspaceReady = useAppStore((s) => s.workspaceReady);
  const initWorkspace = useAppStore((s) => s.initWorkspace);

  // Initialize workspace after Zustand hydration
  useEffect(() => {
    if (hydrated && !workspaceReady) {
      initWorkspace();
    }
  }, [hydrated, workspaceReady, initWorkspace]);

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
      </TooltipProvider>
    </ErrorBoundary>
  );
}
