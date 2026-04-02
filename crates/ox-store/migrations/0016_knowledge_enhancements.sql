-- Property-level correction tracking for knowledge entries.
-- Enables finer-grained staleness detection (e.g., "brand_id is not a property").

ALTER TABLE knowledge_entries
    ADD COLUMN IF NOT EXISTS affected_properties TEXT[] NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_knowledge_affected_properties
    ON knowledge_entries USING GIN (affected_properties);
