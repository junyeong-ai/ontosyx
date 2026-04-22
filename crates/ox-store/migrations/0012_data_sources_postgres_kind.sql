-- ============================================================================
-- 0012_data_sources_postgres_kind.sql
--
-- Extend the data_sources.kind CHECK constraint to permit the
-- 'postgres' adapter kind now that PostgresAdapter::scan() ships.
-- Migration 0011 originally restricted to ('csv', 'json') — adding
-- a new kind requires dropping and recreating the constraint
-- because Postgres has no ALTER CONSTRAINT ADD VALUE syntax for
-- CHECK constraints.
-- ============================================================================

ALTER TABLE data_sources DROP CONSTRAINT data_sources_kind_allowed;
ALTER TABLE data_sources ADD CONSTRAINT data_sources_kind_allowed
    CHECK (kind IN ('csv', 'json', 'postgres'));
