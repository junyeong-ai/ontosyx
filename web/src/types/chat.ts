// ---------------------------------------------------------------------------
// Chat types — requests, pinboard, raw query, execution, suggestions, health
// ---------------------------------------------------------------------------

import type {
  OntologyIR,
  QueryIR,
  QueryResult,
} from "./ontology";

import type {
  ResolvedQueryBindings,
} from "./quality";

// --- Chat API ---

export interface ChatStreamRequest {
  message: string;
  ontology: OntologyIR;
  /** Identity uuid (`ontologies.id`) to pin this session to. Omit for
   *  ad-hoc sessions against a draft IR that hasn't been committed yet. */
  ontology_id?: string;
  /** Active project ID for edit operations */
  ontology_draft_id?: string;
  /** Current project revision (required for edit operations) */
  ontology_draft_revision?: number;
  /** Resume an existing session for multi-turn conversation */
  session_id?: string;
  /** Agent execution mode: auto runs tools immediately, supervised requires approval */
  execution_mode?: "auto" | "supervised";
  /** Override the default model for this chat request */
  model_override?: string;
}

export interface CompiledQuery {
  target: string;
  statement: string;
  params?: Record<string, unknown>;
}

// --- Pinboard ---

export interface PinboardItem {
  id: string;
  query_execution_id: string;
  user_id: string;
  widget_spec: Record<string, unknown>;
  title: string | null;
  pinned_at: string;
}

export interface PinCreateRequest {
  query_execution_id: string;
  widget_spec: Record<string, unknown>;
  title?: string;
}

// --- Raw Query ---

export interface QueryRawRequest {
  query: string;
  /** Optional ontology identity id. When provided the backend's
   *  OntologyValidator rejects unknown labels / relationship types /
   *  properties before the query hits the driver. Omit to run with
   *  safety + workspace-scope only. */
  ontology_id?: string;
}

// --- Query Execution (returned by GET /api/query/history/:id) ---

export interface QueryExecution {
  id: string;
  user_id: string;
  question: string;
  ontology_lineage_id: string;
  ontology_version: number;
  ontology_id: string | null;
  /** Resolved ontology snapshot. Inline when the execution was a draft
   *  (`ontology_id` null); otherwise `null` — the caller resolves the
   *  hydrated IR via `OntologyVersionStore` using `ontology_id` + `created_at`. */
  ontology_snapshot: OntologyIR | null;
  query_ir: QueryIR;
  compiled_target: string;
  compiled_query: string;
  results: QueryResult;
  widget: Record<string, unknown> | null;
  explanation: string;
  model: string;
  execution_time_ms: number;
  query_bindings?: ResolvedQueryBindings;
  /** User feedback: "positive" or "negative" */
  feedback?: string;
  created_at: string;
}

export type QueryFeedback = "positive" | "negative";

// --- Query Execution Summary (returned by GET /api/query/history) ---

export interface QueryExecutionSummary {
  id: string;
  question: string;
  ontology_lineage_id: string;
  ontology_version: number;
  compiled_target: string;
  model: string;
  execution_time_ms: number;
  row_count: number;
  has_widget: boolean;
  created_at: string;
}

// --- Insight Suggestions ---

export interface InsightHint {
  question: string;
  category: string;
  suggested_tool: string;
}

// --- Health Check ---

export interface HealthResponse {
  status: string;
  service: string;
  version: string;
  components: {
    postgres: string;
    graph: string;
    /** Actual graph backend name (e.g. "Neo4j", "Memgraph", "Neptune", "none") */
    graph_backend?: string;
    llm: {
      provider: string;
      model: string;
    };
  };
}

// --- Session Messages (restoration) ---

export interface SessionMessage {
  role: "user" | "assistant";
  content: string;
  thinking?: string;
  tool_calls?: {
    id: string;
    name: string;
    input?: unknown;
    output?: string;
    status: "done" | "error" | "review";
    duration_ms?: number;
  }[];
}
