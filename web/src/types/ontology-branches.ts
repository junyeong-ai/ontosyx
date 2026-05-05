// Wire shape for `GET /api/ontology/versions` + branching tree
// inputs. Workspace × ontology = 1:1, so the tree is a single
// canonical lineage (committed version history) with drafts
// hanging off each version via `parent_version_id`.

export interface OntologyVersionEntry {
  id: string;
  version: string;
  committed_by: string;
  commit_message: string;
  created_at: string;
  /** Parent version this commit was branched from. Absent for
   *  the very first version of the canonical lineage. */
  parent_version_id?: string;
  /** True when this is the current canonical head — the row
   *  whose `valid_to` is null on the BE. */
  is_current: boolean;
}

export interface OntologyVersionsResponse {
  versions: OntologyVersionEntry[];
}

/** Added / removed node — full `NodeTypeDef` on the wire; the
 *  branching surface only renders `id` + `label`. Other
 *  consumers (graph delta visualisation) read the full shape
 *  via the same response. */
export interface DiffAddedNode {
  id: string;
  label: string;
  /** Other `NodeTypeDef` fields ride along on the wire but are
   *  not destructured here. */
  [extra: string]: unknown;
}

export interface DiffAddedEdge {
  id: string;
  label: string;
  [extra: string]: unknown;
}

/** Localized text wire shape mirror — kept narrow here so the
 *  branching surface doesn't pull the full `LocalizedText` import
 *  graph. The drilldown reads `.default` only for now. */
export interface DiffLocalizedText {
  default: string;
  translations?: Record<string, string>;
}

/** PropertyChange variants — one per atomic property field
 *  delta the BE detects. Tagged via `serde(tag = "type",
 *  rename_all = "snake_case")` on the BE side. */
export type PropertyChange =
  | { type: "type_changed"; old: string; new: string }
  | { type: "nullability_changed"; old: boolean; new: boolean }
  | {
      type: "description_changed";
      old: DiffLocalizedText;
      new: DiffLocalizedText;
    }
  | {
      type: "default_value_changed";
      old: string | null;
      new: string | null;
    };

/** NodeChange variants — one per atomic node-level delta. */
export type NodeChange =
  | { type: "label_changed"; old: string; new: string }
  | {
      type: "description_changed";
      old: DiffLocalizedText;
      new: DiffLocalizedText;
    }
  | { type: "property_added"; property: { name: string } & Record<string, unknown> }
  | {
      type: "property_removed";
      property: { name: string } & Record<string, unknown>;
    }
  | {
      type: "property_modified";
      property_name: string;
      changes: PropertyChange[];
    }
  | { type: "constraint_added"; constraint: string }
  | { type: "constraint_removed"; constraint: string };

/** EdgeChange variants — one per atomic edge-level delta. */
export type EdgeChange =
  | { type: "label_changed"; old: string; new: string }
  | {
      type: "description_changed";
      old: DiffLocalizedText;
      new: DiffLocalizedText;
    }
  | { type: "source_changed"; old: string; new: string }
  | { type: "target_changed"; old: string; new: string }
  | { type: "cardinality_changed"; old: string; new: string }
  | { type: "property_added"; property: { name: string } & Record<string, unknown> }
  | {
      type: "property_removed";
      property: { name: string } & Record<string, unknown>;
    }
  | {
      type: "property_modified";
      property_name: string;
      changes: PropertyChange[];
    };

/** Modified node — id + label plus the BE's per-field change
 *  list. */
export interface DiffModifiedNode {
  node_id: string;
  label: string;
  changes: NodeChange[];
}

export interface DiffModifiedEdge {
  edge_id: string;
  label: string;
  changes: EdgeChange[];
}

/** Subset of the BE `OntologyDiff` shape the branching surface
 *  renders. Drives both the inline summary badge on each draft
 *  row and the entity-level drilldown modal. The full IR-level
 *  diff (per-property changes inside a `NodeDiff`) lives on the
 *  canonical schema; first iteration here keeps the entity rows
 *  to id + label so the modal stays scannable. */
export interface OntologyDiffSummary {
  added_nodes: DiffAddedNode[];
  removed_nodes: DiffAddedNode[];
  modified_nodes: DiffModifiedNode[];
  added_edges: DiffAddedEdge[];
  removed_edges: DiffAddedEdge[];
  modified_edges: DiffModifiedEdge[];
  summary: {
    total_changes: number;
    nodes_added: number;
    nodes_removed: number;
    nodes_modified: number;
    edges_added: number;
    edges_removed: number;
    edges_modified: number;
    properties_added: number;
    properties_removed: number;
  };
}

export interface RebaseDraftResponse {
  /** Refreshed `OntologyDraftView` after the rebase — operator drops it
   *  back into the local store via the existing snapshot-apply
   *  path. */
  project: unknown;
}

/** Rebase preview — read-only conflict analysis. Mirrors
 *  `ox_ontology::rebase::RebaseAnalysis` plus a head pin for
 *  optimistic-confirm. */
export interface RebaseAnalysis {
  base_to_head: OntologyDiffSummary;
  base_to_draft: OntologyDiffSummary;
  conflicts: RebaseConflict[];
}

export interface RebasePreviewResponse {
  already_at_head: boolean;
  analysis: RebaseAnalysis;
  head_version_id: string | null;
}

/** Conflict variants — mirror `ox_ontology::rebase::RebaseConflict`
 *  on `serde(tag = "kind", rename_all = "snake_case")`. */
export type RebaseConflict =
  | {
      kind: "add_add";
      entity_kind: "node" | "edge";
      entity_id: string;
      label: string;
    }
  | {
      kind: "modify_remove";
      entity_kind: "node" | "edge";
      entity_id: string;
      label: string;
      modifier: "draft" | "head";
    }
  | {
      kind: "modify_modify";
      entity_kind: "node" | "edge";
      entity_id: string;
      label: string;
      axes: ConflictAxis[];
    };

/** ConflictAxis — atomic clash per modify/modify entity. */
export type ConflictAxis =
  | { axis: "label"; head: string; draft: string }
  | { axis: "description"; head: string; draft: string }
  | { axis: "source"; head: string; draft: string }
  | { axis: "target"; head: string; draft: string }
  | { axis: "cardinality"; head: string; draft: string }
  | {
      axis: "property_overlap";
      property_name: string;
      atoms: PropertyConflictAtom[];
    }
  | {
      axis: "property_modify_remove";
      property_name: string;
      modifier: "draft" | "head";
    }
  | { axis: "property_add_add"; property_name: string };

export type PropertyConflictAtom =
  | { axis: "type"; head: string; draft: string }
  | { axis: "nullability"; head: boolean; draft: boolean }
  | { axis: "description"; head: string; draft: string }
  | {
      axis: "default_value";
      head: string | null;
      draft: string | null;
    };
