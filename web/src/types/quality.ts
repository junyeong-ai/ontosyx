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
import type { components } from "./api.generated";

import type { OntologyDraft } from "./ontology-drafts";

// --- Ontology quality report (returned by design for DB sources) ---

export type QualityGapSeverity = components["schemas"]["QualityGapSeverity"];
export type QualityGapCategory = components["schemas"]["QualityGapCategory"];
export type QualityGapRef = components["schemas"]["QualityGapRef"];
export type QualityGap = components["schemas"]["QualityGap"];
export type QualityConfidence = components["schemas"]["QualityConfidence"];
export type OntologyQualityReport = components["schemas"]["OntologyQualityReport"];

// --- Ontology Revision History ---

export type RevisionSummary = components["schemas"]["OntologySnapshotSummary"];
export type OntologySnapshot = Omit<
  components["schemas"]["OntologySnapshot"],
  "ontology" | "quality_report"
> & {
  ontology: OntologyIR;
  quality_report?: OntologyQualityReport | null;
};
export type RestoreOntologyDraftRevisionResponse = Omit<
  components["schemas"]["RestoreOntologyDraftRevisionResponse"],
  "project"
> & { project: OntologyDraft };

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

export type WorkbenchPerspective = components["schemas"]["WorkbenchPerspective"];
export type UpsertPerspectiveRequest = components["schemas"]["UpsertPerspectiveRequest"];

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
  variable?: string | null;
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
  owner_variable?: string | null;
  property_name: string;
  property_id: string;
  owner_id: string;
  binding_kind: BindingKind;
  scope_path: ScopeSegment[];
  usage_hint: PropertyUsageHint;
}

// --- Reconcile (LLM refine diff) ---

export type EntityKind = components["schemas"]["ReconcileEntityKind"];
export type ReconcileConfidence = components["schemas"]["ReconcileConfidence"];
export type UncertainMatch = components["schemas"]["UncertainMatch"];
export type ReconcileReport = components["schemas"]["ReconcileReport"];
export type MatchDecision = components["schemas"]["MatchDecision"];

export interface PendingReconcile {
  report: ReconcileReport;
  reconciled_ontology: OntologyIR;
}

export type ReconcileOntologyDraftRequest = Omit<
  components["schemas"]["ReconcileOntologyDraftRequest"],
  "reconciled_ontology"
> & { reconciled_ontology: OntologyIR };

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

export type MetricWindowApi = components["schemas"]["MetricWindow"];
export type MetricValue = components["schemas"]["MetricValue"];
export type QualityMetricsReport = components["schemas"]["QualityMetricsReport"];
export type ShaclFailureKind = components["schemas"]["ShaclFailureKind"];
export type ShaclFailureCount = components["schemas"]["ShaclFailureCount"];
export type StaleTypeEntry = components["schemas"]["StaleTypeEntry"];
export type StaleProposalDecision = components["schemas"]["StaleProposalDecision"];
export type StaleConceptProposal = components["schemas"]["StaleConceptProposal"];
export type DecideStaleProposalRequest =
  components["schemas"]["DecideStaleProposalRequest"];
export type BulkDecideStaleProposalsRequest =
  components["schemas"]["BulkDecideStaleProposalsRequest"];
export type BulkDecideStaleProposalsResponse =
  components["schemas"]["BulkDecideStaleProposalsResponse"];
