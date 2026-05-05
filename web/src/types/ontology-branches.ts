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
