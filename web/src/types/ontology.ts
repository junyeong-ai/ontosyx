// ---------------------------------------------------------------------------
// Core ontology types — IR, query, widget, commands
// ---------------------------------------------------------------------------

export type Cardinality = "one_to_one" | "one_to_many" | "many_to_one" | "many_to_many";

// Cursor-based pagination
export interface CursorPage<T> {
  items: T[];
  next_cursor?: string;
}

export interface OntologyIR {
  id: string;
  name: string;
  description?: string | null;
  version: number;
  node_types: NodeTypeDef[];
  edge_types: EdgeTypeDef[];
  indexes?: IndexDef[];

  // ---------------------------------------------------------------------
  // Vocabulary collections (Phase Ω) — surfaced for the Φ4 admin
  // CRUD pages. The exact shapes mirror the OntologyEditOp
  // discriminated union in `lib/api/edit-ops.ts`; we keep them as
  // `unknown[]` here to avoid pulling the full Def shapes into this
  // module. Pages cast through the edit-ops shapes.
  // ---------------------------------------------------------------------
  /** `GlossaryTermDef[]` per `lib/api/edit-ops.ts`. */
  glossary?: unknown[];
  /** `CodeSystemDef[]` per `lib/api/edit-ops.ts`. */
  code_systems?: unknown[];
  /** `ValueSetDef[]` per `lib/api/edit-ops.ts`. */
  value_sets?: unknown[];
  /** `NotationPatternDef[]` per `lib/api/edit-ops.ts`. */
  notation_patterns?: unknown[];
  /** `ConceptMapDef[]` per `lib/api/edit-ops.ts`. */
  concept_maps?: unknown[];
  /** `RuleDef[]` per `lib/api/edit-ops.ts`. */
  rules?: unknown[];
  /** PROV-O `ProvenanceDef[]`. Each entry carries subject /
   *  activity / agent / at_time + used + derived_from. The Φ6
   *  audit trail viewer reads this slice. */
  provenance?: unknown[];
}

export interface NodeTypeDef {
  id: string;
  label: string;
  description?: string | null;
  /** Source table name this node was derived from (set by LLM for DB sources) */
  source_table?: string | null;
  properties: PropertyDef[];
  constraints?: ConstraintDef[];
}

export interface EdgeTypeDef {
  id: string;
  label: string;
  description?: string | null;
  source_node_id: string;
  target_node_id: string;
  properties: PropertyDef[];
  cardinality?: Cardinality;
}

/** Tagged property type from backend: `{"type": "string"}`, `{"type": "list", "element": {...}}` */
export type PropertyType = { type: string; element?: PropertyType };

/** Display a PropertyType as a human-readable string, e.g. "string", "list<int>" */
export function formatPropertyType(pt: PropertyType): string {
  if (pt.type === "list" && pt.element) {
    return `list<${formatPropertyType(pt.element)}>`;
  }
  return pt.type;
}

export type DataClassification = "public" | "internal" | "confidential" | "restricted";

export interface PropertyDef {
  id: string;
  name: string;
  property_type: PropertyType;
  nullable?: boolean;
  default_value?: unknown;
  description?: string | null;
  /** Source column name this property was derived from (set by LLM for DB sources) */
  source_column?: string | null;
  /** Data sensitivity classification (derived from PII detection) */
  classification?: DataClassification | null;
  /**
   * Phase 5-B semantic pointer. Links the technical property to the
   * business concept it realises. Set via
   * `OntologyEditOp::BindPropertyToTerm` through `/api/ontologies/{id}/edits`.
   */
  glossary_term_id?: string | null;
  /**
   * Phase Ω pointer. When set, values must be codes from the named
   * ValueSet; the runtime SHACL validator enforces it.
   */
  value_set_id?: string | null;
  /**
   * Phase Ω pointer. When set, values must match the named
   * NotationPattern.
   */
  notation_pattern_id?: string | null;
}

export type ConstraintDef =
  | { id: string; type: "unique"; property_ids: string[] }
  | { id: string; type: "exists"; property_id: string }
  | { id: string; type: "node_key"; property_ids: string[] };

export interface IndexDef {
  id: string;
  type: string;
  node_id: string;
  property_id?: string;
  property_ids?: string[];
  name?: string;
  dimensions?: number;
  similarity?: string;
}

export interface QueryIR {
  operation: QueryOp;
  limit?: number | null;
  skip?: number | null;
  order_by: OrderClause[];
}

export type QueryOp = Record<string, unknown> & {
  op: string;
};

export interface OrderClause {
  projection: Record<string, unknown>;
  direction: "asc" | "desc";
}

export interface QueryResult {
  columns: string[];
  rows: Record<string, unknown>[];
  /** Structured execution metadata. Shape mirrors the Rust
   *  `QueryMetadata` so fields added on the backend surface here
   *  once types are regenerated. Unknown extra keys stay in the
   *  same object. */
  metadata?: QueryMetadata;
}

/**
 * Execution-time facts about a completed query. The Rust
 * `QueryMetadata` struct carries these — `rows_returned`,
 * `execution_time_ms`, optional mutation counts, and the Π-3
 * `provenance` field.
 */
export interface QueryMetadata {
  execution_time_ms: number;
  rows_returned: number;
  nodes_affected?: number | null;
  edges_affected?: number | null;
  provenance?: QueryProvenance;
  /**
   * Non-blocking diagnostics produced by the advisory validator pass
   * (Cypher complexity + semantic-guard). Errors would have rejected
   * the query upstream; strict-pass errors that slipped through a
   * *permissive* runtime pipeline surface here as `level: "error"`.
   *
   * Structured rather than pre-formatted so the UI can filter by
   * `level` or `validator` without parsing a string. Empty on the
   * federation / DataFusion path — the Cypher-specific validators
   * don't apply to a DataFusion LogicalPlan. Treat `[]` and missing
   * identically.
   */
  warnings?: QueryDiagnostic[];
  [extra: string]: unknown;
}

/** Severity tier for a {@link QueryDiagnostic}. */
export type DiagnosticLevel = "error" | "warning" | "info";

/**
 * Structured advisory diagnostic mirroring the Rust
 * `ox_query_ir::query::QueryDiagnostic`. `validator` matches
 * `CypherValidator::name()` on the backend (e.g. `"complexity"`,
 * `"semantic-guard"`); `level` drives UI colour/iconography;
 * `message` is author-level English.
 */
export interface QueryDiagnostic {
  validator: string;
  level: DiagnosticLevel;
  message: string;
}

/**
 * Π-3 response-attribution trail. All fields are optional — each
 * planner/runtime layer populates what it knows.
 *
 * - `ontology_id` / `ontology_version`: which schema produced this
 *   response.
 * - `as_of`: business-time anchor when the temporal rewriter ran.
 * - `source_ids`: federation-only — data sources the plan touched.
 * - `type_ids`: node / edge type ids that participated.
 * - `filter_summary`: compact human-readable description of the
 *   WHERE clause; `null`/missing on raw-query paths.
 */
export interface QueryProvenance {
  ontology_id?: string;
  ontology_version?: string;
  as_of?: string;
  source_ids?: string[];
  type_ids?: string[];
  filter_summary?: string;
}

export type WidgetSpec = Record<string, unknown> & {
  widget_type?: string;
  title?: string;
  reason?: string;
  chart_type?: string;
  content?: string;
  x_axis?: { field?: string };
  y_axis?: { field?: string };
  data_mapping?: {
    label?: string;
    value?: string;
    delta?: string;
  };
  series?: Array<{ field?: string }>;
  columns?: Array<{ key: string; label?: string }>;
  thresholds?: {
    warning?: number;
    critical?: number;
    direction?: "above" | "below";
  };
  // Graph-specific fields (from GraphSpec)
  node_config?: NodeVizConfig;
  edge_config?: EdgeVizConfig;
  layout?: GraphLayout;
  interactive?: boolean;
  zoom_enabled?: boolean;
  max_nodes?: number;
};

// ---------------------------------------------------------------------------
// GraphSpec — interactive graph visualization types
// ---------------------------------------------------------------------------

export type GraphLayout = "force" | "hierarchical" | "radial" | "dagre";

export interface NodeVizConfig {
  label_field: string;
  color_field?: string;
  color_map?: ColorMapping[];
  size_field?: string;
  tooltip_fields: string[];
}

export interface EdgeVizConfig {
  label_field?: string;
  color_field?: string;
  weight_field?: string;
  directed: boolean;
}

export interface ColorMapping {
  value: string;
  color: string;
}

// --- OntologyCommand (command engine for graph editing) ---

export type OntologyCommand =
  | { op: "add_node"; id: string; label: string; description?: string; source_table?: string }
  | { op: "delete_node"; node_id: string }
  | { op: "rename_node"; node_id: string; new_label: string }
  | { op: "update_node_description"; node_id: string; description?: string }
  | { op: "add_edge"; id: string; label: string; source_node_id: string; target_node_id: string; cardinality: Cardinality }
  | { op: "delete_edge"; edge_id: string }
  | { op: "rename_edge"; edge_id: string; new_label: string }
  | { op: "update_edge_cardinality"; edge_id: string; cardinality: Cardinality }
  | { op: "update_edge_description"; edge_id: string; description?: string }
  | { op: "add_property"; owner_id: string; property: PropertyDef }
  | { op: "delete_property"; owner_id: string; property_id: string }
  | { op: "update_property"; owner_id: string; property_id: string; patch: PropertyPatch }
  | { op: "add_constraint"; node_id: string; constraint: ConstraintDef }
  | { op: "remove_constraint"; node_id: string; constraint_id: string }
  | { op: "add_index"; index: IndexDef }
  | { op: "remove_index"; index_id: string }
  | { op: "batch"; description: string; commands: OntologyCommand[] };

export interface PropertyPatch {
  name?: string;
  property_type?: PropertyType;
  nullable?: boolean;
  default_value?: unknown | null;
  description?: string | null;
  source_column?: string | null;
}

// --- Ontologies (Λ storage model) ---

/**
 * LocalizedText JSONB shape stored on `ontologies.description`.
 * Matches the Rust `LocalizedText { default, translations }` struct.
 */
export interface LocalizedText {
  default: string;
  translations?: Record<string, string>;
}

/**
 * Summary of an ontology's current committed version. Attached to
 * identity rows by the list + detail endpoints. `None` when the
 * identity exists but nothing has been committed yet.
 */
export interface CurrentVersionSummary {
  version_id: string;
  version: string;
  committed_by: string;
  commit_message: string;
  created_at: string;
}

/**
 * One row from `GET /api/ontologies`. Identity-only: the IR itself is
 * intentionally omitted so a 50-row page doesn't pull 50 full hydrated
 * ontologies. Fetch `OntologyDetail` via `GET /api/ontologies/:id` when
 * the IR is needed (e.g. loading into the canvas).
 */
export interface OntologyListItem {
  id: string;
  lineage_id: string;
  name: string;
  description: LocalizedText;
  created_at: string;
  updated_at: string;
  current_version?: CurrentVersionSummary;
}

/**
 * Full detail: identity + current version summary + hydrated IR.
 * `ontology_ir` is `undefined` iff `current_version` is also undefined
 * (identity exists without any committed version yet).
 */
export interface OntologyDetail {
  id: string;
  lineage_id: string;
  name: string;
  description: LocalizedText;
  created_at: string;
  updated_at: string;
  current_version?: CurrentVersionSummary;
  ontology_ir?: OntologyIR;
}

export interface PromptInfo {
  name: string;
  version: string;
}

// --- Element Verification ---

export interface ElementVerification {
  id: string;
  ontology_lineage_id: string;
  element_id: string;
  element_kind: "node" | "edge" | "property";
  verified_by: string;
  verified_by_name?: string;
  review_notes?: string;
  invalidated_at?: string;
  invalidation_reason?: string;
  created_at: string;
}
