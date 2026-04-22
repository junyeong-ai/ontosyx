-- ============================================================================
-- 0015_data_sources_clean_slate.sql
--
-- Two changes that go together:
--
-- 1. Clear every `data_sources` row. The JSONB `config` shape changed
--    with the federation admin refactor: the old wire form was a flat
--    `{"data": "..."}` / `{"connection_string": "...",
--    "schema_name": "..."}`, the new shape is
--    `{"credential": {"kind": "inline"|"secret_ref", "value": "..."},
--     "schema_name"?: "..."}`. The Rust enum layer refuses to decode
--    the legacy shape by design (clean-slate refactor — no backwards
--    compat hacks), so any surviving pre-refactor row would hydrate
--    with a warn and silently drop off its workspace's resolver.
--    Truncating here makes the failure obvious at migration time
--    instead of at first federation query.
--
-- 2. Drop the `data_sources_kind_allowed` CHECK constraint. The Rust
--    `RegisterAdapterKind::from_stored` is the authoritative list of
--    supported kinds; the DB-level CHECK duplicated that list,
--    required a fresh migration every time a new kind shipped
--    (0012 → postgres, 0013 → mysql, 0014 → bigquery), and drifted
--    from the Rust enum under renames. An invalid kind now surfaces
--    as a 400 from the admin handler, not a Postgres constraint
--    error swallowed under a 500.
-- ============================================================================

TRUNCATE TABLE data_sources;

ALTER TABLE data_sources DROP CONSTRAINT IF EXISTS data_sources_kind_allowed;
