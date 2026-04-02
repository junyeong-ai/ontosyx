"use client";

import { useMemo, useState } from "react";
import { useAppStore, type ToolCall } from "@/lib/store";
import type { QueryResult, WidgetSpec } from "@/types/api";
import { respondToolReview, normalizeQueryResult } from "@/lib/api";
import { toast } from "sonner";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowDown01Icon,
  ArrowUp01Icon,
} from "@hugeicons/core-free-icons";
import { Spinner } from "@/components/ui/spinner";
import { CopyButton } from "@/components/ui/copy-button";
import { toolErrorMessage } from "@/lib/error-messages";
import { TOOL_META, DEFAULT_TOOL_META } from "@/lib/constants/tool-meta";

// ---------------------------------------------------------------------------
// ToolCallCard — rich display for tool invocations
// ---------------------------------------------------------------------------

interface ToolCallCardProps {
  toolCall: ToolCall;
}

export function ToolCallCard({ toolCall }: ToolCallCardProps) {
  const [isExpanded, setIsExpanded] = useState(false);
  const meta = TOOL_META[toolCall.name] ?? DEFAULT_TOOL_META;
  const isRunning = toolCall.status === "running";
  const isDone = toolCall.status === "done";
  const isError = toolCall.status === "error";

  // Parse structured result for inline rendering
  const parsedResult = useMemo(() => {
    if (!isDone || !toolCall.output) return null;
    return tryParseToolResult(toolCall.name, toolCall.output);
  }, [isDone, toolCall.output, toolCall.name]);

  return (
    <div
      role={isRunning ? "status" : undefined}
      aria-label={isRunning ? `${toolCall.name} running` : undefined}
      className={`overflow-hidden rounded-xl border transition-colors ${
        isRunning
          ? "border-emerald-200 bg-emerald-50/30 dark:border-emerald-800/40 dark:bg-emerald-950/10"
          : isError
            ? "border-red-200/60 bg-red-50/20 dark:border-red-800/30 dark:bg-red-950/10"
            : "border-zinc-200/80 bg-zinc-50/50 dark:border-zinc-700/50 dark:bg-zinc-800/30"
      }`}
    >
      {/* Header */}
      <div
        role="button"
        tabIndex={0}
        onClick={() => !isRunning && setIsExpanded(!isExpanded)}
        onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); !isRunning && setIsExpanded(!isExpanded); } }}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs cursor-pointer"
      >
        {isRunning ? (
          <Spinner size="sm" className="text-emerald-500" />
        ) : (
          <HugeiconsIcon
            icon={meta.icon}
            className={`h-3.5 w-3.5 ${isError ? "text-red-500" : "text-zinc-500 dark:text-zinc-400"}`}
            size="100%"
          />
        )}

        <span className={`font-medium ${isRunning ? "text-emerald-700 dark:text-emerald-400" : isError ? "text-red-600 dark:text-red-400" : "text-zinc-700 dark:text-zinc-300"}`}>
          {isRunning ? `${meta.verb}...` : meta.label}
        </span>

        {/* Duration badge */}
        {isDone && toolCall.durationMs != null && toolCall.durationMs > 0 && (
          <span className="rounded-full bg-zinc-100 px-1.5 py-0.5 text-[10px] tabular-nums text-zinc-500 dark:bg-zinc-700 dark:text-zinc-400">
            {toolCall.durationMs < 100 ? "<0.1s" : `${(toolCall.durationMs / 1000).toFixed(1)}s`}
          </span>
        )}

        {/* Result summary */}
        {parsedResult?.summary && (
          <span className="ml-1 text-[10px] text-zinc-400 dark:text-zinc-500">
            {parsedResult.summary}
          </span>
        )}

        {/* Jump to Results panel */}
        {isDone && !isError && toolCall.output && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              const store = useAppStore.getState();
              store.setAnalyzeRightTab("results");
              store.setFocusResultId(toolCall.id);
            }}
            className="ml-1 rounded p-0.5 text-zinc-400 hover:bg-zinc-100 hover:text-emerald-600 dark:hover:bg-zinc-700 dark:hover:text-emerald-400"
            title="View in Results panel"
          >
            <span className="text-[10px]">→</span>
          </button>
        )}

        {isError && (
          <span className="rounded-full bg-red-100 px-1.5 py-0.5 text-[10px] text-red-600 dark:bg-red-900/30 dark:text-red-400">
            failed
          </span>
        )}

        {toolCall.status === "review" && (
          <span className="rounded-full bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-700 dark:bg-amber-900/30 dark:text-amber-400">
            review required
          </span>
        )}

        {!isRunning && toolCall.output && (
          <HugeiconsIcon
            icon={isExpanded ? ArrowUp01Icon : ArrowDown01Icon}
            className="ml-auto h-3 w-3 text-zinc-400"
            size="100%"
          />
        )}
      </div>

      {/* HITL approval buttons */}
      {toolCall.status === "review" && (
        <div className="border-t border-amber-200/40 px-3 py-2 dark:border-amber-800/30">
          <p className="text-[11px] text-amber-700 dark:text-amber-400 mb-2">
            This tool requires your approval before execution.
          </p>
          <div className="flex gap-2">
            <button
              onClick={(e) => {
                e.stopPropagation();
                const sessionId = useAppStore.getState().sessionId;
                if (sessionId) {
                  respondToolReview(sessionId, toolCall.id, true)
                    .then(() => toast.success("Tool approved"))
                    .catch(() => toast.error("Failed to approve"));
                }
              }}
              className="rounded-md bg-emerald-600 px-3 py-1 text-xs font-medium text-white hover:bg-emerald-700"
            >
              Approve
            </button>
            <button
              onClick={(e) => {
                e.stopPropagation();
                const sessionId = useAppStore.getState().sessionId;
                if (sessionId) {
                  respondToolReview(sessionId, toolCall.id, false)
                    .then(() => toast.info("Tool rejected"))
                    .catch(() => toast.error("Failed to reject"));
                }
              }}
              className="rounded-md border border-red-200 px-3 py-1 text-xs font-medium text-red-600 hover:bg-red-50 dark:border-red-800 dark:text-red-400 dark:hover:bg-red-950/30"
            >
              Reject
            </button>
          </div>
        </div>
      )}

      {/* Error: user-friendly message with expandable technical detail */}
      {isError && toolCall.output && (() => {
        const { userMessage, technicalDetail } = toolErrorMessage(toolCall.output);
        const compiledQuery = tryExtractCompiledQuery(technicalDetail);
        return (
          <div className="border-t border-red-200/50 px-3 py-2 dark:border-red-900/30">
            <p className="text-xs text-red-600 dark:text-red-400">{userMessage}</p>

            {/* Show attempted query if available */}
            {compiledQuery && (
              <div className="mt-1.5 rounded border border-red-200/40 bg-zinc-100 px-2 py-1.5 dark:border-red-800/30 dark:bg-zinc-900">
                <p className="mb-1 text-[10px] font-medium text-zinc-500 dark:text-zinc-400">Attempted query:</p>
                <pre className="max-h-20 overflow-auto text-[10px] font-mono text-zinc-600 dark:text-zinc-400">
                  {compiledQuery}
                </pre>
              </div>
            )}

            {/* Tips for query_graph translation errors */}
            {toolCall.name === "query_graph" && (
              <div className="mt-2 rounded border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-700 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-400">
                <p className="font-medium">Tips to improve query translation:</p>
                <ul className="mt-1 list-disc pl-4 space-y-0.5">
                  <li>Use specific entity names from your ontology (e.g., &quot;Customer&quot;, &quot;Product&quot;)</li>
                  <li>Mention property names explicitly (e.g., &quot;with name containing X&quot;)</li>
                  <li>Try simpler questions first, then add complexity</li>
                </ul>
                <p className="mt-1.5 text-amber-600 dark:text-amber-500">
                  Or try the{" "}
                  <button
                    className="underline font-medium"
                    onClick={(e) => {
                      e.stopPropagation();
                      const store = useAppStore.getState();
                      store.setAnalyzeRightTab("query");
                    }}
                  >
                    Visual Query Builder
                  </button>
                  {" "}for complex queries.
                </p>
              </div>
            )}

            {isExpanded && (
              <details className="mt-1">
                <summary className="cursor-pointer text-[10px] text-zinc-400 hover:text-zinc-600">
                  Technical details
                </summary>
                <div className="relative mt-1">
                  <CopyButton text={technicalDetail} />
                  <pre className="max-h-32 overflow-auto rounded bg-zinc-100 p-2 pr-8 text-[10px] text-zinc-500 dark:bg-zinc-900 dark:text-zinc-400 select-text">
                    {truncateOutput(technicalDetail)}
                  </pre>
                </div>
              </details>
            )}
          </div>
        );
      })()}

      {/* Success: expanded raw output */}
      {isExpanded && !isError && toolCall.output && (
        <div className="border-t border-zinc-200/50 dark:border-zinc-700/30">
          <div className="relative">
            <CopyButton text={toolCall.output} />
            <JsonBlock raw={toolCall.output} />
          </div>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Parse tool results for inline rendering
// ---------------------------------------------------------------------------

interface ParsedToolResult {
  summary: string;
  widget?: { spec: WidgetSpec; data: QueryResult };
}

function tryParseToolResult(toolName: string, output: string): ParsedToolResult | null {
  try {
    const parsed = JSON.parse(output);

    if (toolName === "query_graph" && parsed.columns && parsed.rows) {
      const rowCount = parsed.row_count ?? parsed.rows?.length ?? 0;
      const columns = parsed.columns as string[];
      // Reuse normalizeQueryResult to handle array→object conversion + PropertyValue unwrapping
      const data: QueryResult = normalizeQueryResult(parsed) ?? { columns, rows: [] };
      const specColumns = columns.map((c: string) => ({ key: c, label: c }));
      const spec: WidgetSpec = { columns: specColumns, widget_type: "auto" };

      return {
        summary: `${rowCount} rows, ${columns.length} columns`,
        widget: rowCount > 0 ? { spec, data } : undefined,
      };
    }

    if (toolName === "visualize" && parsed.chart_type) {
      const spec: WidgetSpec = {
        widget: parsed.chart_type,
        widget_type: parsed.chart_type,
        title: parsed.title,
        x_axis: parsed.x_axis,
        y_axis: parsed.y_axis,
        columns: parsed.columns?.map((c: string) => ({ key: c, label: c })),
      };

      if (parsed.data && parsed.columns) {
        const data: QueryResult = { columns: parsed.columns, rows: parsed.data };
        return {
          summary: `${parsed.chart_type}: ${parsed.title ?? ""}`,
          widget: { spec, data },
        };
      }

      return {
        summary: `${parsed.chart_type}: ${parsed.title ?? ""}`,
      };
    }

    if (toolName === "edit_ontology" && parsed.command_count != null) {
      const commands = parsed.commands as Array<{ type: string }> | undefined;
      let detail = `${parsed.command_count} commands generated`;
      if (commands && commands.length > 0) {
        const typeCounts: Record<string, number> = {};
        for (const cmd of commands) {
          const type = cmd.type?.replace(/_/g, " ") ?? "unknown";
          typeCounts[type] = (typeCounts[type] || 0) + 1;
        }
        detail = Object.entries(typeCounts)
          .map(([t, c]) => `${c} ${t}`)
          .join(", ");
      }
      return { summary: detail };
    }

    if (toolName === "apply_ontology") {
      if (parsed.status === "no_changes") {
        return { summary: "No changes needed" };
      }
      if (parsed.commands_applied != null) {
        const errCount = parsed.errors?.length ?? 0;
        const suffix = errCount > 0 ? ` (${errCount} errors)` : "";
        return { summary: `${parsed.commands_applied} commands applied${suffix}` };
      }
    }

    if (toolName === "execute_analysis") {
      return {
        summary: `exit ${parsed.exit_code}, ${(parsed.duration_ms / 1000).toFixed(1)}s`,
      };
    }

    if (toolName === "recall_memory" && parsed.total != null) {
      return { summary: `${parsed.total} hits` };
    }

    if (toolName === "search_recipes" && parsed.total != null) {
      return { summary: `${parsed.total} recipes` };
    }

    if (toolName === "introspect_source") {
      if (parsed.table_count != null) {
        return { summary: `${parsed.table_count} tables` };
      }
      if (parsed.table_name) {
        const colCount = Array.isArray(parsed.columns) ? parsed.columns.length : 0;
        return { summary: `${parsed.table_name} (${colCount} columns)` };
      }
    }

    if (toolName === "schema_evolution") {
      if (parsed.status === "no_drift") {
        return { summary: "No drift detected" };
      }
      if (parsed.suggestion_count != null) {
        return { summary: `${parsed.suggestion_count} suggestions` };
      }
      if (parsed.summary?.drift_detected != null) {
        const s = parsed.summary;
        const total = (s.unmapped_table_count ?? 0) + (s.orphaned_node_count ?? 0)
          + (s.unmapped_column_count ?? 0) + (s.orphaned_property_count ?? 0);
        return { summary: total > 0 ? `${total} diffs found` : "No drift" };
      }
    }

    if (toolName === "raw_cypher" && parsed.columns) {
      const cols = parsed.columns as string[];
      const data: QueryResult = { columns: cols, rows: parsed.rows ?? [] };
      const specColumns = cols.map((c: string) => ({ key: c, label: c }));
      const spec: WidgetSpec = { columns: specColumns, widget_type: "auto" };
      return {
        summary: `${parsed.rows?.length ?? 0} rows`,
        widget: parsed.rows?.length > 0 ? { spec, data } : undefined,
      };
    }
  } catch {
    // Not JSON — return simple summary
  }

  return { summary: output.length > 100 ? `${output.length} chars` : "" };
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Try to extract a compiled query from error output (JSON or plain text). */
function tryExtractCompiledQuery(output: string): string | null {
  try {
    const parsed = JSON.parse(output);
    if (typeof parsed.compiled_query === "string" && parsed.compiled_query) {
      return parsed.compiled_query;
    }
    if (typeof parsed.query === "string" && parsed.query) {
      return parsed.query;
    }
  } catch {
    // Try regex extraction from plain text
    const match = output.match(/(?:compiled_query|query)["']?\s*[:=]\s*["'](.+?)["']/);
    if (match) return match[1];
  }
  return null;
}

function truncateOutput(output: string, maxLen = 3000): string {
  if (output.length <= maxLen) return output;
  return output.slice(0, maxLen) + "\n... (truncated)";
}

/** Formatted JSON display with collapse/expand for tool outputs. */
function JsonBlock({ raw }: { raw: string }) {
  const [expanded, setExpanded] = useState(false);

  let formatted: string;
  try {
    const parsed = JSON.parse(raw);
    // execute_analysis: parse stdout if it contains JSON
    if (parsed.stdout && typeof parsed.stdout === "string") {
      try {
        parsed.stdout = JSON.parse(parsed.stdout);
      } catch { /* stdout is plain text, keep as-is */ }
    }
    formatted = JSON.stringify(parsed, null, 2);
  } catch {
    formatted = raw;
  }

  const isLarge = formatted.length > 600;
  const display = !expanded && isLarge ? formatted.slice(0, 600) : formatted;

  return (
    <div className="relative">
      <pre className="max-h-64 overflow-auto p-3 pr-10 text-xs font-mono text-zinc-700 dark:text-zinc-300 leading-relaxed">
        {display}
        {!expanded && isLarge && (
          <span className="text-zinc-400">{"\n... "}({formatted.length.toLocaleString()} chars)</span>
        )}
      </pre>
      {isLarge && (
        <button
          onClick={() => setExpanded(!expanded)}
          className="absolute bottom-2 right-2 rounded bg-zinc-200 px-2 py-0.5 text-[10px] text-zinc-600 hover:bg-zinc-300 dark:bg-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-600"
        >
          {expanded ? "Collapse" : "Expand"}
        </button>
      )}
    </div>
  );
}
