"use client";

import { useState } from "react";
import { useAppStore } from "@/lib/store";
import type { AnalyzeRightTab } from "@/lib/store";
import { ChatPanel } from "@/components/chat/chat-panel";
import { HistoryPanel } from "@/components/chat/history-panel";
import { QueryPanel } from "@/components/chat/query-panel";
import { Group, Panel } from "react-resizable-panels";
import { ResizeHandle } from "@/components/ui/resize-handle";
import { TabBar } from "@/components/ui/tab-bar";
import {
  Message01Icon,
  Clock01Icon,
  CommandLineIcon,
  Analytics01Icon,
  BookOpen01Icon,
} from "@hugeicons/core-free-icons";
import { SessionBar } from "@/components/workbench/analyze/session-bar";
import { AnalyzeResultsPanel } from "@/components/workbench/analyze/analyze-results-panel";
import { QueryBuilder } from "@/components/workbench/analyze/query-builder/query-builder";
import { InsightsPanel } from "@/components/recipes/insights-panel";
import { KnowledgePanel } from "@/components/workbench/analyze/knowledge-panel";
import { ErrorBoundary } from "@/components/ui/error-boundary";

// ---------------------------------------------------------------------------
// Analyze layout — Chat (left) | Results (right) OR Query Builder (full)
// ---------------------------------------------------------------------------

type AnalyzeMode = "chat" | "builder";

const ANALYZE_TABS: Array<{ id: AnalyzeRightTab; label: string; icon: import("@hugeicons/react").IconSvgElement }> = [
  { id: "results", label: "Results", icon: Message01Icon },
  { id: "query", label: "Query", icon: CommandLineIcon },
  { id: "history", label: "History", icon: Clock01Icon },
  { id: "insights", label: "Insights", icon: Analytics01Icon },
  { id: "knowledge", label: "Knowledge", icon: BookOpen01Icon },
];

export function AnalyzeLayout() {
  const rightTab = useAppStore((s) => s.analyzeRightTab);
  const setRightTab = useAppStore((s) => s.setAnalyzeRightTab);
  const [analyzeMode, setAnalyzeMode] = useState<AnalyzeMode>("chat");

  return (
    <ErrorBoundary name="Analyze">
      <div className="flex h-full flex-col">
        {/* Mode toggle bar */}
        <div className="flex h-8 shrink-0 items-center gap-1 border-b border-zinc-200 px-3 dark:border-zinc-800">
          <button
            onClick={() => setAnalyzeMode("chat")}
            className={`rounded px-2.5 py-1 text-[11px] font-medium transition-colors ${
              analyzeMode === "chat"
                ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-400"
                : "text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-300"
            }`}
          >
            Chat
          </button>
          <button
            onClick={() => setAnalyzeMode("builder")}
            className={`rounded px-2.5 py-1 text-[11px] font-medium transition-colors ${
              analyzeMode === "builder"
                ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-400"
                : "text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-300"
            }`}
          >
            Query Builder
          </button>
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
                <div className="flex h-full flex-col border-l border-zinc-200 dark:border-zinc-800">
                  {/* Tab bar */}
                  <div className="flex h-8 shrink-0 items-center border-b border-zinc-200 px-1 dark:border-zinc-800">
                    <TabBar
                      tabs={ANALYZE_TABS}
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
