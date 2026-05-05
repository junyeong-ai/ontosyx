-- 0005_design_project_parent_version.sql
--
-- Tracks which canonical ontology version a design project's
-- in-flight `ontology` JSONB was branched from. The completion
-- handler compares this against the canonical's current head; a
-- mismatch means another commit landed while the project was in
-- flight, so the project's local copy would silently overwrite
-- those changes if it committed unconditionally. With this column
-- the handler can refuse the commit and force the operator to
-- rebase onto the new head before retrying.
--
-- `ON DELETE SET NULL` mirrors the policy on
-- `ontology_version_snapshots.parent_version_id` — version
-- snapshots are immutable, but the foreign key relaxes to NULL if
-- one is ever pruned so the project row stays loadable.

ALTER TABLE ontology_drafts
    ADD COLUMN parent_version_id UUID
        REFERENCES ontology_version_snapshots(id) ON DELETE SET NULL;
