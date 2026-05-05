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

/** Subset of `OntologyDiff` the branching surface renders. The
 *  full IR-level diff lives on the canonical schema; the branching
 *  page only needs the high-level counts plus the entity lists for
 *  display. The wire shape stays loose (`unknown` for entity rows)
 *  because the diff component doesn't deserialize past the
 *  identifying fields. */
export interface OntologyDiffSummary {
  added_nodes: unknown[];
  removed_nodes: unknown[];
  modified_nodes: unknown[];
  added_edges: unknown[];
  removed_edges: unknown[];
  modified_edges: unknown[];
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
