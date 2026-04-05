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
  /** When querying against a saved ontology, pass its UUID to avoid redundant snapshot storage */
  saved_ontology_id?: string;
  /** Active project ID for edit operations */
  project_id?: string;
  /** Current project revision (required for edit operations) */
  project_revision?: number;
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
  title?: string;
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
}

// --- Query Execution (returned by GET /api/query/history/:id) ---

export interface QueryExecution {
  id: string;
  user_id: string;
  question: string;
  ontology_id: string;
  ontology_version: number;
  saved_ontology_id: string | null;
  /** Resolved ontology snapshot (inline or via saved_ontology JOIN) */
  ontology_snapshot: OntologyIR;
  query_ir: QueryIR;
  compiled_target: string;
  compiled_query: string;
  results: QueryResult;
  widget?: Record<string, unknown>;
  explanation: string;
  model: string;
  execution_time_ms: number;
  query_bindings?: ResolvedQueryBindings;
  /** User feedback: "positive" or "negative" */
  feedback?: string | null;
  created_at: string;
}

export type QueryFeedback = "positive" | "negative";

// --- Query Execution Summary (returned by GET /api/query/history) ---

export interface QueryExecutionSummary {
  id: string;
  question: string;
  ontology_id: string;
  ontology_version: number;
  compiled_target: string;
  model: string;
  execution_time_ms: number;
  row_count: number;
  has_widget: boolean;
  created_at: string;
}

// --- Insight Suggestions ---

export interface InsightSuggestion {
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
    neo4j: string;
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
