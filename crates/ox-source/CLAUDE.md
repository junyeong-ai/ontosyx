# ox-source

Data source introspection: schema discovery + column profiling.

## Supported Sources

PostgreSQL, MySQL, MongoDB, CSV, JSON, DuckDB, Snowflake, BigQuery. Each implements `DataSourceAdapter` trait via the five atomic primitives `list_tables`, `describe_table`, `count_rows`, `sample_column`, `list_foreign_keys`.

## Table-type policy

`list_tables` and `list_tables_with_summary` MUST surface every queryable relation kind the backend exposes — base tables, views, materialised views, external tables, snapshots, clones. The ontology designer's source surface is whatever the warehouse advertises to its consumers; restricting the listing to managed-table subsets silently excludes valid analysis targets.

Adapter-specific filters:
- **PostgreSQL** — `information_schema.tables` (no `table_type` filter) UNION `pg_matviews`. Materialised views live outside `information_schema.tables`, so they need an explicit union.
- **MySQL** — `TABLE_TYPE <> 'SYSTEM VIEW'` only. SYSTEM VIEW rows are catalogue plumbing, not user data.
- **Snowflake** — no `TABLE_TYPE` filter. INFORMATION_SCHEMA.TABLES surfaces base / transient / view / materialised view / external / dynamic. Temporary tables are session-local and do not appear here.
- **BigQuery** — no `table_type` filter on `INFORMATION_SCHEMA.TABLES`. CLONE / SNAPSHOT / VIEW / EXTERNAL / MATERIALIZED VIEW are all queryable. The legacy `__TABLES__` view is used by `list_tables_with_summary` because it is dataset-scoped (vs region-scoped `INFORMATION_SCHEMA.TABLE_STORAGE`) and includes views.
- **MongoDB** — `system.*` collections are filtered out (catalogue plumbing).
- **DuckDB** — single virtual `data` table per file.

## Adding a New Source

1. Create `my_source.rs` implementing every `DataSourceAdapter` primitive — no default impls; missing methods are a compile error.
2. Register in `registry.rs` via `registry.register("my_source", |input| async { ... })`.
3. Input is `SourceInput` (connection string or file path).

## Cross-Cutting Orchestration

`IntrospectionKernel` wraps any `DataSourceAdapter` to add retry (via `RetryPolicy`), schema caching (via `CacheTtl`), and warning aggregation without each adapter re-implementing them. Callers that want retry + caching (UI request loops, analysis re-runs) should use the kernel instead of calling the adapter directly.

## Concurrency

`introspect_tables_concurrent()` runs table introspection with configurable parallelism (default: 8). Used for large databases with many tables.

## Output Types

- `SourceSchema` — tables, columns (name + raw DB type + nullable), foreign keys.
- `SourceProfile` — row counts, distinct counts, sample values, min/max per column.
- Column `data_type` is stored as raw DB string (e.g., "varchar", "int4"). Use `PropertyType::infer_from_db_type()` in ox-core for mapping.

## PII redaction at sample collection

`build_column_stats` calls `ox_core::source_schema::classify_pii_suspect_by_name` on every column name and, when a heuristic match fires (email / phone / password / token / national_id / payment_card / address / personal_name), drops `sample_values` and `min_value` / `max_value` before the row enters `SourceProfile`. Aggregate counts (`null_count`, `distinct_count`) survive — those carry no PII risk.

The defense is at the producer side rather than every consumer: analyzer reports, LLM prompt context, audit logs and the inspector sample preview all read the redacted profile by default. The user-confirmed `PropertyDef::pii_kind` annotation flow stays independent — `pii_redacted` on `ColumnStats` is the heuristic hint the FE renders as a "Redacted: <kind>" badge until the operator confirms or overrides.

The heuristic matches conservatively: `username` / `display_name` / `ip_address` / `mac_address` are exempt because false negatives are expensive (raw PII leaks) but false positives are cheap (FE badge the user can override).

## Extension flow recovers cross-baseline FKs

`IntrospectionKernel::analyze_extension` runs the subset introspection of the new tables, then re-queries the source's full FK catalogue and appends only cross-table edges (one endpoint baseline, one endpoint new). Without this pass the subset filter would drop relationships that connect new tables back to baseline tables, leaving the merged result blind to the relationship that motivates the extension. Test pin: `analyze_extension_recovers_cross_baseline_foreign_keys`.
