-- ============================================================================
-- 0013_data_sources_mysql_kind.sql
--
-- Allow `kind = 'mysql'` in the data_sources CHECK constraint now
-- that MysqlAdapter::scan() ships. Follows the same drop-and-recreate
-- pattern 0012 used for the Postgres kind.
-- ============================================================================

ALTER TABLE data_sources DROP CONSTRAINT data_sources_kind_allowed;
ALTER TABLE data_sources ADD CONSTRAINT data_sources_kind_allowed
    CHECK (kind IN ('csv', 'json', 'postgres', 'mysql'));
