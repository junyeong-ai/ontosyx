-- 0009_audit_log_lexicon_backfill.sql
--
-- Realigns historical `audit_log` rows with the canonical
-- `ontology_draft` lexicon. The Project → OntologyDraft sweep
-- (commits 420eec2 / 566c49c / 692ff88) renamed the runtime
-- emitters but left audit_log rows authored before the sweep
-- as-is — `WHERE action_type LIKE 'ontology_draft.%'` queries
-- silently miss every pre-rename row, splitting the audit
-- timeline across two lexicons.
--
-- Per the project-wide directive ("처음부터 이렇게 설계된
-- 것처럼, no traces"), historical rows realign in place rather
-- than through a parallel `WHERE action_type IN ('project.*',
-- 'ontology_draft.*')` view. The audit_log table's
-- `(action_type, resource_type)` shape stays — the strings inside
-- update.
--
-- Rows touched:
--   - action_type 'project.create' → 'ontology_draft.create'
--   - action_type 'project.delete' → 'ontology_draft.delete'
--   - action_type 'project.design' → 'ontology_draft.design'
--   - action_type 'project.refine' → 'ontology_draft.refine'
--   - action_type 'project.extend' → 'ontology_draft.extend'
--   - action_type 'project.reanalyze' → 'ontology_draft.reanalyze'
--   - action_type 'project.complete' → 'ontology_draft.complete'
--   - action_type 'project.<anything>' → 'ontology_draft.<anything>'
--     (catches future surfaces the runtime might have emitted
--     between the sweep and this migration)
--   - resource_type 'project' → 'ontology_draft'
--
-- The single REPLACE on action_type catches every dotted
-- variant in one statement; the resource_type update is a
-- separate equality check because that column has no nested
-- hierarchy.

UPDATE audit_log
   SET action_type = REPLACE(action_type, 'project.', 'ontology_draft.')
 WHERE action_type LIKE 'project.%';

UPDATE audit_log
   SET resource_type = 'ontology_draft'
 WHERE resource_type = 'project';

-- The matching `approval_requests` and `usage_records` tables
-- carry their own resource_type strings; align them too so a
-- cross-table audit query (joining audit_log + approval_requests
-- on (resource_type, resource_id)) sees a single lexicon.

UPDATE approval_requests
   SET resource_type = 'ontology_draft'
 WHERE resource_type = 'project';

UPDATE usage_records
   SET resource_type = 'ontology_draft'
 WHERE resource_type = 'project';
