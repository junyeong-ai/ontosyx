import type {
  CursorPage,
  PinCreateRequest,
  PinboardItem,
  QueryExecution,
  QueryExecutionSummary,
  QueryResult,
  QueryRawRequest,
} from "@/types/api";
import { request, DEFAULT_TIMEOUT } from "./client";
import { normalizeQueryResult } from "./normalization";

// ---------------------------------------------------------------------------
// Raw Query
// ---------------------------------------------------------------------------

export async function rawQuery(req: QueryRawRequest): Promise<QueryResult> {
  const raw = await request<Record<string, unknown>>("/query/raw", {
    method: "POST",
    body: JSON.stringify(req),
  });
  // Backend wraps results: { query, target, results: { columns, rows } }
  const results = raw.results ?? raw;
  return normalizeQueryResult(results) ?? { columns: [], rows: [] };
}

// ---------------------------------------------------------------------------
// Query Execution History
// ---------------------------------------------------------------------------

export async function listExecutions(params?: {
  cursor?: string;
  limit?: number;
}): Promise<CursorPage<QueryExecutionSummary>> {
  const qs = new URLSearchParams();
  if (params?.cursor) qs.set("cursor", params.cursor);
  if (params?.limit) qs.set("limit", String(params.limit));
  const query = qs.toString();
  return request(`/query/history${query ? `?${query}` : ""}`);
}

export async function getExecution(id: string): Promise<QueryExecution> {
  const raw = await request<QueryExecution>(`/query/history/${encodeURIComponent(id)}`);
  // Backend stores results as raw PropertyValue-wrapped rows; normalize for display
  const normalized = normalizeQueryResult(raw.results);
  if (normalized) {
    raw.results = normalized;
  }
  return raw;
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
export interface BackendSearchNode {
  element_id: string;
  labels: string[];
  props: Record<string, unknown>;
}

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

export interface ExpandNeighbor {
  element_id: string;
  labels: string[];
  props: Record<string, unknown>;
  relationship_type: string;
  direction: "outgoing" | "incoming";
}

export interface ExpandResult {
  source_id: string;
  neighbors: ExpandNeighbor[];
}

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

export interface LabelStat {
  label: string;
  count: number;
}

export interface RelationshipPattern {
  from_label: string;
  rel_type: string;
  to_label: string;
  count: number;
}

export interface GraphOverview {
  labels: LabelStat[];
  relationships: RelationshipPattern[];
  total_nodes: number;
  total_relationships: number;
}

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
}): Promise<CursorPage<PinboardItem>> {
  const qs = new URLSearchParams();
  if (params?.cursor) qs.set("cursor", params.cursor);
  if (params?.limit) qs.set("limit", String(params.limit));
  const query = qs.toString();
  return request(`/pins${query ? `?${query}` : ""}`);
}

export async function deletePin(id: string): Promise<void> {
  return request(`/pins/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

// ---------------------------------------------------------------------------
// QueryIR Execution (visual query builder)
// ---------------------------------------------------------------------------

export async function executeFromIr(
  queryIr: unknown,
  ontologyId?: string,
): Promise<{
  compiled_query: string;
  compiled_target: string;
  result: { columns: string[]; rows: unknown[][] };
  widget_hint?: unknown;
}> {
  return request<{
    compiled_query: string;
    compiled_target: string;
    result: { columns: string[]; rows: unknown[][] };
    widget_hint?: unknown;
  }>("/query/from-ir", {
    method: "POST",
    body: JSON.stringify({ query_ir: queryIr, ontology_id: ontologyId }),
  });
}
