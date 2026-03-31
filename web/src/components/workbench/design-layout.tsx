"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useAppStore } from "@/lib/store";
import { OntologyCanvas } from "./canvas/ontology-canvas";
import { ExplorerPanel } from "./explorer/explorer-panel";
import { InspectorPanel } from "./inspector/inspector-panel";
import { BottomPanel } from "./bottom-panel/bottom-panel";
import { SearchDialog } from "./search-dialog";
import { HugeiconsIcon } from "@hugeicons/react";
import { PanelLeftIcon, PanelRightIcon, Search01Icon } from "@hugeicons/core-free-icons";
import { Group, Panel, usePanelRef } from "react-resizable-panels";
import { ResizeHandle } from "@/components/ui/resize-handle";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import type { OntologyIR, QualityGap } from "@/types/api";

// ---------------------------------------------------------------------------
// Design layout — Explorer | Canvas | Inspector / Bottom Panel
// ---------------------------------------------------------------------------

export function DesignLayout() {
  const explorerOpen = useAppStore((s) => s.isExplorerOpen);
  const inspectorOpen = useAppStore((s) => s.isInspectorOpen);
  const toggleExplorer = useAppStore((s) => s.toggleExplorer);
  const toggleInspector = useAppStore((s) => s.toggleInspector);
  const ontology = useAppStore((s) => s.ontology);
  const activeProject = useAppStore((s) => s.activeProject);
  const setOntology = useAppStore((s) => s.setOntology);
  const hasUnsavedEdits = useAppStore((s) => s.commandStack.length > 0);
  const isBottomPanelOpen = useAppStore((s) => s.isBottomPanelOpen);
  const bottomPanelRef = usePanelRef();
  const initialTabSetRef = useRef(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const closeSearch = useCallback(() => setSearchOpen(false), []);

  // Sync react-resizable-panels collapse/expand with store state
  useEffect(() => {
    const panel = bottomPanelRef.current;
    if (!panel) return;
    if (isBottomPanelOpen && panel.isCollapsed()) {
      panel.expand();
    } else if (!isBottomPanelOpen && !panel.isCollapsed()) {
      panel.collapse();
    }
  }, [isBottomPanelOpen]);

  // Restore ontology from active project when entering Design mode.
  // If no active project, clear any orphaned ontology from other modes
  // (prevents "No project" header with visible ontology — confusing UX).
  useEffect(() => {
    if (activeProject?.ontology) {
      setOntology(activeProject.ontology as OntologyIR);
    } else {
      useAppStore.getState().resetOntology();
    }
    // Only restore on mount (when switching TO design mode)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Cmd+K / Ctrl+K to open search
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setSearchOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  useEffect(() => {
    if (initialTabSetRef.current) return;
    if (ontology === null && activeProject === null) {
      useAppStore.getState().setDesignBottomTab("workflow");
      initialTabSetRef.current = true;
    }
  }, [ontology, activeProject]);

  useEffect(() => {
    if (!hasUnsavedEdits) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      e.returnValue = "You have unsaved changes";
    };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [hasUnsavedEdits]);

  const gaps: QualityGap[] = activeProject?.quality_report?.gaps ?? [];
  const hasContent = !!ontology;

  return (
    <Group orientation="vertical" className="h-full">
      <Panel defaultSize={hasContent ? "60%" : "40%"}>
        <Group orientation="horizontal" className="h-full">
          {explorerOpen && hasContent && (
            <>
              <Panel defaultSize="18%" minSize="10%" maxSize="35%">
                <div className="flex h-full flex-col border-r border-zinc-200 dark:border-zinc-800">
                  <div className="flex h-7 items-center justify-between border-b border-zinc-200 px-2 dark:border-zinc-800">
                    <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                      Explorer
                    </span>
                    <button onClick={toggleExplorer} className="text-zinc-400 hover:text-zinc-600">
                      <HugeiconsIcon icon={PanelLeftIcon} className="h-3 w-3" size="100%" />
                    </button>
                  </div>
                  <div className="flex-1 overflow-hidden">
                    <ErrorBoundary name="Explorer">
                      <ExplorerPanel gaps={gaps} />
                    </ErrorBoundary>
                  </div>
                </div>
              </Panel>
              <ResizeHandle />
            </>
          )}

          <Panel minSize="30%">
            <div className="relative flex h-full flex-col overflow-hidden">
              {!explorerOpen && hasContent && (
                <button
                  onClick={toggleExplorer}
                  className="absolute left-2 top-2 z-10 rounded-md border border-zinc-200 bg-white p-1 shadow-sm hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900"
                  aria-label="Show Explorer"
                >
                  <HugeiconsIcon icon={PanelLeftIcon} className="h-3.5 w-3.5 text-zinc-500" size="100%" />
                </button>
              )}
              {hasContent && (
                <button
                  onClick={() => setSearchOpen(true)}
                  className="absolute left-1/2 top-2 z-10 flex -translate-x-1/2 items-center gap-1.5 rounded-md border border-zinc-200 bg-white px-2 py-1 shadow-sm hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900"
                  aria-label="Search graph entities"
                >
                  <HugeiconsIcon icon={Search01Icon} className="h-3 w-3 text-zinc-400" size="100%" />
                  <span className="text-[10px] font-medium text-zinc-400">Search...</span>
                  <kbd className="ml-1 rounded border border-zinc-300 px-1 text-[9px] text-zinc-400 dark:border-zinc-600">
                    {typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent) ? "\u2318" : "Ctrl+"}K
                  </kbd>
                </button>
              )}
              {!inspectorOpen && hasContent && (
                <button
                  onClick={toggleInspector}
                  className="absolute right-2 top-2 z-10 rounded-md border border-zinc-200 bg-white p-1 shadow-sm hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900"
                  aria-label="Show Inspector"
                >
                  <HugeiconsIcon icon={PanelRightIcon} className="h-3.5 w-3.5 text-zinc-500" size="100%" />
                </button>
              )}
              <ErrorBoundary name="Canvas">
                <OntologyCanvas gaps={gaps} />
              </ErrorBoundary>
            </div>
          </Panel>

          {inspectorOpen && hasContent && (
            <>
              <ResizeHandle />
              <Panel defaultSize="22%" minSize="15%" maxSize="40%">
                <div className="flex h-full flex-col border-l border-zinc-200 dark:border-zinc-800">
                  <div className="flex h-7 items-center justify-between border-b border-zinc-200 px-2 dark:border-zinc-800">
                    <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                      Inspector
                    </span>
                    <button onClick={toggleInspector} className="text-zinc-400 hover:text-zinc-600">
                      <HugeiconsIcon icon={PanelRightIcon} className="h-3 w-3" size="100%" />
                    </button>
                  </div>
                  <div className="flex-1 overflow-y-auto">
                    <ErrorBoundary name="Inspector">
                      <InspectorPanel gaps={gaps} />
                    </ErrorBoundary>
                  </div>
                </div>
              </Panel>
            </>
          )}
        </Group>
      </Panel>

      <ResizeHandle orientation="vertical" />

      <Panel panelRef={bottomPanelRef} defaultSize="40%" minSize="5%" maxSize="70%" collapsible>
        <ErrorBoundary name="BottomPanel">
          <BottomPanel />
        </ErrorBoundary>
      </Panel>

      <SearchDialog open={searchOpen} onClose={closeSearch} />
    </Group>
  );
}
