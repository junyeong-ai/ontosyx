-- Workspace isolation for queue + ledger tables.
--
-- `idempotency_records` already carries workspace_id but had no RLS
-- policies; `pending_embeddings` lacked the column entirely. Both
-- tables back per-workspace state and must reject cross-workspace
-- reads/writes through the same RLS contract every other
-- workspace-scoped table uses.

ALTER TABLE pending_embeddings
    ADD COLUMN workspace_id uuid NOT NULL
        DEFAULT current_setting('app.workspace_id', true)::uuid;

ALTER TABLE pending_embeddings
    ALTER COLUMN workspace_id DROP DEFAULT;

CREATE INDEX pending_embeddings_workspace_idx
    ON pending_embeddings (workspace_id, created_at);

ALTER TABLE pending_embeddings ENABLE ROW LEVEL SECURITY;
ALTER TABLE pending_embeddings FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON pending_embeddings
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON pending_embeddings
    USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE idempotency_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE idempotency_records FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON idempotency_records
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON idempotency_records
    USING (current_setting('app.system_bypass', true) = 'true');
