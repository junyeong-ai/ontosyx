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

/** Modified node — id + label plus the BE's per-field change
 *  list. The change list is opaque to the FE today; the count
 *  is what the operator sees on the category row. Drilldown
 *  into individual changes lands as a follow-up surface. */
export interface DiffModifiedNode {
  node_id: string;
  label: string;
  changes: unknown[];
}

export interface DiffModifiedEdge {
  edge_id: string;
  label: string;
  changes: unknown[];
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
    added_count: number;
    removed_count: number;
    modified_count: number;
  };
}

export interface RebaseDraftResponse {
  /** Refreshed `ProjectView` after the rebase — operator drops it
   *  back into the local store via the existing snapshot-apply
   *  path. */
  project: unknown;
}
