-- ============================================================================
-- 0027_stale_concept_proposals.sql
--
-- Background-scanned deprecation proposals for types unused beyond
-- the staleness cutoff. Patent matrix says "95% 자동 · deprecated
-- 제안만, 삭제는 HITL" — the cron inserts rows here; the admin UI
-- transitions `decision` to approved / dismissed.
--
-- Natural key (workspace_id, type_id) prevents duplicate open
-- proposals; re-running the cron is idempotent.
--
-- Decision lifecycle:
--   pending → approved | dismissed
-- `decided_at` + `decided_by_user_id` capture audit state; a
-- dismissed proposal can be re-proposed after a further window by
-- clearing the row (admin UI surfaces "re-propose").
-- ============================================================================

CREATE TABLE stale_concept_proposals (
    id                    UUID PRIMARY KEY,
    workspace_id          UUID NOT NULL,
    type_id               UUID NOT NULL,
    type_kind             TEXT NOT NULL,
    last_used_at          TIMESTAMPTZ,
    days_since_last_use   INT NOT NULL,
    proposed_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    decision              TEXT NOT NULL DEFAULT 'pending'
        CHECK (decision IN ('pending', 'approved', 'dismissed')),
    decided_at            TIMESTAMPTZ,
    decided_by_user_id    UUID,
    reason                TEXT,

    UNIQUE (workspace_id, type_id)
);

ALTER TABLE stale_concept_proposals ENABLE ROW LEVEL SECURITY;
ALTER TABLE stale_concept_proposals FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON stale_concept_proposals
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON stale_concept_proposals
    USING (current_setting('app.system_bypass', true) = 'true');

-- List-open-proposals is the hot read path.
CREATE INDEX idx_stale_concept_proposals_open
    ON stale_concept_proposals (workspace_id, proposed_at DESC)
    WHERE decision = 'pending';
