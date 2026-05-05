"use client";

import { useCallback, useEffect, useRef } from "react";
import { useTranslations } from "next-intl";
import { RouteHeading } from "@/components/layout/route-heading";
import { useAppStore, type BottomPanelMode } from "@/lib/store";
import { useShortcut } from "@/lib/shortcuts";
import { useUnsavedChangesGuard } from "@/hooks/use-unsaved-changes-guard";
import { useSelectionUrlSync } from "@/hooks/use-selection-url-sync";
import { KeyboardShortcut } from "@/components/ui/keyboard-shortcut";
import { OntologyCanvas } from "./canvas/ontology-canvas";
import { ExplorerPanel } from "./explorer/explorer-panel";
import { InspectorPanel } from "./inspector/inspector-panel";
import { BottomPanel } from "./bottom-panel/bottom-panel";
import { DesignPanel } from "./bottom-panel/design-panel";
import { SearchDialog } from "./search-dialog";
import { Plus, Search } from "lucide-react";
import { PanelLeft, PanelRight } from "lucide-react";
import { Group, Panel, usePanelRef } from "react-resizable-panels";
import { ResizeHandle } from "@/components/ui/resize-handle";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import type { QualityGap } from "@/types/api";
import { ScopeBadge } from "./design/scope-badge";
// ---------------------------------------------------------------------------
// Design layout — Explorer | Canvas | Inspector / Bottom Panel
// ---------------------------------------------------------------------------

export function DesignLayout() {
  const t = useTranslations("workbench.canvas.toolbar");
  const explorerOpen = useAppStore((s) => s.isExplorerOpen);
  const inspectorOpen = useAppStore((s) => s.isInspectorOpen);
  const toggleExplorer = useAppStore((s) => s.toggleExplorer);
  const toggleInspector = useAppStore((s) => s.toggleInspector);
  const ontology = useAppStore((s) => s.ontology);
  const activeProject = useAppStore((s) => s.activeProject);
  const applyProjectSnapshot = useAppStore((s) => s.applyProjectSnapshot);
  const hasUnsavedEdits = useAppStore((s) => s.commandStack.length > 0);
  const isBottomPanelOpen = useAppStore((s) => s.isBottomPanelOpen);
  const bottomPanelRef = usePanelRef();
  const initialTabSetRef = useRef(false);
  const searchOpen = useAppStore((s) => s.isSearchOpen);
  const setSearchOpen = useAppStore((s) => s.setSearchOpen);
  const closeSearch = useCallback(() => setSearchOpen(false), [setSearchOpen]);

  // Sync react-resizable-panels collapse/expand with store state.
  // `bottomPanelRef` is a panel ref whose `.current` is assigned by the
  // resizable-panels lib after mount; depending on the ref identity is
  // a no-op (refs are stable) but the lint insists — add it verbatim.
  useEffect(() => {
    const panel = bottomPanelRef.current;
    if (!panel) return;
    if (isBottomPanelOpen && panel.isCollapsed()) {
      panel.expand();
    } else if (!isBottomPanelOpen && !panel.isCollapsed()) {
      panel.collapse();
    }
  }, [isBottomPanelOpen, bottomPanelRef]);

  // Sync the ontology cache to the active project on mount and on
  // any subsequent activeProject change (e.g. cache invalidation
  // after save, post-refetch). `applyProjectSnapshot` replays the
  // commandStack on top of the new server snapshot when the project
  // id is unchanged so unsaved edits survive a refetch; switching
  // projects clears the stack atomically.
  useEffect(() => {
    applyProjectSnapshot(activeProject);
  }, [activeProject, applyProjectSnapshot]);

  useShortcut({
    id: "design.search",
    keys: ["mod+/"],
    group: "keyboardShortcuts.sections.workbench",
    description: "keyboardShortcuts.shortcuts.openCommandBar",
    handler: (e) => {
      e.preventDefault();
      const store = useAppStore.getState();
      store.setSearchOpen(!store.isSearchOpen);
    },
  });
  useShortcut({
    id: "design.cycleBottomPanel",
    keys: ["mod+\\"],
    group: "keyboardShortcuts.sections.workbench",
    description: "keyboardShortcuts.shortcuts.cycleBottomPanel",
    handler: (e) => {
      e.preventDefault();
      useAppStore.getState().cycleBottomPanelMode();
    },
  });
  useShortcut({
    id: "design.bottomPanelFullscreen",
    keys: ["mod+shift+\\"],
    group: "keyboardShortcuts.sections.workbench",
    description: "keyboardShortcuts.shortcuts.bottomPanelFullscreen",
    handler: (e) => {
      e.preventDefault();
      useAppStore.getState().setBottomPanelMode("fullscreen");
    },
  });
  useShortcut({
    id: "design.exitFullscreen",
    keys: ["Escape"],
    group: "keyboardShortcuts.sections.workbench",
    description: "keyboardShortcuts.shortcuts.exitFullscreen",
    priority: 10,
    enabled: () => useAppStore.getState().bottomPanelMode === "fullscreen",
    handler: () => {
      useAppStore.getState().setBottomPanelMode("default");
    },
  });

  useEffect(() => {
    if (initialTabSetRef.current) return;
    if (ontology === null && activeProject === null) {
      useAppStore.getState().setDesignBottomTab("workflow");
      initialTabSetRef.current = true;
    }
  }, [ontology, activeProject]);

  useUnsavedChangesGuard(hasUnsavedEdits);
  useSelectionUrlSync();

  const gaps: QualityGap[] = activeProject?.quality_report?.gaps ?? [];
  const hasContent = !!ontology;
  // Phase-aware top panel: when there is no ontology yet (analyse
  // phase, or no project at all) the canvas placeholder is just dead
  // space — the operator's actual work is the project-workflow review
  // (PII, clarifications, gates). Promote the workflow into the
  // primary pane and shrink the bottom panel to chat/quality only.
  // Once the design completes (`ontology !== null`), the layout
  // flips back: canvas is primary, the bottom panel surfaces the
  // workflow tab again.
  const showCanvas = hasContent;
  const bottomPanelMode = useAppStore((s) => s.bottomPanelMode);
  const setBottomPanelMode = useAppStore((s) => s.setBottomPanelMode);
  const isFullscreen = bottomPanelMode === "fullscreen";

  // Resolve the snap mode into concrete top/bottom percentages. Pure
  // function of `(showCanvas, bottomPanelMode)` — keeps the layout
  // stable across re-renders and easy to unit-test.
  const { topSize, bottomSize } = resolvePanelSizes(
    showCanvas,
    bottomPanelMode,
  );

  return (
    <>
      <RouteHeading route="design" />
      <Group orientation="vertical" className="h-full">
      {!isFullscreen && (
      <Panel defaultSize={topSize}>
        {showCanvas ? (
        <Group orientation="horizontal" className="h-full">
          {explorerOpen && hasContent && (
            <>
              <Panel defaultSize="18%" minSize="10%" maxSize="35%">
                <div className="flex h-full flex-col border-e border-divider">
                  <div className="flex h-7 items-center justify-between border-b border-divider px-2">
                    <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                      {t("paneExplorer")}
                    </span>
                    <button type="button" onClick={toggleExplorer} className="text-foreground-muted hover:text-foreground">
                      <PanelLeft className="h-3 w-3" />
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
                <button type="button"
                  onClick={toggleExplorer}
                  className="absolute start-2 top-2 z-canvas rounded-md border border-divider bg-surface-base p-1 shadow-1 hover:bg-surface-raised"
                  aria-label={t("showExplorer")}
                >
                  <PanelLeft className="h-3.5 w-3.5 text-foreground-muted" />
                </button>
              )}
              {hasContent && (
                <button type="button"
                  onClick={() => setSearchOpen(true)}
                  className="absolute start-1/2 top-2 z-canvas flex -translate-x-1/2 items-center gap-1.5 rounded-md border border-divider bg-surface-base px-2 py-1 shadow-1 hover:bg-surface-raised"
                  aria-label={t("searchAria")}
                >
                  <Search className="h-3 w-3 text-foreground-muted" />
                  <span className="text-2xs font-medium text-foreground-muted">{t("searchPlaceholder")}</span>
                  <KeyboardShortcut keys="mod+k" variant="outline" className="ms-1" />
                </button>
              )}
              {hasContent && activeProject && (
                <div className="absolute end-12 top-2 z-canvas flex items-center gap-1.5">
                  <ScopeBadge />
                  <button type="button"
                    onClick={() => {
                      const store = useAppStore.getState();
                      store.setDesignBottomTab("workflow");
                      if (!store.isBottomPanelOpen) store.toggleBottomPanel();
                      store.requestExtendSource();
                    }}
                    className="flex items-center gap-1 rounded-md border border-brand-border bg-brand-surface px-2 py-1 text-2xs font-medium text-brand-foreground shadow-1 hover:bg-brand-surface-strong/40"
                    aria-label={t("extendSourceAria")}
                    title={t("extendSourceAria")}
                  >
                    <Plus className="h-3 w-3" />
                    {t("extendSourceLabel")}
                  </button>
                </div>
              )}
              {!inspectorOpen && hasContent && (
                <button type="button"
                  onClick={toggleInspector}
                  className="absolute end-2 top-2 z-canvas rounded-md border border-divider bg-surface-base p-1 shadow-1 hover:bg-surface-raised"
                  aria-label={t("showInspector")}
                >
                  <PanelRight className="h-3.5 w-3.5 text-foreground-muted" />
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
                <div className="flex h-full flex-col border-s border-divider">
                  <div className="flex h-7 items-center justify-between border-b border-divider px-2">
                    <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                      {t("paneInspector")}
                    </span>
                    <button type="button" onClick={toggleInspector} className="text-foreground-muted hover:text-foreground">
                      <PanelRight className="h-3 w-3" />
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
        ) : (
          <ErrorBoundary name="ProjectReview">
            <DesignPanel />
          </ErrorBoundary>
        )}
      </Panel>
      )}

      {!isFullscreen && <ResizeHandle orientation="vertical" />}

      <Panel
        panelRef={bottomPanelRef}
        defaultSize={bottomSize}
        minSize="5%"
        maxSize="100%"
        collapsible
      >
        <ErrorBoundary name="BottomPanel">
          <BottomPanel
            mode={bottomPanelMode}
            onCycleMode={() => useAppStore.getState().cycleBottomPanelMode()}
            onExitFullscreen={() => setBottomPanelMode("default")}
          />
        </ErrorBoundary>
      </Panel>

      <SearchDialog open={searchOpen} onClose={closeSearch} />
    </Group>
    </>
  );
}

/**
 * Translate the snap-mode preference into concrete top/bottom panel
 * sizes. The numbers are deliberately conservative: `default` keeps
 * canvas as the primary read; `tall` flips the dominance to the
 * bottom panel for heavy review sessions; `fullscreen` is handled
 * outside this function (top panel unmounted).
 *
 * `showCanvas=false` (analyse phase) starts with a slightly larger
 * top panel because the workflow review is the actual work in that
 * phase — the bottom panel only carries chat/quality.
 */
function resolvePanelSizes(
  showCanvas: boolean,
  mode: BottomPanelMode,
): { topSize: string; bottomSize: string } {
  if (mode === "fullscreen") {
    return { topSize: "0%", bottomSize: "100%" };
  }
  if (mode === "tall") {
    return { topSize: "30%", bottomSize: "70%" };
  }
  return showCanvas
    ? { topSize: "60%", bottomSize: "40%" }
    : { topSize: "70%", bottomSize: "30%" };
}
