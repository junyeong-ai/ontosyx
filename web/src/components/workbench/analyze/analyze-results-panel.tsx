"use client";

import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { useAppStore, type ToolCall } from "@/lib/store";
import type { QueryDiagnostic, QueryResult, WidgetSpec } from "@/types/api";
import { addWidget, normalizeQueryResult } from "@/lib/api";
import { useDashboards } from "@/hooks/api/use-dashboards";
import { useExecution } from "@/hooks/api/use-executions";
import { Message01Icon } from "@hugeicons/core-free-icons";
import { CopyButton } from "@/components/ui/copy-button";
import { EmptyState } from "@/components/ui/empty-state";
import { FormInput, FormSelect } from "@/components/ui/form-input";
import { Button } from "@/components/ui/button";
import { WidgetRenderer } from "@/components/dashboard/widgets/widget-renderer";
import { ResponseBasis } from "@/components/dashboard/widgets/response-basis";
import { SaveInsightDialog } from "@/components/workbench/insights/save-insight-dialog";
import { toast } from "@/components/ui/toast";
import { STEP_TIMING_LABELS } from "@/lib/constants/tool-meta";

// ---------------------------------------------------------------------------
// Results panel — displays latest tool outputs as visualizations
// ---------------------------------------------------------------------------

export function AnalyzeResultsPanel() {
  const t = useTranslations("workbench.queryBuilder.results");
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
        title={t("empty.title")}
        description={t("empty.description")}
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
                  ? "border-warning-border bg-warning-surface text-warning-foreground"
                  : "border-info-border bg-info-surface text-info-foreground"
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
  const executionQuery = useExecution(parsed?.execution_id ?? null);
  const execution = executionQuery.data;
  const provenance = execution?.results?.metadata?.provenance ?? undefined;
  const [pinOpen, setPinOpen] = useState(false);
  const [selectedDashId, setSelectedDashId] = useState<string>("");
  const [widgetTitle, setWidgetTitle] = useState(
    toolCall.name === "query_graph" && parsed
      ? parsed.compiled_query || "Query Result"
      : toolCall.name,
  );
  const [isPinning, setIsPinning] = useState(false);
  const [saveInsightOpen, setSaveInsightOpen] = useState(false);

  // "Save as Insight" is meaningful for any query_graph result. We
  // render the button on three distinct states so the user can tell
  // why it is or isn't actionable:
  //   - `ready`       — execution row resolved, click to save
  //   - `loading`     — tool emitted execution_id, fetch still in
  //                     flight (button disabled, tooltip honest)
  //   - `unavailable` — no execution row will ever resolve. Either
  //                     `query_graph` produced no execution_id (e.g.
  //                     failed query) OR the fetch errored (404 from
  //                     a GC'd row, network failure, auth expired).
  //                     Both collapse to the same UX: there is
  //                     nothing to save for this tool call.
  // The canonical QueryIR is always read from the persisted row,
  // never from the LLM-facing tool envelope.
  const tSave = useTranslations("workbench.queryBuilder.results.saveButton");
  const tPin = useTranslations("workbench.queryBuilder.results.pin");
  const tCommon = useTranslations("common");
  const isQueryResult = toolCall.name === "query_graph";
  const hasExecutionId = Boolean(parsed?.execution_id);
  const insightReady = Boolean(execution?.query_ir);
  const saveState: "ready" | "loading" | "unavailable" =
    !hasExecutionId || executionQuery.isError
      ? "unavailable"
      : insightReady
        ? "ready"
        : "loading";

  // Fetch dashboards only while the pin popover is open; Tanstack Query
  // caches the result between opens so repeated toggles don't re-fetch.
  const { data: dashboardsPage } = useDashboards(
    { limit: 50 },
    { enabled: pinOpen },
  );
  const dashboards = useMemo(
    () => dashboardsPage?.items ?? [],
    [dashboardsPage],
  );

  // Auto-seed the first dashboard as the selected target whenever a
  // fresh list arrives and no selection has been made yet. Writes to a
  // component-local state, which is a plain setState-from-effect and
  // would trip the React 19 gate — but `setSelectedDashId` only fires
  // on the transition "no selection → pick first", so the cascade is
  // capped to one render per open cycle.
  useEffect(() => {
    if (pinOpen && dashboards.length > 0 && !selectedDashId) {
      setSelectedDashId(dashboards[0].id);
    }
  }, [pinOpen, dashboards, selectedDashId]);

  const handlePin = async () => {
    if (!selectedDashId || isPinning) return;
    setIsPinning(true);
    try {
      const widgetType = toolCall.name === "query_graph" ? "auto" : "json";
      await addWidget(selectedDashId, {
        title: widgetTitle || tPin("untitled"),
        widget_type: widgetType,
        query: parsed?.compiled_query ?? undefined,
        widget_spec: {},
      });
      toast.success(tPin("pinned"));
      setPinOpen(false);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : tPin("failed"));
    } finally {
      setIsPinning(false);
    }
  };

  return (
    <div data-tool-id={toolCall.id} className="rounded-lg border border-divider bg-surface-base">
      {/* Header — consistent for all tool types */}
      <div className="flex items-center justify-between gap-3 border-b border-divider-soft px-4 py-2">
        <span className="text-xs font-medium text-foreground">
          {toolCall.name}
        </span>
        <div className="flex shrink-0 items-center gap-2">
          {toolCall.durationMs != null && toolCall.durationMs > 0 && (
            <span className="text-2xs text-foreground-muted">
              {toolCall.durationMs < 100 ? "<0.1s" : `${(toolCall.durationMs / 1000).toFixed(1)}s`}
            </span>
          )}
          {isQueryResult && (
            <button type="button"
              onClick={() => setSaveInsightOpen(true)}
              disabled={saveState !== "ready"}
              className="cursor-pointer rounded px-1.5 py-0.5 text-2xs font-medium text-foreground-muted transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-concept-surface hover:text-concept-foreground disabled:cursor-default disabled:opacity-50 disabled:hover:bg-transparent disabled:hover:text-foreground-muted"
              title={tSave(`tooltip.${saveState}`)}
            >
              {tSave("label")}
            </button>
          )}
          <button type="button"
            onClick={() => setPinOpen(!pinOpen)}
            className="cursor-pointer rounded px-1.5 py-0.5 text-2xs font-medium text-foreground-muted transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-brand-surface hover:text-brand-foreground"
            title={tPin("buttonTooltip")}
          >
            {tPin("buttonLabel")}
          </button>
        </div>
      </div>
      {insightReady && execution && (
        <SaveInsightDialog
          open={saveInsightOpen}
          onOpenChange={setSaveInsightOpen}
          queryIr={execution.query_ir}
          originalProvenance={provenance}
          defaultQuestion={execution.question}
        />
      )}
      {/* Cypher query block — below header for query_graph results */}
      {toolCall.name === "query_graph" && parsed?.compiled_query && (
        <div className="px-3 pt-2">
          <QueryBlock query={parsed.compiled_query} />
        </div>
      )}
      {/* Step timings — populated from SSE progress events on toolCall.steps */}
      {toolCall.steps && toolCall.steps.length > 0 && (
        <div className="flex flex-wrap gap-x-3 gap-y-0.5 px-3 pb-2 pt-1 text-2xs text-foreground-muted">
          {toolCall.steps.map((st) => {
            const label = STEP_TIMING_LABELS[st.step] ?? st.step;
            const ms = st.durationMs ?? 0;
            return (
              <span key={st.step}>
                {label} {ms < 100 ? "<0.1s" : `${(ms / 1000).toFixed(1)}s`}
              </span>
            );
          })}
        </div>
      )}

      {/* Pin-to-dashboard inline form */}
      {pinOpen && (
        <div className="flex items-center gap-2 border-b border-divider-soft bg-surface-raised px-4 py-2">
          <FormSelect
            density="compact"
            value={selectedDashId}
            onChange={(e) => setSelectedDashId(e.target.value)}
            aria-label={tPin("buttonTooltip")}
            className="h-7"
          >
            {dashboards.length === 0 && (
              <option value="">{tPin("noDashboards")}</option>
            )}
            {dashboards.map((d) => (
              <option key={d.id} value={d.id}>
                {d.name}
              </option>
            ))}
          </FormSelect>
          <FormInput
            type="text"
            density="compact"
            value={widgetTitle}
            onChange={(e) => setWidgetTitle(e.target.value)}
            placeholder={tPin("widgetTitlePlaceholder")}
            aria-label={tPin("widgetTitlePlaceholder")}
            className="flex-1"
          />
          <Button
            variant="primary"
            size="xs"
            onClick={handlePin}
            disabled={!selectedDashId}
            loading={isPinning}
          >
            {tPin("confirm")}
          </Button>
          <button type="button"
            onClick={() => setPinOpen(false)}
            className="h-7 rounded px-2 text-xs text-foreground-muted hover:text-foreground"
          >
            {tCommon("cancel")}
          </button>
        </div>
      )}

      <div className="p-3 space-y-3">
        {toolCall.name === "query_graph" && parsed ? (
          <>
            <WidgetRenderer
              spec={{ widget_type: parsed.widget_hint?.widget_type ?? "auto" } as WidgetSpec}
              data={{
                ...(normalizeQueryResult(parsed) ?? { columns: parsed.columns, rows: [] }),
                metadata: {
                  execution_time_ms: toolCall.durationMs ?? 0,
                  rows_returned: parsed.row_count,
                  nodes_affected: null,
                  edges_affected: null,
                  provenance,
                },
              }}
            />
            <ResponseBasis provenance={provenance} warnings={parsed.warnings} />
          </>
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
          <AnalysisResultBlock raw={toolCall.output} durationMs={toolCall.durationMs} />
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

function AnalysisResultBlock({
  raw,
  durationMs,
}: {
  raw?: string;
  durationMs?: number;
}) {
  const t = useTranslations("workbench.queryBuilder.results");
  const [expanded, setExpanded] = useState(false);

  if (!raw) return null;

  let exitCode = 0;
  let stdout = "";
  let stderr = "";
  let parsedStdout: unknown = null;

  try {
    const parsed = JSON.parse(raw);
    exitCode = parsed.exit_code ?? 0;
    stdout = typeof parsed.stdout === "string" ? parsed.stdout : JSON.stringify(parsed.stdout);
    stderr = parsed.stderr ?? "";
  } catch {
    stdout = raw;
  }
  const ms = durationMs ?? 0;

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
        <span className={`rounded px-1.5 py-0.5 text-2xs font-medium ${exitCode === 0 ? "bg-success-surface text-success-foreground" : "bg-danger-surface text-danger-foreground"}`}>
          {t("analysis.exitCode", { code: exitCode })}
        </span>
        {ms > 0 && (
          <span className="text-2xs text-foreground-muted">
            {(ms / 1000).toFixed(1)}s
          </span>
        )}
      </div>

      {/* stderr warning */}
      {stderr && (
        <pre className="rounded-md bg-danger-surface p-2 text-xs text-danger-foreground leading-relaxed">
          {stderr}
        </pre>
      )}

      {/* stdout content */}
      <div className="relative">
        <pre className="max-h-80 overflow-auto rounded-md bg-surface-base p-3 text-xs text-brand-foreground leading-relaxed">
          {display}
          {!expanded && isLarge && (
            <span className="text-foreground-muted">{t("analysis.charsTruncated", { count: formatted.length })}</span>
          )}
        </pre>
        {isLarge && (
          <button type="button"
            onClick={() => setExpanded(!expanded)}
            className="absolute bottom-2 end-2 rounded bg-surface-base px-2 py-0.5 text-2xs text-foreground-muted hover:bg-surface-base"
          >
            {expanded ? "Collapse" : "Expand"}
          </button>
        )}
      </div>
    </div>
  );
}

function JsonPreview({ raw }: { raw?: string }) {
  const t = useTranslations("workbench.queryBuilder.results");
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
      <pre className="max-h-80 overflow-auto rounded-md bg-surface-base p-3 text-xs text-brand-foreground leading-relaxed">
        {display}
        {!expanded && isLarge && (
          <span className="text-foreground-muted">{t("analysis.charsTruncated", { count: formatted.length })}</span>
        )}
      </pre>
      {isLarge && (
        <button type="button"
          onClick={() => setExpanded(!expanded)}
          className="absolute bottom-2 end-2 rounded bg-surface-base px-2 py-0.5 text-2xs text-foreground-muted hover:bg-surface-base"
        >
          {expanded ? "Collapse" : "Expand"}
        </button>
      )}
    </div>
  );
}

function tryParseQueryOutput(
  output: string | undefined,
): {
  execution_id: string;
  compiled_query: string;
  columns: string[];
  rows: unknown[][];
  row_count: number;
  widget_hint?: { widget_type: string; title: string };
  /** Structured advisory validator diagnostics — same shape as
   *  `QueryMetadata.warnings` on the HTTP route path. */
  warnings?: QueryDiagnostic[];
} | null {
  if (!output) return null;
  try {
    const parsed = JSON.parse(output);
    if (parsed.columns && parsed.rows && typeof parsed.row_count === "number") {
      return {
        execution_id: parsed.execution_id ?? "",
        compiled_query: parsed.compiled_query ?? "",
        columns: parsed.columns,
        rows: parsed.rows,
        row_count: parsed.row_count,
        widget_hint: parsed.widget_hint ?? undefined,
        warnings: Array.isArray(parsed.warnings) ? parsed.warnings : undefined,
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
// Token classes for the inline Cypher highlighter — kinds map onto
// `.ql-*` CSS classes already wired in globals.css (themeable via
// CSS custom properties). Order matters during tokenisation: strings
// come first so quoted keyword text (e.g. `"MATCH"`) doesn't fragment
// into a keyword span; labels precede keywords so `:Person` doesn't
// pull the colon into a separate token.
type CypherTokenKind = "string" | "keyword" | "label" | "number" | "text";
interface CypherToken {
  kind: CypherTokenKind;
  text: string;
}

const CYPHER_KEYWORD_SET = new Set(
  [
    "MATCH",
    "WHERE",
    "RETURN",
    "WITH",
    "ORDER",
    "BY",
    "LIMIT",
    "SKIP",
    "CREATE",
    "MERGE",
    "DELETE",
    "SET",
    "REMOVE",
    "UNWIND",
    "CALL",
    "YIELD",
    "OPTIONAL",
    "UNION",
    "AS",
    "AND",
    "OR",
    "NOT",
    "IN",
    "IS",
    "NULL",
    "TRUE",
    "FALSE",
    "DISTINCT",
    "COUNT",
    "SUM",
    "AVG",
    "MIN",
    "MAX",
    "COLLECT",
    "DESC",
    "ASC",
    "EXISTS",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
  ].map((k) => k.toUpperCase()),
);

const CYPHER_TOKEN_RE =
  /('[^']*'|"[^"]*")|(:[A-Za-z_`][A-Za-z0-9_`]*)|(\b\d+(?:\.\d+)?\b)|(\b[A-Za-z_][A-Za-z0-9_]*\b)/g;

function tokenizeCypher(query: string): CypherToken[] {
  const tokens: CypherToken[] = [];
  let cursor = 0;
  for (const match of query.matchAll(CYPHER_TOKEN_RE)) {
    const idx = match.index ?? 0;
    if (idx > cursor) {
      tokens.push({ kind: "text", text: query.slice(cursor, idx) });
    }
    const [, str, label, num, word] = match;
    if (str !== undefined) tokens.push({ kind: "string", text: str });
    else if (label !== undefined) tokens.push({ kind: "label", text: label });
    else if (num !== undefined) tokens.push({ kind: "number", text: num });
    else if (word !== undefined) {
      tokens.push({
        kind: CYPHER_KEYWORD_SET.has(word.toUpperCase()) ? "keyword" : "text",
        text: word,
      });
    }
    cursor = idx + match[0].length;
  }
  if (cursor < query.length) {
    tokens.push({ kind: "text", text: query.slice(cursor) });
  }
  return tokens;
}

const CYPHER_TOKEN_CLASS: Record<Exclude<CypherTokenKind, "text">, string> = {
  string: "ql-string",
  keyword: "ql-keyword",
  label: "ql-label",
  number: "ql-number",
};

function QueryBlock({ query }: { query: string }) {
  const tokens = tokenizeCypher(query);
  return (
    <div className="group/qb relative">
      <code className="block max-h-20 overflow-auto rounded bg-surface-base px-2 py-1.5 pe-8 text-2xs font-mono leading-relaxed text-foreground-muted">
        {tokens.map((tok, i) =>
          tok.kind === "text" ? (
            <Fragment key={i}>{tok.text}</Fragment>
          ) : (
            <span key={i} className={CYPHER_TOKEN_CLASS[tok.kind]}>
              {tok.text}
            </span>
          ),
        )}
      </code>
      <div className="opacity-0 group-hover/qb:opacity-100 transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)]">
        <CopyButton text={query} />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Memory Hits List — structured recall_memory display
// ---------------------------------------------------------------------------

function MemoryHitsList({ raw }: { raw?: string }) {
  const t = useTranslations("workbench.queryBuilder.results");
  if (!raw) return null;

  let hits: { content: string; source: string; score: number }[] = [];
  try {
    const parsed = JSON.parse(raw);
    hits = parsed.hits ?? [];
  } catch {
    return <JsonPreview raw={raw} />;
  }

  if (hits.length === 0) {
    return <p className="text-xs text-foreground-muted">{t("memoriesEmpty")}</p>;
  }

  return (
    <div className="max-h-60 space-y-2 overflow-auto">
      {hits.map((hit, i) => (
        <div
          key={i}
          className="rounded border-s-2 border-warning-border bg-surface-raised py-1.5 ps-3 pe-2"
        >
          <div className="flex items-center justify-between">
            <span className="text-2xs font-medium text-warning-foreground">
              {hit.source}
            </span>
            <span className="text-2xs text-foreground-muted">
              {t("memories.matchPercent", { pct: Math.round(hit.score * 100) })}
            </span>
          </div>
          <p className="mt-0.5 text-xs text-foreground line-clamp-2-muted">
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
