// ---------------------------------------------------------------------------
// Quality, diff, perspective, bindings, reconciliation, and streaming types
// ---------------------------------------------------------------------------

import type {
  Cardinality,
  NodeTypeDef,
  EdgeTypeDef,
  LocalizedText,
  PropertyDef,
  OntologyIR,
} from "./ontology";

import type {
  DesignProject,
} from "./projects";

// --- Ontology quality report (returned by design for DB sources) ---

export type QualityGapSeverity = "high" | "medium" | "low";

export type QualityGapCategory =
  | "opaque_enum_value"
  | "numeric_enum_code"
  | "single_value_bias"
  | "small_sample"
  | "missing_description"
  | "sparse_property"
  | "unmapped_source_table"
  | "missing_foreign_key_edge"
  | "missing_containment_edge"
  | "unmapped_source_column"
  | "duplicate_edge"
  | "orphan_node"
  | "property_type_inconsistency"
  | "hub_node"
  | "overloaded_property"
  | "self_referential_edge";

export type QualityGapRef =
  | { ref_type: "node"; node_id: string; label: string }
  | { ref_type: "node_property"; node_id: string; property_id: string; label: string; property_name: string }
  | { ref_type: "edge"; edge_id: string; label: string }
  | { ref_type: "edge_property"; edge_id: string; property_id: string; label: string; property_name: string }
  | { ref_type: "source_table"; table: string }
  | { ref_type: "source_column"; table: string; column: string }
  | { ref_type: "source_foreign_key"; from_table: string; from_column: string; to_table: string; to_column: string };

export interface QualityGap {
  severity: QualityGapSeverity;
  category: QualityGapCategory;
  location: QualityGapRef;
  /**
   * Interpolation values for the FE i18n catalogue. The (category, location.ref_type)
   * pair picks the message key; keys live under `qualityGap.<category>` (with `.node` /
   * `.node_property` / `.edge` / `.edge_property` sub-paths for `missing_description`).
   * Render via `localizeQualityGap()` from `@/lib/quality-gap-text`.
   */
  params: Record<string, string>;
}

export type QualityConfidence = "high" | "medium" | "low";

export interface OntologyQualityReport {
  confidence: QualityConfidence;
  gaps: QualityGap[];
}

// --- Ontology Revision History ---

export interface RevisionSummary {
  id: string;
  revision: number;
  created_at: string;
  node_count: number;
  edge_count: number;
}

export interface OntologySnapshot {
  id: string;
  project_id: string;
  revision: number;
  ontology: OntologyIR;
  quality_report: OntologyQualityReport | null;
  created_at: string;
}

export interface RestoreProjectRevisionResponse {
  project: DesignProject;
}

// --- Ontology Diff ---

export interface OntologyDiff {
  added_nodes: NodeTypeDef[];
  removed_nodes: NodeTypeDef[];
  modified_nodes: NodeDiffEntry[];
  added_edges: EdgeTypeDef[];
  removed_edges: EdgeTypeDef[];
  modified_edges: EdgeDiffEntry[];
  summary: DiffSummary;
}

export interface NodeDiffEntry {
  node_id: string;
  label: string;
  changes: NodeChange[];
}

export type NodeChange =
  | { type: "label_changed"; old: string; new: string }
  | { type: "description_changed"; old: LocalizedText; new: LocalizedText }
  | { type: "property_added"; property: PropertyDef }
  | { type: "property_removed"; property: PropertyDef }
  | { type: "property_modified"; property_name: string; changes: PropertyChange[] }
  | { type: "constraint_added"; constraint: string }
  | { type: "constraint_removed"; constraint: string };

export type PropertyChange =
  | { type: "type_changed"; old: string; new: string }
  | { type: "nullability_changed"; old: boolean; new: boolean }
  | { type: "description_changed"; old: LocalizedText; new: LocalizedText }
  | { type: "default_value_changed"; old: string | null; new: string | null };

export interface EdgeDiffEntry {
  edge_id: string;
  label: string;
  changes: EdgeChange[];
}

export type EdgeChange =
  | { type: "label_changed"; old: string; new: string }
  | { type: "description_changed"; old: LocalizedText; new: LocalizedText }
  | { type: "source_changed"; old: string; new: string }
  | { type: "target_changed"; old: string; new: string }
  | { type: "cardinality_changed"; old: Cardinality; new: Cardinality }
  | { type: "property_added"; property: PropertyDef }
  | { type: "property_removed"; property: PropertyDef }
  | { type: "property_modified"; property_name: string; changes: PropertyChange[] };

export interface DiffSummary {
  total_changes: number;
  nodes_added: number;
  nodes_removed: number;
  nodes_modified: number;
  edges_added: number;
  edges_removed: number;
  edges_modified: number;
  properties_added: number;
  properties_removed: number;
}

// --- Workbench Perspective ---

export interface WorkbenchPerspective {
  id: string;
  user_id: string;
  lineage_id: string;
  topology_signature: string;
  project_id?: string;
  name: string;
  positions: Record<string, { x: number; y: number }>;
  viewport: { x: number; y: number; zoom: number };
  filters: Record<string, unknown>;
  collapsed_groups: string[];
  is_default: boolean;
  created_at: string;
  updated_at: string;
}

export interface UpsertPerspectiveRequest {
  lineage_id: string;
  topology_signature: string;
  project_id?: string;
  name: string;
  positions: Record<string, { x: number; y: number }>;
  viewport: { x: number; y: number; zoom: number };
  filters?: Record<string, unknown>;
  collapsed_groups?: string[];
  is_default?: boolean;
}

// --- Resolved Query Bindings (scope-aware provenance for graph highlighting) ---

export type BindingKind = "match" | "path_find" | "chain" | "exists" | "mutation";

export type ScopeSegment =
  | { type: "root" }
  | { type: "union_branch"; index: number }
  | { type: "exists_subquery"; depth: number }
  | { type: "chain_step"; index: number };

export type PropertyUsageHint =
  | "pattern_filter"
  | "where_filter"
  | "projection"
  | "order_by"
  | "group_by"
  | "aggregation"
  | "mutation"
  | "general";

export interface ResolvedQueryBindings {
  node_bindings: NodeBinding[];
  edge_bindings: EdgeBinding[];
  property_bindings: QueryPropertyBinding[];
}

export interface NodeBinding {
  variable: string;
  node_id: string;
  label: string;
  binding_kind: BindingKind;
  pattern_index: number;
  scope_path: ScopeSegment[];
}

export interface EdgeBinding {
  variable?: string;
  edge_id: string;
  label: string;
  source_node_id: string;
  target_node_id: string;
  binding_kind: BindingKind;
  pattern_index: number;
  scope_path: ScopeSegment[];
}

/// Property binding diagnostic surfaced inside a query-quality
/// report. Distinct from `ontology.ts::PropertyBinding`, which
/// names the semantic binding on a `PropertyDef`.
export interface QueryPropertyBinding {
  owner_variable?: string;
  property_name: string;
  property_id: string;
  owner_id: string;
  binding_kind: BindingKind;
  scope_path: ScopeSegment[];
  usage_hint: PropertyUsageHint;
}

// --- Reconcile (LLM refine diff) ---

export type EntityKind = "node" | "edge" | "property" | "constraint" | "index";
export type ReconcileConfidence = "high" | "medium" | "low";

export interface UncertainMatch {
  original_id: string;
  original_label: string;
  matched_label: string;
  match_reason: string;
  entity_kind: EntityKind;
}

export interface ReconcileReport {
  preserved_ids: Array<{ id: string; label: string; entity_kind: EntityKind }>;
  generated_ids: Array<{ id: string; label: string; entity_kind: EntityKind }>;
  uncertain_matches: UncertainMatch[];
  deleted_entities: Array<{ id: string; label: string; entity_kind: EntityKind }>;
  confidence: ReconcileConfidence;
}

export interface MatchDecision {
  original_id: string;
  accept: boolean;
}

export interface PendingReconcile {
  report: ReconcileReport;
  reconciled_ontology: OntologyIR;
}

export interface ReconcileProjectRequest {
  revision: number;
  reconciled_ontology: OntologyIR;
  decisions: MatchDecision[];
  uncertain_matches: UncertainMatch[];
}

export interface RefineProjectResponse {
  project: DesignProject;
  profile_summary: string;
  reconcile_report: ReconcileReport;
}

// --- Design/Refine SSE streaming ---

export type DesignPhase =
  | "validating"
  | "designing"
  | "assessing_quality"
  | "persisting";

export type RefinePhase =
  | "validating"
  | "profiling"
  | "profiling_complete"
  | "refining"
  | "reconciling"
  | "assessing_quality"
  | "persisting";

export interface PhaseEvent {
  phase: DesignPhase | RefinePhase;
  detail?: string;
}

export interface DesignProjectResponse {
  project: DesignProject;
}

// ---------------------------------------------------------------------------
// Signal-backed 6 창 metrics — matches Rust `ox_store::quality_signal`.
// ---------------------------------------------------------------------------

/** Wire-stable window enum on the server; UI keeps the bare-day shape. */
export type MetricWindowApi = "last7d" | "last30d" | "last90d";

/** One dashboard tile: value + trend + Wilson 95% CI band. */
export interface MetricValue {
  value: number;
  trend_delta: number;
  lower_bound_95: number;
  upper_bound_95: number;
}

export interface QualityMetricsReport {
  anchor_match_rate: MetricValue;
  glossary_hit_rate: MetricValue;
  clarification_success_rate: MetricValue;
  query_reproducibility: MetricValue;
  shacl_pass_rate: MetricValue;
  stale_concept_ratio: MetricValue;
  sample_size: number;
  window: MetricWindowApi;
}

export type ShaclFailureKind =
  | "cardinality_violation"
  | "measure_group_by"
  | "unknown_coded_value"
  | "mandatory_property_missing"
  | "temporal_grain_mismatch"
  | "other";

export interface ShaclFailureCount {
  kind: ShaclFailureKind;
  count: number;
}

export interface StaleTypeEntry {
  workspace_id: string;
  type_id: string;
  type_kind: string;
  last_used_at: string | null;
  days_since_last_use: number;
}

/** Decision status for a `StaleConceptProposal`. */
export type StaleProposalDecision = "pending" | "approved" | "dismissed";

/**
 * One durable deprecation proposal written by the daily stale-concept
 * cron. Natural key guarantees one open row per type; a terminal
 * decision is preserved across re-runs.
 */
export interface StaleConceptProposal {
  id: string;
  workspace_id: string;
  type_id: string;
  type_kind: string;
  last_used_at: string | null;
  days_since_last_use: number;
  proposed_at: string;
  decision: StaleProposalDecision;
  decided_at: string | null;
  decided_by_user_id: string | null;
  reason: string | null;
}
