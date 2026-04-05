# ox-source

Data source introspection: schema discovery + column profiling.

## Supported Sources

PostgreSQL, MySQL, MongoDB, CSV, JSON. Each implements `DataSourceIntrospector` trait.

## Adding a New Source

1. Create `my_source.rs` implementing `DataSourceIntrospector` (introspect_schema, collect_stats, analyze).
2. Register in `registry.rs` via `registry.register("my_source", |input| async { ... })`.
3. Input is `SourceInput` (connection string or file path).

## Concurrency

`introspect_tables_concurrent()` runs table introspection with configurable parallelism (default: 8). Used for large databases with many tables.

## Output Types

- `SourceSchema` — tables, columns (name + raw DB type + nullable), foreign keys.
- `SourceProfile` — row counts, distinct counts, sample values, min/max per column.
- Column `data_type` is stored as raw DB string (e.g., "varchar", "int4"). Use `PropertyType::infer_from_db_type()` in ox-core for mapping.
