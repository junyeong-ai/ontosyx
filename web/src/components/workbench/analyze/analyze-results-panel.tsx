"use client";

import { useState, useEffect, useRef } from "react";
import { useAppStore, type ToolCall } from "@/lib/store";
import type { Dashboard, QueryResult, WidgetSpec } from "@/types/api";
import { listDashboards, addWidget, normalizeQueryResult } from "@/lib/api";
import { HugeiconsIcon } from "@hugeicons/react";
import { Message01Icon } from "@hugeicons/core-free-icons";
import { EmptyState } from "@/components/ui/empty-state";
import { WidgetRenderer } from "@/components/widgets/widget-renderer";
import { toast } from "sonner";

// ---------------------------------------------------------------------------
// Results panel — displays latest tool outputs as visualizations
// ---------------------------------------------------------------------------

export function AnalyzeResultsPanel() {
  const messages = useAppStore((s) => s.messages);
  const focusResultId = useAppStore((s) => s.focusResultId);
  const setFocusResultId = useAppStore((s) => s.setFocusResultId);
  const containerRef = useRef<HTMLDivElement>(null);

  // Find the latest completed tool calls with output
  const toolResults = messages
    .filter((m) => m.role === "assistant" && m.toolCalls?.length)
    .flatMap((m) => m.toolCalls ?? [])
    .filter((tc) => tc.status === "done" && tc.output);

  // Auto-scroll to focused result — must be before any early return (React hooks rule)
  useEffect(() => {
    if (focusResultId && containerRef.current) {
      const el = containerRef.current.querySelector(`[data-tool-id="${focusResultId}"]`);
      if (el) {
        el.scrollIntoView({ behavior: "smooth", block: "start" });
      }
      setFocusResultId(null);
    }
  }, [focusResultId, setFocusResultId]);

  if (toolResults.length === 0) {
    return (
      <EmptyState
        icon={Message01Icon}
        title="No results yet"
        description="Ask a question in the chat to see query results and visualizations here."
      />
    );
  }

  // Extract insights from query results
  const insights = toolResults
    .filter((tc) => tc.name === "query_graph" && tc.output)
    .flatMap((tc) => extractInsights(tc.output!));

  return (
    <div ref={containerRef} className="h-full overflow-auto p-4 space-y-4">
      {/* Insight cards */}
      {insights.length > 0 && (
        <div className="space-y-2">
          {insights.map((insight, i) => (
            <div
              key={i}
              className={`rounded-lg border px-4 py-2.5 text-xs ${
                insight.type === "warning"
                  ? "border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300"
                  : "border-blue-200 bg-blue-50 text-blue-800 dark:border-blue-800 dark:bg-blue-950/30 dark:text-blue-300"
              }`}
            >
              <span className="font-medium">{insight.label}: </span>
              {insight.message}
            </div>
          ))}
        </div>
      )}

      {/* Tool result cards */}
      {toolResults.map((tc) => (
        <ToolResultCard key={tc.id} toolCall={tc} />
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// ToolResultCard — renders query results as charts, others as JSON
// ---------------------------------------------------------------------------

function ToolResultCard({ toolCall }: { toolCall: ToolCall }) {
  const parsed = tryParseQueryOutput(toolCall.output);
  const [pinOpen, setPinOpen] = useState(false);
  const [dashboards, setDashboards] = useState<Dashboard[]>([]);
  const [selectedDashId, setSelectedDashId] = useState<string>("");
  const [widgetTitle, setWidgetTitle] = useState(
    toolCall.name === "query_graph" && parsed
      ? parsed.compiled_query || "Query Result"
      : toolCall.name,
  );
  const [isPinning, setIsPinning] = useState(false);

  useEffect(() => {
    if (!pinOpen) return;
    listDashboards({ limit: 50 })
      .then((page) => {
        setDashboards(page.items);
        if (page.items.length > 0 && !selectedDashId) {
          setSelectedDashId(page.items[0].id);
        }
      })
      .catch(() => { /* non-critical: dashboard list fetch */ });
  }, [pinOpen]);

  const handlePin = async () => {
    if (!selectedDashId || isPinning) return;
    setIsPinning(true);
    try {
      const widgetType = toolCall.name === "query_graph" ? "auto" : "json";
      await addWidget(selectedDashId, {
        title: widgetTitle || "Untitled",
        widget_type: widgetType,
        query: parsed?.compiled_query ?? undefined,
        widget_spec: {},
      });
      toast.success("Pinned to dashboard");
      setPinOpen(false);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to pin");
    } finally {
      setIsPinning(false);
    }
  };

  return (
    <div data-tool-id={toolCall.id} className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-950">
      <div className="flex items-center justify-between gap-3 border-b border-zinc-100 px-4 py-2 dark:border-zinc-800">
        <div className="min-w-0 flex-1">
          {toolCall.name === "query_graph" && parsed?.compiled_query ? (
            <QueryBlock query={parsed.compiled_query} />
          ) : (
            <span className="text-xs font-medium text-zinc-700 dark:text-zinc-300">
              {toolCall.name}
            </span>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {toolCall.durationMs != null && toolCall.durationMs > 0 && (
            <span className="text-[10px] text-zinc-400">
              {toolCall.durationMs < 100 ? "<0.1s" : `${(toolCall.durationMs / 1000).toFixed(1)}s`}
            </span>
          )}
          <button
            onClick={() => setPinOpen(!pinOpen)}
            className="rounded px-1.5 py-0.5 text-[10px] font-medium text-zinc-500 transition-colors hover:bg-emerald-50 hover:text-emerald-600 dark:hover:bg-emerald-950 dark:hover:text-emerald-400"
            title="Pin to Dashboard"
          >
            Pin
          </button>
        </div>
      </div>

      {/* Pin-to-dashboard inline form */}
      {pinOpen && (
        <div className="flex items-center gap-2 border-b border-zinc-100 bg-zinc-50 px-4 py-2 dark:border-zinc-800 dark:bg-zinc-900">
          <select
            value={selectedDashId}
            onChange={(e) => setSelectedDashId(e.target.value)}
            className="h-7 rounded border border-zinc-200 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
          >
            {dashboards.length === 0 && (
              <option value="">No dashboards</option>
            )}
            {dashboards.map((d) => (
              <option key={d.id} value={d.id}>
                {d.name}
              </option>
            ))}
          </select>
          <input
            type="text"
            value={widgetTitle}
            onChange={(e) => setWidgetTitle(e.target.value)}
            placeholder="Widget title"
            className="h-7 flex-1 rounded border border-zinc-200 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
          />
          <button
            onClick={handlePin}
            disabled={!selectedDashId || isPinning}
            className="h-7 rounded bg-emerald-600 px-3 text-xs font-medium text-white transition-colors hover:bg-emerald-700 disabled:opacity-50"
          >
            {isPinning ? "..." : "Confirm"}
          </button>
          <button
            onClick={() => setPinOpen(false)}
            className="h-7 rounded px-2 text-xs text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300"
          >
            Cancel
          </button>
        </div>
      )}

      <div className="p-3">
        {toolCall.name === "query_graph" && parsed ? (
          <WidgetRenderer
            spec={{ widget_type: parsed.widget_hint?.widget_type ?? "auto" } as WidgetSpec}
            data={{
              ...(normalizeQueryResult(parsed) ?? { columns: parsed.columns, rows: [] }),
              metadata: {
                execution_time_ms: toolCall.durationMs ?? 0,
                rows_returned: parsed.row_count,
                nodes_affected: null,
                edges_affected: null,
              },
            }}
          />
        ) : toolCall.name === "visualize" && tryParseVisualize(toolCall.output) ? (
          (() => {
            const viz = tryParseVisualize(toolCall.output)!;
            return (
              <WidgetRenderer
                spec={viz.spec}
                data={viz.data}
              />
            );
          })()
        ) : toolCall.name === "recall_memory" ? (
          <MemoryHitsList raw={toolCall.output} />
        ) : toolCall.name === "execute_analysis" ? (
          <AnalysisResultBlock raw={toolCall.output} />
        ) : (
          <JsonPreview raw={toolCall.output} />
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// JSON Preview — formatted, collapsible JSON display
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// AnalysisResultBlock — structured display for execute_analysis output
// ---------------------------------------------------------------------------

function AnalysisResultBlock({ raw }: { raw?: string }) {
  const [expanded, setExpanded] = useState(false);

  if (!raw) return null;

  let exitCode = 0;
  let durationMs = 0;
  let stdout = "";
  let stderr = "";
  let parsedStdout: unknown = null;

  try {
    const parsed = JSON.parse(raw);
    exitCode = parsed.exit_code ?? 0;
    durationMs = parsed.duration_ms ?? 0;
    stdout = typeof parsed.stdout === "string" ? parsed.stdout : JSON.stringify(parsed.stdout);
    stderr = parsed.stderr ?? "";
  } catch {
    stdout = raw;
  }

  // Try parsing stdout as JSON for structured display
  try {
    parsedStdout = JSON.parse(stdout);
  } catch {
    // stdout is plain text
  }

  const formatted = parsedStdout ? JSON.stringify(parsedStdout, null, 2) : stdout;
  const isLarge = formatted.length > 500;
  const display = !expanded && isLarge ? formatted.slice(0, 500) : formatted;

  return (
    <div className="space-y-2">
      {/* Metadata badges */}
      <div className="flex items-center gap-2">
        <span className={`rounded px-1.5 py-0.5 text-[10px] font-medium ${exitCode === 0 ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400" : "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400"}`}>
          exit {exitCode}
        </span>
        {durationMs > 0 && (
          <span className="text-[10px] text-zinc-400">
            {(durationMs / 1000).toFixed(1)}s
          </span>
        )}
      </div>

      {/* stderr warning */}
      {stderr && (
        <pre className="rounded-md bg-red-950/30 p-2 text-xs text-red-400 leading-relaxed">
          {stderr}
        </pre>
      )}

      {/* stdout content */}
      <div className="relative">
        <pre className="max-h-80 overflow-auto rounded-md bg-zinc-900 p-3 text-xs text-emerald-400 leading-relaxed">
          {display}
          {!expanded && isLarge && (
            <span className="text-zinc-500">{"\n... "}({formatted.length.toLocaleString()} chars)</span>
          )}
        </pre>
        {isLarge && (
          <button
            onClick={() => setExpanded(!expanded)}
            className="absolute bottom-2 right-2 rounded bg-zinc-700 px-2 py-0.5 text-[10px] text-zinc-300 hover:bg-zinc-600"
          >
            {expanded ? "Collapse" : "Expand"}
          </button>
        )}
      </div>
    </div>
  );
}

function JsonPreview({ raw }: { raw?: string }) {
  const [expanded, setExpanded] = useState(false);

  if (!raw) return null;

  // Try to parse and pretty-print
  let formatted: string;
  try {
    const parsed = JSON.parse(raw);
    formatted = JSON.stringify(parsed, null, 2);
  } catch {
    formatted = raw;
  }

  const isLarge = formatted.length > 500;
  const display = !expanded && isLarge ? formatted.slice(0, 500) : formatted;

  return (
    <div className="relative">
      <pre className="max-h-80 overflow-auto rounded-md bg-zinc-900 p-3 text-xs text-emerald-400 leading-relaxed">
        {display}
        {!expanded && isLarge && (
          <span className="text-zinc-500">{"\n... "}({formatted.length.toLocaleString()} chars)</span>
        )}
      </pre>
      {isLarge && (
        <button
          onClick={() => setExpanded(!expanded)}
          className="absolute bottom-2 right-2 rounded bg-zinc-700 px-2 py-0.5 text-[10px] text-zinc-300 hover:bg-zinc-600"
        >
          {expanded ? "Collapse" : "Expand"}
        </button>
      )}
    </div>
  );
}

function tryParseQueryOutput(
  output: string | undefined,
): { compiled_query: string; columns: string[]; rows: unknown[][]; row_count: number; widget_hint?: { widget_type: string; title: string } } | null {
  if (!output) return null;
  try {
    const parsed = JSON.parse(output);
    if (parsed.columns && parsed.rows && typeof parsed.row_count === "number") {
      return {
        compiled_query: parsed.compiled_query ?? "",
        columns: parsed.columns,
        rows: parsed.rows,
        row_count: parsed.row_count,
        widget_hint: parsed.widget_hint ?? undefined,
      };
    }
  } catch {
    // Not query_graph output
  }
  return null;
}

// ---------------------------------------------------------------------------
// Insight extraction — client-side pattern detection from query results
// ---------------------------------------------------------------------------

interface Insight {
  type: "info" | "warning";
  label: string;
  message: string;
}

function extractInsights(output: string): Insight[] {
  const insights: Insight[] = [];
  try {
    const parsed = JSON.parse(output);
    if (!parsed.columns || !parsed.rows) return insights;

    const { columns, rows, row_count } = parsed;

    // Single-row result — highlight as key metric
    if (row_count === 1 && rows.length === 1) {
      const normalized = normalizeQueryResult(parsed);
      if (normalized && normalized.rows.length > 0) {
        const row = normalized.rows[0];
        insights.push({
          type: "info",
          label: "Key Metric",
          message: normalized.columns
            .map((col: string) => {
              const v = row[col];
              const display = v != null && typeof v === "object"
                ? JSON.stringify(v)
                : String(v ?? "\u2014");
              return `${col}: ${display}`;
            })
            .join(", "),
        });
      }
    }

    // Large result set
    if (row_count > 100) {
      insights.push({
        type: "warning",
        label: "Large Result",
        message: `${row_count} rows returned. Consider filtering or aggregating for better performance.`,
      });
    }

    // Detect potential zero/null values
    if (rows.length > 0 && columns.length >= 2) {
      const numericColIdx = columns.findIndex((_: string, i: number) => {
        const sample = rows[0][i];
        const val = sample && typeof sample === "object" && "value" in sample
          ? (sample as { value: unknown }).value
          : sample;
        return typeof val === "number";
      });
      if (numericColIdx >= 0) {
        const values = rows.map((row: unknown[]) => {
          const cell = row[numericColIdx];
          if (cell && typeof cell === "object" && "value" in cell) {
            return (cell as { value: unknown }).value as number;
          }
          return cell as number;
        });
        const max = Math.max(...values);
        const min = Math.min(...values);
        if (max > 0 && min === 0) {
          insights.push({
            type: "warning",
            label: "Zero Values",
            message: `Column "${columns[numericColIdx]}" contains zero values — may indicate missing data.`,
          });
        }
      }
    }
  } catch {
    // Not valid JSON
  }
  return insights;
}

// ---------------------------------------------------------------------------
// Query Block — syntax-highlighted graph query display
// ---------------------------------------------------------------------------

// Keywords for Cypher (extensible to Gremlin, GQL, SPARQL)
const CYPHER_KEYWORDS = /\b(MATCH|WHERE|RETURN|WITH|ORDER BY|LIMIT|SKIP|CREATE|MERGE|DELETE|SET|REMOVE|UNWIND|CALL|YIELD|OPTIONAL|UNION|AS|AND|OR|NOT|IN|IS|NULL|TRUE|FALSE|DISTINCT|COUNT|SUM|AVG|MIN|MAX|COLLECT|DESC|ASC|EXISTS|CASE|WHEN|THEN|ELSE|END)\b/gi;
const CYPHER_LABELS = /(:[\w`]+)/g;
const CYPHER_STRINGS = /('[^']*'|"[^"]*")/g;
const CYPHER_NUMBERS = /\b(\d+\.?\d*)\b/g;

function highlightCypher(query: string): string {
  // Uses .ql-* CSS classes from globals.css (themeable via CSS custom properties)
  const html = query
    .replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
    .replace(CYPHER_STRINGS, '<span class="ql-string">$1</span>')
    .replace(CYPHER_KEYWORDS, '<span class="ql-keyword">$1</span>')
    .replace(CYPHER_LABELS, '<span class="ql-label">$1</span>')
    .replace(CYPHER_NUMBERS, '<span class="ql-number">$1</span>');
  return html;
}

function QueryBlock({ query }: { query: string }) {
  return (
    <div className="group/qb relative">
      <code
        className="block max-h-20 overflow-auto rounded bg-zinc-900 px-2 py-1.5 pr-8 text-[11px] font-mono leading-relaxed text-zinc-300"
        dangerouslySetInnerHTML={{ __html: highlightCypher(query) }}
      />
      <button
        onClick={() => navigator.clipboard.writeText(query)}
        className="absolute right-1 top-1 rounded p-0.5 text-zinc-500 opacity-0 transition-opacity hover:text-zinc-300 group-hover/qb:opacity-100"
        aria-label="Copy query"
        title="Copy query"
      >
        <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
        </svg>
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Memory Hits List — structured recall_memory display
// ---------------------------------------------------------------------------

function MemoryHitsList({ raw }: { raw?: string }) {
  if (!raw) return null;

  let hits: { content: string; source: string; score: number }[] = [];
  try {
    const parsed = JSON.parse(raw);
    hits = parsed.hits ?? [];
  } catch {
    return <JsonPreview raw={raw} />;
  }

  if (hits.length === 0) {
    return <p className="text-xs text-zinc-400">No memories found</p>;
  }

  return (
    <div className="max-h-60 space-y-2 overflow-auto">
      {hits.map((hit, i) => (
        <div
          key={i}
          className="rounded border-l-2 border-amber-400 bg-zinc-50 py-1.5 pl-3 pr-2 dark:bg-zinc-900"
        >
          <div className="flex items-center justify-between">
            <span className="text-[10px] font-medium text-amber-600 dark:text-amber-400">
              {hit.source}
            </span>
            <span className="text-[10px] text-zinc-400">
              {(hit.score * 100).toFixed(0)}% match
            </span>
          </div>
          <p className="mt-0.5 text-xs text-zinc-700 line-clamp-2 dark:text-zinc-300">
            {hit.content}
          </p>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Visualize output parser
// ---------------------------------------------------------------------------

function tryParseVisualize(
  output?: string,
): { spec: WidgetSpec; data: QueryResult } | null {
  if (!output) return null;
  try {
    const parsed = JSON.parse(output);
    if (!parsed.chart_type || !parsed.columns) return null;

    const spec: WidgetSpec = {
      widget_type: parsed.chart_type,
      widget: parsed.chart_type,
      title: parsed.title,
      columns: parsed.columns?.map((c: string) => ({ key: c, label: c })),
    };

    // Data can be array-of-objects or array-of-arrays
    if (parsed.data && parsed.columns) {
      const normalized = normalizeQueryResult({
        columns: parsed.columns,
        rows: Array.isArray(parsed.data) ? parsed.data : [],
      });
      if (normalized && normalized.rows.length > 0) {
        return { spec, data: normalized };
      }
    }

    return null;
  } catch {
    return null;
  }
}
