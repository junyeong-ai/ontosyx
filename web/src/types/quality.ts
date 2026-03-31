// ---------------------------------------------------------------------------
// Quality, diff, perspective, bindings, reconciliation, and streaming types
// ---------------------------------------------------------------------------

import type {
  Cardinality,
  NodeTypeDef,
  EdgeTypeDef,
  PropertyDef,
  OntologyIR,
} from "./ontology";

import type {
  DesignProject,
  SourceMapping,
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
  issue: string;
  suggestion: string;
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
  source_mapping: SourceMapping | null;
  quality_report: OntologyQualityReport | null;
  created_at: string;
}

export interface ProjectRestoreResponse {
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
  | { type: "description_changed"; old: string | null; new: string | null }
  | { type: "property_added"; property: PropertyDef }
  | { type: "property_removed"; property: PropertyDef }
  | { type: "property_modified"; property_name: string; changes: PropertyChange[] }
  | { type: "constraint_added"; constraint: string }
  | { type: "constraint_removed"; constraint: string };

export type PropertyChange =
  | { type: "type_changed"; old: string; new: string }
  | { type: "nullability_changed"; old: boolean; new: boolean }
  | { type: "description_changed"; old: string | null; new: string | null }
  | { type: "default_value_changed"; old: string | null; new: string | null };

export interface EdgeDiffEntry {
  edge_id: string;
  label: string;
  changes: EdgeChange[];
}

export type EdgeChange =
  | { type: "label_changed"; old: string; new: string }
  | { type: "description_changed"; old: string | null; new: string | null }
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

export interface PerspectiveUpsertRequest {
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
  property_bindings: PropertyBinding[];
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

export interface PropertyBinding {
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

export interface ProjectReconcileRequest {
  revision: number;
  reconciled_ontology: OntologyIR;
  decisions: MatchDecision[];
  uncertain_matches: UncertainMatch[];
}

export interface ProjectRefineResponse {
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

export interface ProjectDesignResponse {
  project: DesignProject;
}
