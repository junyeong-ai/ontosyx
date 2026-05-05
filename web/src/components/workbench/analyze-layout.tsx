"use client";

import { useEffect } from "react";
import { z } from "zod";
import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import type { AnalyzeRightTab } from "@/lib/store";
import { useQueryState } from "@/hooks/use-query-state";
import { ChatPanel } from "@/components/chat/chat-panel";
import { HistoryPanel } from "@/components/chat/history-panel";
import { QueryPanel } from "@/components/chat/query-panel";
import { Group, Panel } from "react-resizable-panels";
import { ResizeHandle } from "@/components/ui/resize-handle";
import { TabBar } from "@/components/ui/tab-bar";
import { BarChart3, BookOpen, Clock, MessageCircle, Terminal } from "lucide-react";
import { SessionBar } from "@/components/workbench/analyze/session-bar";
import { AnalyzeResultsPanel } from "@/components/workbench/analyze/analyze-results-panel";
import { QueryBuilder } from "@/components/workbench/analyze/query-builder/query-builder";
import { InsightsPanel } from "@/components/recipes/insights-panel";
import { KnowledgePanel } from "@/components/workbench/analyze/knowledge-panel";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import { RouteHeading } from "@/components/layout/route-heading";

// ---------------------------------------------------------------------------
// Analyze layout — Chat (left) | Results (right) OR Query Builder (full)
// ---------------------------------------------------------------------------

type AnalyzeMode = "chat" | "builder";

// Tab id ↔ icon binding stays static; the human-readable label is
// resolved through the translator at render time so a locale switch
// updates the tab bar without rerunning module code.
const ANALYZE_TAB_ICONS: Array<{
  id: AnalyzeRightTab;
  icon: import("lucide-react").LucideIcon;
}> = [
  { id: "results", icon: MessageCircle },
  { id: "query", icon: Terminal },
  { id: "history", icon: Clock },
  { id: "insights", icon: BarChart3 },
  { id: "knowledge", icon: BookOpen },
];

export function AnalyzeLayout() {
  const t = useTranslations("workbench.analyze");
  const rightTab = useAppStore((s) => s.analyzeRightTab);
  const setRightTab = useAppStore((s) => s.setAnalyzeRightTab);
  const pinnedOntologyId = useAppStore((s) => s.ontologyId);
  const setOntologyId = useAppStore((s) => s.setOntologyId);
  const activeProjectOntologyId = useAppStore(
    (s) => s.activeProject?.ontology?.id ?? null,
  );
  const analyzeTabs = ANALYZE_TAB_ICONS.map((row) => ({
    ...row,
    label: t(`tab.${row.id}`),
  }));
  // URL-backed so "Chat vs Query Builder" + a specific result pane survive
  // reloads and can be shared (`?analyze=builder`).
  const [analyzeMode, setAnalyzeMode] = useQueryState<AnalyzeMode>("analyze", {
    default: "chat",
    parser: z.enum(["chat", "builder"]),
    debounceMs: 0,
  });

  // Auto-pin the Design-mode active ontology when the user crosses
  // into Analyze with nothing pinned. The chat panel reads
  // `ontologyId` to scope NL→Cypher prompts; without this effect a
  // designer who clicks "Analyze" right after saving an ontology
  // sees an empty chat with no context attached. The pin is a
  // user-visible header pill, so the user can still unpin / change
  // it manually after auto-attach.
  useEffect(() => {
    if (!pinnedOntologyId && activeProjectOntologyId) {
      setOntologyId(activeProjectOntologyId);
    }
  }, [pinnedOntologyId, activeProjectOntologyId, setOntologyId]);

  return (
    <ErrorBoundary name="Analyze">
      <div className="flex h-full flex-col">
        <RouteHeading route="analyze" />
        <div className="flex h-9 shrink-0 items-center border-b border-divider px-3">
          <TabBar
            tabs={[
              { id: "chat", label: t("mode.chat") },
              { id: "builder", label: t("mode.builder") },
            ]}
            activeTab={analyzeMode}
            onTabChange={(id) => setAnalyzeMode(id as AnalyzeMode)}
          />
        </div>

        {/* Content */}
        <div className="flex-1 overflow-hidden">
          {analyzeMode === "builder" ? (
            <QueryBuilder />
          ) : (
            <Group orientation="horizontal" className="h-full">
              {/* Left: Session list + Chat */}
              <Panel defaultSize="40%" minSize="25%" maxSize="60%">
                <div className="flex h-full flex-col">
                  <SessionBar />
                  <div className="flex-1 overflow-hidden">
                    <ChatPanel />
                  </div>
                </div>
              </Panel>

              <ResizeHandle />

              {/* Right: Results / Query / History */}
              <Panel minSize="30%">
                <div className="flex h-full flex-col border-s border-divider">
                  {/* Tab bar */}
                  <div className="flex h-8 shrink-0 items-center border-b border-divider px-1">
                    <TabBar
                      tabs={analyzeTabs}
                      activeTab={rightTab}
                      onTabChange={(id) => setRightTab(id as AnalyzeRightTab)}
                    />
                  </div>

                  {/* Content */}
                  <div className="flex-1 overflow-hidden">
                    {rightTab === "results" && <AnalyzeResultsPanel />}
                    {rightTab === "query" && <QueryPanel />}
                    {rightTab === "history" && <HistoryPanel />}
                    {rightTab === "insights" && <InsightsPanel />}
                    {rightTab === "knowledge" && <KnowledgePanel />}
                  </div>
                </div>
              </Panel>
            </Group>
          )}
        </div>
      </div>
    </ErrorBoundary>
  );
}
