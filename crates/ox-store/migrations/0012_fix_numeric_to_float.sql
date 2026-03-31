-- ============================================================
-- 0012: Fix NUMERIC → FLOAT8 for sqlx compatibility
-- ============================================================
-- NUMERIC(n,m) is not directly mappable to Rust f64 via sqlx.
-- Change to FLOAT8 (= DOUBLE PRECISION) which maps cleanly.
-- ============================================================

-- quality_rules.threshold: NUMERIC(5,2) → FLOAT8
ALTER TABLE quality_rules ALTER COLUMN threshold TYPE FLOAT8 USING threshold::float8;

-- usage_records.cost_usd: NUMERIC(12,6) → FLOAT8
ALTER TABLE usage_records ALTER COLUMN cost_usd TYPE FLOAT8 USING cost_usd::float8;

-- ecommerce tables use NUMERIC too but those are in a separate DB (postgres-ecommerce)
-- and are accessed via ox-source introspector, not sqlx models.
