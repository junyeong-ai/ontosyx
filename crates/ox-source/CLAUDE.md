# ox-source

Data source introspection: schema discovery + column profiling.

## Supported Sources

PostgreSQL, MySQL, MongoDB, CSV, JSON, DuckDB, Snowflake, BigQuery. Each implements `DataSourceAdapter` trait via the five atomic primitives `list_tables`, `describe_table`, `count_rows`, `sample_column`, `list_foreign_keys`.

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
