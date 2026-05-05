-- 0005_ontology_draft_parent_version.sql
--
-- Tracks which canonical ontology version an ontology draft's
-- in-flight `ontology` JSONB was branched from. The completion
-- handler compares this against the canonical's current head; a
-- mismatch means another commit landed while the draft was in
-- flight, so the draft's local copy would silently overwrite
-- those changes if it committed unconditionally. With this column
-- the handler can refuse the commit and force the operator to
-- rebase onto the new head before retrying.
--
-- `ON DELETE SET NULL` mirrors the policy on
-- `ontology_version_snapshots.parent_version_id` — version
-- snapshots are immutable, but the foreign key relaxes to NULL if
-- one is ever pruned so the draft row stays loadable.

ALTER TABLE ontology_drafts
    ADD COLUMN parent_version_id UUID
        REFERENCES ontology_version_snapshots(id) ON DELETE SET NULL;
