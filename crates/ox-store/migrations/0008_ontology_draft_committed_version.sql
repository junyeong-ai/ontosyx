-- 0008_ontology_draft_committed_version.sql
--
-- Records which canonical version snapshot a completed draft
-- produced. The earlier `ontology_id` link (dropped in 0006)
-- pointed at the singleton ontology identity — redundant since
-- workspace × ontology = 1:1 already determines it. The
-- `committed_version_id` link is the missing piece on the
-- VERSION axis: which exact `ontology_version_snapshots` row
-- did this draft commit?
--
-- Operators read it to:
--   - Open the snapshot from the draft surface ("see the version
--     this draft produced") without resolving via lineage.
--   - Replay / time-travel to the exact draft → snapshot pair.
--   - Drive the branching tree's "fork from draft" arrow back
--     to the snapshot the draft committed into.
--
-- `ON DELETE SET NULL` mirrors the policy on
-- `ontology_drafts.parent_version_id`: snapshots are immutable,
-- but if one is ever pruned the draft row stays loadable.

ALTER TABLE ontology_drafts
    ADD COLUMN committed_version_id UUID
        REFERENCES ontology_version_snapshots(id) ON DELETE SET NULL;
