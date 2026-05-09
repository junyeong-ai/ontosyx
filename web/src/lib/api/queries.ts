import type {
  PinCreateRequest,
  PinboardItem,
  PinboardItemPage,
  ClientPage,
  QueryExecution,
  QueryExecutionSummaryPage,
  QueryResult,
  QueryRawRequest,
} from "@/types/api";
import type { components } from "@/types/api.generated";
import { request, DEFAULT_TIMEOUT } from "./client";
import { normalizeQueryResult, type WireQueryResult } from "./normalization";
import { OntologyIRSchema } from "@/lib/validation";

interface RawQueryEnvelope {
  results?: WireQueryResult;
}

type WireQueryExecution = components["schemas"]["QueryExecution"];
type WireExecuteFromIrResponse = components["schemas"]["ExecuteFromIrResponse"];

function hasResultsEnvelope(raw: WireQueryResult | RawQueryEnvelope): raw is RawQueryEnvelope {
  return "results" in raw;
}

function normalizeQueryExecution(raw: WireQueryExecution): QueryExecution {
  const ontologySnapshot =
    raw.ontology_snapshot == null
      ? null
      : OntologyIRSchema.parse(raw.ontology_snapshot);
  return {
    ...raw,
    ontology_id: raw.ontology_id ?? null,
    ontology_snapshot: ontologySnapshot,
    query_ir: raw.query_ir,
    results: normalizeQueryResult(raw.results) ?? { columns: [], rows: [] },
    widget: raw.widget ?? null,
    query_bindings: raw.query_bindings,
    feedback: raw.feedback ?? undefined,
  };
}

// ---------------------------------------------------------------------------
// Raw Query
// ---------------------------------------------------------------------------

export async function rawQuery(req: QueryRawRequest): Promise<QueryResult> {
  const raw = await request<
    | WireQueryResult
    | {
        results?: WireQueryResult;
      }
  >("/query/raw", {
    method: "POST",
    body: JSON.stringify(req),
  });
  // Backend wraps results: { query, target, results: { columns, rows } }
  const results = hasResultsEnvelope(raw) ? raw.results : raw;
  return normalizeQueryResult(results) ?? { columns: [], rows: [] };
}

// ---------------------------------------------------------------------------
// Query Execution History
// ---------------------------------------------------------------------------

export async function listExecutions(params?: {
  cursor?: string;
  limit?: number;
}): Promise<QueryExecutionSummaryPage> {
  const qs = new URLSearchParams();
  if (params?.cursor) qs.set("cursor", params.cursor);
  if (params?.limit) qs.set("limit", String(params.limit));
  const query = qs.toString();
  return request<QueryExecutionSummaryPage>(`/query/history${query ? `?${query}` : ""}`);
}

export async function getExecution(id: string): Promise<QueryExecution> {
  const raw = await request<WireQueryExecution>(
    `/query/history/${encodeURIComponent(id)}`,
  );
  return normalizeQueryExecution(raw);
}

// ---------------------------------------------------------------------------
// Query Feedback
// ---------------------------------------------------------------------------

export async function setQueryFeedback(
  executionId: string,
  feedback: "positive" | "negative" | null,
): Promise<void> {
  await request(`/query/history/${encodeURIComponent(executionId)}/feedback`, {
    method: "PATCH",
    body: JSON.stringify({ feedback }),
  });
}

// ---------------------------------------------------------------------------
// Graph Search
// ---------------------------------------------------------------------------

/** Backend search result node (structured, not QueryResult) */
export type BackendSearchNode = components["schemas"]["SearchResultNode"];

export async function searchGraph(
  query: string,
  limit?: number,
  labels?: string[],
): Promise<BackendSearchNode[]> {
  return request<BackendSearchNode[]>("/search", {
    method: "POST",
    body: JSON.stringify({ query, limit: limit ?? 20, labels }),
    timeout: DEFAULT_TIMEOUT,
  });
}

// ---------------------------------------------------------------------------
// Node Expansion (1-hop neighbors)
// ---------------------------------------------------------------------------

export type ExpandNeighbor = components["schemas"]["ExpandNeighbor"];

export type ExpandResult = components["schemas"]["NodeExpansion"];

export async function expandNode(
  elementId: string,
  limit?: number,
): Promise<ExpandResult> {
  return request<ExpandResult>("/search/expand", {
    method: "POST",
    body: JSON.stringify({ element_id: elementId, limit: limit ?? 50 }),
    timeout: DEFAULT_TIMEOUT,
  });
}

// ---------------------------------------------------------------------------
// Graph Overview (schema-level statistics)
// ---------------------------------------------------------------------------

export type LabelStat = components["schemas"]["LabelStat"];
export type RelationshipPattern = components["schemas"]["RelationshipPattern"];
export type GraphOverview = components["schemas"]["GraphSchemaOverview"];

export async function fetchGraphOverview(): Promise<GraphOverview> {
  return request<GraphOverview>("/graph/overview", {
    timeout: DEFAULT_TIMEOUT,
  });
}

// ---------------------------------------------------------------------------
// Pinboard
// ---------------------------------------------------------------------------

export async function createPin(req: PinCreateRequest): Promise<PinboardItem> {
  return request("/pins", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function listPins(params?: {
  cursor?: string;
  limit?: number;
}): Promise<PinboardItemPage> {
  const qs = new URLSearchParams();
  if (params?.cursor) qs.set("cursor", params.cursor);
  if (params?.limit) qs.set("limit", String(params.limit));
  const query = qs.toString();
  return request<PinboardItemPage>(`/pins${query ? `?${query}` : ""}`);
}

export async function deletePin(id: string): Promise<void> {
  return request(`/pins/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

// ---------------------------------------------------------------------------
// Saved PatternIR (canvas layout persistence)
// ---------------------------------------------------------------------------
//
// The wire shape mirrors the Rust `SavedPatternResponse`. `pattern_ir`
// carries the full PatternIR (nodes + edges + filters + `positions` +
// `layout_hints`) so re-opening a saved pattern restores the canvas
// layout without a re-layout pass.

export type PatternIRJson = components["schemas"]["PatternIR"];
export type SavedPattern = components["schemas"]["SavedPatternResponse"];
export type SavedPatternPage =
  ClientPage<components["schemas"]["SavedPatternResponsePage"]>;
export type CreateSavedPatternRequest =
  components["schemas"]["CreateSavedPatternRequest"];
export type UpdateSavedPatternRequest =
  components["schemas"]["UpdateSavedPatternRequest"];

export async function createSavedPattern(
  req: CreateSavedPatternRequest,
): Promise<SavedPattern> {
  return request<SavedPattern>("/query/pattern/saved", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function listSavedPatterns(
  params?: { cursor?: string; limit?: number },
): Promise<SavedPatternPage> {
  const qs = new URLSearchParams();
  if (params?.cursor) qs.set("cursor", params.cursor);
  if (params?.limit) qs.set("limit", String(params.limit));
  const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
  return request<SavedPatternPage>(`/query/pattern/saved${suffix}`);
}

export async function getSavedPattern(id: string): Promise<SavedPattern> {
  return request(`/query/pattern/saved/${encodeURIComponent(id)}`);
}

export async function updateSavedPattern(
  id: string,
  req: UpdateSavedPatternRequest,
): Promise<void> {
  await request(`/query/pattern/saved/${encodeURIComponent(id)}`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function deleteSavedPattern(id: string): Promise<void> {
  await request(`/query/pattern/saved/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

// ---------------------------------------------------------------------------
// QueryIR Execution (visual query builder)
// ---------------------------------------------------------------------------

export async function executeFromIr(
  queryIr: components["schemas"]["QueryIR"],
  _ontologyId?: string,
): Promise<WireExecuteFromIrResponse> {
  return request<WireExecuteFromIrResponse>("/query/from-ir", {
    method: "POST",
    body: JSON.stringify({ query_ir: queryIr }),
  });
}
