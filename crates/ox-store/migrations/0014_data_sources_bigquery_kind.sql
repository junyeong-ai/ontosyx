-- ============================================================================
-- 0014_data_sources_bigquery_kind.sql
--
-- Allow `kind = 'bigquery'` in the data_sources CHECK constraint
-- now that BigQueryAdapter::scan() ships. Follows the drop-and-
-- recreate pattern 0012 / 0013 use.
-- ============================================================================

ALTER TABLE data_sources DROP CONSTRAINT data_sources_kind_allowed;
ALTER TABLE data_sources ADD CONSTRAINT data_sources_kind_allowed
    CHECK (kind IN ('csv', 'json', 'postgres', 'mysql', 'bigquery'));
