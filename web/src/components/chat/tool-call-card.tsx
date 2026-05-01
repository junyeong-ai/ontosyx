"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { useAppStore, type ToolCall } from "@/lib/store";
import type { QueryResult, WidgetSpec } from "@/types/api";
import { respondToolReview, normalizeQueryResult } from "@/lib/api";
import { toast } from "sonner";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowDown01Icon,
  ArrowUp01Icon,
  CheckmarkCircle01Icon,
  CancelCircleIcon,
} from "@hugeicons/core-free-icons";
import { Spinner } from "@/components/ui/spinner";
import { CopyButton } from "@/components/ui/copy-button";
import { toolErrorMessage } from "@/lib/error-messages";
import { TOOL_META, DEFAULT_TOOL_META, STEP_LABELS } from "@/lib/constants/tool-meta";
import { useAuth } from "@/lib/use-auth";

// ---------------------------------------------------------------------------
// ToolCallCard — rich display for tool invocations
// ---------------------------------------------------------------------------

interface ToolCallCardProps {
  toolCall: ToolCall;
}

export function ToolCallCard({ toolCall }: ToolCallCardProps) {
  const t = useTranslations("workbench.chat.toolCall");
  const [isExpanded, setIsExpanded] = useState(false);
  const { isAdmin } = useAuth();
  const meta = TOOL_META[toolCall.name] ?? DEFAULT_TOOL_META;
  const isRunning = toolCall.status === "running";
  const isDone = toolCall.status === "done";
  const isError = toolCall.status === "error";

  // Parse structured result for inline rendering
  const parsedResult = useMemo(() => {
    if (!isDone || !toolCall.output) return null;
    return tryParseToolResult(toolCall.name, toolCall.output, toolCall.durationMs, t);
  }, [isDone, toolCall.output, toolCall.name, toolCall.durationMs, t]);

  return (
    <div
      role={isRunning ? "status" : undefined}
      aria-label={isRunning ? t("runningAria", { name: toolCall.name }) : undefined}
      className={`overflow-hidden rounded-xl border transition-colors ${
        isRunning
          ? "border-emerald-200 bg-emerald-50/30 dark:border-emerald-800/40 dark:bg-emerald-950/10"
          : isError
            ? "border-red-200/60 bg-red-50/20 dark:border-red-800/30 dark:bg-red-950/10"
            : "border-zinc-200/80 bg-zinc-50/50 dark:border-zinc-700/50 dark:bg-zinc-800/30"
      }`}
    >
      {/* Header row — the expand/collapse area is a real <button>; the
          "jump to Results" affordance is a sibling <button> (nesting buttons
          is invalid HTML, so they live side-by-side). */}
      <div className="flex w-full items-center gap-2 px-3 py-2 text-xs">
        <button
          type="button"
          disabled={isRunning}
          aria-expanded={isExpanded}
          onClick={() => !isRunning && setIsExpanded(!isExpanded)}
          className="flex flex-1 items-center gap-2 text-left cursor-pointer disabled:cursor-default"
        >
          {isRunning ? (
            <Spinner size="sm" className="text-emerald-500" />
          ) : (
            <HugeiconsIcon
              icon={meta.icon}
              className={`h-3.5 w-3.5 ${isError ? "text-red-500" : "text-zinc-500 dark:text-muted-foreground"}`}
              size="100%"
            />
          )}

          <span className={`font-medium ${isRunning ? "text-emerald-700 dark:text-emerald-400" : isError ? "text-red-600 dark:text-red-400" : "text-zinc-700 dark:text-zinc-300"}`}>
            {isRunning ? `${meta.verb}...` : meta.label}
          </span>

          {/* Duration badge — show total time */}
          {(isDone || isError) && toolCall.durationMs != null && toolCall.durationMs > 0 && (
            <span className="rounded-full bg-zinc-100 px-1.5 py-0.5 text-[10px] tabular-nums text-zinc-500 dark:bg-zinc-700 dark:text-muted-foreground">
              {toolCall.durationMs < 100 ? t("durationSub100") : t("durationSeconds", { seconds: (toolCall.durationMs / 1000).toFixed(1) })}
            </span>
          )}

          {/* Result summary */}
          {parsedResult?.summary && (
            <span className="ml-1 text-[10px] text-muted-foreground">
              {parsedResult.summary}
            </span>
          )}

          {isError && (
            <span className="rounded-full bg-red-100 px-1.5 py-0.5 text-[10px] text-red-600 dark:bg-red-900/30 dark:text-red-400">
              {t("failedBadge")}
            </span>
          )}

          {toolCall.status === "review" && (
            <span className="rounded-full bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-700 dark:bg-amber-900/30 dark:text-amber-400">
              {t("reviewBadge")}
            </span>
          )}

          {!isRunning && toolCall.output && (isAdmin || isError) && (
            <HugeiconsIcon
              icon={isExpanded ? ArrowUp01Icon : ArrowDown01Icon}
              className="ml-auto h-3 w-3 text-muted-foreground"
              size="100%"
            />
          )}
        </button>

        {/* Jump to Results panel */}
        {isDone && !isError && toolCall.output && (
          <button
            type="button"
            aria-label={t("viewInResults")}
            onClick={() => {
              const store = useAppStore.getState();
              store.setAnalyzeRightTab("results");
              store.setFocusResultId(toolCall.id);
            }}
            className="rounded p-0.5 text-muted-foreground hover:bg-zinc-100 hover:text-emerald-600 dark:hover:bg-zinc-700 dark:hover:text-emerald-400"
            title={t("viewInResults")}
          >
            <span className="text-[10px]">→</span>
          </button>
        )}
      </div>

      {parsedResult?.compiledCypher && (
        <div className="border-t border-emerald-200/30 px-3 py-2 dark:border-emerald-800/20">
          <div className="mb-1 flex items-center gap-1.5 text-[10px] font-medium text-emerald-700 dark:text-emerald-400">
            <span>{t("compiledCypherLabel")}</span>
            <CopyButton text={parsedResult.compiledCypher} variant="inline" />
          </div>
          <pre className="overflow-x-auto whitespace-pre-wrap break-words rounded bg-zinc-50 p-2 font-mono text-[10px] leading-relaxed text-zinc-700 dark:bg-zinc-900 dark:text-zinc-300">
            {parsedResult.compiledCypher}
          </pre>
        </div>
      )}

      {parsedResult?.ambiguityChips && parsedResult.ambiguityChips.length > 0 && (
        <AmbiguityChipStrip chips={parsedResult.ambiguityChips} />
      )}

      {/* Sub-step progress: expanded during execution, collapsed after completion */}
      {isRunning && toolCall.steps && toolCall.steps.length > 0 && (
        <div className="border-t border-emerald-200/30 px-3 py-2 space-y-1 dark:border-emerald-800/20">
          {toolCall.steps.map((step) => (
            <div key={step.step} className="flex items-center gap-2 text-xs">
              {step.status === "started" ? (
                <Spinner size="sm" className="h-3 w-3 text-emerald-500" />
              ) : step.status === "completed" ? (
                <HugeiconsIcon icon={CheckmarkCircle01Icon} className="h-3 w-3 text-emerald-500" size="100%" />
              ) : (
                <HugeiconsIcon icon={CancelCircleIcon} className="h-3 w-3 text-red-500" size="100%" />
              )}
              <span className={
                step.status === "started"
                  ? "text-emerald-700 dark:text-emerald-400 font-medium"
                  : step.status === "failed"
                    ? "text-red-600 dark:text-red-400"
                    : "text-zinc-500 dark:text-muted-foreground"
              }>
                {STEP_LABELS[step.step] ?? step.step}
              </span>
              {step.durationMs != null && (
                <span className="text-[10px] tabular-nums text-muted-foreground">
                  {step.durationMs < 100 ? t("durationSub100") : t("durationSeconds", { seconds: (step.durationMs / 1000).toFixed(1) })}
                </span>
              )}
            </div>
          ))}
        </div>
      )}

      {/* HITL approval buttons */}
      {toolCall.status === "review" && (
        <div className="border-t border-amber-200/40 px-3 py-2 dark:border-amber-800/30">
          <p className="text-[11px] text-amber-700 dark:text-amber-400 mb-2">
            {t("hitl.description")}
          </p>
          <div className="flex gap-2">
            <button
              onClick={(e) => {
                e.stopPropagation();
                const sessionId = useAppStore.getState().sessionId;
                if (sessionId) {
                  respondToolReview(sessionId, toolCall.id, true)
                    .then(() => toast.success(t("hitl.toast.approved")))
                    .catch(() => toast.error(t("hitl.toast.approveFailed")));
                }
              }}
              className="rounded-md bg-emerald-600 px-3 py-1 text-xs font-medium text-white hover:bg-emerald-700"
            >
              {t("hitl.approve")}
            </button>
            <button
              onClick={(e) => {
                e.stopPropagation();
                const sessionId = useAppStore.getState().sessionId;
                if (sessionId) {
                  respondToolReview(sessionId, toolCall.id, false)
                    .then(() => toast.info(t("hitl.toast.rejected")))
                    .catch(() => toast.error(t("hitl.toast.rejectFailed")));
                }
              }}
              className="rounded-md border border-red-200 px-3 py-1 text-xs font-medium text-red-600 hover:bg-red-50 dark:border-red-800 dark:text-red-400 dark:hover:bg-red-950/30"
            >
              {t("hitl.reject")}
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
                <p className="mb-1 text-[10px] font-medium text-zinc-500 dark:text-muted-foreground">{t("error.attemptedQuery")}</p>
                <pre className="max-h-20 overflow-auto text-[10px] font-mono text-zinc-600 dark:text-muted-foreground">
                  {compiledQuery}
                </pre>
              </div>
            )}

            {/* Tips for query_graph translation errors */}
            {toolCall.name === "query_graph" && (
              <div className="mt-2 rounded border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-700 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-400">
                <p className="font-medium">{t("error.tipsHeading")}</p>
                <ul className="mt-1 list-disc pl-4 space-y-0.5">
                  <li>{t("error.tipEntityNames")}</li>
                  <li>{t("error.tipPropertyNames")}</li>
                  <li>{t("error.tipSimpler")}</li>
                </ul>
                <p className="mt-1.5 text-amber-600 dark:text-amber-500">
                  {t("error.tryVisualBuilderPrefix")}
                  <button
                    className="underline font-medium"
                    onClick={(e) => {
                      e.stopPropagation();
                      const store = useAppStore.getState();
                      store.setAnalyzeRightTab("query");
                    }}
                  >
                    {t("error.visualQueryBuilder")}
                  </button>
                  {t("error.tryVisualBuilderSuffix")}
                </p>
              </div>
            )}

            {isExpanded && isAdmin && (
              <details className="mt-1">
                <summary className="cursor-pointer text-[10px] text-muted-foreground hover:text-zinc-600">
                  {t("error.technicalDetails")}
                </summary>
                <div className="relative mt-1">
                  <CopyButton text={technicalDetail} />
                  <pre className="max-h-32 overflow-auto rounded bg-zinc-100 p-2 pr-8 text-[10px] text-zinc-500 dark:bg-zinc-900 dark:text-muted-foreground select-text">
                    {truncateOutput(technicalDetail)}
                  </pre>
                </div>
              </details>
            )}
          </div>
        );
      })()}

      {/* Success: expanded raw output (admin only — may contain internal schema details) */}
      {isExpanded && isAdmin && !isError && toolCall.output && (
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
// AmbiguityChipStrip — renders one deep-link chip per unresolved
// ambiguity context the BE flagged. Standalone component so the
// chip strip can grow extra affordances (resolve-inline, dismiss,
// per-chip popover) without bloating ToolCallCard.
// ---------------------------------------------------------------------------

function AmbiguityChipStrip({ chips }: { chips: readonly AmbiguityChip[] }) {
  const t = useTranslations("workbench.chat.toolCall.ambiguity");
  return (
    <div className="border-t border-amber-200/50 px-3 py-2 dark:border-amber-800/30">
      <p className="mb-1.5 text-[10px] font-medium text-amber-700 dark:text-amber-400">
        {t("heading", { count: chips.length })}
      </p>
      <div className="flex flex-wrap gap-1.5">
        {chips.map((chip) => (
          <a
            key={chip.contextId}
            href={`/glossary?ambiguity=${encodeURIComponent(chip.contextId)}`}
            className="inline-flex items-center gap-1 rounded-full border border-amber-300 bg-amber-50 px-2 py-0.5 font-mono text-[10px] text-amber-800 transition-colors hover:border-amber-400 hover:bg-amber-100 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300 dark:hover:border-amber-700"
            title={t("chipTooltip", {
              relation: chip.relation,
              column: chip.column,
            })}
          >
            <span>
              {chip.relation}.{chip.column}
            </span>
          </a>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Parse tool results for inline rendering
// ---------------------------------------------------------------------------

interface ParsedToolResult {
  summary: string;
  widget?: { spec: WidgetSpec; data: QueryResult };
  /** Compiled Cypher the runtime executed for this tool call. */
  compiledCypher?: string;
  /** Unresolved ambiguity contexts the agent flagged on this
   *  query — one chip per unresolved column. */
  ambiguityChips?: AmbiguityChip[];
}

interface AmbiguityChip {
  contextId: string;
  relation: string;
  column: string;
}

function extractAmbiguityChips(raw: unknown): AmbiguityChip[] {
  if (!Array.isArray(raw)) return [];
  const out: AmbiguityChip[] = [];
  for (const entry of raw) {
    if (!entry || typeof entry !== "object") continue;
    const e = entry as Record<string, unknown>;
    const contextId = typeof e.context_id === "string" ? e.context_id : null;
    const relation = typeof e.relation === "string" ? e.relation : null;
    const column = typeof e.column === "string" ? e.column : null;
    if (!contextId || !relation || !column) continue;
    out.push({ contextId, relation, column });
  }
  return out;
}

type ToolCallTranslator = ReturnType<typeof useTranslations<"workbench.chat.toolCall">>;

function tryParseToolResult(
  toolName: string,
  output: string,
  durationMs: number | undefined,
  t: ToolCallTranslator,
): ParsedToolResult | null {
  try {
    const parsed = JSON.parse(output);

    if (toolName === "query_graph" && parsed.columns && parsed.rows) {
      const rowCount = parsed.row_count ?? parsed.rows?.length ?? 0;
      const columns = parsed.columns as string[];
      // Reuse normalizeQueryResult to handle array→object conversion + PropertyValue unwrapping
      const data: QueryResult = normalizeQueryResult(parsed) ?? { columns, rows: [] };
      const specColumns = columns.map((c: string) => ({ key: c, label: c }));
      const spec: WidgetSpec = { columns: specColumns, widget_type: "auto" };
      const compiledCypher =
        typeof parsed.compiled_query === "string" && parsed.compiled_query.length > 0
          ? (parsed.compiled_query as string)
          : undefined;
      const ambiguityChips = extractAmbiguityChips(parsed.unresolved_ambiguities);

      return {
        summary: t("summary.queryRowsColumns", { rows: rowCount, columns: columns.length }),
        widget: rowCount > 0 ? { spec, data } : undefined,
        compiledCypher,
        ambiguityChips: ambiguityChips.length > 0 ? ambiguityChips : undefined,
      };
    }

    if (toolName === "visualize" && parsed.chart_type) {
      const spec: WidgetSpec = {
        widget_type: parsed.chart_type,
        title: parsed.title,
        x_axis: parsed.x_axis,
        y_axis: parsed.y_axis,
        columns: parsed.columns?.map((c: string) => ({ key: c, label: c })),
      };

      if (parsed.data && parsed.columns) {
        const data: QueryResult = { columns: parsed.columns, rows: parsed.data };
        return {
          summary: t("summary.chartType", { type: parsed.chart_type, title: parsed.title ?? "" }),
          widget: { spec, data },
        };
      }

      return {
        summary: t("summary.chartType", { type: parsed.chart_type, title: parsed.title ?? "" }),
      };
    }

    if (toolName === "edit_ontology" && Array.isArray(parsed.commands)) {
      const commands = parsed.commands as Array<{ type: string }>;
      let detail = t("summary.commands", { count: commands.length });
      if (commands.length > 0) {
        const typeCounts: Record<string, number> = {};
        for (const cmd of commands) {
          const type = cmd.type?.replace(/_/g, " ") ?? "unknown";
          typeCounts[type] = (typeCounts[type] || 0) + 1;
        }
        detail = Object.entries(typeCounts)
          .map(([kind, c]) => `${c} ${kind}`)
          .join(", ");
      }
      return { summary: detail };
    }

    if (toolName === "apply_ontology") {
      if (parsed.status === "no_changes") {
        return { summary: t("summary.noChanges") };
      }
      if (parsed.commands_applied != null) {
        const errCount = parsed.errors?.length ?? 0;
        const summary = errCount > 0
          ? t("summary.commandsAppliedErrors", { count: parsed.commands_applied, errors: errCount })
          : t("summary.commandsApplied", { count: parsed.commands_applied });
        return { summary };
      }
    }

    if (toolName === "execute_analysis") {
      return {
        summary: t("summary.execExit", {
          code: parsed.exit_code,
          duration: ((durationMs ?? 0) / 1000).toFixed(1),
        }),
      };
    }

    if (toolName === "recall_memory" && parsed.total != null) {
      return { summary: t("summary.recallHits", { count: parsed.total }) };
    }

    if (toolName === "search_recipes" && parsed.total != null) {
      return { summary: t("summary.recipesHits", { count: parsed.total }) };
    }

    if (toolName === "introspect_source") {
      if (parsed.table_count != null) {
        return { summary: t("summary.tablesCount", { count: parsed.table_count }) };
      }
      if (parsed.table_name) {
        const colCount = Array.isArray(parsed.columns) ? parsed.columns.length : 0;
        return { summary: t("summary.tableColumns", { name: parsed.table_name, count: colCount }) };
      }
    }

    if (toolName === "schema_evolution") {
      if (parsed.status === "no_drift") {
        return { summary: t("summary.noDrift") };
      }
      if (parsed.suggestion_count != null) {
        return { summary: t("summary.driftSuggestions", { count: parsed.suggestion_count }) };
      }
      if (parsed.summary?.drift_detected != null) {
        const s = parsed.summary;
        const total = (s.unmapped_table_count ?? 0) + (s.orphaned_node_count ?? 0)
          + (s.unmapped_column_count ?? 0) + (s.orphaned_property_count ?? 0);
        return { summary: total > 0 ? t("summary.driftDiffs", { count: total }) : t("summary.noDriftShort") };
      }
    }

    if (toolName === "raw_cypher" && parsed.columns) {
      const cols = parsed.columns as string[];
      const data: QueryResult = { columns: cols, rows: parsed.rows ?? [] };
      const specColumns = cols.map((c: string) => ({ key: c, label: c }));
      const spec: WidgetSpec = { columns: specColumns, widget_type: "auto" };
      return {
        summary: t("summary.rawRows", { count: parsed.rows?.length ?? 0 }),
        widget: parsed.rows?.length > 0 ? { spec, data } : undefined,
      };
    }
  } catch {
    // Not JSON — return simple summary
  }

  return { summary: output.length > 100 ? t("summary.charsCount", { count: output.length.toLocaleString() }) : "" };
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
  const t = useTranslations("workbench.chat.toolCall");
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
          <span className="text-muted-foreground">{"\n... ("}{t("json.truncatedChars", { count: formatted.length.toLocaleString() })}{")"}</span>
        )}
      </pre>
      {isLarge && (
        <button
          onClick={() => setExpanded(!expanded)}
          className="absolute bottom-2 right-2 rounded bg-zinc-200 px-2 py-0.5 text-[10px] text-zinc-600 hover:bg-zinc-300 dark:bg-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-600"
        >
          {expanded ? t("json.collapse") : t("json.expand")}
        </button>
      )}
    </div>
  );
}
