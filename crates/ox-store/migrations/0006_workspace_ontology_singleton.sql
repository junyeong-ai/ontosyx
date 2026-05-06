-- 0006_workspace_ontology_singleton.sql
--
-- Workspace × Ontology cardinality is 1:1 — every workspace owns
-- exactly one canonical ontology. Aligns with every validated
-- knowledge-graph platform (Foundry, Stardog, Neo4j Aura): the
-- KG's value lives in cross-entity connections, and per-project
-- ontologies break the "same Customer" guarantee that connection
-- reach depends on.
--
-- Two structural consequences:
--
-- 1. `ontologies(workspace_id)` carries a UNIQUE constraint so the
--    schema rejects a second ontology being created in the same
--    workspace. The two compound uniqueness constraints
--    (`ontologies_ws_id_uq`, `ontologies_ws_name_uq`) become
--    redundant and drop with it — workspace-uniqueness already
--    implies workspace-scoped name + id uniqueness.
--
-- 2. `ontology_drafts.ontology_id` was a redundant FK back into
--    the workspace's only ontology. With singleton enforced, the
--    workspace_id IS the ontology pointer. The column drops along
--    with the compound FK that paired it with workspace_id.

ALTER TABLE ontology_drafts
    DROP CONSTRAINT ontology_drafts_ontology_ws_fk,
    DROP COLUMN ontology_id;

-- `query_executions` referenced `ontologies(workspace_id, id)` to
-- pin which canonical ontology owned each query. Under the
-- workspace × ontology singleton invariant the workspace_id alone
-- already names the canonical, so the compound FK is structural
-- deadweight — drop it before we drop the unique constraint it
-- depends on. The `ontology_id` column survives as a stable
-- correlation key on the row but no longer ties back to the
-- ontologies table; lookups go through `get_workspace_ontology()`.
ALTER TABLE query_executions
    DROP CONSTRAINT query_executions_ontology_ws_fk;

ALTER TABLE ontologies
    DROP CONSTRAINT ontologies_ws_id_uq,
    DROP CONSTRAINT ontologies_ws_name_uq,
    ADD CONSTRAINT ontologies_workspace_singleton_uq UNIQUE (workspace_id);
