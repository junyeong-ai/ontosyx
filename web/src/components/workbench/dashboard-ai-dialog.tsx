"use client";

import { useCallback, useRef, useState } from "react";
import { useAppStore } from "@/lib/store";
import { chatStream, addWidget, type StreamCallbacks } from "@/lib/api";
import type { DashboardWidget, OntologyIR, QueryResult, WidgetSpec } from "@/types/api";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  AiNetworkIcon,
  Cancel01Icon,
  ChartLineData02Icon,
  PlusSignIcon,
  ArrowRight01Icon,
} from "@hugeicons/core-free-icons";
import { Spinner } from "@/components/ui/spinner";
import { toast } from "sonner";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface DashboardAiDialogProps {
  isOpen: boolean;
  onClose: () => void;
  dashboardId: string;
  onWidgetAdded: (widget: DashboardWidget) => void;
}

/** Parsed widget preview extracted from a visualize tool_complete event */
interface WidgetPreview {
  id: string;
  chartType: string;
  title: string;
  query?: string;
  spec: WidgetSpec;
  data?: QueryResult;
  isAdding: boolean;
  isAdded: boolean;
}

// ---------------------------------------------------------------------------
// DashboardAiDialog — slide-over panel for inline AI widget generation
// ---------------------------------------------------------------------------

export function DashboardAiDialog({
  isOpen,
  onClose,
  dashboardId,
  onWidgetAdded,
}: DashboardAiDialogProps) {
  const ontology = useAppStore((s) => s.ontology);

  // Local state only — not global ChatSlice
  const [input, setInput] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [statusText, setStatusText] = useState("");
  const [previews, setPreviews] = useState<WidgetPreview[]>([]);
  const [sessionId, setSessionId] = useState<string | undefined>(undefined);

  // Track the most recent query_graph Cypher so we can attach it to visualize previews
  const lastCypherRef = useRef<string | undefined>(undefined);
  const scrollRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = useCallback(() => {
    requestAnimationFrame(() => {
      scrollRef.current?.scrollTo({
        top: scrollRef.current.scrollHeight,
        behavior: "smooth",
      });
    });
  }, []);

  // --------------------------------------------------
  // Parse visualize tool output into a WidgetPreview
  // --------------------------------------------------
  const parseVisualizeOutput = useCallback(
    (toolId: string, output: string): WidgetPreview | null => {
      try {
        const parsed = JSON.parse(output);
        if (!parsed.chart_type) return null;

        const spec: WidgetSpec = {
          widget: parsed.chart_type,
          widget_type: parsed.chart_type,
          title: parsed.title,
          x_axis: parsed.x_axis,
          y_axis: parsed.y_axis,
          data_mapping: parsed.data_mapping,
          columns: parsed.columns?.map((c: string) => ({ key: c, label: c })),
        };

        const data: QueryResult | undefined =
          parsed.data && parsed.columns
            ? { columns: parsed.columns, rows: parsed.data }
            : undefined;

        return {
          id: toolId,
          chartType: parsed.chart_type,
          title: parsed.title ?? "Untitled Widget",
          query: parsed.query,
          spec,
          data,
          isAdding: false,
          isAdded: false,
        };
      } catch {
        return null;
      }
    },
    [],
  );

  // --------------------------------------------------
  // Send message to AI
  // --------------------------------------------------
  const handleSend = useCallback(async () => {
    const trimmed = input.trim();
    if (!trimmed || !ontology || isStreaming) return;

    setInput("");
    setIsStreaming(true);
    setStatusText("Thinking...");
    setPreviews([]);
    lastCypherRef.current = undefined;

    const callbacks: StreamCallbacks = {
      onText: () => {
        setStatusText("Generating widgets...");
      },
      onToolStart: (event) => {
        if (event.name === "visualize") {
          setStatusText("Creating visualization...");
        } else if (event.name === "query_graph") {
          setStatusText("Querying graph...");
        } else {
          setStatusText(`Running ${event.name}...`);
        }
      },
      onToolComplete: (event) => {
        // Capture Cypher query from query_graph so we can attach it to the next visualize
        if (event.name === "query_graph" && !event.is_error) {
          try {
            const parsed = JSON.parse(event.output);
            if (parsed.compiled_query) {
              lastCypherRef.current = parsed.compiled_query;
            }
          } catch {
            // ignore parse errors
          }
        }
        if (event.name === "visualize" && !event.is_error) {
          const preview = parseVisualizeOutput(event.id, event.output);
          if (preview) {
            // If the visualize output had no query, use the last captured Cypher
            if (!preview.query && lastCypherRef.current) {
              preview.query = lastCypherRef.current;
            }
            setPreviews((prev) => [...prev, preview]);
            scrollToBottom();
          }
        }
      },
      onComplete: (event) => {
        setSessionId(event.session_id);
        setIsStreaming(false);
        setStatusText("");
      },
      onError: (error) => {
        setIsStreaming(false);
        setStatusText("");
        toast.error(error);
      },
    };

    const prompt =
      `Generate dashboard widgets for: ${trimmed}\n\n` +
      `Use the visualize tool to create each widget. ` +
      `Include the Cypher query in each visualization. ` +
      `Prefer chart types: bar_chart, line_chart, pie_chart, stat_card, table.`;

    await chatStream(
      {
        message: prompt,
        ontology: ontology as OntologyIR,
        session_id: sessionId,
      },
      callbacks,
    );
  }, [input, ontology, isStreaming, sessionId, parseVisualizeOutput, scrollToBottom]);

  // --------------------------------------------------
  // Add a previewed widget to the dashboard
  // --------------------------------------------------
  const handleAddWidget = useCallback(
    async (preview: WidgetPreview) => {
      setPreviews((prev) =>
        prev.map((p) => (p.id === preview.id ? { ...p, isAdding: true } : p)),
      );

      try {
        const widget = await addWidget(dashboardId, {
          title: preview.title,
          widget_type: preview.chartType,
          query: preview.query,
          widget_spec: preview.spec as unknown as Record<string, unknown>,
          position: {
            x: (previews.filter((p) => p.isAdded).length % 2) * 6,
            y: Math.floor(previews.filter((p) => p.isAdded).length / 2) * 4,
            w: 6,
            h: 4,
          },
        });

        setPreviews((prev) =>
          prev.map((p) =>
            p.id === preview.id ? { ...p, isAdding: false, isAdded: true } : p,
          ),
        );

        onWidgetAdded(widget);
        toast.success(`Widget "${preview.title}" added`);
      } catch {
        setPreviews((prev) =>
          prev.map((p) =>
            p.id === preview.id ? { ...p, isAdding: false } : p,
          ),
        );
        toast.error("Failed to add widget");
      }
    },
    [dashboardId, onWidgetAdded],
  );

  // --------------------------------------------------
  // Keyboard handler
  // --------------------------------------------------
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend],
  );

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex justify-end">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/20 dark:bg-black/40"
        onClick={onClose}
      />

      {/* Slide-over panel */}
      <div className="relative flex h-full w-full max-w-md flex-col border-l border-zinc-200 bg-white shadow-xl dark:border-zinc-800 dark:bg-zinc-900">
        {/* Header */}
        <div className="flex h-12 shrink-0 items-center justify-between border-b border-zinc-200 px-4 dark:border-zinc-800">
          <div className="flex items-center gap-2">
            <HugeiconsIcon
              icon={AiNetworkIcon}
              className="h-4 w-4 text-emerald-500"
              size="100%"
            />
            <span className="text-sm font-semibold text-zinc-800 dark:text-zinc-100">
              AI Widget Generator
            </span>
          </div>
          <button
            onClick={onClose}
            className="rounded-md p-1 text-zinc-400 transition-colors hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
          >
            <HugeiconsIcon icon={Cancel01Icon} className="h-4 w-4" size="100%" />
          </button>
        </div>

        {/* Scrollable content area */}
        <div ref={scrollRef} className="flex-1 overflow-auto p-4 space-y-4">
          {/* Empty state */}
          {previews.length === 0 && !isStreaming && (
            <div className="flex flex-col items-center justify-center gap-3 py-12">
              <div className="flex h-12 w-12 items-center justify-center rounded-full bg-emerald-50 dark:bg-emerald-950/30">
                <HugeiconsIcon
                  icon={ChartLineData02Icon}
                  className="h-5 w-5 text-emerald-500"
                  size="100%"
                />
              </div>
              <p className="text-sm font-medium text-zinc-700 dark:text-zinc-300">
                Describe the widgets you need
              </p>
              <p className="text-center text-xs text-zinc-400">
                The AI will generate chart widgets based on your ontology and add
                them directly to your dashboard.
              </p>
            </div>
          )}

          {/* Status indicator */}
          {isStreaming && (
            <div className="flex items-center gap-2 rounded-lg border border-emerald-200 bg-emerald-50/50 px-3 py-2 dark:border-emerald-800/40 dark:bg-emerald-950/20">
              <Spinner size="sm" className="text-emerald-500" />
              <span className="text-xs text-emerald-700 dark:text-emerald-400">
                {statusText}
              </span>
            </div>
          )}

          {/* Widget preview cards */}
          {previews.map((preview) => (
            <WidgetPreviewCard
              key={preview.id}
              preview={preview}
              onAdd={() => handleAddWidget(preview)}
            />
          ))}
        </div>

        {/* Input area at bottom */}
        <div className="shrink-0 border-t border-zinc-200 p-3 dark:border-zinc-800">
          {!ontology ? (
            <p className="text-center text-xs text-zinc-400">
              Load an ontology to generate widgets
            </p>
          ) : (
            <div className="flex items-center gap-2">
              <input
                type="text"
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder="What kind of widgets do you want?"
                disabled={isStreaming}
                className="flex-1 rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2 text-sm text-zinc-800 placeholder:text-zinc-500 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none disabled:opacity-50 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100 dark:placeholder:text-zinc-500"
              />
              <button
                onClick={handleSend}
                disabled={!input.trim() || isStreaming}
                className="flex h-9 w-9 items-center justify-center rounded-lg bg-emerald-600 text-white transition-colors hover:bg-emerald-700 disabled:opacity-50"
              >
                {isStreaming ? (
                  <Spinner size="sm" className="text-white" />
                ) : (
                  <HugeiconsIcon
                    icon={ArrowRight01Icon}
                    className="h-4 w-4"
                    size="100%"
                  />
                )}
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// WidgetPreviewCard — individual widget preview with "Add to Dashboard" button
// ---------------------------------------------------------------------------

function WidgetPreviewCard({
  preview,
  onAdd,
}: {
  preview: WidgetPreview;
  onAdd: () => void;
}) {
  return (
    <div className="overflow-hidden rounded-lg border border-zinc-200 bg-white dark:border-zinc-700 dark:bg-zinc-800/50">
      {/* Card header */}
      <div className="flex items-center justify-between border-b border-zinc-100 px-3 py-2 dark:border-zinc-700/50">
        <div className="flex items-center gap-2 min-w-0">
          <span className="shrink-0 rounded-full bg-emerald-100 px-2 py-0.5 text-[10px] font-medium text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-400">
            {preview.chartType.replace(/_/g, " ")}
          </span>
          <span className="truncate text-xs font-medium text-zinc-700 dark:text-zinc-300">
            {preview.title}
          </span>
        </div>
      </div>

      {/* Query preview */}
      {preview.query && (
        <div className="border-b border-zinc-100 px-3 py-2 dark:border-zinc-700/50">
          <pre className="max-h-16 overflow-auto text-[10px] font-mono text-zinc-500 dark:text-zinc-400">
            {preview.query}
          </pre>
        </div>
      )}

      {/* Action button */}
      <div className="px-3 py-2">
        {preview.isAdded ? (
          <span className="flex items-center gap-1.5 text-xs text-emerald-600 dark:text-emerald-400">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              viewBox="0 0 20 20"
              fill="currentColor"
              className="h-3.5 w-3.5"
            >
              <path
                fillRule="evenodd"
                d="M16.704 4.153a.75.75 0 01.143 1.052l-8 10.5a.75.75 0 01-1.127.075l-4.5-4.5a.75.75 0 011.06-1.06l3.894 3.893 7.48-9.817a.75.75 0 011.05-.143z"
                clipRule="evenodd"
              />
            </svg>
            Added to dashboard
          </span>
        ) : (
          <button
            onClick={onAdd}
            disabled={preview.isAdding}
            className="flex w-full items-center justify-center gap-1.5 rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-emerald-700 disabled:opacity-50"
          >
            {preview.isAdding ? (
              <Spinner size="sm" className="text-white" />
            ) : (
              <>
                <HugeiconsIcon icon={PlusSignIcon} className="h-3 w-3" size="100%" />
                Add to Dashboard
              </>
            )}
          </button>
        )}
      </div>
    </div>
  );
}
