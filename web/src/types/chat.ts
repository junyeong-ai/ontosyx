// ---------------------------------------------------------------------------
// Chat types — requests, pinboard, raw query, execution, suggestions, health
// ---------------------------------------------------------------------------

import type { OntologyIR, QueryResult } from "./ontology";
import type { ClientPage } from "./pagination";
import type { ResolvedQueryBindings } from "./quality";
import type { components } from "./api.generated";

export type WidgetHint = components["schemas"]["WidgetHint"];
export type QueryIR = components["schemas"]["QueryIR"];

// --- Chat API ---

export type ChatStreamRequest = Omit<
  components["schemas"]["ChatStreamRequest"],
  "ontology"
> & {
  ontology: OntologyIR;
};

// --- Pinboard ---

export type PinboardItem = components["schemas"]["PinboardItem"];
export type PinboardItemPage = ClientPage<components["schemas"]["PinboardItemPage"]>;
export type PinCreateRequest = components["schemas"]["CreatePinRequest"];

// --- Raw Query ---

export type QueryRawRequest = components["schemas"]["ExecuteRawQueryRequest"];

// --- Query Execution (returned by GET /api/query/history/:id) ---

export type QueryExecution = Omit<
  components["schemas"]["QueryExecution"],
  "ontology_snapshot" | "query_bindings" | "results"
> & {
  ontology_snapshot: OntologyIR | null;
  results: QueryResult;
  query_bindings?: ResolvedQueryBindings;
};

export type QueryFeedback = NonNullable<
  components["schemas"]["SubmitQueryFeedbackRequest"]["feedback"]
>;

// --- Query Execution Summary (returned by GET /api/query/history) ---

export type QueryExecutionSummary = components["schemas"]["QueryExecutionSummary"];
export type QueryExecutionSummaryPage =
  ClientPage<components["schemas"]["QueryExecutionSummaryPage"]>;

// --- Insight Suggestions ---

export type InsightHint = components["schemas"]["InsightHint"];

// --- Health Check ---

export type HealthResponse = components["schemas"]["HealthResponse"];

// --- Session Messages (restoration) ---

export type SessionMessage = components["schemas"]["SessionChatMessage"];
