-- ============================================================
-- 0018: Add column-level property mappings to data_lineage
-- ============================================================
-- Tracks which source columns map to which graph properties,
-- including any transformations applied during load.
-- Enables column-level lineage visualization.
-- ============================================================

ALTER TABLE data_lineage ADD COLUMN IF NOT EXISTS property_mappings JSONB;

COMMENT ON COLUMN data_lineage.property_mappings IS
    'JSON array of {source_column, graph_property, transform?} mappings';
